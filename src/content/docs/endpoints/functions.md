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
- In LiveHybrid mode, cold `shopifyFunction` / `shopifyFunctions` reads forward upstream and cache observed Function metadata for later local read-after-write behavior. Snapshot mode does not invent Function catalog rows that are absent from local state.
- Supported mutation roots stage locally, append the original raw GraphQL request body to the mutation log for ordered `__meta/commit` replay, and must not proxy to Shopify at runtime.
- The runtime does not execute external Function code, invoke Function WASM, run checkout/cart transform behavior, or call tax calculation callbacks. `taxAppConfigure` stores readiness metadata only, with a synthetic `TaxAppConfiguration` ID that remains stable for later reads in the same staged state.

Validation behavior:

- `validationCreate` resolves a `ShopifyFunction` from local staged/cached state or a LiveHybrid upstream hydrate before staging and stores the resolved Function ID regardless of whether input used `functionId` or `functionHandle`.
- `validationCreate` rejects missing or multiple identifiers, unresolved references, and known non-validation API types without staging.
- Before an enabled create or update applies the shop-wide active-validation cap, LiveHybrid scans only `id`, `enabled`, and `shopifyFunction.id` in pages of 250. It stops as soon as 25 effective active rows prove the cap or the connection ends, caches those fields as private decision facts, and does not mark or expose a complete validation catalog. Caller-selected partial validation reads likewise do not make that lifecycle catalog authoritative. If the decision read fails, the mutation fails closed without staging rather than treating partial state as complete. The captured 26th-active payload uses `field: null`, code `MAX_VALIDATIONS_ACTIVATED`, and message `Cannot have more than 25 active validation functions.`.
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
- LiveHybrid resolves the requested Function exactly, reuses cached validation decision facts, and scans further validation decision pages only when a wrong-API Function needs the captured registration-precedence check. A correct cart-transform Function uses `cartTransforms(first: 1) { nodes { id functionId } }` to prove reuse or shop-wide presence without loading transform objects. A registered Function therefore returns `FUNCTION_ALREADY_REGISTERED` even when its lifecycle row had not previously been observed. An unavailable decision read fails closed without staging. With one different cart transform already present, Shopify's captured second-transform payload has null field/code and message `An API client cannot have more than 1 cart transform functions per shop`.
- `cartTransformDelete` checks ownership against the staged transform's resolved `ShopifyFunction` owner and current app installation when that metadata exists. Missing installation, Function, or owner metadata returns `UNAUTHORIZED_APP_SCOPE`.
- Cart-transform metafield IDs are derived from the owner ID, namespace, and key; `compareDigest` is derived from the current metafield value. A digest read from a staged cart transform can be used with `metafieldsSet` for optimistic-concurrency updates on the same owner/key.
- `CartTransform` reads expose the modeled Admin field set: `id`, `functionId`, `blockOnFailure`, `shopifyFunction`, `errorHistory`, and HasMetafields selections. Fabricated scalar fields such as `title`, `functionHandle`, `createdAt`, or `updatedAt` are not projected.

Fulfillment constraint rule behavior:

- `fulfillmentConstraintRuleCreate` accepts direct top-level `functionId` or `functionHandle`, `deliveryMethodTypes`, and optional `metafields`.
- Valid create input resolves a fulfillment-constraint `ShopifyFunction`, stages a local `FulfillmentConstraintRule` with `id`, `function`, `deliveryMethodTypes`, and HasMetafields data, and appends the original raw request for commit replay.
- Create rejects missing identifiers, multiple identifiers, unknown Functions, known wrong API types, and empty `deliveryMethodTypes` with mutation-scoped `userErrors` and no staged rule.
- `fulfillmentConstraintRuleUpdate` resolves an unknown local target through an exact read-only LiveHybrid `node(id:)` lookup, updates effective base or staged `deliveryMethodTypes`, and returns the updated rule. Only an authoritative miss returns `NOT_FOUND`; empty `deliveryMethodTypes` still returns its input userError without staging a replacement.
- `fulfillmentConstraintRuleDelete` uses the same exact target hydrate before deciding existence, tombstones effective base or staged rules, and returns `{ success: true, userErrors: [] }`. An authoritative unknown ID returns `{ success: false, userErrors: [...] }`; a failed target read returns a distinct null-code hydration error rather than a false `NOT_FOUND`. The payload does not use `deletedId`.
- `fulfillmentConstraintRules` reads from the effective base-plus-staged rule catalog. In LiveHybrid mode, a local fulfillment-constraint overlay hydrates the upstream rule list before rendering, keeps unrelated upstream rules visible, appends staged creates in local insertion order, and treats staged tombstones as authoritative. The captured 2026-04 schema exposes no singular `fulfillmentConstraintRule(id:)` query root; direct lookup is available through generic `node(id:)` for locally known rule IDs.
- `FulfillmentConstraintRule` reads expose `id`, `function`, `deliveryMethodTypes`, and HasMetafields selections. The local model stores Function identifiers and metadata on the rule for downstream `function` projections, but read selections of `functionId`, `functionHandle`, or `shopifyFunction` return Shopify-style `undefinedField` errors.
- The local model does not execute fulfillment-constraint Functions or model checkout/order-routing runtime decisions.

Function catalog and hydration:

- `shopifyFunction` and `shopifyFunctions` preserve Function identity, API type, `appKey`, selected `app` fields, and a handle when it is known from staged input or request-originated hydration metadata.
- In LiveHybrid mode, local Function reads use an effective catalog rather than staged-only state once a lifecycle mutation has been staged. Requested validation, cart-transform, fulfillment-constraint-rule, and `shopifyFunctions` roots hydrate their upstream/base family as needed, merge those rows with staged inserts/updates, and keep staged tombstones authoritative.
- Only the proxy's dedicated complete lifecycle reads mark validation, cart-transform, or fulfillment-rule catalogs authoritative. Decision-only threshold/existence reads stay in private caches and never set those public catalog-completeness flags. A forwarded caller query with a bounded/filtered selection can contribute observed rows but never suppress the later authoritative minimal preflight required for a global decision.
- Singular `validation(id:)`, `shopifyFunction(id:)`, and generic `node` / `nodes` reads can hydrate unresolved upstream IDs outside snapshot mode. Connection filters, sorting, aliases, pagination, counts, and `pageInfo` are computed from the effective base-plus-staged catalog.
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
- Fulfillment constraint success-path parity uses the released `conformance-fulfillment-constraint` Function and covers create-backed read-only hydration followed by direct update/delete. Function execution and checkout/order-routing effects remain outside this lifecycle model.
- No root listed here is registry-only. Validation-only behavior is limited to captured input coercion, userErrors, and guardrails that fail before local staging.
