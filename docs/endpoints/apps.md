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

Billing and delegated-token mutation handlers synthesize local confirmation URLs and delegated tokens, then stage the derived records in memory without sending runtime writes to Shopify. Delegated-token raw values are intentionally not stored in meta-visible state; the proxy retains only a SHA-256 hash, redacted preview, owning app id, and parent access-token hash for local destroy lookup, hierarchy checks, and inspection.

Live-hybrid app installation reads can hydrate this local app model from upstream read responses. Snapshot and local-only reads return Shopify-like null/empty structures when no app installation state has been staged or hydrated.

Generic Admin `node(id:)` / `nodes(ids:)` dispatch resolves app-domain records
that already exist in the local app model: `App`, `AppInstallation`,
`AppSubscription`, `AppPurchaseOneTime`, and `AppUsageRecord`. Missing IDs
return `null`, and subscription line items remain nested under
`AppSubscription.lineItems` rather than being claimed as standalone Node
implementors because Shopify's captured Node implementor inventory does not list
`AppSubscriptionLineItem`.

Current modeled behavior:

- `appPurchaseOneTimeCreate` stages a pending one-time purchase and returns a synthetic local confirmation URL. Local validation rejects missing/blank `returnUrl`, blank trimmed `name`, prices below the local 0.50 minimum, and `price.currencyCode` values that do not match the effective shop billing currency (defaulting to `USD` when no shop record has been hydrated).
- `appSubscriptionCreate` stages a pending subscription, usage/recurring line-item pricing details, trial days, and a synthetic local confirmation URL.
- `appSubscriptionCancel` stages cancellation only for `PENDING`, `ACCEPTED`, and `ACTIVE` subscriptions. `CANCELLED`, `DECLINED`, `EXPIRED`, `FROZEN`, and other non-cancellable statuses return an `id` userError shaped like Shopify's invalid transition payload without mutating local state. Unknown subscription IDs return an `id` userError with a record-not-found message and no error code.
- `appSubscriptionLineItemUpdate` validates the subscription line-item GID shape before lookup, only accepts usage-pricing line items, rejects capped-amount currency mismatches, and requires the requested capped amount to be greater than the existing usage cap. The public Admin GraphQL 2026-04 schema exposes only `id` and `cappedAmount`; omitting the hidden/internal `requireApproval` argument follows Shopify's default approval-required branch, returns a synthetic confirmation URL, and keeps the active line item's `cappedAmount` unchanged in the payload and downstream local reads until approval/commit. Internal/test callers that pass `requireApproval: false` use the synchronous branch: the proxy stages the new `cappedAmount` immediately and returns `confirmationUrl: null`. The proxy does not currently model Shopify Core's extra `shop_accepts_charge?` guard because local app state has no shop chargeability signal. Recurring-pricing line items, malformed IDs, unknown local IDs, currency mismatches, and non-increasing caps return userErrors without mutating local state.
- `appSubscriptionTrialExtend` validates Shopify's `days` range (`1..1000`), rejects unknown IDs with `SUBSCRIPTION_NOT_FOUND`, rejects non-`ACTIVE` subscriptions with `SUBSCRIPTION_NOT_ACTIVE`, rejects expired active trials with `TRIAL_NOT_ACTIVE`, and only mutates `trialDays` for active subscriptions still inside their trial window.
- `appUsageRecordCreate` stages usage records only for usage-pricing line items. It rejects recurring line items, over-255-character idempotency keys, currency mismatches against the line item's capped amount, and charges whose proposed cumulative `balanceUsed` would exceed `cappedAmount`. Successful creates increment `AppUsagePricing.balanceUsed`, expose the staged record through `AppSubscriptionLineItem.usageRecords`, and reuse the prior staged record when the same idempotency key is repeated for the same line item.
- `appRevokeAccessScopes` removes locally granted optional scopes from the
  current app installation. Validation failures do not partially revoke any
  scope: catalog-unknown handles return `UNKNOWN_SCOPES`, catalog-known but
  non-granted handles return `CANNOT_REVOKE_UNDECLARED_SCOPES`, requested app
  scopes return `CANNOT_REVOKE_REQUIRED_SCOPES`, and read scopes implied by a
  granted write scope return `CANNOT_REVOKE_IMPLIED_SCOPES`. Missing local app
  context is surfaced as `MISSING_SOURCE_APP`, a current installation whose app
  record cannot be resolved as `APPLICATION_CANNOT_BE_FOUND`, and an app record
  without an installed current installation as `APP_NOT_INSTALLED`.
