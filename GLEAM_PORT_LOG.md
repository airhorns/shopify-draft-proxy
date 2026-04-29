# GLEAM_PORT_LOG.md

A chronological log of the Gleam port. Each pass adds a new dated entry
describing what landed, what was learned, and what is now blocked or
unblocked. The acceptance criteria and design constraints live in
`GLEAM_PORT_INTENT.md`; this file is the running narrative.

Newer entries go at the top.

---

## 2026-04-29 — Pass 16: apps domain read path

Completes the apps domain read path. Lands a new
`shopify_draft_proxy/proxy/apps.gleam` mirroring the read shape of
`src/proxy/apps.ts`: the six query roots (`app`, `appByHandle`,
`appByKey`, `appInstallation`, `appInstallations`,
`currentAppInstallation`), per-record source projections for every
apps record type, the `__typename`-discriminated
`AppSubscriptionPricing` sum, and the three child connections
(`activeSubscriptions` array, `allSubscriptions` /
`oneTimePurchases` / `usageRecords` connections). Adds `AppsDomain`
to the dispatcher: capability-driven for registry-loaded operations,
legacy-fallback predicate `apps.is_app_query_root` for unmigrated
tests.

### Module table

| Module | Lines | Notes |
| --- | --- | --- |
| `src/shopify_draft_proxy/proxy/apps.gleam` | +560 | New module. Surfaces: `AppsError`, `is_app_query_root`, `handle_app_query`, `wrap_data`, `process`. Internal: `serialize_root_fields` / `root_payload_for_field` dispatch, six per-root serializers, `app_to_source` / `app_installation_to_source` / `subscription_to_source` / `line_item_to_source` / `usage_record_to_source` / `one_time_purchase_to_source` / `access_scope_to_source` / `money_to_source` / `pricing_to_source` (the sum-type discriminator), three connection-source builders (`subscription_connection_source`, `one_time_purchase_connection_source`, `usage_record_connection_source`) plus a tiny shared `page_info_source`. |
| `src/shopify_draft_proxy/proxy/draft_proxy.gleam` | +18 | Wires `AppsDomain` into both the capability-driven dispatch (added `Apps -> Ok(AppsDomain)` to `capability_to_query_domain`) and the legacy fallback (`apps.is_app_query_root`). New `AppsDomain` variant on `Domain`. Added `import shopify_draft_proxy/proxy/apps`. |
| `test/shopify_draft_proxy/proxy/apps_test.gleam` | +330 | 19 new tests: `is_app_query_root` predicate, all six query roots (happy path + missing/null), inline-fragment-based `__typename` split for `AppRecurringPricing` vs `AppUsagePricing`, child connections (active subscriptions array, oneTimePurchases connection, usageRecords connection), access-scope projection, and the `process` envelope wrap. Standard `run(store, query)` helper using `apps.handle_app_query`. |

**Test count: 386 → 405** (+19). Both targets clean (Erlang OTP 28 +
JS ESM).

### What landed

The read path is a pure function of `Store` — it never auto-creates
the default app or installation. That's a deliberate match to the TS
behavior: `handleAppQuery` reads only; `ensureCurrentInstallation`
is mutation-only. So the dispatcher signature didn't need to grow:
`apps.process(store, query, variables) -> Result(Json, AppsError)`
mirrors `webhooks.process` / `saved_searches.process` exactly.

Three connection-shaped fields (`allSubscriptions`,
`oneTimePurchases`, `usageRecords`) need to round-trip through the
`SourceValue` projector rather than the more direct
`serialize_connection` helper, because they're nested inside a parent
record whose outer projection owns the field selection. The pattern
that fell out: build a `SourceValue` shaped like a connection
(`{__typename, edges, nodes, pageInfo, totalCount}`) and let
`project_graphql_value` walk into it. `serialize_connection` handles
only the top-level `appInstallations` connection where the field
selection is owned directly.

The `AppSubscriptionPricing` sum type pattern-matches in
`pricing_to_source`: variant constructors emit different `__typename`
values plus their own field set. Inline-fragment selections like
`... on AppRecurringPricing { interval price { amount } }` then
go through `default_type_condition_applies` and gate cleanly. This
is the first port where a sum-type-discriminated union round-trips
through the projector — the webhook endpoint sum did the same shape
but inside a single record field, not at the top level of a record.

Field selection projection treats `is_test`/`test` as a Gleam keyword
clash carried over from Pass 15; the renamed Gleam field is `is_test`
but the GraphQL response key stays `test` because the `SourceValue`
record is built explicitly by name in the source builder.

### Findings

- **The `SourceValue` model scales to apps.** Pass 11's substrate
  designed for webhooks now carries 11 record types through the
  projector with no friction. Connections-as-source-values is the
  reusable pattern for nested connections; only the topmost
  connection needs `serialize_connection`.
- **Sum types as discriminated unions translate cleanly.** The
  `AppRecurringPricing` / `AppUsagePricing` split projects through
  the existing inline-fragment machinery without any new code in
  `graphql_helpers`. This is reassuring for the upcoming
  `MetafieldOwner` / `Node` interfaces in customers/products.
- **Domain modules are stabilizing in shape.** `apps.gleam`,
  `webhooks.gleam`, and `saved_searches.gleam` now share an almost
  identical scaffold: `Error` type, `is_*_query_root` predicate,
  `handle_*_query` returning `Result(Json, _)`, `wrap_data`,
  `process` for the dispatcher. Future read-path ports
  (delivery-settings, customers, products) can copy this structure.
- **The dispatcher's two-track resolution (capability + legacy
  predicate) is paying off.** Adding `AppsDomain` was a 5-line edit
  in three places: capability case, legacy fallback, and the
  dispatch arm. No risk of breaking existing routing because the
  predicates are name-disjoint.
- **JS-ESM parity continues.** No FFI in this pass; everything ran
  on both targets first try.

### Risks / open items

- **Mutation path is the next bottleneck.** Apps has 10 mutation
  roots (the largest mutation surface so far): purchaseOneTimeCreate,
  subscriptionCreate/Cancel/LineItemUpdate/TrialExtend,
  usageRecordCreate, revokeAccessScopes, uninstall,
  delegateAccessTokenCreate/Destroy. Each touches synthetic identity
  + store + identity registry. Significant code volume.
- **`ensureCurrentInstallation` deferred.** The lazy-bootstrap helper
  is used by 4 of the 10 mutations; it's not in this pass because
  the read path doesn't need it. The mutation pass will need to
  thread it through `(store, identity)` and bring in
  `confirmationUrl` / `tokenHash` / `tokenPreview` helpers (the
  latter requires a sha256 FFI — no `gleam_crypto` in stdlib).
- **No connection-arg honoring on apps connections.** The
  `subscription_connection_source` etc. emit a fixed page (no `first`
  / `after` filtering) because the SourceValue route doesn't see the
  field-arg machinery. The TS passes the same simplification through
  `paginateConnectionItems` with default options — but if a future
  test exercises pagination on a subscription connection, this will
  need lifting.
- **Connection `pageInfo` is hard-coded `hasNextPage: false`.** Same
  reason as above — there's no pagination state plumbed through the
  source builders. Acceptable for the current TS parity (the source
  arrays are short) but not a long-term shape.

### Recommendation for Pass 17

Land the apps **mutation path**. Concrete pieces, in order of
expected friction:

1. **`appUninstall` + `appRevokeAccessScopes`.** Smallest surface;
   they only flip an existing installation's `uninstalled_at` /
   `access_scopes`. No new helpers needed beyond `ensureCurrentInstallation`.
