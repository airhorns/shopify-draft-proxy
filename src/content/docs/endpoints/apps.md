---
title: 'Apps, Billing, And Access'
description: 'Coverage notes and fidelity boundaries for Apps, Billing, And Access.'
---

This endpoint group covers Shopify Admin GraphQL app identity, app installation, app billing, access-scope, uninstall, and delegated-access roots. The mutation roots are sensitive because they can affect billing, app grants, app installation state, or delegated credentials in real Shopify.

## Current support and limitations

### Supported roots

Read roots:

- `app(id:)`
- `appByHandle(handle:)`
- `appByKey(apiKey:)`
- `appInstallation(id:)`
- `appInstallations(...)`
- `currentAppInstallation`

Mutation roots:

- `appPurchaseOneTimeCreate`
- `appSubscriptionCreate`
- `appSubscriptionCancel`
- `appSubscriptionLineItemUpdate`
- `appSubscriptionTrialExtend`
- `appUsageRecordCreate`
- `appRevokeAccessScopes`
- `appUninstall`
- `delegateAccessTokenCreate`
- `delegateAccessTokenDestroy`

### Local behavior

The app domain uses normalized local buckets for app identity, current app installation, access scopes, app subscriptions, subscription line items, one-time purchases, usage records, and delegated-token metadata.

Read behavior:

- `currentAppInstallation`, `appInstallation(id:)`, `app(id:)`, `appByHandle(handle:)`, and `appByKey(apiKey:)` project the local app model after staged app mutations or LiveHybrid hydration from an upstream app installation read.
- Snapshot reads return Shopify-like `null` or empty values when no app installation has been staged or hydrated.
- `appInstallations(first:)` serializes the current staged/hydrated installation as a connection for local read-after-write checks. Authorized multi-installation catalog behavior remains outside current local support.
- Generic `node(id:)` / `nodes(ids:)` resolves app-domain records already present in local state: `App`, `AppInstallation`, `AppSubscription`, `AppPurchaseOneTime`, and `AppUsageRecord`. Missing IDs return `null`. `AppSubscriptionLineItem` remains nested under `AppSubscription.lineItems` and is not claimed as a standalone Node implementor.

Supported mutations stage locally, append the original raw mutation request to the meta log for ordered commit replay, and synthesize Shopify-like payloads without sending runtime writes to Shopify.

Billing behavior:

- Billing payloads serialize `MoneyV2.amount` strings with Shopify Decimal-style formatting: whole numbers keep one trailing zero and fractional amounts drop superfluous trailing zeros. The same normalized staged values are read back through `currentAppInstallation`, `node(id:)`, and `nodes(ids:)` app-domain reads.
- `appPurchaseOneTimeCreate` stages a pending one-time purchase and returns a synthetic local confirmation URL. Local validation covers missing/blank `returnUrl`, blank trimmed names, prices below the local minimum, and shop billing currency mismatches.
- `appSubscriptionCreate` stages a pending subscription, usage/recurring line-item pricing details, trial days, and a synthetic local confirmation URL.
- `appSubscriptionCancel` stages cancellation only for cancellable subscription statuses. Non-cancellable and unknown subscriptions return Shopify-shaped userErrors without mutating local state.
- `appSubscriptionLineItemUpdate` validates usage-pricing line items, capped amount currency, increasing cap values, and approval behavior. Approval-required updates return a confirmation URL and keep downstream active line-item caps unchanged; internal/test callers can use the synchronous no-approval branch when explicitly modeled.
- `appSubscriptionTrialExtend` validates the supported day range, subscription existence, active status, and active trial window before mutating `trialDays`.
- `appUsageRecordCreate` stages usage records for usage-pricing line items, enforces idempotency-key length, currency compatibility, capped amount limits, and idempotent reuse for repeated keys.

Access and uninstall behavior:

- `appRevokeAccessScopes` removes locally granted optional scopes from the current app installation. Unknown, non-granted, required, and implied scopes return Shopify-shaped userErrors without partial revocation. Local-runtime missing-source-app coverage is modeled through request-owned source-app context rather than the GraphQL operation name.
- The `appRevokeAccessScopes` missing-source-app branch is Core-source-derived, not real-Shopify-recorded: valid public Admin requests carry source app context, while unauthenticated requests fail before the mutation resolver. Local replay uses `x-shopify-draft-proxy-source-app-missing` and returns `MISSING_SOURCE_APP` on `["id"]` with `No app found on the access token.`.
- `appUninstall` resolves the target app from `input.id` or the current installation, enforces current-installation visibility and the `apps` scope where needed, marks the target installation uninstalled on success, clears its active access grant, cancels locally staged active/pending subscriptions, and destroys stored delegated tokens.
- Downstream `currentAppInstallation` reads return `null` when the current installation is uninstalled, and app-subscription Node reads show cancelled status for locally cancelled subscriptions.

