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
- Product merchandising guardrail captures route bundle, combined-listing, and
  ProductVariant relationship roots locally even when no success lifecycle is
  ported yet. Preserve the captured validation priority: unknown
  `productBundleUpdate.input.productId` returns `field: null` before checking
  empty components, empty `productBundleCreate.input.components` returns
  `field: null`, missing combined-listing parents return code
  `PARENT_PRODUCT_NOT_FOUND`, and missing ProductVariant relationship IDs use a
  compact JSON string list in the `PRODUCT_VARIANTS_NOT_FOUND` message.
- Product variant bulk validation/atomicity captures need their setup Product
  options and default variant hydrated from `seed.setupOptionsResponse` before
  replay. Validate the full create/update/delete batch before staging any
  variant, option, inventory item, inventory quantity, or Product summary
  changes. Bulk update validation returns `productVariants: null`; bulk create
  validation returns `productVariants: []`; nullable `userErrors.field` appears
  on the empty update branch.
- Async Product `productDuplicate` captures stage a completed
  `ProductDuplicateOperation` but serialize the mutation-time operation as
  `CREATED` with `newProduct: null`; missing Products expose the user error on
  the later `productOperation(id:)` read, not the mutation payload. Seed the
  source Product from the capture before replay, and project root
  `productOperation` with the raw selection set so inline-fragment fields like
  `id`, `newProduct`, and `userErrors` are not dropped.
- Synchronous Product `productDuplicate` captures need the source Product graph
  hydrated from `setup.sourceReadBeforeDuplicate.data.product`, including
  collections, memberships, and Product metafields. Duplicate the local graph
  deeply enough for downstream reads: Product options and values, variants and
  inventory items, collection memberships, and Product metafields get fresh GIDs
  where Shopify returns new resources, while immediate duplicate media remains
  empty even when the source Product had ready image media. Existing
  expected-difference paths may use quoted connection segments like
  `variants["nodes"][0].id`; normalize those in the diff layer rather than
  rewriting captured specs.
- Product `productSet` captures use `input`, optional `identifier`, and
  ProductSet-specific inventory quantity inputs (`quantity`, `name`,
  `locationId`) rather than the older bulk-variant `availableQuantity` shape.
  Seed captured location names in the parity runner before replay, mirror
  `available` writes into `on_hand`, keep `incoming` present, and preserve
  Shopify's Product inventory-summary quirk: create sums tracked/not-explicitly
  untracked variants, while update keeps the Product's previous
  `totalInventory` even after inventory-level quantities change.
- Product sort-key read captures need local Product connection sorting and
  cursor handling to stay separate: when a Product row has a stored upstream
  cursor, keep that cursor authoritative; when the captured seed row has no
  cursor, synthesize Shopify-style base64 JSON cursors from `last_id` plus the
  sort-key value. The captured `VENDOR` and `PRODUCT_TYPE` tie-breaks are
  resource-id based, and partial alias seed rows must be merged so sparse
  selections like ID-only or publishedAt-only rows do not overwrite richer
  Product metadata needed by other aliases.
- Captured advanced Product search read fixtures can often seed local parity
  directly from every captured Product connection edge, but preserve each
  upstream edge cursor on the seeded Product row. Pagination captures may show
  only the selected page edge while `count`/`pageInfo` prove additional matching
  store rows exist; in that case seed scenario-local sentinel Products that
  match the same query and sort after the captured edge instead of weakening
  the captured request or expected comparison.
- The `products-search-grammar-read` fixture is an older TS-passing Product
  overlay read whose capture only contains the phrase aliases while the replay
  request also selects NOT and `tag_not` aliases. Mirror the TypeScript parity
  harness by using the target `upstreamCapturePath` as the primary actual
  response for that scenario; do not rewrite the fixture, request, variables,
  or comparison contract just to make the selected aliases line up.
- SellingPlanGroup Product/ProductVariant overlays have separate visibility and
  count semantics. Product `sellingPlanGroups.nodes` should include groups made
  visible by either direct Product membership or variant membership for that
  Product, while `sellingPlanGroupsCount` counts only direct Product
  membership. ProductVariant nodes are visible through direct variant membership
  or Product-level membership, while the variant count includes only direct
  variant membership. Preserve that split for product/variant join and leave
  roots.
- Product media async plan fixtures depend on timing-sensitive lifecycle state:
  create returns `UPLOADED` in the mutation payload, the immediate downstream
  Product media read is null-url `PROCESSING`, and later successful media
  operations may observe the same local staged media as `READY`. Do not seed the
  create plan's media row before the primary request; seed only the Product.
  Update/delete plan captures need the captured Product and existing media row
  hydrated from mutation/downstream data, including deleted ProductImage IDs for
  delete payload parity.
- Gift Cards has executable Gleam lifecycle/search parity, but the TypeScript
  gift-card runtime and legacy integration coverage stay in place until a later
  reviewer-approved runtime cutover.
- Privacy `dataSaleOptOut` is a privacy-domain mutation whose downstream read
  effect belongs on `CustomerRecord.data_sale_opt_out`. Keep only that root in
  `proxy/privacy.gleam`; seed its parity capture from the downstream customer
  read so the proxy returns the captured customer id without broadening shop
  privacy settings support.