- `appUninstall` resolves `input.id` to a known app when provided, otherwise
  falls back to the current app installation. Unknown app IDs return
  `APP_NOT_FOUND` on `field: ["id"]`; known apps without a visible local
  installation return `APP_NOT_INSTALLED` on `field: ["id"]`, while omitted
  input uses `field: ["base"]`. Input IDs for an app other than the current
  installation require the current installation to hold the `apps` access
  scope; otherwise the mutation returns `INSUFFICIENT_PERMISSIONS` without
  staging. Successful uninstall marks the targeted installation uninstalled,
  clears its active access grant, cancels locally staged
  `PENDING`/`ACCEPTED`/`ACTIVE` subscriptions attached to the installation, and
  destroys stored delegated access tokens so later token-destroy calls return
  `ACCESS_TOKEN_NOT_FOUND`. Downstream `currentAppInstallation` reads return
  `null` when the current installation is targeted, and app-subscription Node
  reads show cancelled status.
- `delegateAccessTokenCreate` accepts the current `delegateAccessScope` list input, returns the validated scope handles through the payload's `accessScopes` list, and stores the owning app id plus parent access-token hash alongside the token hash/preview. Empty scope lists, non-positive `expiresIn`, catalog-unknown scope handles, active delegate-token parents, and expiring parent tokens whose delegated `expiresIn` would outlive the parent return Shopify-like user errors without staging a new token. The parent expiry check is driven by request-owned parent-token context; parity and test harnesses model that context with `x-shopify-draft-proxy-access-token-expires-at`, while permanent parent tokens omit it. The payload's non-null `shop` field is projected through the Apps current-shop helper: hydrated `store.get_effective_shop(store)` data wins, and otherwise the proxy returns a stable synthetic Shop with `id`, `myshopifyDomain`, and `currencyCode`. The older local fixture shape using `input.accessScopes` remains tolerated only when `delegateAccessScope` is absent; the broad app-billing local-runtime parity replay keeps that older request shape while `config/parity-specs/apps/delegate-access-token-current-input-local-staging.json` executes the current list-shaped `delegateAccessScope` input.
- `delegateAccessTokenDestroy` matches the raw token against the stored hash, checks the caller app id and parent/delegate token hierarchy, marks allowed delegate tokens destroyed locally, and returns a non-null `shop` payload through the Apps current-shop helper on both success and user-error branches. Unknown or repeated tokens return `ACCESS_TOKEN_NOT_FOUND` with `field: null` and `Access token does not exist.` Parent access-token self-destroy returns `CAN_ONLY_DELETE_DELEGATE_TOKENS` with `Can only delete delegate tokens.` Cross-app and non-parent delegate hierarchy attempts return `ACCESS_DENIED`; all error paths leave token state unchanged and emit a failed log draft.

The implementation does not perform real billing, merchant approval, app uninstall, app grant changes, or delegated-token changes during normal runtime.

### HAR-455 fidelity review notes

Admin GraphQL 2026-04 billing docs and public app examples continue to treat billing create/update flows as confirmation-URL handoffs. The proxy's synthetic confirmation URLs intentionally prove the local lifecycle boundary without pretending that merchant approval, charge activation, subscription proration, usage-charge billing, or app-plan enforcement happened in Shopify.

Delegate access token docs use the `delegateAccessScope` create input as a list and return the selected permissions through the token payload's `accessScopes` list. The local runtime accepts that current input shape, stores token hash/preview plus owning app and parent-token metadata, and treats a non-destroyed delegated token hash matching the active request token as a delegate parent for conservative local validation. The executable runtime test and `delegate-access-token-create-validation` parity spec cover `delegateAccessScope` validation; the broad app-billing local-runtime parity replay continues to use the already-recorded `accessScopes` request shape so replay evidence is not silently changed without a fresh capture.

Delegate destroy uses `X-Shopify-Access-Token` or `Authorization: Bearer ...` as the active caller token. Test and parity harnesses can set `x-shopify-draft-proxy-api-client-id` to model the caller app id; otherwise the proxy falls back to the current local app installation, then to the synthetic local app id for legacy local-runtime fixtures.

`appRevokeAccessScopes` and `appUninstall` are locally staged only as downstream app-installation state changes. Real app grant revocation and app uninstall side effects remain external Shopify/app-installation events that can only happen later through explicit commit replay or intentional live conformance work on a disposable shop.

HAR-747 tightened `appUninstall` error and cascade fidelity with
`config/parity-specs/apps/app-uninstall-error-codes-and-cascade.json`. The
scenario is executable local-runtime evidence because the current custom app
credential cannot exercise billing-backed subscription setup live; it earns
setup state through replayed `appSubscriptionCreate`, `delegateAccessTokenCreate`,
and `appUninstall` requests rather than pre-seeding parity runner state.

### Safety notes