Delegated-token behavior:

- `delegateAccessTokenCreate` accepts the current `delegateAccessScope` list input, returns validated scope handles through payload `accessScopes`, and stores only token hash, redacted preview, owning app ID, and parent-token hash in meta-visible state.
- Empty scopes, non-positive `expiresIn`, unknown scope handles, active delegate-token parents, and delegated expiry beyond the parent token return Shopify-like userErrors without staging.
- Request-owned parent-token context is modeled with `x-shopify-draft-proxy-access-token-expires-at`; permanent parent tokens omit that header.
- `delegateAccessTokenDestroy` hashes the raw token, checks app ownership and parent/delegate hierarchy, marks allowed delegate tokens destroyed locally, and returns non-null shop payloads on success and user-error branches.
- Unknown/repeated tokens, parent self-destroy, cross-app, and non-parent hierarchy attempts return captured userErrors and leave token state unchanged.

Synthetic confirmation URLs use `signature=shopify-draft-proxy-local-redacted`. Delegated token raw values are returned only in mutation payloads and are intentionally absent from `__meta/state`.

### Boundaries

- The proxy does not perform real billing, merchant approval, app uninstall, app grant changes, or delegated-token changes during normal runtime.
- Billing approval, charge activation, subscription proration, usage-charge billing, app-plan enforcement, and Shopify Core chargeability guards are not emulated beyond local staged state and validation evidence.
- Live success-path captures for billing approval, uninstall, and app grant revocation require explicitly approved disposable credentials. Billing mutation success captures specifically require a disposable billing-capable app/store credential that can use Shopify's Billing API and safely approve test charges; the current custom-app conformance credential cannot exercise those success paths.
- Authorized multi-installation catalog behavior for `appInstallations(...)` remains unsupported without a suitable live grant.
- No listed app root is registry-only. Validation-only behavior is limited to guardrails that reject before staging and to local-runtime evidence for branches that cannot be exercised safely against the current disposable app credential.

### Evidence

- `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/apps/app-billing-access-read.json`
- `fixtures/conformance/local-runtime/2026-04/apps/app-billing-access-local-staging.json`
- `fixtures/conformance/local-runtime/2026-04/apps/app-purchase-one-time-create-validation.json`
- `fixtures/conformance/local-runtime/2026-04/apps/app-usage-record-create-cap-and-idempotency.json`
- `fixtures/conformance/local-runtime/2026-04/apps/app-subscription-line-item-update-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/apps/delegate-access-token-create-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/apps/delegate-access-token-create-expires-after-parent.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/apps/delegate-access-token-destroy-codes.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/apps/delegate-access-token-shop-payload.json`
- `config/parity-specs/apps/app-billing-access-local-staging.json`
- `config/parity-specs/apps/app-purchase-one-time-create-validation.json`
- `config/parity-specs/apps/app-usage-record-create-cap-and-idempotency.json`
- `config/parity-specs/apps/app-subscription-line-item-update-validation.json`
- `config/parity-specs/apps/app-revoke-access-scopes-error-codes.json`
- `config/parity-specs/apps/app-uninstall-error-codes-and-cascade.json`
- `config/parity-specs/apps/app-subscription-cancel-status-transitions.json`
- `config/parity-specs/apps/app-subscription-trial-extend-validation.json`
- `config/parity-specs/apps/delegate-access-token-current-input-local-staging.json`
- `config/parity-specs/apps/delegate-access-token-create-validation.json`
- `config/parity-specs/apps/delegate-access-token-create-expires-after-parent.json`
- `config/parity-specs/apps/delegate-access-token-destroy-codes.json`
- `config/parity-specs/apps/delegate-access-token-shop-payload.json`
- `tests/unit/app-billing-conformance-fixture.test.ts`

### Validation

- `corepack pnpm parity -- app-billing-access-local-staging`
- `corepack pnpm parity -- app-usage-record-create-cap-and-idempotency`
- `corepack pnpm parity -- app-subscription-line-item-update-validation`
- `corepack pnpm parity -- delegate-access-token-current-input-local-staging`
- `corepack pnpm parity -- delegate-access-token-destroy-codes`
- `corepack pnpm conformance:check`
