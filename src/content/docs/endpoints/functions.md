---
title: 'Functions'
description: 'Coverage notes and fidelity boundaries for Functions.'
---

This endpoint group covers Shopify Function-backed Admin metadata roots for validations,
cart transforms, fulfillment constraint rules, Shopify Function catalog reads, and
tax app readiness.

## Current support and limitations

### Supported roots

Read roots:

- `validation(id:)`
- `validations(...)`
- `cartTransforms(...)`
- `fulfillmentConstraintRules`
- `shopifyFunction(id:)`
- `shopifyFunctions(...)`

Mutation roots:

- `validationCreate`
- `validationUpdate`
- `validationDelete`
- `cartTransformCreate`
- `cartTransformDelete`
- `fulfillmentConstraintRuleCreate`
- `fulfillmentConstraintRuleUpdate`
- `fulfillmentConstraintRuleDelete`
- `taxAppConfigure`

### Local behavior

Function-backed behavior is modeled as Admin metadata/state only:

- The proxy records Function handles or IDs attached to validations, cart transforms, and fulfillment constraint rules, preserves or hydrates local `ShopifyFunction` rows, and updates relevant detail/catalog roots after staged writes.
- In LiveHybrid mode, cold `shopifyFunction` / `shopifyFunctions` reads forward the caller's complete document once and preserve Shopify's payload, edge cursors, and `pageInfo`. Observed Function metadata remains available for later local mutation resolution. Snapshot mode does not invent Function catalog rows that are absent from local state.
- Supported mutation roots stage locally, append the original raw GraphQL request body to the mutation log for ordered `__meta/commit` replay, and must not proxy to Shopify at runtime.
- The runtime does not execute external Function code, invoke Function WASM, run checkout/cart transform behavior, or call tax calculation callbacks. `taxAppConfigure` stores readiness metadata only, with a synthetic `TaxAppConfiguration` ID that remains stable for later reads in the same staged state.

Validation behavior:

- `validationCreate` resolves a `ShopifyFunction` from local staged/cached state or a LiveHybrid upstream hydrate before staging and stores the resolved Function ID regardless of whether input used `functionId` or `functionHandle`.
- `validationCreate` rejects missing or multiple identifiers, unresolved references, and known non-validation API types without staging.
- Omitted or explicit `null` title uses the resolved Function-derived title; explicit empty string is preserved.
- Omitted `enable` / `enabled` defaults to `false`, activation is capped at 25 active validations, and validation-owned metafields persist for downstream reads.
- `validationUpdate` rejects Function rebinding input shape before resolver execution because public `ValidationUpdateInput` does not expose `functionId` or `functionHandle`. Inline literals return Shopify's captured top-level `argumentLiteralsIncompatible` field-not-defined error, while variable-bound input returns top-level `INVALID_VARIABLE`; neither branch produces mutation-scoped `userErrors`.
- Valid `validationUpdate` input applies the same active-validation cap and upserts non-empty metafield input by `(namespace, key)` while preserving unrelated rows.
- Invalid validation metafields reject the entire mutation before staging with index-scoped userErrors. Downstream `validation`, `validations`, `metafields(...)`, and `metafield(namespace:, key:)` reads expose persisted rows.
- `Validation` reads expose the captured public Admin 2026-04 field set: `id`, `title`, `enabled`, `blockOnFailure`, `shopifyFunction`, `errorHistory`, and HasMetafields selections. Local storage keeps Function identifiers and timestamps for lifecycle modeling, but read selections of `functionId`, `functionHandle`, `createdAt`, `updatedAt`, or input-only `enable` return Shopify-style `undefinedField` errors.

Cart transform behavior:

