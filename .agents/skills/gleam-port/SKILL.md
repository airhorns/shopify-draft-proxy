---
name: gleam-port
description: Project-specific guidance for the in-progress Gleam port of `shopify-draft-proxy`. Load whenever the working directory is under `gleam/` or the task touches the port. Complements (does not replace) the generic `gleam` skill.
---

# Gleam port (shopify-draft-proxy)

The port re-implements the TypeScript draft proxy in Gleam, targeting both
Erlang (BEAM) and JavaScript (Node ESM). It is **incremental** — passes land
domain-by-domain, with the original TypeScript implementation and tests kept
intact until the whole port reaches verified 100% parity across the repository.
Each pass appends an entry to `GLEAM_PORT_LOG.md`; the immutable
acceptance bar lives in `GLEAM_PORT_INTENT.md`.

This skill captures the patterns that have stabilized across passes 1–20 so
new passes don't re-derive them. For generic Gleam idioms (decoders,
opaque types, pattern matching, OTP), use the `gleam` skill.

## Read first

1. `GLEAM_PORT_INTENT.md` — non-negotiables, acceptance criteria,
   working principles. Do not violate; if a constraint binds, flag it.
2. `GLEAM_PORT_LOG.md` — most recent 2–3 passes (top of file, newest first).
   Tells you what just landed, what's deferred, and what the next pass
   candidates were. Skip the rest unless your task touches an already-ported
   domain.
3. `AGENTS.md` — repository-wide non-negotiables. The Gleam port inherits
   all of them.

## Decision tree

```
Working in gleam/?
├─ Porting a new domain          → references/domain-port-template.md
├─ Modifying an existing domain  → read its module + that pass's log entry
├─ Hitting a "weird" error       → references/port-gotchas.md
├─ Adding a substrate helper     → check if it was already lifted in a pass
│                                  (search GLEAM_PORT_LOG.md for "substrate")
├─ Cross-target FFI needed       → references/port-gotchas.md (FFI section)
└─ Don't know what to port next  → tail of GLEAM_PORT_LOG.md → "Pass N candidates"
```

## Cross-target rule

**Both targets must be green for every change.** Drift between Erlang and
JavaScript is the most expensive bug class to find later. Run both:

```sh
cd gleam
gleam test --target erlang
gleam test --target javascript
```

CI runs both; do not push without local confirmation. If you add FFI, you
must add both `.erl` and `.mjs` shims at the same path-stem under
`src/shopify_draft_proxy/`. See `crypto.gleam` + `crypto_ffi.{erl,mjs}` for
the canonical example.

## TypeScript preservation rule

Leave the original TypeScript implementation and TypeScript tests alone during
incremental Gleam port work. A domain reaching local Gleam parity is not enough
to delete, rewrite, or weaken its TypeScript runtime, its TypeScript tests, TS
dispatcher wiring, or TS conformance/parity runner support. Those files remain
the shipping Node/Koa implementation and the reference harness until the final
all-port cutover proves 100% parity across domains, integration coverage, CI,
packaging, and docs.

Allowed during normal port passes:

- Add or update Gleam source and Gleam tests.
- Add bridge or shim code needed for interop while preserving existing TS
  behavior.
- Add parity-runner support that consumes existing fixtures without weakening
  the TypeScript runner.

Not allowed during normal port passes:

- Deleting `src/proxy/*` domain modules, TypeScript store slices, dispatcher
  entries, TypeScript integration tests, or TypeScript conformance/parity
  runner coverage because the corresponding Gleam domain now passes locally.
- Rewriting TypeScript tests into weaker assertions or removing TypeScript
  coverage to make the port appear complete.
- Treating per-domain parity as authority to retire TypeScript runtime code.

## Stable patterns

These are no longer design questions — they are templates. Use them.

### Domain module surface

Every ported domain module exposes the same shape:

```gleam
pub type <Domain>Error { ParseFailed(root_field.RootFieldError) }
pub fn is_<x>_query_root(name: String) -> Bool
pub fn is_<x>_mutation_root(name: String) -> Bool   // if the domain mutates
pub fn handle_<x>_query(store, document, variables) -> Result(Json, <Domain>Error)
pub fn wrap_data(data: Json) -> Json
pub fn process(store, document, variables) -> Result(Json, <Domain>Error)
pub fn process_mutation(store, identity, request_path, document, variables)
  -> Result(MutationOutcome, <Domain>Error)   // if it mutates
```

