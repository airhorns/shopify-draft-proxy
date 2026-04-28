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

These reads are registry-only gaps for now. They are not marked implemented and do not route through a local app model yet.

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

These mutation roots are registered as local-staging gaps, but none are implemented. Runtime requests still use the unsupported mutation escape hatch and would be proxied to Shopify. The mutation log records `registeredOperation` metadata and `unsupported-app-billing-access-mutation` safety metadata so billing/access passthrough is visible.

Do not mark these roots supported until the proxy can stage the full lifecycle locally, expose downstream read-after-write effects, and preserve the original raw mutations for ordered `__meta/commit` replay. Validation-only captures or branch-only handling are not enough support for these roots.

### Safety notes

- Billing create/update/cancel roots can create confirmation URLs, alter subscription state, change capped usage amounts, or create usage charges.
- `appRevokeAccessScopes` can alter the app's current access grant.
- `appUninstall` can remove the app installation from the store.
- Delegated-access roots can create or destroy credentials whose effects are authorization-sensitive.

Until local staging exists, any live capture work must avoid these mutation roots unless the ticket explicitly requires side-effect capture and the store/app are disposable for that purpose.

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

- `tests/integration/proxy-capability-classification.test.ts`
- `tests/unit/app-billing-conformance-fixture.test.ts`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
