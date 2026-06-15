---
title: 'Functions'
description: 'Coverage notes and fidelity boundaries for Functions.'
---

This endpoint group covers Shopify Function-backed Admin metadata roots for validations, cart transforms, Shopify Function catalog reads, and tax app readiness.

## Current support and limitations

### Supported roots

Read roots:

- `validation(id:)`
- `validations(...)`
- `cartTransforms(...)`
- `shopifyFunction(id:)`
- `shopifyFunctions(...)`

Mutation roots:

- `validationCreate`
- `validationUpdate`
- `validationDelete`
- `cartTransformCreate`
- `cartTransformDelete`
- `taxAppConfigure`

### Local behavior

Function-backed behavior is modeled as Admin metadata/state only:

- The proxy records Function handles or IDs attached to validations and cart transforms, preserves or hydrates local `ShopifyFunction` rows, and updates relevant detail/catalog roots after staged writes.
- Supported mutation roots stage locally, append the original raw GraphQL request body to the mutation log for ordered `__meta/commit` replay, and must not proxy to Shopify at runtime.
- The runtime does not execute external Function code, invoke Function WASM, run checkout/cart transform behavior, or call tax calculation callbacks. `taxAppConfigure` stores readiness metadata only.

Validation behavior:

- `validationCreate` resolves a `ShopifyFunction` from local state or cassette-backed hydration before staging and stores the resolved Function ID regardless of whether input used `functionId` or `functionHandle`.
- `validationCreate` rejects missing or multiple identifiers, unresolved references, and known non-validation API types without staging.
- Omitted or explicit `null` title uses the resolved Function-derived title; explicit empty string is preserved.
- Omitted `enable` / `enabled` defaults to `false`, activation is capped at 25 active validations, and validation-owned metafields persist for downstream reads.
- `validationUpdate` rejects Function rebinding input shape, applies the same active-validation cap, and upserts non-empty metafield input by `(namespace, key)` while preserving unrelated rows.
- Invalid validation metafields reject the entire mutation before staging with index-scoped userErrors. Downstream `validation`, `validations`, `metafields(...)`, and `metafield(namespace:, key:)` reads expose persisted rows.

Cart transform behavior:

- `cartTransformCreate` accepts direct top-level `functionId` / `functionHandle`, `blockOnFailure`, and `metafields` arguments.
- The proxy rejects non-Shopify argument shapes such as a `cartTransform: { ... }` wrapper or `title` argument as GraphQL validation errors before staging.
- Valid create input resolves the referenced Function, persists cart-transform metadata locally, and stores valid metafields in input order.
- Missing metafield values and malformed JSON values return `INVALID_METAFIELDS` userErrors without staging.
- Known non-cart-transform Function references return the captured identifier-specific branch: `FUNCTION_NOT_FOUND` on `functionId`, `FUNCTION_DOES_NOT_IMPLEMENT` on `functionHandle`. For `functionId`, an already-staged Function instance takes precedence before API-type validation and returns `FUNCTION_ALREADY_REGISTERED`, including when the Function is already bound to a validation.
- `cartTransformDelete` checks ownership against the staged transform's resolved `ShopifyFunction` owner and current app installation when that metadata exists. Missing installation, Function, or owner metadata returns `UNAUTHORIZED_APP_SCOPE`.
- `CartTransform` reads expose the modeled Admin field set: `id`, `functionId`, `blockOnFailure`, `shopifyFunction`, `errorHistory`, and HasMetafields selections. Fabricated scalar fields such as `title`, `functionHandle`, `createdAt`, or `updatedAt` are not projected.

Function catalog and guardrails:

- `shopifyFunction` and `shopifyFunctions` preserve captured Function identity, handle, API type, `appKey`, and selected `app` fields from seeded or hydrated metadata.
- Function create guardrail metadata can locally return `FUNCTION_PENDING_DELETION`, `FUNCTION_IS_PLUS_ONLY`, `REQUIRED_INPUT_FIELD`, and `CUSTOM_APP_FUNCTION_NOT_ELIGIBLE` before staging when the needed Function/shop/app state is known.
- If neither local state nor an upstream/cassette response supplies shop plan metadata, the proxy does not guess plan eligibility and keeps the normal create path.

### Boundaries

- Function execution outcomes, checkout validation behavior, cart transform runtime effects, and tax calculation callbacks are out of scope.
- Function catalog reads prove Admin metadata shape only; they do not prove that the corresponding extension code can run.
- Cross-app Function reference behavior is limited to the owner metadata available in local/captured state.
- Tax-app authority and real tax-service readiness are not emulated beyond local readiness metadata.
- No root listed here is registry-only. Validation-only behavior is limited to captured input coercion, userErrors, and guardrails that fail before local staging.

### Evidence

- `fixtures/conformance/local-runtime/2026-04/functions/functions-metadata-flow.json`
- `fixtures/conformance/local-runtime/2026-04/functions/functions-owner-metadata-flow.json`
- `fixtures/conformance/local-runtime/2026-04/functions/functions-create-guardrails.json`
- `fixtures/conformance/local-runtime/2026-04/functions/functions-validation-create-validation.json`
- `fixtures/conformance/local-runtime/2026-04/functions/functions-validation-update-shape.json`
- `fixtures/conformance/local-runtime/2026-04/functions/functions-validation-max-cap.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-live-owner-metadata-read.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-validation-create-error-shape.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/validation-create-title-fallback-parity.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-validation-metafields-input-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-validation-update-metafields-upsert.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-cart-transform-create-api-mismatch-by-identifier.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-cart-transform-create-registered-wrong-api-precedence.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-cart-transform-create-metafields.json`
- `config/parity-specs/functions/functions-metadata-local-staging.json`
- `config/parity-specs/functions/functions-owner-metadata-local-staging.json`
- `config/parity-specs/functions/functions-create-guardrails.json`
- `config/parity-specs/functions/functions-validation-create-validation.json`
- `config/parity-specs/functions/functions-validation-update-shape.json`
- `config/parity-specs/functions/functions-validation-max-cap.json`
- `config/parity-specs/functions/functions-live-owner-metadata-read.json`
- `config/parity-specs/functions/functions-validation-create-error-shape.json`
- `config/parity-specs/functions/validation-create-title-fallback-parity.json`
- `config/parity-specs/functions/functions-validation-metafields-input-validation.json`
- `config/parity-specs/functions/functions-validation-update-metafields-upsert.json`
- `config/parity-specs/functions/functions-cart-transform-create-api-mismatch-by-identifier.json`
- `config/parity-specs/functions/functions-cart-transform-create-registered-wrong-api-precedence.json`
- `config/parity-specs/functions/functions-cart-transform-create-metafields.json`
- `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`

### Validation

- `corepack pnpm parity -- functions-metadata-local-staging`
- `corepack pnpm parity -- functions-validation-create-validation`
- `corepack pnpm parity -- functions-cart-transform-create-metafields`
- `corepack pnpm conformance:check`
