---
name: shopify-endpoint-implementation
description: Implement or audit Shopify Admin GraphQL endpoint support in shopify-draft-proxy as a faithful, scalable staged digital twin. Use when adding a query or mutation root, promoting a registry entry to local support, reviewing state and read-after-write behavior, or running a domain-wide gap audit across runtime modes, evidence, tests, commit replay, and performance.
---

# Shopify Endpoint Implementation

This guide documents the important best practices that all endpoints should conform to within this system. An endpoint is supported only when its local state model reproduces the captured mutation result and the downstream reads Shopify would materialize, without writing to Shopify before explicit commit.

## Establish the contract

1. Read `docs/original-intent.md` and the relevant parts of `docs/architecture.md`.
2. Read `docs/helpers.md` before adding parsers, serializers, search, scalar, metafield, or connection utilities. Search `docs/hard-and-weird-notes.md` for relevant captured behavior.
3. Inventory the versioned schema roots and types, operation-registry entries, domain callbacks, field resolvers, node loaders, store records, endpoint docs, runtime tests, and captured parity targets already covering the lifecycle.
4. Define the supported slice explicitly: success, validation and not-found branches; create/update/delete or equivalent transitions; mutation payloads; singular, connection, count, relationship, and generic `node`/`nodes` read-after-write effects; cross-domain effects caused by supported writes; and mode-specific behavior. For read-only roots, identify which writes should materialize or change their data.
5. Capture uncertain behavior from a real Shopify interaction before implementing it. Use `$shopify-conformance-expansion` for capture/parity work. Never turn proxy output, guesses, snapshots, or hand-authored data into Shopify evidence, and do not modify protected parity evidence without explicit user approval. If required live capture is blocked by auth, record the blocker and do not substitute runtime tests for parity evidence.

Keep a root registry-only or unsupported when its lifecycle or required read effects are not modeled. Claim support only with declared runtime tests and captured evidence for the exact behavior.

## Audit a domain

- Partition an endpoint group into coherent lifecycle or read slices; do not give one status to a mixed group. For each root, trace schema/catalog presence, registry metadata, actual callback, state/effective readers, Node loader, mode-specific transport, declared runtime tests, exact captured comparison targets, and endpoint-doc claims. Inspect executable paths: no single inventory or document proves support.
- Classify each slice as `unsupported` (inventory only), `passthrough`, `partial`, or `supported`. Use `partial` for no-data-only, validation-only, staged-only, single-mode, unbounded, or incompletely evidenced behavior. Track registry `implemented` separately: local dispatch, an empty snapshot shape, or successful passthrough is not by itself support.
- Exercise representative cold base/upstream data, a write targeting a cold live record, staged create/update/tombstone, missing IDs, filtered count, connection boundary refill, relationships and Node reads, rejected-write immutability, and reset/dump/restore/commit behavior. Mark inapplicable cases explicitly rather than silently omitting them.
- State the maximum upstream calls and rows/pages expected for cold and overlaid reads and writes. Test partial upstream windows, tombstones, and sort/filter-changing updates; a correct small fixture can still conceal an unbounded scan or an under-filled page on a large shop.
- Report a compact matrix with `slice | status | state/read effects | modes and scale budget | evidence | gaps`. Then produce issue-ready gaps naming the current observable behavior, risk (`correctness`, `write leak`, `mode`, `scale`, `commit/meta`, `evidence`, or `claim drift`), violated rule, implementation pieces, and exact runtime/parity/performance proof needed. Separate shared infrastructure work from root-specific work and do not duplicate it per endpoint.

## Model effective state

