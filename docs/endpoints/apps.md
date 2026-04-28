# Apps, Billing, And Access

This endpoint group tracks Admin GraphQL app identity, app installation, app billing, access-scope, uninstall, and delegated-access roots. These roots are sensitive because several mutation roots can alter merchant billing, revoke the app grant, uninstall the app, or create delegated tokens.

## Current support and limitations

### Registered read roots

- `app(id:)`
- `appByHandle(handle:)`
- `appByKey(apiKey:)`
- `appInstallation(id:)`
- `appInstallations(...)`
- `currentAppInstallation`

`currentAppInstallation`, `appInstallation(id:)`, `app(id:)`, `appByHandle(handle:)`, and `appByKey(apiKey:)` can now project the local app model after a staged app mutation or after live-hybrid hydration from an upstream app installation read. Snapshot reads return Shopify-like null/empty values when no app installation has been staged or hydrated. `appInstallations(first:)` serializes the current staged/hydrated installation as a connection for local read-after-write checks, but authorized multi-installation catalog behavior still needs a suitable live credential.

### Registered mutation roots

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

HAR-364 implements local staging for these roots. Supported runtime requests no longer proxy to Shopify; they append the original raw mutation request to the meta log for ordered commit replay and synthesize Shopify-like payloads from in-memory state.

### Local state model

The app domain uses dedicated normalized state buckets for app identity, current app installation, access scopes, app subscriptions, subscription line items, one-time purchases, usage records, and delegated-token metadata. This keeps side-effect-heavy app behavior separate from product and shop state while preserving read-after-write consistency for app installation billing/access reads.

Billing and delegated-token mutation handlers synthesize local confirmation URLs and delegated tokens, then stage the derived records in memory without sending runtime writes to Shopify. Delegated-token raw values are intentionally not stored in meta-visible state; the proxy retains only a SHA-256 hash and redacted preview for local destroy lookup and inspection.

Live-hybrid app installation reads can hydrate this local app model from upstream read responses. Snapshot and local-only reads return Shopify-like null/empty structures when no app installation state has been staged or hydrated.

Current modeled behavior:

- `appPurchaseOneTimeCreate` stages a pending one-time purchase and returns a synthetic local confirmation URL.
- `appSubscriptionCreate` stages a pending subscription, usage/recurring line-item pricing details, trial days, and a synthetic local confirmation URL.
- `appSubscriptionCancel`, `appSubscriptionLineItemUpdate`, and `appSubscriptionTrialExtend` mutate staged subscription state and return userErrors for unknown local IDs.
- `appUsageRecordCreate` stages usage records under staged usage line items and exposes them through `AppSubscriptionLineItem.usageRecords`.
- `appRevokeAccessScopes` removes locally granted scopes from the current app installation and returns per-scope errors for requested scopes that are not locally granted.
- `appUninstall` marks the current staged/hydrated installation uninstalled; downstream `currentAppInstallation` reads return `null`.
- `delegateAccessTokenCreate` returns a synthetic delegated token once and stores only a SHA-256 hash plus redacted preview in meta-visible state.
- `delegateAccessTokenDestroy` matches the raw token against the stored hash, marks it destroyed locally, and returns `ACCESS_TOKEN_NOT_FOUND` when repeated or unknown.

The implementation does not perform real billing, merchant approval, app uninstall, app grant changes, or delegated-token changes during normal runtime.

### Safety notes

- Billing create/update/cancel roots can create confirmation URLs, alter subscription state, change capped usage amounts, or create usage charges.
- `appRevokeAccessScopes` can alter the app's current access grant.
- `appUninstall` can remove the app installation from the store.
- Delegated-access roots can create or destroy credentials whose effects are authorization-sensitive.

The local proxy uses synthetic confirmation URLs containing `signature=shopify-draft-proxy-local-redacted`; these URLs are not real Shopify approval links and should still be treated as sensitive in examples and fixtures. Delegated tokens are returned only in the mutation payload and are intentionally absent from `__meta/state`.

Live success-path captures for billing approval, uninstall, app grant revocation, and delegated-token creation/destruction remain blocked unless a disposable app/store credential is explicitly approved for those external effects. The local runtime fixture records that blocker and the integration test covers strict local behavior instead of mutating a real shop.

## Historical and developer notes

### Captured read evidence

HAR-301 captured safe read evidence in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/apps/app-billing-access-read.json` through `corepack pnpm conformance:capture-app-billing`.

The capture records:

- `currentAppInstallation` for the active conformance app, including app identity, active access scopes, `launchUrl`, nullable `uninstallUrl`, and requested app scopes.
- Billing no-data behavior for the active install: `activeSubscriptions` returns an empty list, and both `allSubscriptions(first:)` and `oneTimePurchases(first:)` return non-null empty connections with empty `nodes`/`edges`, false pageInfo booleans, and null cursors.
- `app(id:)`, `appByHandle(handle:)`, and `appByKey(apiKey:)` return the same active app object for known identity values and return `null` for unknown id/handle/key probes.
- `appInstallation(id:)` returns the active installation object for the current installation id and returns `null` for an unknown installation id.
- `appInstallations(first:)` currently returns a top-level `ACCESS_DENIED` error and `data: null` for this credential, so authorized catalog empty/non-empty behavior remains blocked until a suitable grant is available.

### Validation

- `tests/integration/app-billing-access-flow.test.ts`
- `tests/integration/proxy-capability-classification.test.ts`
- `tests/unit/app-billing-conformance-fixture.test.ts`
- `config/parity-specs/app-billing-access-local-staging.json`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