- Billing create/update/cancel roots can create confirmation URLs, alter subscription state, change capped usage amounts, or create usage charges.
- `appRevokeAccessScopes` can alter the app's current access grant.
- `appUninstall` can remove the app installation from the store.
- Delegated-access roots can create or destroy credentials whose effects are authorization-sensitive.

The local proxy uses synthetic confirmation URLs containing `signature=shopify-draft-proxy-local-redacted`; these URLs are not real Shopify approval links and should still be treated as sensitive in examples and fixtures. Delegated tokens are returned only in the mutation payload and are intentionally absent from `__meta/state`.

Live success-path captures for billing approval, uninstall, and app grant revocation remain blocked unless a disposable app/store credential is explicitly approved for those external effects. Delegated-token create/destroy success and cleanup are covered by HAR-749/HAR-751 disposable-shop probes, while broader app-billing local-runtime evidence continues to cover strict local behavior without mutating a real shop during normal runtime.

HAR-751 live probes against `harry-test-heelo.myshopify.com` on 2026-05-05
confirmed the public Admin GraphQL 2026-04 destroy payloads for missing token,
parent-token self destroy, sibling hierarchy denial, successful destroy, and
repeat destroy. Shopify returned uppercase error codes, `field: null`,
`Access token does not exist.`, and `Can only delete delegate tokens.` The
checked-in `delegate-access-token-destroy-codes` parity spec replays those
stable payload branches locally and uses focused Gleam tests for failed-log /
no-state assertions plus the cross-app denial branch, which still needs a
second disposable app credential for live capture.

HAR-749 captured live 2026-04 `delegateAccessTokenCreate` validation against
`harry-test-heelo.myshopify.com` on 2026-05-05. The checked-in
`delegate-access-token-create-validation` parity spec covers empty
`delegateAccessScope`, non-positive `expiresIn`, unknown scope handles, and a
successful `read_products` delegated token create with immediate cleanup. The
generic parity runner cannot yet replay a later request with the newly returned
delegate token as its active auth header, so parent-is-delegate validation is
covered by focused Gleam runtime tests until auth swapping lands in the harness.

HAR-1034 captured live 2026-04 `delegateAccessTokenCreate` validation against
`harry-test-heelo.myshopify.com` on 2026-05-07 with the expiring conformance
credential and a very large `expiresIn`. Shopify returned
`EXPIRES_AFTER_PARENT`, message
`The delegate token can't expire after the parent token.`, `field: null`, and
no delegate token. The proxy replay models the parent token's `expires_at`
through request-owned parent-token context instead of ambient auth state.

HAR-631 attempted a live `appSubscriptionCreate`/`appSubscriptionCancel` transition capture against the current conformance store on 2026-05-05. Shopify returned `Custom apps cannot use the Billing API`, so repeat-cancel and forced-status transition coverage remains executable local-runtime evidence rather than live billing mutation evidence for this app credential.

HAR-646 sampled `appPurchaseOneTimeCreate` validation against the same conformance store on 2026-05-05. The current custom app credential still returns `Custom apps cannot use the Billing API` before name/price/currency service validation, so those billing validation branches are enforced by `config/parity-specs/apps/app-purchase-one-time-create-validation.json` and focused Gleam tests. Missing and blank `returnUrl` were observed as Shopify GraphQL coercion errors; the local proxy preserves missing `returnUrl` as a coercion error through the full request path and covers blank `returnUrl` in the supported mutation handler.

HAR-672 live probes on 2026-05-05 confirmed the public Admin schema only exposes
the `scopes` argument for `appRevokeAccessScopes` on both 2025-01 and 2026-04.
Safe error probes against `harry-test-heelo.myshopify.com` confirmed
`UNKNOWN_SCOPES` for invalid handles and `CANNOT_REVOKE_REQUIRED_SCOPES` for
the conformance app's declared scopes. The public credential could not exercise
the internal source-app/id resolver branches directly, so those branches are
covered by executable local runtime tests and the HAR-672 local parity fixture.

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

- `tests/unit/app-billing-conformance-fixture.test.ts`
- `config/parity-specs/apps/app-billing-access-local-staging.json`, including
  app-domain generic Node read targets
- `config/parity-specs/apps/app-purchase-one-time-create-validation.json`
- `config/parity-specs/apps/app-revoke-access-scopes-error-codes.json`
- `config/parity-specs/apps/app-uninstall-error-codes-and-cascade.json`
- `config/parity-specs/apps/app-usage-record-create-cap-and-idempotency.json`
- `config/parity-specs/apps/app-subscription-cancel-status-transitions.json`
- `config/parity-specs/apps/app-subscription-trial-extend-validation.json`
- `config/parity-specs/apps/delegate-access-token-destroy-codes.json`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