- `cartTransformCreate` accepts direct top-level `functionId` / `functionHandle`, `blockOnFailure`, and `metafields` arguments.
- The proxy rejects non-Shopify argument shapes such as a `cartTransform: { ... }` wrapper or `title` argument as GraphQL validation errors before staging.
- Valid create input resolves the referenced Function from local staged/cached state or a LiveHybrid upstream hydrate, persists cart-transform metadata locally, and stores valid metafields in input order.
- Missing metafield values and malformed JSON values return `INVALID_METAFIELDS` userErrors without staging.
- Truly unresolved Function identifiers return `FUNCTION_NOT_FOUND` without staging: `functionId` uses Shopify's current-app message, while `functionHandle` returns `Could not find function with handle: <handle>.`.
- Known non-cart-transform Function references return the captured identifier-specific branch: `FUNCTION_NOT_FOUND` on `functionId`, `FUNCTION_DOES_NOT_IMPLEMENT` on `functionHandle`. For `functionId`, an already-staged Function instance takes precedence before API-type validation and returns `FUNCTION_ALREADY_REGISTERED`, including when the Function is already bound to a validation.
- `cartTransformDelete` checks ownership against the staged transform's resolved `ShopifyFunction` owner and current app installation when that metadata exists. Missing installation, Function, or owner metadata returns `UNAUTHORIZED_APP_SCOPE`.
- Cart-transform metafield IDs are derived from the owner ID, namespace, and key; `compareDigest` is derived from the current metafield value. A digest read from a staged cart transform can be used with `metafieldsSet` for optimistic-concurrency updates on the same owner/key.
- `CartTransform` reads expose the modeled Admin field set: `id`, `functionId`, `blockOnFailure`, `shopifyFunction`, `errorHistory`, and HasMetafields selections. Fabricated scalar fields such as `title`, `functionHandle`, `createdAt`, or `updatedAt` are not projected.

Fulfillment constraint rule behavior:

- `fulfillmentConstraintRuleCreate` accepts direct top-level `functionId` or `functionHandle`, `deliveryMethodTypes`, and optional `metafields`.
- Valid create input resolves a fulfillment-constraint `ShopifyFunction`, stages a local `FulfillmentConstraintRule` with `id`, `function`, `deliveryMethodTypes`, and HasMetafields data, and appends the original raw request for commit replay.
- Create rejects missing identifiers, multiple identifiers, unknown Functions, known wrong API types, and empty `deliveryMethodTypes` with mutation-scoped `userErrors` and no staged rule.
- `fulfillmentConstraintRuleUpdate` updates staged rule `deliveryMethodTypes` and returns the updated rule. Unknown IDs and empty `deliveryMethodTypes` return userErrors without staging a replacement.
- `fulfillmentConstraintRuleDelete` removes the staged rule and returns `{ success: true, userErrors: [] }`. Unknown IDs return `{ success: false, userErrors: [...] }`; the payload does not use `deletedId`.
- `fulfillmentConstraintRules` reads from the effective base-plus-staged rule catalog. In LiveHybrid mode, a local fulfillment-constraint overlay hydrates the upstream rule list before rendering, keeps unrelated upstream rules visible, appends staged creates in local insertion order, and treats staged tombstones as authoritative. The captured 2026-04 schema exposes no singular `fulfillmentConstraintRule(id:)` query root; direct lookup is available through generic `node(id:)` for locally known rule IDs.
- `FulfillmentConstraintRule` reads expose `id`, `function`, `deliveryMethodTypes`, and HasMetafields selections. The local model stores Function identifiers and metadata on the rule for downstream `function` projections, but read selections of `functionId`, `functionHandle`, or `shopifyFunction` return Shopify-style `undefinedField` errors.
- The local model does not execute fulfillment-constraint Functions or model checkout/order-routing runtime decisions.

Function catalog and hydration:

