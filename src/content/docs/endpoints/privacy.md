---
title: 'Privacy Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Privacy Endpoint Group.'
---

The privacy group covers shop-level privacy settings, consent policy configuration, and the customer data-sale opt-out mutation. It does not cover customer marketing consent or legal shop policy body content.

## Current support and limitations

`src/operation_registry.rs` currently keeps all privacy roots conservative with `implemented: false`, and `src/proxy.rs` does not currently dispatch a Rust privacy-domain local handler. Checked-in parity specs and fixtures remain useful evidence for the intended `dataSaleOptOut` behavior, but they do not make the root supported by the current Rust runtime until a local dispatcher is present.

### Registry-only roots

Planned overlay reads:

- `privacySettings`
- `consentPolicy`
- `consentPolicyRegions`

Planned local staged mutations:

- `consentPolicyUpdate`
- `privacyFeaturesDisable`

All five roots are `implemented: false` until captured fixtures, parity specs, and runtime behavior exist. Registry presence is a local-model commitment only; it does not make either privacy mutation a supported runtime operation.

### Validation-only evidence

Parity-backed but not currently supported by Rust runtime dispatch:

- `dataSaleOptOut`

`dataSaleOptOut` is tracked separately because it is a customer email data-sale opt-out mutation, not a shop privacy settings read and not the unsupported consent-policy update flow. The current docs keep its captured behavior visible as validation evidence, but do not mark it as supported operation handling while both the registry snapshot and Rust dispatcher remain conservative.

The customer data-erasure request/cancel roots are documented under the customers endpoint group:

- `customerRequestDataErasure`
- `customerCancelDataErasure`

These are customer privacy side-effect roots. The local runtime stages request/cancel intents for known normalized customers and keeps raw mutations for commit replay. Granted-scope capture records real request/cancel success payloads, unchanged immediate downstream customer reads, unknown-customer `DOES_NOT_EXIST` userErrors, and repeat-cancel `NOT_BEING_ERASED` cleanup behavior.

### Coverage boundaries

- `privacySettings` returns shop privacy settings such as cookie banner, data sale opt-out page, and privacy policy settings. Shopify documents the `PrivacySettings` object as requiring the `read_privacy_settings` access scope.
- `consentPolicy` and `consentPolicyRegions` are shop consent policy reads. They are separate from customer contact consent fields.
- `consentPolicyUpdate` and `privacyFeaturesDisable` require `write_privacy_settings` and have real shop side effects. They must remain unsupported at runtime until the proxy can stage the changes locally and replay the original raw mutations during commit.
- `dataSaleOptOut` requires `write_privacy_settings`, accepts a customer email address, and returns `customerId` plus `userErrors`. Captured evidence shows successful opt-out sets downstream `Customer.dataSaleOptOut` to `true`, repeat calls are idempotent, invalid email strings return `customerId: null` with `code: FAILED`, and an unknown valid email creates an opted-out customer.
- `dataSaleOptOut.email` is a non-null `String!` schema argument. Missing inline `email` and explicit null variable values are rejected by GraphQL coercion before the resolver runs, so the proxy returns a top-level `errors` envelope with no `data.dataSaleOptOut` payload. This differs from `email: ""`, which reaches the resolver and returns the captured `FAILED` userError shape.
- `dataSaleOptOut` sanitizes the email before validation by stripping the whitespace characters observed in live Admin API capture: internal spaces and newlines are removed. For example, `her mes@example.com` is validated and staged as `hermes@example.com`; if stripping leaves an empty value or the remaining value does not match the email validator approximation, the mutation returns the captured `FAILED` userError and stages nothing. Live 2025-01 capture rejected tab characters with the same `FAILED` shape, so the proxy deliberately preserves that boundary instead of treating tabs as removable whitespace.
- `dataSaleOptOut` is related to, but distinct from, shop privacy settings fields such as `privacySettings.dataSaleOptOutPage` and consent policy fields such as `dataSaleOptOutRequired`: those reads describe shop configuration and policy requirements, while the mutation records an opt-out action for a customer email.
- The intended local staging model keeps the operation under the privacy registry domain, but stores its read-after-write effect on the normalized customer record so `customer(id:)` and `customerByIdentifier(...)` can serialize `dataSaleOptOut`.
- In cassette-backed `LiveHybrid` parity evidence, existing-email `dataSaleOptOut` first reads `customerByIdentifier(identifier: { emailAddress })` from upstream to learn Shopify's authoritative customer ID, then stages the opt-out locally without sending the mutation upstream. That evidence is not a current Rust support claim until the dispatcher path is present.
- Customer email/SMS marketing consent is tracked under the customers endpoint group through `customerEmailMarketingConsentUpdate` and `customerSmsMarketingConsentUpdate`.
- Executable parity evidence covers existing-customer opt-out, repeat idempotency, invalid-email userErrors, downstream customer reads, and fresh-customer defaults. Shop-level privacy settings roots remain the deliberate fidelity gap for this endpoint group and must stay unsupported until local shop privacy state, side-effect boundaries, and commit replay are modeled.
- Legal policy body updates are tracked under store properties through `shopPolicyUpdate`.
- `dataSaleOptOut` is present as a mutation root in the checked-in 2025-01 root introspection fixture. Fixture-backed parity coverage lives in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/privacy/data-sale-opt-out-parity.json`.

### Evidence and capture guidance

The live capture entry point is `corepack pnpm conformance:capture-privacy`, backed by `scripts/capture-privacy-conformance.ts`.

The script uses the canonical conformance auth helper and defaults to Admin GraphQL `2026-04`. By default it captures only safe reads. Mutation capture is guarded by `SHOPIFY_CONFORMANCE_CAPTURE_PRIVACY_MUTATIONS=true` and requires explicit JSON inputs:

- `SHOPIFY_CONFORMANCE_PRIVACY_CONSENT_POLICIES_JSON` for `consentPolicyUpdate`
- `SHOPIFY_CONFORMANCE_PRIVACY_FEATURES_TO_DISABLE_JSON` for `privacyFeaturesDisable`

`dataSaleOptOut` has a dedicated capture entry point: `corepack pnpm conformance:capture-data-sale-opt-out`, backed by `scripts/capture-data-sale-opt-out-conformance.ts`. It creates disposable customers, captures existing-email opt-out, repeat idempotency, invalid-email userErrors, unknown-email customer creation, fresh-customer defaults, internally-whitespaced email sanitization, downstream `Customer.dataSaleOptOut` reads, and cleanup.

Do not check in planned-only parity specs or parity request placeholders for this group. Add parity specs only after live capture produces fixture evidence and a strict comparison contract can run.

### Validation anchors

- Fixture-backed parity scenario: `config/parity-specs/privacy/dataSaleOptOut-parity.json`
- Fixture-backed whitespace sanitization scenario: `config/parity-specs/privacy/data-sale-opt-out-whitespace-email.json`
- Fixture-backed missing-email coercion scenario: `config/parity-specs/privacy/data-sale-opt-out-missing-email.json`
- General registry checks: `tests/unit/operation-registry.test.ts`
- Root inventory fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`
