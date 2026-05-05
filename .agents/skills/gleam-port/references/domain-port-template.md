# Domain port template

Concrete checklist for porting one domain (e.g. `customers`, `localization`)
from `src/proxy/<domain>.ts` to `gleam/src/shopify_draft_proxy/proxy/<domain>.gleam`.
Distilled from passes 11–20.

A domain port is a **single pass**. Do not interleave with another domain.

## Sizing

Before starting, size the domain:

```sh
wc -l src/proxy/<domain>.ts
grep -c "^pub " gleam/src/shopify_draft_proxy/proxy/saved_searches.gleam   # rough comparable
```

- ≤ 1.5K TS LOC: one pass.
- 1.5K–4K TS LOC: split into substrate (state + read) + mutations (and
  optionally hybrid hydration). See pass 11/12/13 for webhooks split, or
  pass 15/16/17 for apps split.
- More than 4K TS LOC: unusual; pre-plan multiple passes and record "Pass A of N"
  in the workpad or Linear notes.

## Step 1 — State types

Edit `gleam/src/shopify_draft_proxy/state/types.gleam`. Add records mirroring
the TS shapes in `src/state/types.ts`. Rules:

- Optional in TS → `Option(T)`.
- Discriminated union in TS (`__typename` + per-variant optional fields) →
  Gleam **sum type** with one variant per kind, each carrying only the
  fields its kind uses. See `WebhookSubscriptionEndpoint` and
  `AppSubscriptionPricing`.
- TS reserved field `test` → rename to `is_test` on the Gleam record. The
  GraphQL response key stays `"test"` because the source builder names it
  explicitly.
- `MoneyV2` → reuse `types.Money`. Don't roll a new one.
- Add a doc comment that names the TS counterpart and flags any field
  intentionally omitted (e.g. `app: jsonObjectSchema.optional()` on
  `ShopifyFunctionRecord`).

## Step 2 — Store slice

Edit `gleam/src/shopify_draft_proxy/state/store.gleam`. Decide which shape:

| Shape                         | When to use                     | Example                                                                                                             |
| ----------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Dict + order + deleted-ids    | Collection that supports delete | `saved_searches`, `segments`, `validations`, `webhook_subscriptions`                                                |
| Dict + order (no deleted-ids) | Append-only collection          | `gift_cards`, `apps`, `app_subscriptions`, `shopify_functions`                                                      |
| `Option(Record)` (singleton)  | One-per-shop config             | `tax_app_configuration`, `gift_card_configuration`, `current_installation_id` (modeled as `Option(String)` pointer) |

Add fields to **both** `BaseState` and `StagedState`. Existing record
constructors will need to switch to `..base` / `..staged` spread for the
unrelated fields.

Add helpers (copy from the closest existing slice — segments and
saved_searches are the simplest reference, gift cards is the canonical
no-deletion variant).

Tests for the store slice belong in
`gleam/test/shopify_draft_proxy/state/store_test.gleam`.

## Step 3 — Read path (`proxy/<domain>.gleam`)

Public surface (matches every existing domain — copy the scaffold):

```gleam
pub type <Domain>Error { ParseFailed(root_field.RootFieldError) }

pub fn is_<x>_query_root(name: String) -> Bool { ... }

pub fn handle_<x>_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, <Domain>Error) { ... }

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, <Domain>Error) {
  use data <- result.try(handle_<x>_query(store, document, variables))
  Ok(wrap_data(data))
}
```

Internal pieces:

- `serialize_root_fields(store, fields, fragments, variables) -> Json`
  iterates each root field, dispatches by name, builds `[#(key, value)]`,
  wraps with `json.object`.
- `root_payload_for_field(store, field, fragments, variables) -> Json`
  matches on the field name and routes to a per-root serializer.
- Per-root serializers build a `SourceValue` (typically `src_object([...])`)
  and route through `project_graphql_value(field, source, fragments)`.
- For connections, build a `ConnectionWindow` from the resource list,
  pass through `paginate_connection_items` then `serialize_connection`.
- Inline-fragment + FragmentSpread handling for `__typename`-discriminated
  unions: walk the selection set against type-condition matching against
  the parent typename. Copy the pattern from `gift_cards.gleam`'s
  `serialize_gift_card` or `apps.gleam`'s pricing serialization.

## Step 4 — Mutation path (if applicable)

```gleam
pub fn is_<x>_mutation_root(name: String) -> Bool { ... }

pub type UserError {
  UserError(field: List(String), message: String)
  // add `code: Option(String)` if the domain emits typed user errors
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> mutation_helpers.MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> { ... }
  }
}
```

