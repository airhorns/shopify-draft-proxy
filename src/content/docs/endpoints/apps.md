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

- `currentAppInstallation`, `appInstallation(id:)`, `app(id:)`, `appByHandle(handle:)`, and `appByKey(apiKey:)` project the local app model after staged app mutations, LiveHybrid hydration from an upstream app installation read, or request-owned app context (`x-shopify-draft-proxy-api-client-id`, optional `x-shopify-draft-proxy-app-installation-id`, app metadata, and access-scope headers).
- LiveHybrid hydration keeps the upstream `currentAppInstallation.id` and nested `app` identity attached to the authenticated request context, so later locally staged app billing or access-scope mutations do not replace the real installation id or app handle with deterministic local fallback values.
- LiveHybrid `currentAppInstallation` billing reads observe Shopify subscriptions as the ordered baseline, then overlay app-owned local creates and lifecycle updates by ID. Unrelated observed subscriptions remain visible in `allSubscriptions`, while `activeSubscriptions` is derived from the same effective records by status.
- Snapshot reads use the staged/hydrated/request-owned current installation when present and otherwise fall back to the deterministic local app identity instead of fixture sentinel IDs. Missing non-current IDs return `null`.
- `appInstallations(first:)` serializes the current staged/hydrated installation as a connection for local read-after-write checks. Authorized multi-installation catalog behavior remains outside current local support.
- Generic `node(id:)` / `nodes(ids:)` resolves app-domain records from the same ownership-aware effective state used by installation reads: `App`, `AppInstallation`, `AppSubscription`, `AppPurchaseOneTime`, and `AppUsageRecord`. Locally updated subscription and usage records override their observed baseline records, and missing or tombstoned IDs return `null`. `AppSubscriptionLineItem` remains nested under `AppSubscription.lineItems` and is not claimed as a standalone Node implementor.
- `currentAppInstallation.accessScopes` projects granted scope handles from `x-shopify-draft-proxy-access-scopes`, defaults to the local products pair when the header is omitted, and removes locally revoked optional scopes from downstream reads.
- `POST /__meta/dump` and `POST /__meta/restore` preserve observed subscription order, local subscription overlays, and subscription tombstones so the effective billing view survives process-backed state persistence.

Supported mutations stage locally, append the original raw mutation request to the meta log for ordered commit replay, and synthesize Shopify-like payloads without sending runtime writes to Shopify.

Billing behavior:

- Billing payloads serialize `MoneyV2.amount` strings with Shopify Decimal-style formatting: whole numbers keep one trailing zero and fractional amounts drop superfluous trailing zeros. The same normalized staged values are read back through `currentAppInstallation`, `node(id:)`, and `nodes(ids:)` app-domain reads.
- `appPurchaseOneTimeCreate` stages an active one-time purchase with a unique synthetic ID, stores the input currency instead of forcing USD, computes missing-`returnUrl` error locations from the submitted document, and returns a confirmation URL derived from `returnUrl`.
- `appSubscriptionCreate` stages a subscription with a unique synthetic ID, unique synthetic line-item IDs, usage/recurring line-item pricing details, trial days, a `currentPeriodEnd` derived from the synthetic app-domain clock plus `trialDays`, and a confirmation URL derived from `returnUrl`. Test subscriptions activate locally; non-test subscriptions remain pending.
- `appSubscriptionCancel` and `appSubscriptionTrialExtend` query-hydrate an unstaged target subscription in LiveHybrid mode before validating or staging it. Hydration forwards only a read containing the subscription and current app identity; the submitted supported mutation remains local and is retained unchanged for commit replay.
- `appSubscriptionCancel` stages cancellation only for cancellable subscription statuses. Non-cancellable, cross-app, and unknown subscriptions return Shopify-shaped userErrors without mutating local state.
- `appSubscriptionLineItemUpdate` query-hydrates the current app's active subscription line items and pricing before validating a cold target. It rejects cross-app, inactive, and recurring/non-variable line items, including the Core-source-derived base userError `field: null`, `message: "Only variable subscriptions can be updated."`, and validates capped amount currency, increasing cap values, and approval behavior. Approval-required updates return a confirmation URL derived from `x-shopify-draft-proxy-app-url` or the configured Admin origin and keep downstream active line-item caps unchanged; internal/test callers can use the synchronous no-approval branch when explicitly modeled.
- `appSubscriptionTrialExtend` validates the supported day range, subscription existence, active status, and active trial window before mutating `trialDays`.
- `appUsageRecordCreate` query-hydrates cold active line items with usage pricing, current balance, cap, and existing usage records. It stages usage records with unique synthetic IDs, enforces app ownership, active status, pricing type, idempotency-key length, currency compatibility, and capped amount limits, and reuses an observed or local record for repeated app-owned idempotency keys.

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
- Live success-path captures for billing approval, uninstall, and app grant revocation require explicitly approved disposable credentials. Billing mutation success captures specifically require a disposable billing-capable app/store credential that can use Shopify's Billing API and safely approve test charges; the current custom-app conformance credential returns `Custom apps cannot use the Billing API` and cannot exercise those success paths. Local billing lifecycle, effective-state overlay, cold hydration, uninstall cascade, and access-scope mutation branches are therefore runtime-test-backed rather than represented as captured parity evidence until a suitable live app credential exists.
- Authorized multi-installation catalog behavior for `appInstallations(...)` remains unsupported without a suitable live grant.
- No listed app root is registry-only. Validation-only behavior is limited to guardrails that reject before staging and to runtime-test-backed coverage for branches that cannot be exercised safely against the current disposable app credential.