- `shopifyFunction` and `shopifyFunctions` preserve Function identity, API type, `appKey`, selected `app` fields, and a handle when it is known from staged input or request-originated hydration metadata.
- LiveHybrid connection observations are scoped by root, caller app identity, filters, sort/reverse arguments, pagination direction, and the exact requested window. Observing one first page does not mark another scope or the whole catalog complete. Scoped observations and opaque cursors round-trip through dump/restore; reset clears request caches without discarding restored base observations.
- When a `validations` or `cartTransforms` read intersects staged state, the proxy overlays only the caller's upstream window. Staged updates merge into matching rows, staged creates enter the requested sort/window when applicable, and tombstones remove matching rows. If the caller omitted row IDs or a tombstone/order-affecting row could under-fill the page, the proxy issues at most one additional bounded read. It requests the window plus relevant staged deltas and one boundary row when that fits Shopify's 250-row limit; a maximum-size window instead requests only the small tail after the caller's opaque boundary cursor.
- Upstream edge cursors remain opaque. When a page boundary is a locally generated cursor, the next bounded read resumes from a neighboring observed Shopify cursor and compares modeled sort fields; it does not decode or synthesize an upstream cursor.
- Bounded refills reuse the caller's selected node fields and nested arguments. A narrow `metafield(namespace:, key:)` read therefore does not page every metafield on the owner, and partial observations deep-merge with previously observed owner data instead of treating omitted fields as empty.
- `fulfillmentConstraintRules` has no connection arguments in the captured Admin schema. LiveHybrid forwards that caller read once and overlays staged rules and tombstones on the returned list rather than issuing a fixed-prefix catalog hydrate. If a local overlay exists and the caller omitted `id`, one same-selection list read adds only the identity needed to apply it.
- Singular `validation(id:)` and `shopifyFunction(id:)` reads remain targeted. Generic `node(id:)` forwards one cold lookup, while `nodes(ids:)` batches cold Function-backed IDs in one caller document and preserves input ordering and null placeholders.
- Local Function catalog reads and mutation resolution filter owner metadata against `x-shopify-draft-proxy-api-client-id` when the caller supplies it. With no app-identity header, the proxy uses its synthetic local app identity for not-found messaging.
- Unknown Function references in supported create mutations are not satisfied from a baked catalog. Outside snapshot mode, the handler attempts the production upstream Function hydrate path; unresolved hydrate responses return Shopify-shaped not-found or wrong-API userErrors without staging.
- `validationCreate` with omitted or explicit `null` `title` falls back to the hydrated public Function title available from Admin `shopifyFunctions`; explicit `title: ""` remains empty. Live Shopify can persist a private raw extension name instead (`t:name` in the conformance app), but that value is not exposed by the exact Admin Function hydrate response available to the proxy.

Tax app readiness:

- `taxAppConfigure` requires the local request to declare both `write_taxes` in `x-shopify-draft-proxy-access-scopes` and `x-shopify-draft-proxy-tax-calculations-app: true`. Requests without that posture return Shopify's captured top-level `ACCESS_DENIED` envelope and do not stage tax readiness.
- Eligible requests allocate a synthetic `TaxAppConfiguration` ID through the proxy synthetic ID allocator, persist `ready`, derived `state`, and `updatedAt` in staged state, and reuse the same configuration ID on later readiness updates.
- Staged tax configuration is readable through generic `node(id:)` selections for `TaxAppConfiguration` fields and round-trips through dump/restore with the rest of staged state.

### Boundaries

- Function execution outcomes, checkout validation behavior, cart transform runtime effects, and tax calculation callbacks are out of scope.
- Function catalog reads prove Admin metadata shape only; they do not prove that the corresponding extension code can run.
- Cross-app Function reference behavior is limited to the owner metadata available in local/captured state.
- Private Function extension manifest fields that are not exposed through Admin GraphQL, such as the raw extension name used by Shopify's validation title fallback in the conformance app, are not modeled from fabricated local metadata.
- Function create guardrails that depend on private Shopify shop/app eligibility state are not modeled from fabricated local catalog rows. When Shopify exposes such a branch only through live mutation behavior, it needs captured live parity before the proxy should claim support for it.
- Tax-app authority is modeled from explicit local request metadata only; the proxy does not infer real app-extension eligibility or call a tax service.
- Fulfillment constraint success-path live parity requires a released `FULFILLMENT_CONSTRAINT_RULE` Function in the conformance app. The checked-in live fixture covers deterministic validation branches and empty reads because the current conformance app has no released fulfillment-constraint Function.
- No root listed here is registry-only. Validation-only behavior is limited to captured input coercion, userErrors, and guardrails that fail before local staging.