`MutationOutcome` (defined per-domain but with the same fields) is:

```gleam
pub type MutationOutcome {
  MutationOutcome(
    data: Json,                                  // full envelope
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}
```

### Store slice for a collection resource

```gleam
// In state/store.gleam, on BOTH BaseState and StagedState:
{plural}: Dict(String, {Singular}Record),
{singular}_order: List(String),
deleted_{plural}_ids: Dict(String, Bool),       // omit if resource never deletes
```

Helpers (mirror existing slices byte-for-byte — copy from saved-searches or
segments):

- `upsert_base_{singular}(store, records)` — base upsert; clears any
  deleted markers for the same id.
- `upsert_staged_{singular}(store, record)` — staged upsert; appends to
  staged order list only if id is new.
- `delete_staged_{singular}(store, id)` — drop staged + set staged
  deleted-marker. (Skip if the resource doesn't delete.)
- `get_effective_{singular}_by_id(store, id)` — staged wins; either
  side's deleted marker suppresses.
- `list_effective_{plural}(store)` — ordered ids first (deduped across
  base+staged), then unordered ids sorted by id.

### Singleton resource

```gleam
{singular}_configuration: Option({Singular}Record)   // on both states
```

Helpers: `set_staged_{singular}(store, record)`,
`get_effective_{singular}(store)` with a `default_{singular}()` fallback.
No order list, no deleted markers. See `tax_app_configuration` and
`gift_card_configuration` for the canonical shape.

For Store Properties-style singleton records that do not have a default
fallback (for example `shop`), keep `Option({Singular}Record)` on both base and
staged state, make staged replace base wholesale, and return `None`/GraphQL
`null` rather than inventing a fake local record when no captured shop baseline
exists.

### Dispatcher wiring (per new domain)

5 lines in `proxy/draft_proxy.gleam`:

1. New `<Domain>Domain` variant on the local `Domain` type.
2. Add the root to the explicit local dispatch table in
   `local_query_dispatch_domain` and/or `local_mutation_dispatch_domain`.
3. The registry decides whether a known root is implemented; the local dispatch
   table decides whether this Gleam port can actually handle that root today.
4. Dispatch arm in `route_query` / `route_mutation` calling
   `<domain>.process(...)` / `<domain>.process_mutation(...)`.
5. Import the new module.

### Operation registry sync

The TypeScript-side `config/operation-registry.json` is the source of truth
while the port is in progress. The Gleam mirror lives in
`gleam/src/shopify_draft_proxy/proxy/operation_registry_data.gleam` and is
generated deterministically:

```sh
gleam/scripts/sync-operation-registry.sh
```

CI checks drift through `corepack pnpm conformance:check`, which runs:

```sh
gleam/scripts/sync-operation-registry.sh --check
```

Capability lookup mirrors the TypeScript registry for every implemented match
name. Local dispatch is gated separately by the explicit local dispatch table
and the ported domain root predicates; an implemented TypeScript root whose
domain or specific root is not ported to Gleam remains unsupported locally and
uses live-hybrid passthrough instead of being claimed as staged/overlay support.

### Mutation validation

Use `proxy/mutation_helpers` for AST-level validation. The split between
"validate against AST" (which alone distinguishes omitted / literal-null /
unbound-variable) and "execute against resolved-arg dict" is load-bearing.
Do not collapse it.

- `validate_required_field_arguments(field, variables, op_name, required, op_path)`
  for general required-arg checking.
- `validate_required_id_argument(...)` for `*Delete` mutations whose only
  top-level requirement is `id`. Returns `#(Option(String), List(Json))`.
- `read_optional_string` / `read_optional_string_array` for resolved-arg
  reads.

### Synthetic identity

Two mint helpers, **not interchangeable** — each domain follows the TS
handler:

- `synthetic_identity.make_synthetic_gid(identity, "Type")` →
  `gid://shopify/Type/N` (looks like a real upstream id). Used by
  segments, webhook subs, mutation log entries, gift card transactions,
  apps, app installations.
- `synthetic_identity.make_proxy_synthetic_gid(identity, "Type")` →
  `gid://shopify/Type/N?shopify-draft-proxy=synthetic`. Used by saved
  searches and gift cards.

If your test fixtures use the wrong form, look-by-id misses silently.
Trust the actual handler output rather than guessing — Pass 19 + 20 both
hit this.

### Parity runner capture seeding

Some parity specs use a setup mutation against an upstream resource
that already exists in the live capture. Do not edit those specs or
rewrite the setup request. Seed the Gleam proxy from capture data in
`test/parity/runner.gleam` before executing the primary request, keyed
by scenario id, mirroring the TS parity harness. Pass 27's
`gift-card-search-filters` seeding is the current template: decode only
fields present in the capture, upsert them into base state, then let
the setup mutation produce the staged read-after-write state.

If an existing parity spec uses wildcard expected-difference paths such as
`$.shop.shopPolicies[*].updatedAt`, teach the Gleam diff layer to honor that
path syntax instead of narrowing or rewriting the checked-in spec.

If an existing parity target declares `selectedPaths`, preserve that contract in
the Gleam parity runner instead of broadening or narrowing the checked-in spec.
The Gift Cards lifecycle parity spec depends on target-level selected-path
diffing so mutation payload comparisons ignore unselected Shopify fields while
still strictly comparing the requested stable slices.

### TypeScript runtime retirement

Keep the legacy TypeScript runtime, dispatch hooks, and tests in place during
ordinary per-domain parity passes. When a final explicit cleanup phase deletes a
domain from the TypeScript proxy, update both `config/parity-specs/<domain>/*.json`
and `config/operation-registry.json` so runtime-test metadata points at the
Gleam parity/direct tests that then own the behavior. Then run
`gleam/scripts/sync-operation-registry.sh` so the vendored Gleam registry
matches the JSON source.

### Functions parity note

Captures with `seedShopifyFunctions` can share one runner seeding helper for
local staging and live read-only scenarios. When a local-runtime Functions
fixture appears one synthetic id/timestamp step ahead, check whether the
TypeScript conformance harness seeds the synthetic registry before the primary
request; mirror that seed in the Gleam runner rather than adding broad
synthetic-id/timestamp expected differences.

### Porting notes

- Events is a read-only, no-data domain. Gleam coverage for `event`, `events`,
  and `eventsCount` should still include parity and dispatcher-level tests, but
  the TS handler and TS runtime coverage stay in place until the final all-port
  cutover.
- Product-owned metafield creates replayed from captured upstream owners can
  mint low local synthetic IDs such as `gid://shopify/Metafield/1`, while
  Shopify would allocate a later upstream ID. Keep owner metafield connection
  ordering Shopify-like by placing those low draft-digest local IDs after
  captured upstream IDs; do not broaden parity expected differences just to
  hide ordering drift.
- The singular Product `metafieldDelete` compatibility parity spec shares the
  plural `metafieldsDelete` live capture and compares user-errors plus
  downstream owner state, not Shopify's removed singular payload. Seed the
  local deleted metafield ID expected by the compatibility request while
  keeping the owner metafield siblings from the plural capture unchanged.
- Product `metafieldsSet` inputs supplied through a GraphQL variable are
  rejected as top-level `INVALID_VARIABLE` errors when `ownerId`, `key`, or
  `value` is missing or null. Do not serialize those branches as
  `metafieldsSet.userErrors`, and abort the local mutation without staging
  store changes or draft log entries.
- Product `metafieldsSet` owner-expansion parity needs argument-aware
  serialization on every selected owner shape. ProductVariant and Collection
  `metafield` / `metafields` fields must read from owner-scoped staged
  metafields, including nested Product `variants`, instead of falling through
  generic source projection.
- Product and ProductVariant contextual pricing captures should be stored as a
  walkable state JSON value and projected through normal source projection.
  Do not hardcode contextual pricing response fragments in the parity runner;
  the capture seeding path should hydrate the Product/Variant records.
- Inactive inventory levels are stateful rows, not deleted rows. Model
  `isActive` on the level, exclude inactive levels from default
  `inventoryLevels` reads, honor `includeInactive: true`, and make
  reactivation flip the same row back to active while preserving quantities.
- Versioned parity specs may set `proxyRequest.apiVersion`; the Gleam parity
  runner must execute that request through the matching Admin route before
  domain code can observe route-gated Shopify contract drift. For 2026-04
  inventory quantity roots, require `changeFromQuantity` and `@idempotent`
  before staging, return top-level GraphQL errors with `{data: {root: null}}`,
  and use `changeFromQuantity` as the compare value for successful set/adjust
  mutations.
- Product media validation scenarios need explicit `seedProductMedia`
  hydration in the parity runner before the primary request. Model
  `productCreateMedia` as partial for valid create inputs plus
  `mediaUserErrors`, but keep `productUpdateMedia` and `productDeleteMedia`
  atomic when any requested media ID is unknown. Empty product IDs and invalid
  `CreateMediaInput.mediaContentType` are top-level `INVALID_VARIABLE` GraphQL
  errors, not payload user errors.
- Product `productReorderMedia` captures need setup media seeded from the
  captured `productCreateMedia.product.media.nodes` response, not just the
  product row. Reuse the collection-style `MoveInput` parsing and sequential
  zero-based reorder semantics, but serialize failures through
  `mediaUserErrors` without collection reorder codes. Downstream Product reads
  may select both `media` and `images`; media-only captures should expose
  Shopify's empty Product `images` connection rather than omitting the field.
- Product relationship roots combine multiple local slices. For
  `collectionAddProductsV2`, reuse collection membership staging but return the
  async Job payload and apply Shopify's non-manual prepend-reverse ordering
  (`MANUAL` collections append). For ProductVariant media roots, store ordered
  media IDs on the variant and resolve `ProductVariant.media` through Product
  media records; do not duplicate media rows per variant. Relationship captures
  may need `seedCollections` hydrated in addition to generic `seedProducts` and
  `seedProductMedia`.
- Gift Cards has executable Gleam lifecycle/search parity, but the TypeScript
  gift-card runtime and legacy integration coverage stay in place until a later
  reviewer-approved runtime cutover.
- Privacy `dataSaleOptOut` is a privacy-domain mutation whose downstream read
  effect belongs on `CustomerRecord.data_sale_opt_out`. Keep only that root in
  `proxy/privacy.gleam`; seed its parity capture from the downstream customer
  read so the proxy returns the captured customer id without broadening shop
  privacy settings support.

## Workflow for a new pass

1. Pick a candidate from the most recent log entry's "Pass N candidates"
   list, or from `config/operation-registry.json` filtered by
   `implemented: true` and not yet ported.
2. Read the corresponding `src/proxy/<domain>.ts` (TS source) and its
   slice of `src/state/types.ts` and `src/state/store.ts`.
3. Skim parity specs at `config/parity-specs/<domain>/` if any exist —
   they are the oracle when behaviour is ambiguous.
4. Order your work: state types → store slice → read path → mutation
   path → dispatcher wiring. Do **not** interleave domains.
5. Land tests alongside, on both targets. Prefer the `run(store, query)`
   helper pattern from existing test files.
6. Append an entry to `GLEAM_PORT_LOG.md` with the standard sections:
   summary paragraph, module table, "What landed", "Findings", "Risks /
   open items", "Pass N+1 candidates".

See `references/domain-port-template.md` for the concrete checklist.

## What NOT to do

- Do not run supported mutations against real Shopify at runtime (inherits
  from `AGENTS.md`).
- Do not rewrite parity specs or conformance fixtures — they are bytes the
  port must match.
- Do not delete, rewrite, or weaken the original TypeScript implementation or
  TypeScript tests during incremental domain/substrate passes. Keep TS and
  Gleam side-by-side until final all-port parity is proven.
- Do not "improve" Shopify's behaviour; match the recorded fixtures.
- Do not pull in `gleam_regexp` for one-off predicate sets — hand-roll
  string predicates (Pass 20 finding). The dependency footprint matters
  for cross-target portability.
- Do not skip the JS target. "Erlang green" is half a result.
- Do not wire `AsyncLocalStorage`-style implicit context. Thread store +
  identity explicitly through every handler.

## Reference files

- `references/domain-port-template.md` — concrete checklist and code
  templates for a new domain pass.
- `references/port-gotchas.md` — distilled trap list from passes 1–20.