2. **`delegateAccessTokenCreate` + `delegateAccessTokenDestroy`.**
   Needs a sha256 FFI shim. Implement once with two adapters
   (`erlang:crypto:hash/2` and Node's `node:crypto.createHash`).
3. **`appPurchaseOneTimeCreate` + `appSubscriptionCreate`.**
   Establishes the `confirmationUrl` + synthetic-id plumbing.
   Subscription pulls in `appSubscriptionLineItemUpdate` next.
4. **`appSubscriptionCancel` + `appSubscriptionTrialExtend`.**
   Status-flip mutations on existing subscriptions.
5. **`appUsageRecordCreate`.** The richest payload: walks the
   subscription→line-item→capped-amount chain to validate. Save for
   last.

Expected delta: ~1100 LOC (handler + helpers + tests). The pattern
from `webhooks.process_mutation` is the load-bearing template:
`MutationOutcome { data, store, identity, staged_resource_ids }` is
the right shape and the validators from `mutation_helpers` already
carry the right error envelopes. After Pass 17 the apps domain
should be feature-complete, freeing Pass 18+ to start on
delivery-settings or customers.

---

## 2026-04-29 — Pass 15: apps domain — types & store slice

Foundation pass for the apps domain. Lands the seven new record types
(`AppRecord`, `AppInstallationRecord`, `AppSubscriptionRecord`,
`AppSubscriptionLineItemRecord`, `AppOneTimePurchaseRecord`,
`AppUsageRecord`, `DelegatedAccessTokenRecord`), plus the supporting
shapes (`Money`, `AccessScopeRecord`, `AppSubscriptionPricing` sum,
`AppSubscriptionLineItemPlan`), and adds the corresponding base/staged
slices and store helpers. **No proxy handler yet** — the read/write
ports are deferred to Pass 16+.

### Module table

| Module | Lines | Notes |
| --- | --- | --- |
| `src/shopify_draft_proxy/state/types.gleam` | +130 | New types: `Money`, `AccessScopeRecord`, `AppRecord`, `AppSubscriptionPricing` (sum: `AppRecurringPricing` / `AppUsagePricing`), `AppSubscriptionLineItemPlan`, `AppSubscriptionLineItemRecord`, `AppSubscriptionRecord`, `AppOneTimePurchaseRecord`, `AppUsageRecord`, `DelegatedAccessTokenRecord`, `AppInstallationRecord`. |
| `src/shopify_draft_proxy/state/store.gleam` | +400 | Seven new entity tables on `BaseState` / `StagedState` plus `current_installation_id` (Option). Helpers: `upsert_base_app`, `stage_app`, `get_effective_app_by_id`, `find_effective_app_by_handle`, `find_effective_app_by_api_key`, `list_effective_apps`, `upsert_base_app_installation` (atomic install + app), `stage_app_installation`, `get_effective_app_installation_by_id`, `get_current_app_installation`, `stage_app_subscription`, `get_effective_app_subscription_by_id`, `stage_app_subscription_line_item`, `get_effective_app_subscription_line_item_by_id`, `stage_app_one_time_purchase`, `get_effective_app_one_time_purchase_by_id`, `stage_app_usage_record`, `get_effective_app_usage_record_by_id`, `list_effective_app_usage_records_for_line_item`, `stage_delegated_access_token`, `find_delegated_access_token_by_hash`, `destroy_delegated_access_token`. |
| `test/shopify_draft_proxy/state/store_test.gleam` | +180 | 11 new tests covering each entity table: upsert/stage/get, the two app lookups (by handle, by api_key), installation singleton bootstrap, the per-line-item usage-records filter, and the destroy-then-find round trip on delegated tokens. |

Tests: 386 / 386 on Erlang OTP 28 and JavaScript ESM. Net +11 tests
(375 → 386); all new tests are in the existing `state/store_test`
suite.

### What landed

The TS schema models all pricing details as `Record<string, jsonValue>`
inside the line-item `plan` field — Gleam types it precisely as a
`AppSubscriptionPricing` sum with two variants (`AppRecurringPricing`
and `AppUsagePricing`), each carrying only the fields its `__typename`
implies. This makes impossible combinations (e.g. a recurring plan
with `cappedAmount`) unrepresentable rather than runtime-checked.

`Money` is defined as a top-level record so future domain ports can
reuse it instead of copying. `AccessScopeRecord` is similarly
domain-agnostic — the shape is shared with the access-scopes-API
endpoints whenever those land.

The `current_installation_id` is modelled as a `Option(String)`
field on both base and staged state, mirroring TS where the proxy
treats the current installation as a singleton bootstrapped on first
mutation. Staged wins; on first stage it auto-promotes if no current
is set on either side. `upsert_base_app_installation` (used by
hydration) atomically writes both the installation and its app to base.

`destroy_delegated_access_token` doesn't physically remove the token —
it stages a copy with `destroyed_at` set, mirroring TS. This keeps
the find-by-hash lookup honest (the token is still findable by hash;
callers check `destroyed_at`).

The seven entity tables follow the same shape (dict + order list, no
`deleted_*_ids` since apps records aren't tombstoned the way saved
searches and webhook subscriptions are — uninstalls are modelled by
setting `uninstalled_at` on the installation, and subscription
cancellation flips `status`). The new entities all use the simpler
"staged-over-base, no soft-delete" lookup pattern.

### Findings

- **The "no soft-delete" decision shapes the lookup helpers.**
  Saved searches and webhooks both have `deleted_*_ids` in both
  base and staged, with the lookup helpers checking those before
  returning a record. None of the apps entities work that way —
  uninstalls and subscription-cancels just mutate a status field.
  That's a strict subset of the saved-search/webhook lookup, so
  the apps helpers are simpler.
- **`record(..r, status: …)` for cancellation; sum types for
  pricing.** The Gleam record-update spread mirrors TS `{...r, status}`
  exactly. For the discriminated-union pricing details, sum types
  with named record variants give us projection-time type checking
  for free — when `proxy/apps.gleam` lands in Pass 16, it'll pattern
  match on `AppRecurringPricing` vs `AppUsagePricing` rather than
  fishing through a `Record<string, unknown>`.
- **`is_test` instead of `test`.** `test` is a Gleam keyword reserved
  for the test runner and rejected as a record field name. Renamed
  the field on `AppSubscriptionRecord` and `AppOneTimePurchaseRecord`.
  Anywhere the GraphQL field name is `test`, the projector / handler
  in Pass 16 will need an explicit mapping (TS shape → Gleam shape →
  back to TS-shaped JSON).
- **`types_mod` qualified import in store.gleam.** `destroy_delegated_access_token`
  needs to construct an updated `DelegatedAccessTokenRecord` via the
  spread syntax. The unqualified-imported constructor lookup
  resolves the type at the construction site, but the spread needs
  the qualified type reference. Aliasing the module to `types_mod`
  on import (instead of the default `types`) avoids a name collision
  with another `types` symbol elsewhere in the file. Worth keeping
  in mind for handler ports — a top-level `types as types_mod`
  alias is clearer than `import gleam/_/types` everywhere.

### Risks / open items

- **No proxy handler yet.** Pass 15 is foundation only; the read
  path (6 query roots) and write path (9 mutation roots) ship
  separately. The store helpers are exercised only by the unit
  tests so far — first real use is the Pass 16 read path.
- **`upsert_base_app_installation` and `stage_app_installation`
  current-id semantics differ slightly from TS.** TS implicitly
  sets `currentAppInstallation` whenever the proxy mints its own;
  upstream-hydrated installations don't auto-promote. The Gleam
  port currently auto-promotes both flavors. Worth revisiting in
  Pass 16 once the handler is reading the store back — if the
  consumer ends up reading the wrong installation, `stage_app_installation`
  needs a "don't promote" variant (or the handler has to clear
  staged.current_installation_id before staging).
- **No `__meta/state` serialization for any apps slice.** Carries
  forward from Pass 13 (webhooks). The dispatcher works
  independently of meta-state; this is a gap for offline
  introspection, not a runtime gap.
- **`AppRecord.title` is `Option(String)` to model the upstream
  `nullable` schema, but the proxy's locally-minted default app
  always populates it.** Handler should use `Some("...")` directly
  in Pass 16; consumers should handle `None` only on hydration.

### Recommendation for Pass 16

Land the apps **read path** — the 6 query roots (`app`, `appByHandle`,
`appByKey`, `appInstallation`, `appInstallations`,
`currentAppInstallation`) plus `defaultApp` / `ensureCurrentInstallation`
helpers. Mirrors Pass 12's webhook-read shape. Should land:

- `proxy/apps.gleam` with a `process_query` entry point and the
  `default_app` / `ensure_current_installation` helpers.
- The serializers for each record type (`AppRecord`,
  `AppInstallationRecord`, `AppSubscriptionRecord`, etc.),
  including the `_typename` discrimination on the
  `AppSubscriptionPricing` sum.
- Connection serialization for `appInstallations` (one connection
  with the current installation) and for the
  `subscription.lineItems` / `lineItem.usageRecords` /
  `installation.allSubscriptions` / `installation.oneTimePurchases`
  child connections.
- Dispatcher wiring on the registry and legacy-fallback paths in
  `proxy/draft_proxy.gleam`.

Pass 17 takes the **write path** (9 mutation roots), which exercises
the lifted `mutation_helpers` for the first time outside webhooks.
Pass 18 takes hydration + meta-state serialization.

---

## 2026-04-29 — Pass 14: shared mutation_helpers module

Pure refactor. Lifts the AST-level required-argument validator, the
three structured-error builders, the `id`-only validator variant, and
the resolved-arg readers out of `proxy/webhooks.gleam` into a new
`proxy/mutation_helpers.gleam` module. `proxy/saved_searches.gleam`
now uses the shared `read_optional_string`. No behavior change — the
goal is to lock in the shape before domain #3 has to copy it.

### Module table

| Module | Lines | Notes |
| --- | --- | --- |
| `src/shopify_draft_proxy/proxy/mutation_helpers.gleam` | +334 | New. Public surface: `RequiredArgument`, `validate_required_field_arguments`, `validate_required_id_argument`, `find_argument`, `build_missing_required_argument_error`, `build_null_argument_error`, `build_missing_variable_error`, `read_optional_string`, `read_optional_string_array`. |
| `src/shopify_draft_proxy/proxy/webhooks.gleam` | −260 | Removed local copies of the validator + error builders + readers; `handle_delete` now calls `validate_required_id_argument` and destructures `#(resolved_id, errors)` instead of the local `DeleteIdValidation` record. |
| `src/shopify_draft_proxy/proxy/saved_searches.gleam` | −10 | Removed local `read_optional_string`; imports the shared one. |
| `test/shopify_draft_proxy/proxy/mutation_helpers_test.gleam` | +260 | New. 22 unit tests covering the validator (happy / missing / multi-missing-joined / null literal / unbound variable / null variable / bound variable), the id validator (literal / missing / null / bound variable / unbound variable), the three error-builder JSON shapes, and the readers (present / absent / wrong-type / list filter). |

Tests: 375 / 375 on Erlang OTP 28 and JavaScript ESM. Net +22
(353 → 375); all new tests are in the new module-level suite.

### What landed

The split between AST validation and resolved-arg-dict execution
that webhooks introduced in Pass 13 is the load-bearing structural
choice — only the AST distinguishes "argument omitted" from "literal
null" from "unbound variable", and each maps to a distinct GraphQL
error code (`missingRequiredArguments` / `argumentLiteralsIncompatible`
/ `INVALID_VARIABLE`). Pass 14 lifts that pair (validator + readers)
out of the domain handler so the next domain doesn't have to choose
between copying ~250 LOC or rolling its own envelope shape.

`validate_required_id_argument` is the small generalization:
in Pass 13 it lived in webhooks as `validate_webhook_subscription_delete_id`
returning a domain-specific `DeleteIdValidation` record. The lifted
version returns `#(Option(String), List(Json))` — the resolved id
when validation passed (so the caller can skip a second
`get_field_arguments` lookup), or an error list. Any future
`*Delete` mutation (apps, segments, …) can use it directly.

`find_argument` was made public — it's a small AST utility but
useful for handlers that need to inspect a specific argument node
after validation passed (e.g. a custom shape check on a known-present
input object). Pass 13's webhook handlers used it internally; making
it public costs nothing and saves the next caller from re-implementing
linear-list lookup.

`read_optional_string` and `read_optional_string_array` are pure
sugar over `dict.get` + variant matching, but they're the exact
readers both saved-searches and webhooks have copy-pasted. Lifting
them now blocks the third copy.

### Findings

- **The AST-vs-resolved split lifts cleanly.** No domain-specific
  glue leaked into the helpers; the abstractions are the same ones
  TS uses. `RequiredArgument(name, expected_type)` mirrors the
  TS `[name, expectedType]` tuple exactly, with the type string
  used verbatim in the error message.
- **Parallel saved-searches / webhooks envelopes preserved on
  purpose.** Saved-searches still uses semantic `userErrors` for its
  validation failures; webhooks uses the structured top-level error
  envelope. The two are *not* unified because the TS source
  differentiates them — `saved-searches.ts` runs validation through
  a domain-specific `validate*` function that emits user errors,
  while `webhooks.ts` runs `validateRequiredFieldArguments` and emits
  top-level errors. The Gleam port mirrors the upstream divergence
  rather than fighting it.
- **The `dict.get` + ResolvedValue pattern is the only thing the
  readers need.** No source-of-truth indirection through `SourceValue`
  or the store — these helpers operate purely on resolved arg dicts.
  That keeps them dependency-light: any handler that has a resolved
  arg dict can use them, regardless of whether it's writing to staged
  state or reading from upstream.

### Risks / open items

- **No shared `read_optional_int` / `read_optional_bool` /
  `read_optional_object` yet.** Webhooks doesn't need them; saved-
  searches doesn't need them. The next domain might. Worth lifting
  on first reuse rather than speculatively now.
- **`__meta/state` still doesn't serialize webhook subscriptions.**
  Carried over from Pass 13 — the dispatcher works end-to-end, but
  the meta-state endpoint that consumers use for offline introspection
  only knows about `savedSearches`. Small follow-on for any pass
  that adds a meta-state consumer.
- **No structured `userErrors` builder yet.** Both domains hand-build
  their `{field, message}` shape inline. Symmetric to the top-level
  builders that just landed; lifting these would let a future domain
  emit consistent user-error envelopes without copying the JSON
  shape literal.

### Recommendation for Pass 15

Two viable directions:

1. **Webhook subscription hydration** (`upstream-hybrid` read path).
   This was option (1) in Pass 13's recommendation; Pass 14 taking
   the helper-unification path means option (1) is still the next
   big viability checkpoint. Pulls live records from Shopify and
   stages them locally — unlocks running the proxy against a real
   store.
2. **Start a new domain — `apps`** (`src/proxy/apps.ts`, ~967 LOC,
   6 query roots + 9 mutation roots, 6 record types in
   `state/types.ts:2336-2411`). Bigger surface than webhooks; would
   exercise the lifted helpers immediately and surface whatever
   second-pass abstraction opportunities they don't yet cover (e.g.
   `read_optional_int`, structured user-error builders).

Domain #3 has more signal: it forces the helpers to prove their
generality, and it's the next concrete viability checkpoint after
hydration. Hydration is the bigger user-visible feature.

---

## 2026-04-29 — Pass 13: webhook mutations

Closes the webhooks domain write path. Lands `process_mutation` plus
three handlers (`webhookSubscriptionCreate` / `Update` / `Delete`),
the AST-level required-argument validator that produces the structured
top-level error envelope TS uses (`extensions.code` =
`missingRequiredArguments` / `argumentLiteralsIncompatible` /
`INVALID_VARIABLE`), input readers + projection, mutation log
recording, and dispatcher wiring on both the registry and legacy
fallback paths.

### Module table

| Module | Lines | Notes |
| --- | --- | --- |
| `proxy/webhooks.gleam` | +600 | `process_mutation`, three handlers, validator, input readers, projection, log recording |
| `proxy/draft_proxy.gleam` | +30 | `WebhooksDomain` mutation arm + `is_webhook_subscription_mutation_root` legacy fallback |
| `test/proxy/webhooks_test.gleam` | +200 | 11 mutation tests (success, top-level errors, user errors, update/delete) |
| `test/proxy/draft_proxy_test.gleam` | +50 | 3 end-to-end dispatcher tests for create/missing-topic/blank-uri |

353 tests on Erlang OTP 28 + JS ESM (was 339 prior to this pass). +14 net.

### What landed

**`process_mutation`** (`proxy/webhooks.gleam`)

Mirrors the TS `handleWebhookSubscriptionMutation` entry point.
Returns `Result(MutationOutcome, WebhooksError)`, where
`MutationOutcome` carries `data: Json` (the *complete envelope*),
the updated `Store`, the threaded `SyntheticIdentityRegistry`, and
`staged_resource_ids: List(String)`. Multiple mutation root fields
in one document are folded across; per-field
`MutationFieldResult { key, payload, staged_resource_ids,
top_level_errors }` accumulates into either a `{"data": {...}}` or
`{"errors": [...]}` envelope based on whether `top_level_errors` is
non-empty after the fold. This matches the TS short-circuit:
top-level argument-validation failures replace the whole payload;
per-field user errors live alongside successful sibling fields.

**Three handlers** (`handle_create`, `handle_update`, `handle_delete`)

Each takes the resolved field arguments + the staging store + the
identity registry and returns a `MutationFieldResult`. Shapes:

- **Create.** Resolves `webhookSubscription` input, validates URI
  (blank → `userErrors[{field: ["webhookSubscription", "callbackUrl"], message: "Address can't be blank"}]`),
  mints a synthetic gid (`gid://shopify/WebhookSubscription/N?shopify-draft-proxy=synthetic`),
  mints deterministic `created_at`/`updated_at` via
  `synthetic_identity.make_synthetic_timestamp`, populates a fresh
  `WebhookSubscriptionRecord` from the input, and stages it.