`MutationOutcome` is the shared type from `proxy/mutation_helpers.gleam`:

```gleam
pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}
```

Domains do not return `Result(MutationOutcome, <Domain>Error)` — the
phantom error wrapper was removed. The only structurally-failing branch
is "document failed to re-parse," handled by
`mutation_helpers.parse_failed_outcome`.

For each mutation root, write a per-root handler returning a
`MutationFieldResult { key, payload, staged_resource_ids, top_level_errors,
log_drafts }`. The `process_mutation` fold accumulates these into either a
`{"data": ...}` or `{"errors": [...]}` envelope and returns the collected
`log_drafts` to the dispatcher; do **not** call
`store.record_mutation_log_entry` from inside the domain. The dispatcher
calls `mutation_helpers.record_log_drafts(...)` once after the handler
returns, threading the synthetic-identity registry through the entry id /
timestamp mints.

A few domains take an additional `upstream_query.UpstreamContext` parameter
(read-before-write merges, app origin lookups, etc.). The dispatch table
in `draft_proxy.gleam` adapts each domain's `process_mutation` signature to
the unified `MutationHandler` closure shape — see Step 5.

Use `mutation_helpers.validate_required_field_arguments` for AST-level
required-arg validation. Use the resolved-arg readers for execution.

Mint synthetic gids via the right helper:

- `make_synthetic_gid(identity, "Type")` for resources whose ids should
  look like real upstream ids (most domains).
- `make_proxy_synthetic_gid(identity, "Type")` for resources where the
  TS handler uses `makeProxySyntheticGid` (saved searches, gift cards).

When in doubt, **read the TS handler** — `makeSyntheticGid` vs
`makeProxySyntheticGid` is a per-resource choice the proxy mirrors.

## Step 5 — Dispatcher wiring

Edit `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`. There is no
per-domain enum variant anymore; the dispatcher is a flat table keyed by
root-field name (or predicate) that hands back a `QueryHandler` or
`MutationHandler` closure.

1. Import the new module.
2. Add an arm to `local_query_handler` for fixed-name root fields, or a
   tuple to its `first_matching_handler([...])` fallback for
   predicate-driven dispatch (e.g. `is_<x>_query_root(name)`):
   ```gleam
   #(<domain>.is_<x>_query_root(name), <domain>.handle_query_request),
   ```
3. If the domain mutates, write a `<domain>_mutation_handler` adapter
   closure that pulls `proxy.store`, `proxy.synthetic_identity`, and (if
   applicable) the `UpstreamContext` out of the inputs and forwards them
   to `<domain>.process_mutation`. Then add a fixed-name arm or a
   predicate tuple in `local_mutation_handler` returning that adapter.
4. Most domains do not need a custom dispatch arm in `route_query` /
   `route_mutation` — the shared path invokes the closure returned by
   `query_handler_for` / `mutation_handler_for`.

The handler closure types defined in `draft_proxy.gleam` are:

```gleam
type QueryHandler =
  fn(DraftProxy, Request, ParsedOperation, String, String,
     Dict(String, root_field.ResolvedValue))
  -> #(Response, DraftProxy)

type MutationHandler =
  fn(DraftProxy, String, String, Dict(String, root_field.ResolvedValue),
     upstream_query.UpstreamContext)
  -> mutation_helpers.MutationOutcome
```

If your domain's `handle_query_request` is just a `process(...)` wrapper
returning a Shopify-shaped error envelope, use
`mutation_helpers.respond_to_query` rather than building the
`#(Response, DraftProxy)` tuple by hand.

## Step 6 — Tests

Two test files (read + mutation) under
`gleam/test/shopify_draft_proxy/proxy/`. Every existing pair is a good
template; segments is the smallest current example.

Test patterns to copy:

- `run(store, query) -> String` helper that calls `handle_*_query` and
  serializes to a string.
- `seed(store, record) -> store.Store` helper that upserts into staged.
- Predicate tests for `is_*_query_root` / `is_*_mutation_root`.
- Per-root happy-path test asserting the exact JSON output literal
  (escape quotes; do not pretty-print).
- Per-root user-error / top-level-error variants.
- Connection tests with `first:` / `last:` / `after:` / `query:` /
  `reverse:` if the domain has connection roots.

## Step 7 — Both targets, every change

```sh
cd gleam
gleam test --target erlang
gleam test --target javascript
```

Do not ship Erlang-only or JS-only changes.

## Step 8 — Handoff notes

Record pass-specific findings, risks, and follow-up candidates in the active
Linear workpad or linked follow-up issues. Keep `GLEAM_PORT_INTENT.md` for
non-negotiables only.
