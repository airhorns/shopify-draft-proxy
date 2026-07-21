---
title: 'Apps, Billing, And Access'
description: 'Coverage notes and fidelity boundaries for Apps, Billing, And Access.'
---

This endpoint group covers Shopify Admin GraphQL app identity, app installation, app billing, access-scope, uninstall, and delegated-access roots. The mutation roots are sensitive because they can affect billing, app grants, app installation state, or delegated credentials in real Shopify.

## Current support and limitations

### Supported roots

Current-installation read:

- `currentAppInstallation`

Arbitrary identity and installation lookups:

- `app(id:)`
- `appByHandle(handle:)`
- `appByKey(apiKey:)`
- `appInstallation(id:)`

Locally implemented multi-installation catalog:

- `appInstallations(...)`

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

The app domain uses one normalized, instance-owned graph for observed app identities, app installations, installation indexes, request-context associations, scoped catalog windows, and staged app effects. Access scopes, app subscriptions, subscription line items, one-time purchases, usage records, and delegated-token metadata remain linked to the owning app.

Current-installation behavior:

- `currentAppInstallation` resolves the authenticated request context through the same effective graph as the other roots. LiveHybrid observations preserve Shopify's authoritative installation ID and nested app ID, handle, and API key across later local billing or access-scope effects.
- Explicit `x-shopify-draft-proxy-api-client-id`, `x-shopify-draft-proxy-app-installation-id`, app metadata, and access-scope headers form request-owned context. Read-only context is ephemeral and does not populate the observed or staged catalog; successful local app mutations retain only the normalized context needed for their downstream reads.
- A cold snapshot with no observed, staged, or explicit request context does not create a default app installation. Nullable singular lookups return `null`, and the installation connection is empty. Local app mutations can still allocate their documented synthetic current context.
- `currentAppInstallation.accessScopes` removes locally revoked optional scopes. Header-provided grants remain request-scoped; the local products grant is used only for the synthetic mutation context that preserves existing app-mutation behavior.
- When a live caller selects installation identity without `accessScopes`, later supported local product-scope app mutations remain usable without fabricating scope rows in reads. A subsequently observed authoritative scope list replaces that identity-only compatibility state.

Arbitrary lookup behavior:

- `app(id:)`, `appByHandle(handle:)`, `appByKey(apiKey:)`, and `appInstallation(id:)` use indexed app/installation identities from the normalized graph. `app` and `appInstallation` with an omitted ID resolve the same request-owned current identity as `currentAppInstallation`.
- Cold LiveHybrid reads forward the complete caller document once and return Shopify's response unchanged while observing reusable identities. When staged effects are relevant, every selected app root reuses that same request-wide response and overlays only the affected normalized records.
- Generic `node(id:)` / `nodes(ids:)` resolves `App` and `AppInstallation` from the same indexes and effective values as the singular roots. It also resolves locally staged `AppSubscription`, `AppPurchaseOneTime`, and `AppUsageRecord` records. Missing IDs return `null`; `AppSubscriptionLineItem` remains nested under `AppSubscription.lineItems`.

Multi-installation catalog behavior:

- `appInstallations(...)` records membership separately for each `category`, `privacy`, and `sortKey` scope. Complete scopes support `first`/`last`, `after`/`before`, `reverse`, and cursor windows locally; partial scopes retain exact observed windows and do not treat the current installation or one page as the whole catalog.
- Opaque Shopify edge cursors are stored with their observed rows. A staged access, billing, or uninstall effect overlays only rows in the requested observed window, preserves unrelated installations, and does not enumerate or hydrate the rest of the catalog.
- Dump/restore round-trips observed apps, installations, request-context associations, scoped completeness, partial windows, and opaque cursors. Reset removes staged effects while preserving the observed base graph, matching other normalized resource stores. Raw Admin access tokens are never stored in the graph or meta state.

Supported mutations stage locally, append the original raw mutation request to the meta log for ordered commit replay, and synthesize Shopify-like payloads without sending runtime writes to Shopify.

Billing behavior:

- Billing payloads serialize `MoneyV2.amount` strings with Shopify Decimal-style formatting: whole numbers keep one trailing zero and fractional amounts drop superfluous trailing zeros. The same app-scoped staged values are read back through current, arbitrary installation, connection, `node(id:)`, and `nodes(ids:)` reads.
- `appPurchaseOneTimeCreate` stages an active one-time purchase with a unique synthetic ID, stores the input currency instead of forcing USD, computes missing-`returnUrl` error locations from the submitted document, and returns a confirmation URL derived from `returnUrl`.
- `appSubscriptionCreate` stages a subscription with a unique synthetic ID, unique synthetic line-item IDs, usage/recurring line-item pricing details, trial days, a `currentPeriodEnd` derived from the synthetic app-domain clock plus `trialDays`, and a confirmation URL derived from `returnUrl`. Test subscriptions activate locally; non-test subscriptions remain pending.
- `appSubscriptionCancel` stages cancellation only for cancellable subscription statuses. Non-cancellable and unknown subscriptions return Shopify-shaped userErrors without mutating local state.
- `appSubscriptionLineItemUpdate` validates usage-pricing line items, rejects recurring/non-variable line items with the Core-source-derived base userError `field: null`, `message: "Only variable subscriptions can be updated."`, validates capped amount currency, increasing cap values, and approval behavior. Approval-required updates return a confirmation URL derived from `x-shopify-draft-proxy-app-url` or the configured Admin origin and keep downstream active line-item caps unchanged; internal/test callers can use the synchronous no-approval branch when explicitly modeled.
- `appSubscriptionTrialExtend` validates the supported day range, subscription existence, active status, and active trial window before mutating `trialDays`.
- `appUsageRecordCreate` stages usage records with unique synthetic IDs for usage-pricing line items, enforces idempotency-key length, currency compatibility, capped amount limits, and idempotent reuse for repeated keys.