- Store normalized, instance-owned resource state. Separate observed/snapshot base state from local staged records, tombstones, relationships, ordering/cursors, count baselines, and completeness metadata.
- Provide one effective view that overlays staged records on base records, suppresses tombstones, preserves stable order, and recomputes derived fields. Use it for mutation payloads, singular reads, connections, counts, relationships, and generic Node reads.
- When a supported write affects a read owned by another endpoint group, stage that derived record, audit trail, relationship, or aggregate in the shared graph; do not let domain boundaries break read-after-write.
- Treat partial observations as partial. Preserve previously known fields and children when an upstream response omits them; distinguish omission, explicit `null`, and an observed empty list. Mark replacement semantics explicitly for mutations that replace a child collection.
- Derive identifiers, timestamps, shop capabilities, currencies, and related resources from arguments, effective state, captured upstream observations, or session-owned allocators and clocks. Never invent sentinel resources or hardcode captured IDs.
- Keep synthetic identities and time stable within a session. Dump/restore all modeled base/staged records, tombstones, order, completeness, counters, caches, and logs. Reset staged effects, session identities, caches, and logs without corrupting configured snapshot/base state.
- Return canonical, alias-free schema values from domain code. Let the executable GraphQL engine own parsing, versioned validation, coercion, aliases, fragments, directives, projection, abstract types, and null propagation. Never inspect a scenario/document name or return a canned fixture-shaped payload.

## Preserve runtime-mode semantics

| Path                                      | Upstream behavior                                                                                    | Required result                                                                                                                 |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Snapshot read                             | Never call upstream                                                                                  | Resolve snapshot base plus staged overlay; return Shopify-like `null`, empty connection/list, count, or error for missing data. |
| LiveHybrid read, no relevant local effect | Forward the caller's complete document once                                                          | Preserve Shopify's status, headers, errors, aliases, cursors, and payload unchanged while observing reusable state when safe.   |
| LiveHybrid read, local effect present     | Reuse the request-wide upstream result or perform bounded query-only hydration                       | Render the effective upstream-plus-staged graph; staged updates win and tombstones hide upstream rows.                          |
| Passthrough read                          | Forward unchanged without overlay                                                                    | Provide the debugging baseline even when staged state exists.                                                                   |
| Supported mutation in any read mode       | Never forward the write before commit; query-only prerequisite hydration is allowed outside Snapshot | Validate and stage locally, synthesize the Shopify-like payload, and retain replay input when the operation should commit.      |
| Unsupported mutation                      | Follow `unsupportedMutationMode`                                                                     | Passthrough or reject visibly; never register permanent passthrough as endpoint support.                                        |

Keep read mode independent from mutation support. Reject mixed local/passthrough mutation documents rather than splitting their atomic behavior or leaking a supported write upstream.

## Implement reads

- Route through the domain-owned callback and consume engine-coerced arguments from `RootInvocation`; use requested field paths only to plan hydration.
- For singular roots, resolve staged value, tombstone, observed base, then a targeted LiveHybrid lookup when necessary. Snapshot misses stay local.
- For connections, form the effective scoped set, then apply search/filter, sort with a deterministic tiebreaker, `reverse`, and cursor windowing in that order. Make counts use the same predicate. Preserve opaque upstream cursors when replaying observed rows and keep count precision/limits when applying staged deltas.
- Key completeness and count baselines by every argument that changes membership or ordering, including owner/parent scope. Never infer a complete catalog from one page or one singular observation.
- Resolve relationships and generic Node roots from the same normalized graph. Batch cold `nodes(ids:)` hydration and preserve input order and null placeholders.
- Reuse shared connection, search, scalar, validation, money, resource-ID, and resolved-value helpers. Treat unsupported search syntax as the captured no-match, warning, or error behavior rather than broadening results.

## Implement writes