- Orders abandonment parity can start with the safe abandoned-checkout slice:
  `abandonedCheckouts`, `abandonedCheckoutsCount`, `abandonment`,
  `abandonmentByAbandonedCheckoutId`, and
  `abandonmentUpdateActivitiesDeliveryStatuses`. Keep this dispatch predicate
  narrow until draft orders, order lifecycle, fulfillments, refunds, and returns
  have their own executable parity. Unknown-abandonment delivery status updates
  are a handled local validation branch with a `Failed` mutation-log draft, not
  a reason to claim broader orders mutation support.
- Orders access-denied guardrails may be ported when the checked-in capture
  proves Shopify returns a top-level `ACCESS_DENIED` error plus a selected null
  root payload. Keep these documented as guardrails, not full lifecycle support:
  `orderCreateManualPayment` unknown/non-local order access denial does not
  imply local synthetic-order payment success is ported, and `taxSummaryCreate`
  access denial does not imply tax-app success semantics are ported.
- Draft-order create parity can start with raw captured `DraftOrderRecord`
  staging plus a tiny variant catalog seed derived from the captured created
  line items. Preserve Shopify's split between line-item `sku`,
  line-item `variantTitle`, and nested `variant.sku`: default-title variants
  render `variantTitle` as null, line-item `sku` may be `""`, and nested
  variant `sku` may still be null.
- Standalone draft-order read parity can seed `$.response.data.draftOrder` into
  base draft-order state as captured JSON. Keep this scenario-specific until
  the draft-order lifecycle roots prove which normalized fields and indexes are
  truly needed.
- Draft-order validation guardrails such as `draftOrderComplete` required-`id`
  branches should stay documented as guardrails. Do not treat omitted/null
  argument parity as evidence that completion, payment, source-name handling, or
  downstream Order materialization is ported.
- Fulfillment validation guardrails for `fulfillmentCancel` and
  `fulfillmentTrackingInfoUpdate` can share the required-argument validator, but
  keep the root-specific argument name exact (`id` vs. `fulfillmentId`). Do not
  treat these branches as fulfillment lifecycle support; happy paths still need
  local fulfillment state, order downstream visibility, and mutation-log effects.
- `orderCreate` missing-order validation can use the same top-level required
  argument helper with `OrderCreateOrderInput!`. Keep the direct-order happy
  path gated until local order state, payment/transaction effects, inventory
  bypass, and downstream reads are implemented.
- `orderUpdate` missing-id validation is nested inside `OrderInput`, not a
  top-level required argument. Mirror Shopify's error message/extensions for
  inline missing/null `input.id` and variable-backed missing id without treating
  update success, downstream reads, or timestamp behavior as ported.
- Order-edit missing-id validation for `orderEditBegin`,
  `orderEditAddVariant`, `orderEditSetQuantity`, and `orderEditCommit` can use
  the shared top-level required-argument helper with `ID!`. Keep the edit
  session lifecycle, calculated edits, commit effects, and downstream order
  reads gated until those state transitions are modeled together.
- `fulfillmentCreate` invalid fulfillment-order id is a GraphQL
  `RESOURCE_NOT_FOUND` error with `data.fulfillmentCreate: null`, not a
  `userErrors` payload. Treat this as a guardrail only; successful fulfillment
  creation still needs local fulfillment-order state and downstream order
  fulfillment visibility.
- `draftOrderDelete` should delete from staged draft-order state and add a
  deleted-id marker so downstream `draftOrder(id:)` reads return null even when
  the draft was seeded in base state. Keep duplicate/update/complete success
  paths separate until their own parity fixtures are executable.
- `draftOrderUpdate` parity can seed the setup draft order from
  `$.setup.draftOrderCreate.mutation.response.data.draftOrderCreate.draftOrder`
  as captured JSON, then stage field-level updates over that record. Preserve
  captured stable fields such as id/name/invoice URL/customer/addresses, and
  recalculate money totals from effective line items, order discount, and
  shipping line before serializing downstream `draftOrder(id:)` reads.
- `draftOrderDuplicate` parity uses the same captured setup draft-order seed
  path, then creates a new staged draft and fresh line-item ids. Shopify's
  duplicate clears source shipping, order-level discount, line-item discounts,
  `taxExempt`, and `reserveInventoryUntil`; recalculate totals from the
  cleared duplicate rather than copying source totals.
- `draftOrderComplete` parity seeds the captured setup draft-order, then marks
  that same draft `COMPLETED` and attaches a nested synthetic order on the
  staged draft. Preserve the TS normalization where any non-null completion
  `sourceName` becomes `347082227713`, `paymentPending: false` maps to
  `paymentGatewayNames: ["manual"]` and `displayFinancialStatus: "PAID"`, and
  order line-item ids are fresh `LineItem` gids while draft line-item ids remain
  unchanged.
- `draftOrderInvoiceSend` parity is currently a safety/validation slice only.
  Seed the captured no-recipient open/completed draft states, leave deleted ids
  unseeded so they behave as not found, serialize user-error `field` as nullable
  when Shopify returns `null`, and do not mutate staged draft-order state or
  claim recipient-backed email-send success behavior.

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
