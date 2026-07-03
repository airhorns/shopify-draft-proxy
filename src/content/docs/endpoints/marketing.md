---
title: 'Marketing'
description: 'Coverage notes and fidelity boundaries for Marketing.'
---

This endpoint group covers Shopify Admin GraphQL marketing activity, marketing event, external marketing activity, and marketing engagement roots.

## Current support and limitations

### Supported roots

Read roots:

- `marketingActivities(...)`
- `marketingActivity(id:)`
- `marketingEvents(...)`
- `marketingEvent(id:)`

Mutation roots:

- `marketingActivityCreate`
- `marketingActivityUpdate`
- `marketingActivityCreateExternal`
- `marketingActivityUpdateExternal`
- `marketingActivityUpsertExternal`
- `marketingActivityDeleteExternal`
- `marketingActivitiesDeleteAllExternal`
- `marketingEngagementCreate`
- `marketingEngagementsDelete`

### Local behavior

Snapshot reads are served from normalized marketing activity and event records hydrated from conformance captures or seeded directly in tests:

- Missing singular activity/event lookups return `null`.
- Absent catalogs return non-null empty connections with empty `nodes`/`edges`, false page booleans, and null cursors.
- Connection serialization preserves selected `nodes`, `edges`, `cursor`, and `pageInfo`. Captured Shopify cursors are reused when present; locally seeded records without captured cursors use stable synthetic `cursor:<gid>` cursors.
- Local activity filtering supports captured/default text fields, `title`, `app_name`, `id`, date terms, scheduled date terms, exact `tactic`, `marketingActivityIds`, and `remoteIds`.
- Local event filtering supports default text, `description`, `id`, `started_at`, and exact `type`.
- Modeled activity sort keys are `CREATED_AT`, `ID`, and `TITLE`. Modeled event sort keys are `ID` and `STARTED_AT`.

External activity lifecycle:

- External create/update/upsert/delete roots stage `MarketingActivity` and nested `MarketingEvent` records locally from remote ID and UTM attribution evidence.
- External create/update/upsert preserve supplied request app identity, remote ID, UTM attribution, channel handle, activity-level `adSpend`, schedule input, and `referringDomain` values in staged activity state when supplied. Updates that omit those fields keep the previous staged values; creates that omit optional remote ID or UTM fields keep those values nullable rather than inventing local tracking placeholders. The public 2026-04 schema exposes read-back for `MarketingActivity.adSpend` and nested `MarketingEvent.scheduledToEndAt`, while the currently captured public `MarketingActivity` type does not expose scheduled or referring-domain output fields directly.
- Selector resolution by `remoteId`, `marketingActivityId`, and UTM is app-scoped when `x-shopify-draft-proxy-api-client-id` is present. Legacy unowned fixture records remain visible to all callers. This proxy-owned request header is not Shopify parity evidence: a 2026-04 live probe with the conformance app showed Shopify scopes ownership to the OAuth app/token and ignores that custom header, so cross-app local scoping is covered by Rust integration tests rather than a parity spec until a two-installed-app capture harness exists.
- Multiple selectors must resolve to the same effective activity before validation or staging. Conflicts return `MARKETING_ACTIVITY_DOES_NOT_EXIST` with no local mutation.
- Upsert creates or updates by `remoteId`; delete can resolve by activity ID or remote ID and applies the same selector consistency rule.
- `marketingActivitiesDeleteAllExternal` records an in-flight local job and immediately removes the calling app's external activities and events from downstream reads. While the app-scoped in-flight flag exists, that app's external create/update/upsert calls return `DELETE_JOB_ENQUEUED`.
- Status labels are derived from staged activity state rather than copied from a lookup. Native staged activities preserve `targetStatus` when supplied so paused/active/deleted transitions surface local labels.
- Successful staged lifecycle mutations keep stable synthetic IDs/timestamps and retain original raw mutation bodies in the meta log for commit replay.

External activity validation:

- Create and upsert-create accept any non-empty `channelHandle` when supplied, enforce budget/ad-spend currency agreement, and reject duplicate remote IDs, UTM triplets, and URL parameter values within the requesting app.
- Update/upsert-update reject immutable `channelHandle`, URL parameter, UTM, hierarchy, parent, currency, and tactic changes according to captured branch-specific userErrors.
- Non-external records, missing nested marketing events, and parent changes to a different resolved event fail before staging.
- `remoteUrl` and `remotePreviewImageUrl` accept only `http` and `https` schemes. URL scalar failures remain top-level coercion errors before mutation handling.
- External delete rejects missing selectors, missing external records, non-external records, and parent activity deletes that still have local child activity references.
- The missing-selector and missing-ID/remote delete guards are covered by live parity. The non-external and child-reference delete guards are runtime-test-backed because the current disposable conformance shop has no discoverable non-external activity and cannot create the campaign-level parent needed for the child-event delete branch; the live delete-guards fixture records those setup blockers.

Native/deprecated activity behavior:

- `marketingActivityCreate` and `marketingActivityUpdate` stage non-external `MarketingActivity` records locally and do not synthesize nested `MarketingEvent` rows unless executable evidence proves native events materialize for that branch.
- Native create payloads expose only `userErrors` in the current public schema, so downstream reads and meta state are the observable local activity surface.
- Native update checks staged activity ownership before applying local updates. Cross-app callers receive Shopify's top-level `ACCESS_DENIED` shape and leave state unchanged.

Engagement behavior:

- `marketingEngagementCreate` accepts activity-level selectors by `marketingActivityId` or external activity `remoteId`, validates selector count, and stages engagement records in meta state for supported branches.
- Channel-handle engagement accepts any non-empty handle. Empty handles return `INVALID_CHANNEL_HANDLE`.
- Currency validation follows captured order: selector-count checks first, channel-handle checks before currency on channel paths, and input currency checks before missing activity lookup on activity/remote paths.
- On Admin API 2026-04, `MarketingEngagementInput.occurredOn`, `utcOffset`, and `isCumulative` are required schema fields. Omitting any of them returns top-level GraphQL coercion errors before the local handler stages an engagement; successful responses echo the supplied literals without synthesized defaults.
- Activity-level duplicate same-day writes are accepted locally with latest metric values replacing the local engagement record.
- Immediate downstream `marketingActivity.adSpend` reads remain `null` after captured activity-level engagement writes, so the proxy does not invent aggregate attribution.
- `marketingEngagementsDelete` validates the selector guard before deletion: exactly one of `channelHandle` or `deleteEngagementsForAllChannels: true` must be supplied.
- Single-channel engagement deletion accepts any non-empty handle and reports the handle as marked for deletion. Empty handles return `INVALID_CHANNEL_HANDLE`.
- All-channel engagement deletion reports the count of distinct locally known channel handles for the calling app. Activity-level engagement records are retained.

### Boundaries

- Unsupported marketing reads outside the registered roots continue through the generic unknown-operation path outside snapshot parity execution.
- Native/deprecated activity success-path behavior is narrow and runtime-test-backed because the current disposable conformance app does not expose a deprecated marketing activity extension.
- Recognized channel-level engagement success is unsupported unless hydrated event data supplies a known channel handle.
- Marketing aggregate attribution, native event creation, and uncaptured app-specific marketing extension semantics are not inferred.
- No listed marketing root is registry-only. Validation-only branches are limited to captured userErrors/coercion and runtime-test-backed local guardrails that do not stage on failure.
