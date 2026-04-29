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
- > 4K TS LOC: unusual; pre-plan multiple passes and add an entry to the
  log marking "Pass A of N".

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

| Shape | When to use | Example |
| --- | --- | --- |
| Dict + order + deleted-ids | Collection that supports delete | `saved_searches`, `segments`, `validations`, `webhook_subscriptions` |
| Dict + order (no deleted-ids) | Append-only collection | `gift_cards`, `apps`, `app_subscriptions`, `shopify_functions` |
| `Option(Record)` (singleton) | One-per-shop config | `tax_app_configuration`, `gift_card_configuration`, `current_installation_id` (modeled as `Option(String)` pointer) |

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

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

pub type UserError {
  UserError(field: List(String), message: String)
  // add `code: Option(String)` if the domain emits typed user errors
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, <Domain>Error) { ... }
```

For each mutation root, write a per-root handler returning a
`MutationFieldResult { key, payload, staged_resource_ids, top_level_errors }`.
The `process_mutation` fold accumulates these into either a `{"data": ...}`
or `{"errors": [...]}` envelope.

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

Edit `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`:

1. Import the new module.
2. Add `<Domain>Domain` variant to the local `Domain` type.
3. Add capability arm in `capability_to_query_domain`:
   ```gleam
   <Domain> -> Ok(<Domain>Domain)
   ```
   And the same in `capability_to_mutation_domain` if it mutates.
4. Add legacy fallback:
   ```gleam
   case <domain>.is_<x>_query_root(name) {
     True -> Ok(<Domain>Domain)
     False -> Error(Nil)
   }
   ```
5. Add dispatch arm in `route_query` / `route_mutation` calling
   `<domain>.process(...)` / `<domain>.process_mutation(...)`.

The capability variant comes from
`shopify_draft_proxy/proxy/operation_registry.{type CapabilityDomain}`.
If your domain isn't already a variant, add it there (and update
`parse_domain` for the JSON kebab-case mapping).

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

## Step 8 — Log entry

Append to `GLEAM_PORT_LOG.md` (newer entries go at the top):

```markdown
## YYYY-MM-DD — Pass N: <one-line summary>

<2–4 sentence summary of what this pass shipped, what's deferred, and any
load-bearing decisions.>

### Module table
| Module | Lines | Notes |
| --- | --- | --- |
| ... | ... | ... |

**Test count: <before> → <after>** (+N). Both targets clean (Erlang OTP 28 + JS ESM).

### What landed
<bullet list of substantive landings with code references>

### Findings
<bullet list of patterns confirmed, surprises, decisions made>

### Risks / open items
<bullet list of explicit deferrals and gaps the next pass should know about>

### Pass N+1 candidates
<2–3 ranked candidates for the next pass>
```

The log is the running narrative. Keep `GLEAM_PORT_INTENT.md` for
non-negotiables only — do not append to it per pass.