Access and uninstall behavior:

- `appRevokeAccessScopes` removes locally granted optional scopes from the current app installation. Unknown-scope and required-scope validation use the stored current installation grant and requested/required scope metadata rather than literal scope handles. Live 2026-04 validation evidence covers unknown handles, required-scope rejection, and mixed unknown-plus-required input: failed validation returns `revoked: null`, and unknown handles take precedence over the required-scope guard. Optional-grant success, non-granted, implied-scope, and missing-source-app branches remain runtime-test-backed; local missing-source-app coverage is modeled through request-owned source-app context rather than the GraphQL operation name.
- The `appRevokeAccessScopes` missing-source-app branch is Core-source-derived, not real-Shopify-recorded: valid public Admin requests carry source app context, while unauthenticated requests fail before the mutation resolver. Local replay uses `x-shopify-draft-proxy-source-app-missing` and returns `MISSING_SOURCE_APP` on `["id"]` with `No app found on the access token.`.
- `appUninstall` resolves the target app from `input.id` or the current installation, enforces current-installation visibility and the `apps` scope where needed, marks the target installation uninstalled on success, clears its active access grant, cancels locally staged active/pending subscriptions, and destroys stored delegated tokens.
- `appUninstall` APP_NOT_FOUND and APP_NOT_INSTALLED userError messages follow Core's `apps.admin.graph_api_errors.app_uninstall` i18n strings (`App not found`, `App is not installed on shop`) rather than the mutation's `add_error_code` placeholder text.
- Downstream `currentAppInstallation` reads return `null` when the current installation is uninstalled, and app-subscription Node reads show cancelled status for locally cancelled subscriptions.

Delegated-token behavior:

- `delegateAccessTokenCreate` accepts the current `delegateAccessScope` list input, validates requested handles against the current installation grant, returns validated scope handles through payload `accessScopes`, and stores only token hash, redacted preview, owning app ID, and parent-token hash in meta-visible state.
- Empty scopes, non-positive `expiresIn`, unknown scope handles, active delegate-token parents, and delegated expiry beyond the parent token return Shopify-like userErrors without staging.
- Request-owned parent-token context is modeled with `x-shopify-draft-proxy-access-token-expires-at`; permanent parent tokens omit that header.
- `delegateAccessTokenDestroy` hashes the raw token, checks app ownership and parent/delegate hierarchy, marks allowed delegate tokens destroyed locally, and returns non-null shop payloads on success and user-error branches.
- `delegateAccessTokenCreate` and `delegateAccessTokenDestroy` project selected `shop` payload fields from the effective shop state, including restored or hydrated shop identity, instead of using a helper-owned default shop record.
- Unknown/repeated tokens, parent self-destroy, cross-app, and non-parent hierarchy attempts return captured userErrors and leave token state unchanged.

Synthetic confirmation URLs append `shopify_draft_proxy_confirmation=1` to the derived local confirmation target. Delegated token raw values are returned only in mutation payloads and are intentionally absent from `__meta/state`.

### Boundaries

- The proxy does not perform real billing, merchant approval, app uninstall, app grant changes, or delegated-token changes during normal runtime.
- Billing approval, charge activation, subscription proration, usage-charge billing, app-plan enforcement, and Shopify Core chargeability guards are not emulated beyond local staged state and validation evidence.
- Live success-path captures for billing approval, uninstall, and app grant revocation require explicitly approved disposable credentials. Billing mutation success captures specifically require a disposable billing-capable app/store credential that can use Shopify's Billing API and safely approve test charges; the current custom-app conformance credential cannot exercise those success paths. Local billing lifecycle, uninstall cascade, and access-scope mutation branches are therefore runtime-test-backed rather than represented as captured parity evidence until a suitable live app credential exists.
- The current conformance credential returns `ACCESS_DENIED` for `appInstallations(...)`. Local multi-installation windowing and overlay mechanics are runtime-test-backed, while a non-empty Shopify catalog comparison remains unavailable until a credential with that cross-installation grant can be captured. The proxy does not synthesize a Shopify catalog fixture to fill that evidence gap.
- No listed app root is registry-only. Validation-only behavior is limited to guardrails that reject before staging and to runtime-test-backed coverage for branches that cannot be exercised safely against the current disposable app credential.
