# Privacy Endpoint Group

The privacy group started as registry-only groundwork in HAR-250. These roots describe shop-level privacy settings and consent policy configuration, not customer marketing consent and not legal shop policy content.

## Registry-only roots

HAR-250 added the initial privacy endpoint grouping for five shop privacy and consent-policy roots:

Planned overlay reads:

- `privacySettings`
- `consentPolicy`
- `consentPolicyRegions`

Planned local staged mutations:

- `consentPolicyUpdate`
- `privacyFeaturesDisable`

All five roots are `implemented: false` until captured fixtures, parity specs, and runtime behavior exist. Registry presence is a local-model commitment only; it does not make either privacy mutation a supported runtime operation.

HAR-255 adds separate customer/privacy mutation coverage for:

- `dataSaleOptOut`

`dataSaleOptOut` is `implemented: true`. It is tracked separately because it is a customer email data-sale opt-out mutation, not a shop privacy settings read and not the consent-policy update flow covered by HAR-250.

HAR-322 adds customer data-erasure request/cancel roots under the customers endpoint group:

- `customerRequestDataErasure`
- `customerCancelDataErasure`

These are customer privacy side-effect roots. The local runtime stages request/cancel intents for known normalized customers and keeps raw mutations for commit replay, but live success-path conformance capture is blocked until the conformance app/user has `write_customer_data_erasure` and the Shopify permission to erase customer data.

## Coverage boundaries

- `privacySettings` returns shop privacy settings such as cookie banner, data sale opt-out page, and privacy policy settings. Shopify documents the `PrivacySettings` object as requiring the `read_privacy_settings` access scope.
- `consentPolicy` and `consentPolicyRegions` are shop consent policy reads. They are separate from customer contact consent fields.
- `consentPolicyUpdate` and `privacyFeaturesDisable` require `write_privacy_settings` and have real shop side effects. They must remain unsupported at runtime until the proxy can stage the changes locally and replay the original raw mutations during commit.
- `dataSaleOptOut` requires `write_privacy_settings`, accepts a customer email address, and returns `customerId` plus `userErrors`. Captured evidence shows successful opt-out sets downstream `Customer.dataSaleOptOut` to `true`, repeat calls are idempotent, invalid email strings return `customerId: null` with `code: FAILED`, and an unknown valid email creates an opted-out customer.
- `dataSaleOptOut` is related to, but distinct from, shop privacy settings fields such as `privacySettings.dataSaleOptOutPage` and consent policy fields such as `dataSaleOptOutRequired`: those reads describe shop configuration and policy requirements, while the mutation records an opt-out action for a customer email.
- Local staging keeps the operation under the privacy registry domain, but stores its read-after-write effect on the normalized customer record so `customer(id:)` and `customerByIdentifier(...)` can serialize `dataSaleOptOut`.
- Customer email/SMS marketing consent is already tracked under the customers endpoint group and HAR-153 through `customerEmailMarketingConsentUpdate` and `customerSmsMarketingConsentUpdate`.
- Legal policy body updates are already tracked under store properties and HAR-173 through `shopPolicyUpdate`.
- `dataSaleOptOut` is present as a mutation root in the checked-in 2025-01 root introspection fixture. Fixture-backed parity coverage lives in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/data-sale-opt-out-parity.json`.

## Capture guidance

The live capture entry point is `corepack pnpm conformance:capture-privacy`, backed by `scripts/capture-privacy-conformance.ts`.

The script uses the canonical conformance auth helper and defaults to Admin GraphQL `2026-04` so fixture work can align with the privacy docs cited by HAR-250. By default it captures only safe reads. Mutation capture is guarded by `SHOPIFY_CONFORMANCE_CAPTURE_PRIVACY_MUTATIONS=true` and requires explicit JSON inputs:

- `SHOPIFY_CONFORMANCE_PRIVACY_CONSENT_POLICIES_JSON` for `consentPolicyUpdate`
- `SHOPIFY_CONFORMANCE_PRIVACY_FEATURES_TO_DISABLE_JSON` for `privacyFeaturesDisable`

`dataSaleOptOut` has a dedicated capture entry point: `corepack pnpm conformance:capture-data-sale-opt-out`, backed by `scripts/capture-data-sale-opt-out-conformance.ts`. It creates disposable customers, captures existing-email opt-out, repeat idempotency, invalid-email userErrors, unknown-email customer creation, downstream `Customer.dataSaleOptOut` reads, and cleanup.

Do not check in planned-only parity specs or parity request placeholders for this group. Add parity specs only after live capture produces fixture evidence and a strict comparison contract can run.

## Validation anchors

- Local data-sale opt-out staging and read-after-write behavior: `tests/integration/customer-draft-flow.test.ts`
- Privacy mutation observability and capability behavior: `tests/integration/proxy-capability-classification.test.ts`
- Fixture-backed parity scenario: `config/parity-specs/dataSaleOptOut-parity.json`
- General registry checks: `tests/unit/operation-registry.test.ts`
- Root inventory fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json`