1. Let schema coercion reject invalid GraphQL shapes before domain code. Reproduce captured payload `userErrors` and resolver-level errors only for behavior beyond schema validation.
2. Load only prerequisites needed for ownership, uniqueness, capability, relationship, or current-state validation. Batch and deduplicate IDs, reuse request/base caches, and do no hydration in Snapshot. Use targeted probes and count baselines for shop-wide uniqueness or limits instead of enumerating the catalog.
3. Validate before mutating state. Rejected branches must not partially stage records, consume synthetic IDs, alter derived state, or accidentally add replay entries.
4. Apply the transition atomically across the normalized resource, owned children, relationships, indexes, tombstones, and derived aggregates. Merge partial updates over the effective prior record.
5. Build the mutation payload from the post-transition effective view so it agrees with the next read. Preserve captured no-op and idempotency semantics.
6. Add a mutation-log draft only when the Shopify-equivalent operation should be replayed. Retain the original path, document, variables, operation order, and staged IDs; configure authoritative commit ID mappings for synthetic IDs consumed by later mutations.

Model calculate/preview operations as side-effect-free unless captured behavior proves otherwise. Model external side effects as staged intent, not as runtime emails, payments, webhooks, Functions, or other real-world actions.

## Enforce scale and transport budgets

- Make ordinary request work proportional to the requested page/IDs plus relevant staged changes, not to total shop size. Never hydrate an entire catalog or unrelated nested graph to answer one read or write.
- Perform at most one upstream call for an unaffected cold read: the caller's original complete document. Share it across aliases, sibling roots, and local resolvers through the request-scoped cache.
- Use a small bounded number of secondary queries only when the caller's document cannot supply mutation prerequisites or overlay evidence. Prefer one deduplicated `nodes(ids:)` batch or one narrow relationship/capability query; never issue per-node N+1 requests.
- Overlay connections with a window-aware plan: fetch only enough upstream rows to fill the requested window and prove its boundaries after relevant staged inserts/updates/tombstones. Use upstream count objects plus known deltas for count-only reads. Do not exhaust all pages merely to reconstruct a shop-wide list.
- Record completeness only after the queried scope is proven complete. Cache authoritative misses as carefully as hits so repeated work does not trigger repeated hydration.
- Leave a shape explicitly unsupported if exact behavior has no bounded plan. Do not trade correctness for an unbounded scan or hide a scale failure behind plausible output.

Add transport-count tests and a large-catalog test whenever a path introduces hydration, list overlays, relationship expansion, or bulk inputs.

## Wire every implementation piece

- Add or reuse the normalized Store records, effective getters/lists/counts, tombstones, completeness keys, indexes, and session allocators.
- Add one domain-owned root callback path plus canonical child-field resolvers for argument-bearing or derived fields. Extend the type-to-loader inventory for locally resolvable Node implementors.
- Register the root in the operation registry only when it dispatches locally. Add accurate `runtimeTests` and commit ID mapping metadata; do not create a second routing inventory.
- Preserve meta behavior: state/log inspection, reset, dump/restore, ordered raw commit replay, auth forwarding at commit, synthetic-ID rewriting, and stop-on-first-failure reporting.
- Update `src/content/docs/endpoints/<group>.md` with current consumer-facing behavior and boundaries. Put cross-cutting architecture in `docs/architecture.md`, shared helper rules in `docs/helpers.md`, and surprising Shopify evidence/rationale in `docs/hard-and-weird-notes.md`.

## Prove the endpoint

Add public-request-surface parity coverage and runtime tests for:

- success, captured error/no-op branches, and no state/log change on rejection
- create/update/delete read-after-write through payload, singular, list/count, relationships, and `node`/`nodes`
- Snapshot, LiveHybrid, and passthrough behavior, including a guard that supported writes never reach the upstream transport
- aliases, fragments, variables/defaults, omitted versus null inputs, version differences, null/empty shapes, search/sort/reverse/pagination/count precision
- reset/log/state/commit order and synthetic-ID mapping when applicable
- bounded upstream request counts on large or partial catalogs

Map every fidelity behavior changed in the implementation to an explicit captured parity comparison target. Prefer whole selected resource payloads with narrow volatile-path exclusions. Run targeted tests while iterating, then the full verification loop from `AGENTS.md` before handoff.
