# GLEAM_PORT_LOG.md

A chronological log of the Gleam port. Each pass adds a new dated entry
describing what landed, what was learned, and what is now blocked or
unblocked. The acceptance criteria and design constraints live in
`GLEAM_PORT_INTENT.md`; this file is the running narrative.

Newer entries go at the top.

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