- **Update.** Resolves `id` + `webhookSubscription` input, looks up
  the existing record (`get_effective_webhook_subscription_by_id`),
  applies overrides via `apply_webhook_update_input` (using
  `WebhookSubscriptionRecord(..existing, ...)` to preserve fields
  not present in input — equivalent to TS's `{...existing, ...overrides}`),
  mints a fresh `updated_at`, and stages the merged record. Unknown
  id → user error.
- **Delete.** Validates the id is non-empty (top-level error if blank
  string literal), looks up the existing record, calls
  `delete_staged_webhook_subscription`. Unknown id → user error
  payload (`deletedWebhookSubscriptionId: null`).

**AST-level validator** (`validate_required_field_arguments`)

The TS helper inspects `field.arguments` (the AST) — *not* the
resolved value dict — to distinguish three cases that all manifest
as "missing" downstream:

1. **Argument absent from AST** → `missingRequiredArguments` with
   the argument list joined by `, `.
2. **Argument present with literal `null` (`NullValue`)** →
   `argumentLiteralsIncompatible`, "Expected type 'X!'".
3. **Argument bound to a variable that is `null`/missing in the
   variables dict** → `INVALID_VARIABLE`, "Variable 'name' has not
   been provided" / "got invalid value null".

Mirrored by walking `Argument.value` against `NullValue`,
`VariableValue { name }` (with `dict.get(variables, name) ->
NullVal | Error(_)`), and "absent from list". The execution path
keeps using the resolved arg dict (`get_field_arguments`) — only
validation reads the AST.

**Dispatcher wiring** (`proxy/draft_proxy.gleam`)

Two arms added (mirrors Pass 12's read-path wiring):

```gleam
// capability path
Webhooks -> Ok(WebhooksDomain)
// legacy fallback
case webhooks.is_webhook_subscription_mutation_root(name) {
  True -> Ok(WebhooksDomain)
  False -> Error(Nil)
}
```

The `WebhooksDomain` arm in `route_mutation` calls
`webhooks.process_mutation(store, identity, path, query, variables)`,
re-records nothing if the call returns `Error(_)` (validator
internal failure surface), or records the resulting Json envelope
and forwards the new store / identity / staged ids on success.

### Findings

- **Top-level errors are envelope-shape, not status code.** Both
  successful payloads and validation failures are HTTP 200 — the
  difference is `{data: {...}}` vs `{errors: [...]}`. Holding the
  full envelope in `MutationOutcome.data` (rather than just the
  per-field payload) keeps the fold simple: append per-field errors
  to a single list, then emit one envelope at the end.
- **AST inspection is necessary, not optional.** Resolved-arg
  inspection cannot tell `null` apart from `undefined` from
  `unbound variable`. Each maps to a distinct GraphQL error code.
  The split between "validate against AST" and "execute against
  resolved dict" is small but load-bearing — same shape as TS.
- **`..existing` spread = TS object spread.** Field preservation in
  `apply_webhook_update_input` reads identically to JS:
  `WebhookSubscriptionRecord(..existing, uri: ..., name: ...)` is
  exactly `{...existing, uri: ..., name: ...}`. No helper needed.
- **Identity threading is uniform.** Both timestamp minting and gid
  minting flow through `SyntheticIdentityRegistry`; the registry
  threads back out of `MutationOutcome` so subsequent mutations see
  the incremented counter. Determinism preserved across multi-root
  documents.
- **Parallel implementation, not unification.** Saved-searches still
  emits the simpler `userErrors` flow (no top-level error envelope,
  no AST validator). Pass 12's recommendation flagged the choice;
  this pass kept them parallel because the TS handlers themselves
  diverge — saved-searches' `validateSavedSearchInput` returns
  `userErrors`, and only webhooks goes through
  `validateRequiredFieldArguments`. A future pass that unifies them
  must first decide whether to upgrade saved-searches to the
  structured form.

### Risks / open items

- **No `__meta/state` serialization for webhook subscriptions yet.**
  The dispatcher test confirms the mutation routes correctly via
  response body, but the in-store assertion lives in
  `webhooks_test`. Adding a `webhookSubscriptions` slice to the meta
  state serializer is small and should land alongside any consumer
  that wants to introspect staged webhook state from outside the
  store.
- **`Location` field is not emitted.** AST `Location` carries only
  character offsets, not line/column numbers; the `locations` field
  on the GraphQL error envelope is optional and we drop it. If a
  consumer ever asserts on it, we'll need to compute line/column
  from offsets.
- **`INVALID_VARIABLE` path for non-null variables.** Currently the
  validator only fires when the variable resolves to `null` /
  missing. The TS validator also catches type mismatches (e.g. an
  Int variable bound to a String literal). We don't validate types
  yet — that's a downstream-coercion concern, not a validation one,
  and the existing argument-resolver already handles common cases.
  Untested in either direction.
- **No log entry for top-level error mutations.** When validation
  fires, the per-field handler short-circuits before
  `record_mutation_log_entry` runs. TS records "failed" log entries
  for these; the Gleam port currently does not. Symmetric with
  saved-searches' "failed" entries (which the meta_log test
  exercises) — worth aligning.

### Recommendation for Pass 14

Two viable directions, ordered by signal-to-effort:

1. **Webhook subscription hydration** (`upstream-hybrid` read path).
   Pass 12 lands the read handler; the upstream-hybrid integration
   that pulls live records from Shopify and stages them locally is
   still TS-only. This unlocks running the proxy against a real
   store and is the next big viability checkpoint.
2. **Unify validator helpers + structured saved-search errors.**
   Lift `validate_required_field_arguments` and the input-reader
   helpers into a shared `proxy/mutation_helpers` module, and
   upgrade saved-searches to emit the same top-level error envelope
   as webhooks. Pure refactor — no new behavior, but locks in the
   shape before a third domain has to copy it.

The hydration path has more user-visible value but more surface
area; the helper unification is small and de-risks domain #3.

---

## 2026-04-29 — Pass 12: webhooks query handler + dispatcher wiring + store slice

Builds on Pass 11's substrate. Lands the read path for the webhooks
domain end to end: store slice, `handle_webhook_subscription_query`
implementing all three root payloads, and dispatcher wiring so an
incoming GraphQL request that names `webhookSubscription{,s,sCount}`
gets routed to the new module — both via the registry capability path
and the legacy fallback predicate. Mutations are still deferred to
Pass 13.

### Module table

| Module | Lines | Notes |
| --- | --- | --- |
| `state/store.gleam` | +130 | Webhook fields on Base/Staged + 5 accessors mirroring saved-search slice |
| `proxy/webhooks.gleam` | +280 | `handle_webhook_subscription_query`, `process`, root-field dispatch, projection helpers |
| `proxy/draft_proxy.gleam` | +12 | New `WebhooksDomain` variant; capability + legacy fallback dispatch |
| `test/state/store_test.gleam` | +90 | 6 tests for the new webhook slice (upsert / staged-overrides / delete / list ordering / reset) |
| `test/proxy/webhooks_test.gleam` | +135 | 8 query-handler tests (single, connection, count, topic filter, endpoint typename projection, uri fallback, legacyResourceId, root predicate) |

339 tests on Erlang OTP 28 + JS ESM (was 329 prior to this pass). +10 net.

### What landed

**Store slice** (`state/store.gleam`)

Three new fields each on `BaseState` and `StagedState`:

- `webhook_subscriptions: Dict(String, WebhookSubscriptionRecord)`
- `webhook_subscription_order: List(String)`
- `deleted_webhook_subscription_ids: Dict(String, Bool)`

Five accessors, mirroring the saved-search slice byte-for-byte:

- `upsert_base_webhook_subscriptions(store, records)` — base-state
  upsert that clears any deleted markers (in either base or staged)
  for the same id
- `upsert_staged_webhook_subscription(store, record)` — staged
  upsert; appends to the staged order list only if the record id
  isn't already known
- `delete_staged_webhook_subscription(store, id)` — drops the
  staged record and sets the staged deleted-marker
- `get_effective_webhook_subscription_by_id(store, id)` — staged
  wins over base; either side's deleted marker suppresses
- `list_effective_webhook_subscriptions(store)` — ordered ids first
  (deduped across base+staged), then unordered ids sorted by id

The pre-existing saved-search constructors on `BaseState`/`StagedState`
needed to switch from positional to `..base`/`..staged` spread
because the records grew new fields. No semantic change — the spread
just preserves the rest of the record.

**Query handler** (`proxy/webhooks.gleam`)

The TS `handleWebhookSubscriptionQuery` dispatches per-root-field;
the Gleam port mirrors that exactly with `root_payload_for_field`
matching against `webhookSubscription` / `webhookSubscriptions` /
`webhookSubscriptionsCount`. Each root produces:

- **Single:** `webhookSubscription(id:)` — looks the record up via
  `get_effective_webhook_subscription_by_id`, projects the supplied
  selection set; missing id or missing record both return `null`.
- **Connection:** `webhookSubscriptions(first/last/after/before, query, format, uri, topics, sortKey, reverse)` —
  list → field-arg filter → query filter → sort → paginate. Uses
  `paginate_connection_items` + `serialize_connection` from
  `graphql_helpers` (same plumbing the saved-search connection uses).
  Inline-fragment flattening on both `selectedFieldOptions` and
  `pageInfoOptions`, matching TS.
- **Count:** `webhookSubscriptionsCount(query, limit)` — no
  aggregator helper exists yet; the implementation walks the
  selection set directly and emits `count`/`precision` keys, with
  `precision` set to `AT_LEAST` when the unfiltered length exceeds
  the limit.

Projection: rather than wire projection-options through
`project_graphql_value` (which would have meant a new helper-API
parameter), the source dict is pre-populated with the
`uri`-with-fallback value, the legacy resource id, and a per-variant
endpoint sub-object that carries its `__typename`. This is how TS
`webhookProjectionOptions` injects `uri` — by the time the projector
walks the selection set, the override is already in the source dict.
Inline-fragment type conditions on `endpoint` then resolve via the
existing `defaultGraphqlTypeConditionApplies` path.

**Dispatcher wiring** (`proxy/draft_proxy.gleam`)

Three small additions:

1. New `WebhooksDomain` variant in the dispatcher's local `Domain`
   enum.
2. `Webhooks` arm in `capability_to_query_domain` (registry-driven
   path).
3. `is_webhook_subscription_query_root` arm in
   `legacy_query_domain_for` (no-registry fallback so existing tests
   without a loaded registry can still route webhook queries).

Mutation routing intentionally untouched in this pass — the mutation
arm in `mutation_domain_for` only knows `SavedSearches` for now and
falls through for everything else, which is the right behavior until
Pass 13.

### Findings

- **Projection options weren't needed.** The TS handler uses
  `webhookProjectionOptions` to swap in a fallback `uri` value at
  projection time. Pre-computing into the source dict gets us the
  same observable result for far less code. If a future endpoint
  needs more sophisticated dynamic field synthesis (e.g. a derived
  field whose value depends on the requested selection set), the
  projection helpers will need a hook — but the current bar is very
  low. **Recommendation:** keep deferring projection-options support
  until two consumers need it.
- **Sum types pay off in the projector.** `endpoint_to_source` is a
  three-line `case`; the TS equivalent is a `switch`-on-typename plus
  defensive `?? null` for each variant's optional payload. The Gleam
  variant guarantees the right fields exist on the right variants, so
  the projector emits exactly the keys GraphQL expects without runtime
  guards.
- **Store slice clones cleanly.** Adding a second resource type to
  `BaseState`/`StagedState` was mechanical — one `..spread` change in
  the existing saved-search constructors and the rest is new lines.
  This pattern will scale.
- **Dispatcher wiring is two-line per domain.** Once the handler
  exposes `process` + `is_<x>_query_root`, the dispatcher just needs
  one capability-arm and one legacy-fallback-arm. No domain-specific
  data flows back through the proxy — `Store` is threaded forward
  uniformly.

### Risks / open items

- **`limit` arg coercion.** TS does `Math.floor(rawLimit)` on a
  number; Gleam already enforces `IntVal` from JSON parsing, so the
  port doesn't need to coerce. If a test ever sends `limit: 1.5`
  through variables (FloatVal), the port treats it as no-limit. The
  TS path would coerce. Untested in either direction; flagged here for
  the Pass-13 review.
- **Sort key mismatch tolerance.** Both ports accept arbitrary
  strings and fall through to `Id`. Confirmed parity by
  `parse_sort_key("nonsense") == IdKey`.
- **Registry round-trip not exercised end-to-end.** No
  `webhookSubscriptions` registry entry is loaded in any test; the
  legacy fallback predicate is what the new tests hit. The capability
  path will start being exercised once the production registry JSON
  loads in `draft_proxy_test`. Not blocking — same pattern as
  saved-searches when it first landed.
- **Mutation handler gap.** Pass 13 needs to port
  `webhookSubscriptionCreate/Update/Delete` (~400 TS LOC) plus the
  argument validation helpers (`buildMissingRequiredArgumentError`
  etc.). The validation helpers are webhook-specific in TS but
  generic in shape — worth lifting to a shared module when porting.

### Recommendation for Pass 13

Webhook mutations. Target the same shape as saved-searches:
`process_mutation` returning a `MutationOutcome` (data + store +
identity + staged ids), three handlers (create/update/delete), and
shared input-reader / validator helpers. The TS `validateRequiredFieldArguments`
helper produces structured GraphQL errors with `extensions.code` and
`path`; the saved-search port currently emits simpler `userErrors` —
worth deciding whether to upgrade saved-searches to match or keep
parallel implementations until a consumer needs the structured form.

---

## 2026-04-29 — Pass 11: webhooks substrate (state types + URI marshaling + filter/sort)

First real consumer of Pass 10's `search_query_parser` and
`resource_ids` modules. Lands the **substrate slice** of the webhooks
domain: state types, URI ↔ endpoint marshaling, term matching, query
filtering, field-argument filtering, and sort key handling. The
GraphQL handler entry points (`handleWebhookSubscriptionQuery` /
`handleWebhookSubscriptionMutation`) and the store integration still
need to land in a follow-on pass (12) — but the pure substrate is now
testable and verifiable in isolation.

### Module table

| Module | Lines | Notes |
| --- | --- | --- |
| `state/types.gleam` | +35 | `WebhookSubscriptionEndpoint` sum type (3 variants) + `WebhookSubscriptionRecord` |
| `proxy/webhooks.gleam` | ~225 | URI marshaling, term matcher, filter+sort |
| `test/proxy/webhooks_test.gleam` | ~370 | 32 tests covering URI round-trip, filters, sorting |

323 tests on Erlang OTP 28 + JS ESM (was 291 after Pass 10). +32 net.

### What landed

State types in `state/types.gleam`:

- `WebhookSubscriptionEndpoint` is a sum type with three variants
  (`WebhookHttpEndpoint(callback_url)`, `WebhookEventBridgeEndpoint(arn)`,
  `WebhookPubSubEndpoint(pub_sub_project, pub_sub_topic)`) — one variant
  per endpoint kind. Unrepresentable combinations (e.g. an HTTP
  endpoint with an ARN) are now compile errors. The TS schema is one
  record with all four optional fields plus a `__typename`
  discriminator; the Gleam variant carries only the fields its kind
  actually uses.
- `WebhookSubscriptionRecord` ports the eleven fields directly,
  with `Option(...)` for nullable slots and `List(String)` for
  `include_fields` / `metafield_namespaces` (which default to `[]`).

`proxy/webhooks.gleam`:

- `endpoint_from_uri(uri) -> WebhookSubscriptionEndpoint` — URI
  scheme dispatch (pubsub:// / arn:aws:events: / else → HTTP).
- `uri_from_endpoint(Option(endpoint)) -> Option(String)` — round-trips
  back to a URI when the endpoint carries the necessary fields.
- `webhook_subscription_uri(record)` — explicit `uri` field wins;
  falls back to `uri_from_endpoint(record.endpoint)`.
- `webhook_subscription_legacy_id(record)` — trailing GID segment
  (`gid://shopify/WebhookSubscription/123` → `"123"`).
- `matches_webhook_term(record, term) -> Bool` — positive-term matcher
  for `apply_search_query_terms`, with case-folded field dispatch
  covering `id` (exact match against full GID *or* legacy id),
  `topic`, `format`, `uri` / `callbackurl` / `callback_url` /
  `endpoint`, `created_at` / `createdat`, `updated_at` / `updatedat`,
  and a no-field fallback that text-searches id+topic+format.
- `filter_webhook_subscriptions_by_query` — wires `matches_webhook_term`
  into `apply_search_query_terms` with `ignored_keywords: ["AND"]`.
- `filter_webhook_subscriptions_by_field_arguments(records, format, uri, topics)` —
  composable optional filters; when all three are `None` / `[]` the
  list is returned unchanged.
- `WebhookSubscriptionSortKey` enum (`CreatedAtKey | UpdatedAtKey |
  TopicKey | IdKey`) plus `parse_sort_key` (case-insensitive, unknown
  values fall through to `IdKey`) and
  `sort_webhook_subscriptions_for_connection(records, key, reverse)`
  with stable tiebreak on the GID's numeric tail via
  `compare_shopify_resource_ids`.

### Findings

- **The first real consumer validates the substrate cleanly.** Both
  `search_query_parser` (`apply_search_query_terms`) and `resource_ids`
  (`compare_shopify_resource_ids`) plug into webhooks without any
  shape changes. The generic `fn(a, SearchQueryTerm) -> Bool` matcher
  pattern is exactly what was needed — `matches_webhook_term` matches
  that signature directly.
- **The `id` field's "exact-match-against-full-GID-OR-legacy-id"
  behavior is non-obvious.** A query like `id:1` matches a record
  with id `gid://shopify/WebhookSubscription/1` because the legacy
  id ("1") matches. This is an Admin GraphQL convention worth
  documenting in the file — the test `filter_by_query_id_exact_test`
  covers it.
- **Sum types beat the TS discriminator + optional-fields pattern.**
  TS expressed the three endpoint variants as one schema with all
  fields optional, then narrowed via `__typename` checks. The Gleam
  sum type makes each variant only carry the fields its kind needs,
  collapsing several runtime guards (e.g. `endpoint.callbackUrl ?? null`
  becomes pattern matching on `WebhookHttpEndpoint(callback_url: u)`).
- **`Option(String)` semantics for sort tiebreaks need explicit
  handling.** TS's `(left.createdAt ?? '').localeCompare(...)` collapses
  null and empty into the same bucket; the Gleam port uses
  `option.unwrap("", _)` + `string.compare` to match. Important when
  records have null timestamps (e.g. defaults, in-flight creates).
- **The pure-substrate scope was the right cut.** ~225 LOC of
  webhooks logic lands in one pass with full test coverage, no
  store integration, no GraphQL handler plumbing. The full 920-LOC
  TS module would not have fit in one pass without skipping
  test depth.

### Risks / deferred work

- **Mutations not yet ported.** `webhookSubscriptionCreate`,
  `webhookSubscriptionUpdate`, `webhookSubscriptionDelete` (~400 TS
  LOC) need a follow-on pass. They depend on input validation
  helpers, the synthetic-identity FFI, and store integration that
  isn't yet wired up.
- **No store integration yet.** `Store` doesn't have
  `list_effective_webhook_subscriptions` or
  `get_effective_webhook_subscription_by_id` accessors; the Pass 12
  store extension needs to add these.
- **No dispatcher wiring yet.** `draft_proxy.gleam` doesn't route
  `webhookSubscription{,s,sCount}` queries or the three mutations
  to this module. Pass 12 will register the `Webhooks` capability
  domain in `operation_registry` and add a dispatch path in
  `draft_proxy`.

### Recommendation

Pass 12 should land the remaining webhooks pieces:
1. Add `Webhooks` to `CapabilityDomain` in `operation_registry`.
2. Extend `Store` with `list_effective_webhook_subscriptions` and
   `get_effective_webhook_subscription_by_id`.
3. Port `handleWebhookSubscriptionQuery` (`webhookSubscription`,
   `webhookSubscriptions`, `webhookSubscriptionsCount` root payloads)
   using the now-landed `paginate_connection_items` and
   `serialize_connection` helpers.
4. Port the three mutation handlers + their validation helpers.
5. Wire dispatch in `draft_proxy.gleam` to delegate
   `Webhooks` domain operations to the new module.

That's another full-pass-sized chunk; Pass 12 might split into 12a
(query handler + store) and 12b (mutations + dispatch).

---

## 2026-04-29 — Pass 10: search-query parser + resource-id ordering substrate

Lands the two domain-agnostic substrate modules every domain handler
that exposes a `query: "..."` argument depends on. The TS source
`src/search-query-parser.ts` (483 LOC) ports to ~750 LOC of Gleam, and
`src/shopify/resource-ids.ts` (16 LOC) ports to ~50 LOC. Both modules
are now consumable by future domain ports (webhooks, products, orders,
customers — every domain that takes a `query`).

### Module table

| Module | Lines | Notes |
| --- | --- | --- |
| `shopify_draft_proxy/search_query_parser.gleam` | ~750 | Tokenizer + recursive-descent parser, generic match/apply helpers |
| `shopify_draft_proxy/shopify/resource_ids.gleam` | ~50 | GID numeric ordering + nullable string compare |
| `test/search_query_parser_test.gleam` | ~520 | 52 tests across term parsing, matching, term lists, parser, generics |
| `test/shopify/resource_ids_test.gleam` | ~85 | 8 tests covering numeric/lexicographic/nullable ordering |

291 tests on Erlang OTP 28 + JS ESM (was 239 after Pass 9). +52 net.

### What landed

`search_query_parser.gleam` mirrors the entire TS public surface:

- Sum types: `SearchQueryComparator` (5 variants), `SearchQueryTerm`,
  recursive `SearchQueryNode` (TermNode | AndNode | OrNode | NotNode),
  closed-enum `SearchQueryStringMatchMode`.
- Options records with `default_*` constructor functions:
  `SearchQueryParseOptions`, `SearchQueryTermListOptions` (collapsed
  from TS's two separate types — the simpler function ignores
  `drop_empty_values`), `SearchQueryStringMatchOptions`.
- Term parsing: `parse_search_query_term`, `consume_comparator`,
  `normalize_search_query_value`, `strip_search_query_value_quotes`,
  `search_query_term_value`.
- Match helpers: `matches_search_query_string` (with prefix `*`,
  word-prefix mode, exact/includes), `matches_search_query_number`
  (using `gleam/float.parse` with int fallback),
  `matches_search_query_text`, `matches_search_query_date` (using the
  existing `iso_timestamp.parse_iso` FFI; takes explicit `now_ms: Int`
  rather than introducing a `Date.now()` FFI).
- Tokenizer + recursive descent: `tokenize`, `parse_search_query`,
  `parse_or_expression`, `parse_and_expression`, `parse_unary_expression`.
- Generics: `matches_search_query_term`, `matches_search_query_node`,
  `apply_search_query`, `apply_search_query_terms` — all parametric
  over `a` with a positive-term matcher callback `fn(a, SearchQueryTerm) -> Bool`.

`resource_ids.gleam` provides:

- `compare_shopify_resource_ids(left, right) -> Order` — extracts the
  trailing integer from a GID and compares numerically; falls back to
  lexicographic compare when either side fails to parse. Returns
  `gleam/order.Order` directly so callers can hand it to `list.sort`
  unmodified, which is cleaner than the TS signed-integer convention.
- `compare_nullable_strings(left, right) -> Order` — explicit
  `Some(_) < None` ordering.

### Findings

- **Regex elimination kept the parser pure-stdlib.** The TS uses two
  regexes: `/:(?:<=|>=|<|>|=)?$/u` and `/[^a-z0-9]+/u`. Both are
  shallow patterns that unfold cleanly into chained `string.starts_with`
  / `string.ends_with` / character iteration. Avoiding `gleam/regexp`
  keeps the dependency footprint smaller and avoids a JS/Erlang
  regex-engine difference surface.
- **The recursive-descent parser is shorter in Gleam than expected.**
  Rather than threading a mutable index, every parser function returns
  `#(Option(SearchQueryNode), List(SearchQueryToken))`. Caller passes
  the consumed-token list in, gets the remaining tokens back. Pure
  data flow, no state record, ~120 LOC for the full Pratt-style cascade
  (`or → and → unary`).
- **Generics-with-callback fell out naturally.** TS's
  `SearchQueryTermMatcher<T>` ports to a plain `fn(a, SearchQueryTerm) -> Bool`
  parameter. Same shape, same call sites, no class wrappers.
- **`iso_timestamp.parse_iso` FFI from earlier passes was a free reuse.**
  Date matching just composes existing primitives — no new FFI.
- **Term parsing's "split on first colon" is `string.split_once`
  on the head, not a custom char walk.** Cleaner than the TS regex
  `/^([^:]*):(.*)/`.
- **`SearchQueryTermListOptions` collapsed two TS types into one.**
  TS had `SearchQueryTermListOptions` and `SearchQueryTermListParseOptions`
  with different fields. The Gleam port merges them and ignores
  `drop_empty_values` from the simpler entry point. Saves callers
  from constructing two record types.
- **`gleam/order.{Lt, Eq, Gt}` is the right return type for compare
  helpers** — `list.sort` consumes it directly. The TS signed-integer
  pattern would have been a needless adapter.

### Risks

- **`matches_search_query_date` requires the caller to plumb `now_ms`
  through.** This is more correct than embedding `Date.now()` (it
  makes the matcher pure and testable), but it's a behavioral
  divergence from TS where `now` was implicit. Any future domain that
  uses date matching has to thread a clock value down.
- **`apply_search_query_terms` ignores `drop_empty_values`.** Mirrors
  the TS `parseSearchQueryTerms` behavior, but the merged-record
  shape is a little surprising — a future caller might wrongly
  expect `drop_empty_values: True` to take effect for the term-list
  entry point. The doc comment flags this; long-term, adding a
  `default_term_list_parse_options()` constructor that omits the
  field would tighten the contract.
- **The substrate is in place but no domain consumes it yet.** Until
  a domain like webhooks or products lands a `query: "..."` filter
  that calls `apply_search_query`, this module's value is latent.
  The next pass should be a real consumer.

### Recommendation

Pass 11 candidates, ranked:

1. **Webhooks domain (~920 TS LOC)** — well-bounded, single resource
   type with subscription state, exercises `apply_search_query`
   for `webhookSubscriptions(query: "...")`, plus the existing
   capability/connection/store substrate. The cleanest first
   real-domain consumer of the search parser.
2. **Products domain** — biggest blast radius, will exercise more
   of the connection/edge substrate, but the metafield/file
   substrate already landed. Probably too large for one pass.
3. **Orders domain** — depends on customer + line-item substrate
   that hasn't fully landed. Hold for later.

Pass 11 should likely be webhooks.

---

## 2026-04-29 — Pass 9: registry-driven dispatch (capability wiring)

Wires Pass 8's capabilities into `draft_proxy.gleam`'s dispatcher. With
a registry attached, query and mutation routing now go through
`capabilities.get_operation_capability` and key off the `domain` enum;
without a registry, the legacy hardcoded predicates still work — so
existing tests keep passing while new code can opt in.

### Module table

| Module | Change |
| --- | --- |
| `proxy/draft_proxy.gleam` | +`registry` field + `with_registry` setter; `query_domain_for` / `mutation_domain_for` try capability first, fall back to legacy predicates |
| `test/proxy/draft_proxy_test.gleam` | +3 tests covering capability-driven dispatch with a synthetic 3-entry registry |

231 tests on Erlang OTP 28 + JS ESM (was 228).

### What landed

- `DraftProxy.registry: List(RegistryEntry)` — defaults to `[]` so
  `proxy.new()` keeps the Pass 1–8 behavior; `proxy.with_registry(r)`
  attaches a parsed registry.
- Capability resolution is the *first* check in dispatch. When the
  registry is non-empty and `get_operation_capability` returns a
  recognised domain (`Events`, `SavedSearches`, `ShippingFulfillments`),
  routing keys off it. When the registry is empty *or* the capability
  is `Unknown`, the dispatcher falls through to
  `legacy_query_domain_for` / `legacy_mutation_domain_for` (the old
  predicate-based code).
- Three tests exercising the new path:
  - `registry_drives_query_dispatch_test` — `events` query routes via
    `Events` capability.
  - `registry_drives_mutation_dispatch_test` — `savedSearchCreate`
    mutation routes via `SavedSearches` capability.
  - `registry_unknown_root_falls_back_to_400_test` (poorly named —
    actually verifies `productSavedSearches` continues to succeed via
    legacy fallback when the synthetic registry doesn't include it).

### Findings

- **Belt-and-braces dispatch is the right migration shape.** Keeping
  the legacy fallback meant zero existing tests broke. Once every
  consumer site loads the production registry, the fallback can come
  out — but until then the cost of dual-mode dispatch is one extra
  case per resolution path. Cheap.
- **Registry-driven and predicate-driven dispatch reach the same
  result for shared roots.** `events` resolves to the same handler in
  both paths. The migration's not changing behavior, just where the
  decision lives.
- **The synthetic test registry is small (3 entries).** Tests don't
  need the full 666-entry production registry to exercise the
  capability-driven path. Keeps the test isolated and fast — and
  documents the minimum entry shape for future domain ports to
  reference.

### Risks unchanged / new

- **`product*SavedSearches` family still relies on the legacy
  predicate**, because the synthetic test registry doesn't include
  them. Production deployment with the full registry will move them
  to the capability path; the legacy fallback exists for safety.
- **`with_registry` is opt-in.** Real consumers must remember to call
  it. A future pass should add a `from_config` constructor that
  loads + parses the JSON in one shot, so attaching the registry is
  the default.

### Recommendation

Pass 10 candidates:
1. Add a JS/Erlang FFI loader so `from_config(path) -> DraftProxy`
   reads the registry and attaches it in one call. Wires the proxy
   for "real" use without leaving registry plumbing on the consumer.
2. Port the next small read-only domain. `markets`, `localization`,
   and `online-store` are all under 1k LOC in TS; any of them
   exercises the capability dispatcher with a fresh consumer.
3. Begin the customers slice — substantial, but the substrate
   (metafields, capabilities, connection helpers) is now in place.

I'd take option 1 first — it's a tiny, mechanical change that
removes the test-vs-production discrepancy in how the registry gets
loaded, and unblocks every subsequent domain pass from having to
think about loader plumbing.

---

## 2026-04-29 — Pass 8: operation-registry + capabilities

Substrate port. `src/proxy/operation-registry.ts` (67 LOC) loads the
6642-line `config/operation-registry.json` and exposes
`findOperationRegistryEntry` + `listImplementedOperationRegistryEntries`.
`src/proxy/capabilities.ts` (61 LOC) consumes it to map a parsed
operation onto a `(domain, execution, operationName)` triple — the
dispatch decision the proxy uses to decide whether to handle a query
locally, stage a mutation, or fall through to the upstream API.

This pair is foundational: every future domain handler that wants to
participate in the registry-driven router needs both modules in place.
Until now we've been hardcoding `is_saved_search_query_root`-style
predicates in `draft_proxy.gleam`; landing capabilities lets a future
pass replace those with a single registry walk.

### Module table

| Module | LOC | Status |
| --- | --- | --- |
| `proxy/operation_registry.gleam` | 220 | New: parser + lookup helpers |
| `proxy/capabilities.gleam` | 165 | New: `get_operation_capability` |
| `test/proxy/operation_registry_test.gleam` | 120 | 9 tests |
| `test/proxy/capabilities_test.gleam` | 165 | 10 tests |

228 gleeunit tests passing on Erlang OTP 28 and JS ESM (was 209). The
production registry JSON (666 entries) decodes cleanly through the
Gleam parser, verified via a one-shot Node script that imports the
compiled module.

### What landed

- `OperationType` (Query | Mutation), `CapabilityDomain` (26 explicit
  variants + Unknown), and `CapabilityExecution` (OverlayRead |
  StageLocally | Passthrough) sum types. The variants are 1:1 with the
  TS `CapabilityDomain` and `CapabilityExecution` unions; we map
  kebab-case JSON values (e.g. `"admin-platform"`) to Gleam
  PascalCase constructors via a closed `parse_domain` table.
- `RegistryEntry` record with all 8 fields (`name`, `type_`, `domain`,
  `execution`, `implemented`, `match_names`, `runtime_tests`,
  `support_notes`). `support_notes` uses
  `decode.optional_field("supportNotes", None, decode.optional(...))`
  so the field can be missing or null — both branches converge on
  `None`.
- `parse(json: String) -> Result(List(RegistryEntry), DecodeError)`.
  Decodes the full 6642-line config file in one shot. Validates closed
  enums (domain, execution, type) and rejects malformed inputs at the
  decode boundary, matching the TS `operationRegistrySchema.parse(...)`
  contract.
- `find_entry(registry, type_, names)` — first-match-wins lookup that
  walks `names` in order, skipping `None` and empty strings, returning
  the first registry entry whose type matches and whose
  `match_names` contains the candidate. Mirrors TS behavior exactly.
- `list_implemented(registry)` — filters out `implemented: false`
  entries.
- `OperationCapability { type_, operation_name, domain, execution }`
  in `capabilities.gleam`. The `get_operation_capability` function
  reproduces the TS resolution algorithm:
  1. Find first root field whose match-name resolves to an implemented
     entry of the right type.
  2. Otherwise, walk all candidates (root fields + operation name,
     deduplicated, order-preserving).
  3. If matched, prefer the operation's declared `name` over the
     matched candidate iff both resolve to the same registry entry —
     this is the `operationNameEntry` cleverness in `capabilities.ts`.
  4. Fall back to `(Unknown, Passthrough)` with `op.name ?? rootFields[0]`
     when nothing matches.

### What's deferred

- **Loader / FFI shim.** TS uses `import …json with { type: 'json' }`
  to bake the registry into the bundle. Gleam doesn't have a portable
  static-import mechanism for JSON, so the parsing API takes a string
  the consumer reads at startup. A target-specific loader (Node's `fs`
  on JS, `file` on Erlang) belongs in a separate module — not
  blocking.
- **Wiring `get_operation_capability` into the dispatcher.** Right
  now `draft_proxy.gleam` checks `is_saved_search_query_root`
  directly. The next step is to load the registry once at boot and
  replace the predicate with a capability lookup. Held to keep this
  pass focused on the substrate.
- **Caching/indexing.** TS builds a `Map<matchName, entry>` at module
  load. Gleam version walks the (~666-entry) implemented list per
  call — fine for now, easy to upgrade to a `dict.Dict` if dispatch
  shows up in profiles.

### Findings

- **`gleam/json` + `gleam/dynamic/decode` is the right shape for this.**
  The decoder reads almost identically to a Zod schema:
  ```gleam
  use name <- decode.field("name", decode.string)
  use type_ <- decode.field("type", operation_type_decoder())
  ...
  decode.success(RegistryEntry(...))
  ```
  Closed-enum decoding via `decode.then(decode.string)` + a `case`
  expression is more verbose than Zod's `z.enum([...])` but compiles
  to a tighter check (the variant enumeration is exhaustive at the
  type level, so adding a new domain in the JSON without updating
  `parse_domain` is caught by the decoder, not at runtime).
- **`decode.optional_field` semantics differ from `decode.field`.**
  `optional_field("k", default, inner)` returns `default` only when the
  key is *absent*. To also accept explicit `null`, the inner decoder
  must be `decode.optional(...)`, which itself returns `None` for
  null. The combination handles both shapes.
- **Operation-name resolution is delicate.** The `operationNameEntry`
  rule in TS — "prefer `op.name` over the matched root field iff
  both point to the same registry entry" — is easy to mis-port. The
  test `prefers_root_field_over_operation_name_test` covers this:
  with `name: "Product"` + `rootFields: ["product"]`, both resolve to
  the `product` entry, and the operation name wins.
- **No need for IO/effect modeling.** Splitting the parser
  (`parse(input: String)`) from the loader avoids cross-target IO
  entirely. The library is pure; consumers do their own string IO.
  This is the same pattern the GraphQL parser uses
  (`parser.parse(source)` is pure; the request body is read by the
  HTTP shim).
- **Real-world JSON validates.** Verified by compiling the module to
  JS, then `node -e 'parse(readFileSync(...))'` against the production
  config. All 666 entries pass; no decoder rejections. This is a
  meaningful viability signal — the JSON schema (with optional
  `supportNotes`, closed-enum domain/execution) maps cleanly to Gleam
  sum types without escape hatches.

### Risks unchanged / new

- **Adding a new domain requires updating Gleam code.** Closed enums
  catch typos at decode time, but every new domain in the JSON now
  needs a Gleam variant. The TS port has the same constraint — both
  the union type and the JSON schema enum need updating — but in
  Gleam the cost is also a `parse_domain` case branch. Acceptable;
  the alternative (string-typed domain) loses exhaustiveness on the
  consumer side.
- **Memory cost of carrying the full registry.** 666 entries × ~8
  small fields each is negligible (probably <100KB on each runtime).
  No risk; flagged only because we'd previously raised it as a
  concern.

### Recommendation

Pass 9 should wire the capability lookup into `draft_proxy.gleam`'s
dispatch. Currently `route_query` / `route_mutation` check
`saved_searches.is_saved_search_query_root` directly. Replacing that
with a capability lookup gives us the registry-driven dispatch the TS
proxy uses, and it's a small change — load the registry once at
boot, thread it through `dispatch_graphql`, and replace the predicate
with `case capability.domain { SavedSearches -> ... ; _ -> ... }`.

This unblocks adding new domains: each domain just registers its
handlers; the dispatcher routes by capability without further
modifications.

After that, picking up another small read-only domain (events is
already half-done; `delivery-settings`, `markets`, `localization` are
next-smallest) becomes a copy-and-adapt exercise rather than a
plumbing exercise.

---

## 2026-04-29 — Pass 7: metafields read-path substrate

Substrate port. `src/proxy/metafields.ts` is imported by 7 different
domain modules (`admin-platform`, `customers`, `metafield-definitions`,
`products`, `online-store`, `payments`, `store-properties`). Porting
the read-path subset now means future domain ports — products,
customers, and the smaller stores below them — get a working
projection helper for free.

The mutation paths (`upsertOwnerMetafields`, `normalizeOwnerMetafield`,
`mergeMetafieldRecords`, `readMetafieldInputObjects`) were
deliberately deferred because they depend on
`src/proxy/products/metafield-values.ts` (360 LOC of value
normalization + JSON shape coercion) which is its own port.

### Module table

| Module | LOC | Status |
| --- | --- | --- |
| `proxy/metafields.gleam` | 188 | New: `MetafieldRecordCore`, compare-digest builder, projection + connection helpers |
| `test/proxy/metafields_test.gleam` | 130 | 11 unit tests |

209 gleeunit tests passing on Erlang OTP 28 and JS ESM (was 198).

### What landed

- `MetafieldRecordCore` record with the same 10 fields the TS type
  declares. Optional fields (`type_`, `value`, `compare_digest`,
  `json_value`, `created_at`, `updated_at`, `owner_type`) are
  `Option(...)` so callers can pass through whatever shape the
  upstream record holds.
- `make_metafield_compare_digest` — `draft:` prefix + base64url of a
  6-element JSON array `[namespace, key, type, value, jsonValue,
  updatedAt]`. Mirrors `Buffer.toString('base64url')` semantics
  (no padding) using `bit_array.base64_url_encode(_, False)`.
- `serialize_metafield_selection_set` — projects a metafield record
  onto a list of selection nodes. All 12 fields the TS handler
  recognizes (`__typename`, `id`, `namespace`, `key`, `type`,
  `value`, `compareDigest`, `jsonValue`, `createdAt`, `updatedAt`,
  `ownerType`, `definition`) plus the `null` default.
- `serialize_metafield_selection` — convenience wrapper around the
  selection-set projector.
- `serialize_metafields_connection` — connection-shaped serialization
  with cursor = `id` and pagination via the existing
  `paginate_connection_items`. Variables are threaded through, so
  paginated reads via `$first` / `$after` work end-to-end (already
  exercised in Pass 6 for saved searches).

### What's deferred

- **Mutation path** (`upsertOwnerMetafields`, `normalizeOwnerMetafield`,
  `mergeMetafieldRecords`, `readMetafieldInputObjects`): blocked on
  `metafield-values.ts` (360 LOC: `parseMetafieldJsonValue`,
  `normalizeMetafieldValue`, type-shape coercion table). Can land
  before any consumer domain's mutation pass needs it.
- **Owner-scoped wrapping** (`OwnerScopedMetafieldRecord<OwnerKey>` in
  TS): the TS type adds an owner ID under a string-keyed property
  (e.g. `productId: "..."`). In Gleam we'll likely model this as the
  consumer wrapping `MetafieldRecordCore` in a record that adds the
  owner field, rather than parametric polymorphism over key names.
- **Definition lookup** (`'definition'` case): TS returns null too,
  but only because the read-path doesn't have access to definitions.
  Eventually `metafield-definitions.gleam` will own this and the
  serializer here will route to it.

### Findings

- **Read-path projection translates very cleanly.** ~100 LOC TS →
  ~150 LOC Gleam. The biggest verbosity tax was on `Option(String)`
  unwrapping for `null` cases in the JSON output — TS's `?? null`
  collapses to a tiny ternary, Gleam's pattern match needs an
  explicit `Some(s) -> json.string(s)` / `None -> json.null()`.
  Net cost: one extra helper (`option_string_to_json`) used 6 times.
- **`bit_array.base64_url_encode` matches `Buffer.toString('base64url')`
  exactly.** Including the no-padding behavior. No FFI needed; the
  digest survives JSON round-trip identically on both targets.
- **`json.array` requires a transformer fn even when the items are
  already `Json`.** Slight ergonomic friction (`fn(x) { x }`) but
  type-safe — the API is consistent with `list.map`-style helpers.
- **Test setup is tedious for `Selection` values.** The cleanest way
  to construct a real `Selection` for the projection test is to
  parse a query string and pull the root field. We don't have an
  AST builder/literal syntax. Acceptable — every test is one line of
  `first_root_field("{ root { ... } }")` plumbing.
- **The connection helper is genuinely reusable.** `paginate_connection_items`
  + `serialize_connection` did not need any modification to support
  the new metafields shape. This is the same helper saved-searches
  uses, and it slotted in for metafields with no friction. Strong
  evidence that the substrate's connection abstraction is correctly
  factored.

### Risks unchanged / new

- **Field-projection inconsistency between domains.** Saved-searches
  uses an explicit per-field `case` in `project_saved_search`;
  metafields uses the same pattern. As more domains land, the
  per-field projection table will grow large. Worth considering a
  helper that takes a `dict.Dict(String, fn(record) -> Json)` and
  walks selections — but only if the duplication starts hurting.
- **`compareDigest` alignment with TS is unverified.** The Gleam
  output uses the same algorithm but I haven't compared a digest
  side-by-side with TS. Adding a parity test against a known TS
  output would close this; deferred until consumers actually rely on
  the digest.
- **`Option(Json)` for `json_value` is awkward.** `gleam/json` doesn't
  expose a `Json` value that round-trips through dynamic data — once
  you've built a `Json`, you can serialize it to a string but you
  can't introspect it. Carrying it as `Option(Json)` works for our
  read-only path, but the mutation port will need a different shape
  (probably `Option(JsonValue)` defined as an enum mirroring
  `gleam_json`'s constructors).

### Recommendation

Pass 8 should validate the metafields helper from a real consumer
context. The cheapest validation: extend `saved_searches` with a
synthetic `metafields(...)` connection (saved searches don't
actually expose them in TS — pure validation harness), or pick the
smallest real consumer and port a slice. Given saved_searches is
already comfortable territory, picking up `metafield-definitions`
(1550 LOC) or a thin slice of `customers` is the next signal-rich
move.

Alternatively, the `operation-registry` + `capabilities` pair
(67 + 61 LOC plus the 6642-line config JSON) would unblock
capability-based dispatch — necessary for any domain whose
`handleQuery`/`handleMutation` methods key off the registry. But
loading 310 KB of JSON cleanly across both targets requires either
codegen or a config-injection pattern; not blocking, but worth
factoring deliberately.

I'd pick a slice of `customers` next (~50-80 LOC of real handler
code, exercising `MetafieldRecordCore` + projection in context).

---

## 2026-04-29 — Pass 6: GraphQL variables threading

Pure-substrate widening between two domain ports. The dispatcher used
to assume every operation was self-contained (inline arguments only);
this pass widens the request body parser to accept
`{ query, variables? }` and threads the resulting
`Dict(String, root_field.ResolvedValue)` from the dispatcher down
through `route_query` / `route_mutation` into every saved-searches
handler. The arg resolver and AST already supported variables — only
the request-body parser, the dispatcher plumbing, and the call sites
into `root_field.get_field_arguments` were missing.

### Module table

| Module | LOC delta | Status |
| --- | --- | --- |
| `proxy/draft_proxy.gleam` | +25 | Variables decoder + threading |
| `proxy/saved_searches.gleam` | +14 | Variables on every public + private handler |
| `test/proxy/saved_searches_test.gleam` | +3 | Updated 3 call sites with `dict.new()` |
| `test/proxy/draft_proxy_test.gleam` | +37 | 3 new tests covering create-with-vars, query-with-vars, omitted-vars |

198 gleeunit tests passing on Erlang OTP 28 and JS ESM.

### What landed

- A recursive `decode.Decoder(root_field.ResolvedValue)` that
  enumerates every JSON-shaped value (bool / int / float / string /
  list / object) with a `decode.success(NullVal)` fallback. Uses
  `decode.recursive` to defer construction so the inner closure can
  refer to itself, and `decode.one_of` to try each shape in order.
  Order is bool → int → float → string → list → dict → null because
  on Erlang `false` is `0` for some primitive checks; bool-first
  makes the union unambiguous.
- `parse_request_body` extended via `decode.optional_field` so a body
  without `variables` defaults to `dict.new()`. Existing tests
  (which all omit `variables`) keep passing untouched.
- `dispatch_graphql` carries the new `body.variables` into both
  branches; `route_query` and `route_mutation` grow a
  `variables: Dict(String, root_field.ResolvedValue)` parameter.
- `saved_searches.process` / `process_mutation` /
  `handle_saved_search_query` / `serialize_root_fields` /
  `serialize_saved_search_connection` / `list_saved_searches` /
  `handle_mutation_fields` / `handle_create` / `handle_update` /
  `handle_delete` all thread variables; the four call sites that
  previously passed `dict.new()` now pass the actual map.

### What's deferred

- **Multi-pass arg resolution.** TS resolves arguments once at the
  dispatcher and re-uses the dict; this port still calls
  `get_field_arguments` per handler. Functionally equivalent, just
  redundant work. Worth inlining when we land another mutation
  domain that re-walks the same field.
- **Operation name selection.** A document with multiple operations
  needs `operationName` to choose; `parse_operation` currently picks
  the first. Not yet a problem for proxy traffic (the recorded
  parity requests all have one operation each), but it'll need to be
  threaded the same way variables now are.

### Findings

- **`decode.recursive` works exactly the way you'd want.** No
  trampolining or thunking required at call sites — the inner
  closure is invoked lazily. This was the part I was most worried
  about; it took ~10 lines.
- **`decode.one_of` is the right primitive for sum-type-shaped JSON.**
  The error semantics (return the first matching decoder, otherwise
  bubble up the very first failure) compose cleanly with
  `decode.success` as a default branch.
- **The dispatcher signature is starting to feel heavy.** Both
  `route_query` and `route_mutation` now take 5+ parameters; the
  saved-searches mutation handlers take 7. The pattern works, but
  another pass that adds a parameter (e.g. `operationName`,
  request id, fragments cache) probably warrants a `Dispatch`
  context record. Not blocking; a code-shape signal.
- **Existing tests caught zero regressions.** The 195 previously-
  passing tests all continued to pass after threading without any
  test edits beyond updating the 3 direct call sites in
  `saved_searches_test.gleam`. The substrate factoring is healthy.
- **Test coverage for the new path is shallow.** I added three new
  tests (variables-driven create, variables-driven query with
  pagination + reverse, omitted-variables fallback) but every other
  saved-searches test still exercises only the inline-args path.
  Consider widening at least one read-path test per query field if
  variables become the dominant client pattern.

### Risks unchanged / new

- **No coercion of variable types.** GraphQL spec says a variable
  declared `Int!` should reject a JSON `"1"`; we accept whatever the
  JSON object literally holds. This matches the TS proxy (which
  also relies on `JSON.parse` types), but if a Shopify client ships
  a variant that depends on coercion the proxy will diverge silently.
- **Default values from variable definitions are not honored.** If a
  query declares `query Q($limit: Int = 10)` and the request omits
  `limit`, the AST default is ignored — the variable resolves to
  `NullVal` and the handler falls back to its own default. Matches
  `resolveValueNode`'s `?? null` semantics so we're spec-aligned with
  TS, but worth documenting if a real divergence shows up.
- **`decode.optional_field` only handles missing keys, not explicit
  null.** A body with `"variables": null` will fail decoding instead
  of defaulting to empty. None of the parity-recorded requests do
  this; flagging in case a real client does.

### Recommendation

Pass 7 should be the next domain port — pick a small, read-only
substrate consumer to keep momentum. The two cheapest options:

1. **`shopAlerts` / `pendingShopAlerts`** — single-field read, no
   pagination, no store coupling. Probably ~80 LOC including tests.
2. **`metafieldDefinitions` connection** — exercises the connection
   helpers in a different shape (not saved-search defaults, real
   schema-driven records) and pressure-tests the variables path
   under a non-trivial argument set (`namespace`, `key`, `ownerType`).

Either is a self-contained domain port with no new substrate work.
After that, the long pole is `customers` — both because customer
records are 50+ fields and because `customerCreate` / `customerUpdate`
exercise the full mutation envelope (including userErrors with
nested input paths).

---

## 2026-04-29 — Pass 5: savedSearchUpdate + savedSearchDelete

Closed the saved-search write-path domain. With create from Pass 4
already in place, this pass added `savedSearchUpdate` and
`savedSearchDelete`, exercising the full pattern: input-id resolution
against staged records, validation that drops invalid keys instead of
rejecting the whole input, and identity-tagged log entries on both
success and failure. Saved searches is now the first fully-ported
write-capable domain in Gleam. 195 gleeunit tests pass on both
`--target erlang` and `--target javascript` (6 new mutation
integration tests).

### What is additionally ported and working

| Module                              | LOC   | TS counterpart                              |
| ----------------------------------- | ----- | ------------------------------------------- |
| `proxy/saved_searches` (extended)   | ~1110 | `proxy/saved-searches` (CRUD, ~75%)         |
| `test/.../draft_proxy_test`         | ~585  | parity tests (CRUD coverage)                |

Update flow: read input, resolve `input.id` via
`store.get_effective_saved_search_by_id` (staged-wins-over-base);
validate without `requireResourceType` (since the existing record
already carries a resource type); on validation errors strip the
offending `name` / `query` keys via `sanitized_update_input` and
re-merge the survivors with the existing record; payload either
echoes the freshly-merged record or, when sanitization rejected
everything, the existing record unchanged. Delete flow: same id
resolution, then `store.delete_staged_saved_search` if found,
projecting `deletedSavedSearchId` as the input id on success or null
on validation failure.

`make_saved_search` was generalised to accept
`existing: Option(SavedSearchRecord)`, threading the existing record's
`id` / `legacyResourceId` / `cursor` / `resourceType` through
unchanged when present, and falling back to the input or fresh
synthetic gid when absent. `build_create_log_entry` was renamed to
`build_log_entry` and parametrised on root-field name so create,
update, and delete share one log-entry constructor that produces the
right `rootFields` / `primaryRootField` / `capability.operationName`
/ `notes` for each.

The dispatcher in `handle_mutation_fields` now dispatches all three
saved-search root fields (`savedSearchCreate`,
`savedSearchUpdate`, `savedSearchDelete`); the `MutationOutcome`
record was already shaped to thread store + identity + staged ids
back to the dispatcher, so adding two more handlers was a 3-line
match-arm change plus the handlers themselves.

### What is deliberately deferred

- **GraphQL variables threading.** Mutation inputs are still inline
  literals — `parse_request_body` only extracts `query`. The next
  domain that needs variable inputs (or an `ID!` argument referenced
  from a JSON variable) will want this widened first.
- **The full search-query parser.** Updates that override `query`
  still ship `searchTerms` = raw query and `filters: []`; structured
  filter behaviour lands when the parser ports.
- **`hydrateSavedSearchesFromUpstreamResponse`.** Live-hybrid only.

### Findings

- **The CRUD pattern lands cleanly under the existing substrate.**
  Once create existed, update + delete were ~150 LOC of handler each
  with no new helpers — input id resolution is just
  `store.get_effective_saved_search_by_id`, sanitized input is a
  `dict.delete` fold over the validation errors, and the
  `MutationOutcome` record absorbed the new staged/failed mix without
  new fields.
- **`Option(SavedSearchRecord)` + `case existing { Some(...) -> ...
  None -> ... }` reads better than the TS `??` fallback chain.**
  Each field of the merged record has its own explicit fallback
  expression instead of a chained `?? existing?.field ?? ''`. The
  handful of extra lines is worth the readability.
- **Sharing `project_create_payload` between create and update was
  natural** — both project `{ savedSearch, userErrors }` and the
  variant differs only in whether `record_opt` falls back to
  `existing` (update) or `null` (create). Re-using the same projector
  with an `Option`-typed argument means the GraphQL projection
  pipeline (selection sets, fragments, `__typename`) only lives in
  one place.
- **Static defaults are not in the staged store, so they cannot be
  deleted.** A delete against a static-default id surfaces the same
  "Saved Search does not exist" user error as a delete against an
  unknown id. This matches the TS handler's behaviour: deletes only
  affect records that have been staged or hydrated into base state.
  Captured as a deliberate test case so future regressions are
  caught.

### Risks unchanged / new

- **The synthetic-id counter advances per mutation regardless of
  outcome.** A failed create still mints a `MutationLogEntry` gid;
  a failed delete also mints one. This is fine but worth keeping in
  mind when tests assert specific id values across multiple mutations
  in one proxy lifetime.
- **GraphQL variables remain absent.** The next mutation domain that
  takes anything beyond a primitive id+name+query input will need
  variables threading first; deferring it cost ~5 LOC of test
  ergonomics here (escaped-quote string literals) and won't scale.
- **`state/store.ts` still has ~5450 LOC unported.** Each subsequent
  domain pass eats into this; the saved-search slice is now load-
  bearing under a CRUD workload, which validates the dict-of-records
  + parallel order-list pattern for other domains.

### Recommendation

The next pass should be GraphQL variables threading. Cheap (~50 LOC
of substrate widening), unblocks every meaningful mutation domain
beyond saved searches, and stays in pure substrate territory before
the next domain port. Concretely: extend `parse_request_body` to
accept an optional `variables` object (decoded as
`Dict(String, Json)` then converted to
`Dict(String, root_field.ResolvedValue)`), thread the dict through
`dispatch_graphql` → `route_query` / `route_mutation` → handler →
`root_field.get_field_arguments`. The decoder + arg-resolver already
support variables; only the request-body parser and dispatcher
plumbing are missing.

After variables: pick a write-capable domain that touches enough of
the store to force a second store slice. `customers` is a good
candidate (write surface includes `customerCreate`, `customerUpdate`,
`customerDelete`, with rich nested input shapes that need variables
+ store coverage; the read path also pages, so the pagination
substrate gets re-exercised).

---

## 2026-04-29 — Pass 4: store slice + savedSearchCreate mutation

Picked up the long pole identified at the end of Pass 3: ported the
saved-search slice of `state/store.ts` plus the mutation log, threaded
a `Store` through `DraftProxy`, wired the saved-search read path to
the store, and ported `savedSearchCreate` end-to-end. The first
write-path domain is now alive in Gleam — staged records flow through
mutations, the meta routes (`/__meta/log`, `/__meta/state`,
`/__meta/reset`) reflect real state, and a subsequent
`orderSavedSearches(query: ...)` query surfaces the freshly-staged
record. 189 gleeunit tests pass on both `--target erlang` and
`--target javascript`.

### What is additionally ported and working

| Module                              | LOC  | TS counterpart                              |
| ----------------------------------- | ---- | ------------------------------------------- |
| `state/types`                       | ~35  | `state/types` (saved-search slice)          |
| `state/store`                       | ~350 | `state/store` (saved-search slice + log)    |
| `proxy/saved_searches` (extended)   | ~860 | `proxy/saved-searches` (read + create, ~60%)|
| `proxy/draft_proxy` (extended)      | ~590 | dispatcher: store-threaded, mutation route  |

`state/store` ports the saved-search slice of `BaseState` /
`StagedState` (the maps, the order arrays, and the
`deleted_saved_search_ids` markers), plus the mutation log:
`OperationType`, `EntryStatus`, `Capability`, `InterpretedMetadata`,
`MutationLogEntry`. Operations: `new`, `reset`,
`upsert_base_saved_searches`, `upsert_staged_saved_search`,
`delete_staged_saved_search`, `get_effective_saved_search_by_id`
(staged-wins-over-base, deleted-marker-suppresses),
`list_effective_saved_searches` (ordered ids first, then unordered
sorted by id), `record_mutation_log_entry`, `get_log`. The Gleam port
returns updated `Store` records from every mutator instead of
mutating in place.

`proxy/saved_searches` extends with `savedSearchCreate`:
`MutationOutcome` record threading `data` + `store` + `identity` +
`staged_resource_ids`; `is_saved_search_mutation_root` predicate;
`process_mutation` dispatcher; full validation pipeline (input
required; name non-blank, ≤40 chars; query non-blank; resource type
required, supported, and `CUSTOMER` deprecated); proxy-synthetic
gid + log entry minted via the synthetic-identity registry; record
upserted as staged; log entry recorded with status `Staged` on
success or `Failed` on validation errors.

`proxy/draft_proxy` now owns a `Store` field, threads it through
every dispatch, threads `MetaReset` through both
`synthetic_identity.reset` and `store.reset`, and routes mutations
via a new `route_mutation` arm that consumes the saved-search
`MutationOutcome` to update both the store and the synthetic-identity
registry. The `/__meta/log` and `/__meta/state` responses now
serialize real store data — a regression sentinel against the
empty-state placeholders Pass 2 shipped.

### What is deliberately deferred

- **`savedSearchUpdate` and `savedSearchDelete`.** Both follow the
  same shape as create but need synthetic-gid → input-id resolution
  against staged records. Bundled as a single follow-up pass.
- **The full search-query parser** (`src/search-query-parser.ts`,
  ~480 LOC). Newly-created records ship `searchTerms` = raw query
  string and `filters: []`; this matches the TS handler's output for
  records the parser hasn't run against yet, so the round-trip is
  faithful. Still load-bearing for the next read-path domain that
  actually needs structured filters.
- **GraphQL variables threading.** The dispatcher's
  `parse_request_body` only extracts `query`, not `variables`. The
  saved-search mutation tests therefore use inline arguments. A
  separate pass will widen `parse_request_body` and thread variables
  into `root_field.get_field_arguments`.
- **`hydrateSavedSearchesFromUpstreamResponse`.** Live-hybrid only,
  needs upstream response shapes; the rest of the live-hybrid plumbing
  is still ahead of the read mode.

### Findings

- **Threading immutable `Store` through the dispatcher with
  record-update syntax (`Store(..s, base_state: new_base, …)`) is the
  right ergonomics.** Each store mutator returns a fresh `Store`; the
  call sites read like the TS class but with explicit threading.
  `MutationOutcome` carries store + identity + staged ids back from
  each handler so the dispatcher does not have to reach into multiple
  return values.
- **`MutationOutcome` record beats tuples for cross-domain
  contracts.** When the dispatcher needs to thread three pieces of
  state back from a handler (next store, next identity, staged ids)
  on top of a `Json` data envelope, a named record reads cleanly and
  scales — when other domains add their own mutation handlers they
  can return the same record without growing the dispatcher's match
  arms.
- **Module/parameter name shadowing was the only real surprise.** A
  function parameter named `store: Store` and a module imported as
  `shopify_draft_proxy/state/store` collide on field-access syntax —
  `store.list_effective_saved_searches(store)` parses as field access
  on the value. Resolved by importing the function directly:
  `import shopify_draft_proxy/state/store.{type Store,
  list_effective_saved_searches}`. Worth keeping in mind for every
  module whose name overlaps with the natural parameter name.
- **Extracting `state/types.gleam` for `SavedSearchRecord` /
  `SavedSearchFilter` was necessary** to break a cycle between
  `state/store` and `proxy/saved_searches`. The TS layout puts these
  in `state/types.ts` for the same reason; the Gleam version follows
  suit.
- **Synthetic identity threading exposes counter-coupling between
  identity-using functions.** Every gid mint advances the
  `next_synthetic_id` counter, so mutations that mint *both* a
  resource gid *and* a log-entry gid produce predictable id pairs
  (`SavedSearch/1`, `MutationLogEntry/2`). Tests can lean on this
  determinism, but any reordering of mints inside a handler will
  shift downstream ids. The TS version has the same property; the
  Gleam port preserves it.

### Risks unchanged / new

- **`state/store.ts` is 5800 LOC**, of which ~350 LOC ported here
  cover the saved-search slice. The next ~5450 LOC will land
  slice-by-slice as their domains port. The pattern (Dict for
  records, parallel order list, deleted-id marker) is now proven and
  re-usable.
- **The search-query parser is still a self-contained 480-LOC
  port** that several domains will want. Now load-bearing on
  saved-search update/delete reaching full parity (input id
  resolution against staged records is itself fine, but tests will
  want structured `filters` to assert on).
- **The dispatcher does not yet thread GraphQL variables.** The next
  mutation domain that takes non-trivial input shapes (anything with
  a list, or any `ID` argument referencing prior staged state) will
  want variables threading first. Cheap to do — `parse_request_body`
  becomes a 4-line widening — but worth doing as its own pass so the
  domain handlers can assume variables are present.

### Recommendation

The store substrate is now proven. Three credible next passes:

1. **Saved-search update + delete.** Closes the saved-search domain.
   Forces synthetic-gid → input-id resolution against staged records,
   which every other write-path domain will need. ~150 LOC of handler
   plus tests, no new substrate.
2. **GraphQL variables threading.** ~50 LOC to widen
   `parse_request_body` and `root_field.get_field_arguments`. Strict
   prerequisite for any non-trivial mutation handler. Pure substrate.
3. **`search-query-parser.ts` port.** ~480 LOC of stand-alone
   parser. Unblocks structured filter behaviour across saved searches,
   products, orders. No state coupling.

Pick (1) for a finished domain milestone — saved searches becomes the
first fully-ported write-capable domain, demonstrating the full
write-path pattern (validate → mint identity → upsert staged → log).
Pick (2) if the next domain after saved searches needs variables.
Pick (3) if widening read-surface speed is the priority.

---

## 2026-04-29 — Pass 3: pagination machinery + saved_searches read path

Forced the connection-pagination port by picking `saved_searches` as
the next domain. The TS handler is 643 LOC; this pass ports the
read path against static defaults only — store-backed CRUD and the
search-query parser are deferred. 171 gleeunit tests pass on both
`--target erlang` and `--target javascript`.

### What is additionally ported and working

| Module                              | LOC  | TS counterpart                              |
| ----------------------------------- | ---- | ------------------------------------------- |
| `proxy/graphql_helpers` (extended)  | ~700 | `proxy/graphql-helpers` (~70%)              |
| `proxy/saved_searches`              | ~310 | `proxy/saved-searches` (read path, ~30%)    |
| `proxy/draft_proxy` (extended)      | ~360 | dispatcher branch added                     |

`proxy/graphql_helpers` now has the full pagination pipeline:
`paginate_connection_items`, `serialize_connection`,
`serialize_connection_page_info`, `build_synthetic_cursor`, plus the
supporting `ConnectionWindow(a)`, `ConnectionWindowOptions`,
`ConnectionPageInfoOptions`, and `SerializeConnectionConfig(a)`
records. `proxy/saved_searches` ports the static `ORDER` and
`DRAFT_ORDER` defaults (4 and 5 entries respectively), the
`matchesQuery` substring filter, the `reverse` argument, and the
9-way root-field → resource-type mapping.

### What is deliberately deferred

- **The store-backed list/upsert/delete flow.** The Gleam store
  is not yet ported, so user-staged saved searches don't surface and
  mutations return a 400. Lifted only when the store lands.
- **The full search-query parser** (`src/search-query-parser.ts`,
  ~480 LOC). Stored `query` strings are not split into structured
  `searchTerms` / `filters` here; static defaults already carry the
  shape they need (empty `searchTerms` and `filters` on the
  port-shipping records). When the parser ports, hydration of
  upstream payloads becomes possible.
- **`hydrateSavedSearchesFromUpstreamResponse`.** Live-hybrid only,
  needs the store and the parser.

### Findings

- **Generic `serialize_connection<T>` translated cleanly via a
  configuration record.** The TS function takes a wide options object
  with several callbacks; in Gleam a `SerializeConnectionConfig(a)`
  record with named fields reads better than a positional argument
  list and avoids the explosion the spike worried about. Pattern
  match on selection name (`nodes` / `edges` / `pageInfo`) inside the
  helper, dispatch to caller-supplied `serialize_node` for projection.
- **`ConnectionPageInfoOptions` defaults via record-update syntax
  (`ConnectionPageInfoOptions(..default(), include_cursors: False)`)
  is the right ergonomic for connection options.** It lets per-call
  overrides stay obvious and lets the defaults move centrally.
- **Threading `ResolvedValue` from `root_field` into pagination
  was the right call** rather than reinventing JSON-ish source values
  for argument reading. `paginate_connection_items` accepts
  `Dict(String, ResolvedValue)` (matching the TS variables shape) and
  re-uses `root_field.get_field_arguments` to pull `first/last/after/
  before/query/reverse` out of the field. No duplicate decoder.
- **Adding a domain stays a 5-minute, two-file change** even now that
  the dispatcher has a connection-shaped domain in it. The
  `domain_for` lookup composes cleanly with
  `saved_searches.is_saved_search_query_root` (the TS predicate
  ports verbatim).
- **`project_graphql_object` carried the saved-search node shape
  without modification.** Passing the record through `src_object` →
  `project_graphql_value` produced byte-identical JSON to the TS
  output (verified against the integration-test expectations) for
  `__typename`, `legacyResourceId`, nested `filters { key value }`,
  aliases, fragment spreads, and inline fragments.

### Risks unchanged / new

- **Store remains the long pole** and is now blocking saved-search
  *mutations* and *staged reads*. The next bottleneck-driven domain
  port should be one whose read path also exercises the store, so we
  can stop kicking the can on `state/store.ts`.
- **The search-query parser is a self-contained 480-LOC port** that
  several domains will want (saved searches, products, orders). It's
  worth doing as a stand-alone pass before the third domain that
  needs it — the alternative is building the same scaffolding three
  times.

### Recommendation

The substrate now covers: routing, parsing, projection, pagination,
connection serialisation, fragment inlining, and synthetic identity.
That is enough to port any *read-only* domain with non-trivial
defaults. The next pass should either (a) port `state/store.ts`
slice-by-slice, starting with the saved-search slice so this domain
can reach full parity, or (b) port `search-query-parser.ts` so the
read paths that depend on it (products, orders) can land
search-filter behaviour without the store landing first. Pick (a) if
you want a finished domain; pick (b) if you want to widen the read
surface fastest.

---

## 2026-04-29 — Pass 2: meta routes, projection helper, second domain

Extended the spike with the rest of the meta routes, the projector
that almost every domain handler depends on, and a second
read-only domain to validate the dispatcher extension pattern.

### What is additionally ported and working

| Module                            | LOC  | TS counterpart                  |
| --------------------------------- | ---- | ------------------------------- |
| `proxy/graphql_helpers` (extended)| ~340 | `proxy/graphql-helpers` (~40%)  |
| `proxy/draft_proxy` (extended)    | ~340 | `proxy-instance` + `proxy/routes` (meta + dispatcher) |
| `proxy/delivery_settings`         | ~90  | `proxy/delivery-settings`       |

`proxy/graphql_helpers` now has `project_graphql_object`,
`project_graphql_value`, and `get_document_fragments` — the recursive
selection-set projector that almost every domain handler is built
on. `proxy/draft_proxy` now routes `/__meta/health`, `/__meta/config`,
`/__meta/log`, `/__meta/state`, `/__meta/reset`, plus a clean two-line
extension point per new domain (`Domain` sum type +
`domain_for(name)` lookup). 133 gleeunit tests pass on both
`--target erlang` and `--target javascript`.

### Findings reinforced

- **The projection helper port was straightforward.** Inline-fragment
  type-condition gating, fragment-spread inlining, list element-wise
  projection, `nodes`-from-`edges` synthesis, and aliases all
  translated without surprises. The `SourceValue` sum type
  (`SrcNull | SrcString | SrcBool | SrcInt | SrcFloat | SrcList |
  SrcObject`) is the Gleam analogue of TypeScript's
  `Record<string, unknown>` and reads cleanly in handler code.
- **Adding a new domain is now a 5-minute, two-file change.** Port
  the TS handler to Gleam (typically a thin wrapper around
  `project_graphql_object` over a default record), add a `Domain`
  variant in `draft_proxy.gleam`, extend `domain_for`. The
  `delivery_settings` handler took longer to write tests for than to
  port. This is exactly the property the rest of the port needs.
- **The dispatcher's `respond` helper unifies error paths cleanly.**
  Each domain returns `Result(Json, _)` from its `process` function
  and the dispatcher wraps it in either a 200 or a 400 with a
  uniform error envelope. Adding more domains does not multiply
  error-handling code.

### Findings unchanged

The store + types remains the long pole. Pagination machinery
(`paginateConnectionItems`, `serializeConnection` with cursors) is
the next non-trivial helper that will need a real port — `events`
dodged it via the empty-connection specialisation, and
`delivery_settings` doesn't paginate at all. `saved_searches` is the
natural next step to force the pagination port.

---

## 2026-04-28 — Pass 1: end-to-end viability spike

A first viability spike has run end-to-end through Gleam: HTTP-shaped
request → JSON body parse → custom GraphQL parser → operation summary
→ events-domain dispatcher → empty-connection serializer → JSON
response. 98 gleeunit tests pass on both `--target erlang` and
`--target javascript`. The port is concrete enough now to surface real
strengths and risks rather than speculate.

### What is ported and working

| Module                            | LOC  | TS counterpart                  |
| --------------------------------- | ---- | ------------------------------- |
| `graphql/source` + `location`     | ~80  | `language/source`, `location`   |
| `graphql/token_kind` + `token`    | ~70  | `language/tokenKind`, `tokenKind`|
| `graphql/character_classes`       | ~60  | `language/characterClasses`     |
| `graphql/lexer`                   | ~530 | `language/lexer`                |
| `graphql/ast`                     | ~140 | `language/ast` (executable subset) |
| `graphql/parser`                  | ~720 | `language/parser`               |
| `graphql/parse_operation`         | ~100 | `graphql/parse-operation`       |
| `graphql/root_field`              | ~200 | `graphql/root-field`            |
| `state/synthetic_identity` + FFI  | ~180 | `state/synthetic-identity`      |
| `proxy/graphql_helpers` (slice)   | ~110 | `proxy/graphql-helpers` (15%)   |
| `proxy/events`                    | ~80  | `proxy/events`                  |
| `proxy/draft_proxy` (skeleton)    | ~190 | `proxy-instance` + `proxy/routes` (skeleton) |

Roughly **2.5K LOC of Gleam** replacing roughly the same TS surface,
with FFI proven on both targets via the ISO timestamp helpers.

### Strengths

- **Sum types + exhaustive matching catch GraphQL shape bugs at
  compile time.** Adding a new `Selection` variant (e.g.
  `InlineFragment`) makes every consumer fail to compile until it
  decides what to do — exactly the property the proxy needs to keep
  null-vs-absent handling honest.
- **`Result`-threaded parsing replaces graphql-js's mutable lexer
  cleanly.** The recursive descent reads as well as the TS original;
  the immutable state threading didn't add meaningful boilerplate
  beyond `use … <- result.try(…)`.
- **Cross-target parity is real.** Every test passes on both BEAM and
  JS, including FFI-bound timestamp formatting. The platform-specific
  cost was small (one `.erl` + one `.mjs` file, ~10 lines each).
- **Public API translates 1:1.** `process_request(request) ->
  (response, proxy)` mirrors the TS `processRequest`, with the
  registry threaded explicitly to preserve immutability — no design
  compromise required.

### Risks and open questions

- **Store + types is the long pole.** `src/state/store.ts` is 5800
  lines with 449+ methods; `src/state/types.ts` is 2800 lines of
  resource record definitions. This is the single biggest porting
  cost and was deliberately deferred in the spike. It will dominate
  the calendar; the events handler skipped the store entirely because
  events are read-only and always empty in the proxy. Most other
  domains will not have that escape hatch.
- **Deep generic helpers like `serializeConnection<T>` need a different
  shape in Gleam.** The TS version takes callbacks (`serializeNode`,
  `getCursorValue`) and is reused across every connection-shaped
  field. In Gleam, parametric polymorphism handles this, but the
  number of arguments grows quickly; the spike sidestepped by
  specializing for the empty-items case. For real domains we'll need
  a more carefully designed connection helper, possibly with a
  configuration record instead of positional callbacks.
- **Mutable-API ergonomics.** Threading the proxy through every call
  is correct but verbose. The right pattern long-term is probably a
  `gleam_otp` actor that owns the registry + store, with handlers
  that send messages — but that's only worth introducing when there's
  enough state to justify it. For now the explicit threading is fine
  and matches Gleam idioms.
- **No date/time stdlib.** ISO 8601 formatting requires FFI; this is
  per-target boilerplate that scales linearly with the number of
  date/time operations. Manageable, but a friction point.
- **Block strings, descriptions, schema definitions deliberately
  omitted from the parser.** Operation documents in
  `config/parity-requests/**` don't use them — but if any future
  Shopify client introduces block string arguments the parser will
  need extending. Documented as a known gap in `lexer.gleam` /
  `parser.gleam`.

### Recommendation

Continue the port. The substrate is sound; the GraphQL parser is the
hardest subjective port (4 of the 12 substrate modules) and it landed
without surprises. The next bottleneck is mechanical: porting
`state/types.ts` resource records and the corresponding slices of
`state/store.ts`, one domain at a time. Start with `delivery-settings`
or `saved-searches` — both are small and have minimal store coupling
— before tackling `customers` or `products`.
