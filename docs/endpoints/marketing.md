# Marketing

## Supported read roots

- `marketingActivities`
- `marketingActivity`
- `marketingEvents`
- `marketingEvent`

## Supported mutation roots

- `marketingActivityCreateExternal`
- `marketingActivityUpdateExternal`
- `marketingActivityUpsertExternal`
- `marketingActivityDeleteExternal`
- `marketingActivitiesDeleteAllExternal`
- `marketingEngagementCreate`
- `marketingEngagementsDelete`

Deprecated/non-external `marketingActivityCreate` and `marketingActivityUpdate` remain registered gaps, not implemented support. They are still explicit in the operation registry so unsupported runtime passthrough is observable, but the proxy does not claim local emulation for them.

## Snapshot behavior

- Snapshot mode serves marketing activity and event reads from normalized raw marketing records hydrated from conformance captures or seeded directly in tests.
- Staged external activity creates, updates, upserts, deletes, and bulk delete-all overlays are applied to snapshot/local reads immediately.
- Missing singular lookups return `null`.
- Absent catalogs return non-null empty connections with empty `nodes`/`edges`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors.
- Local connection serialization preserves selected `nodes`, `edges`, `cursor`, and `pageInfo` fields. Captured Shopify cursors are reused when present; locally seeded records without captured cursors use stable synthetic `cursor:<gid>` cursors.

## Captured scope

HAR-212 captures the safe read model for:

- catalog reads with `first`, `sortKey`, and `reverse`
- empty search-filter aliases using `query`
- singular lookup nullability for absent activity/event IDs
- selected `MarketingActivity` fields: identity, title, timestamps, status/status label, tactic, channel type, source/medium, external/main-workflow booleans, app identity, and nested marketing event identity/attribution fields
- selected `MarketingEvent` fields: identity, type, remote ID, start/end timestamps, URLs, UTM fields, description, channel type, and source/medium

The read capture script also records an invalid-ID probe and schema inventory as evidence files.

HAR-213 captures external lifecycle write evidence with `write_marketing_events`:

- createExternal happy path with remote ID, UTM, selected activity fields, and nested marketing event attribution
- updateExternal by `remoteId` for title, status, and remote URL changes
- upsertExternal create and update behavior keyed by `remoteId`
- deleteExternal by activity ID and remote ID, including missing-activity userErrors
- deleteAllExternal asynchronous `Job` payload with `done: false`
- userErrors for missing non-hierarchical attribution and immutable UTM changes

The HAR-213 parity spec replays the external lifecycle through the local proxy parity harness. It compares stable selected mutation/read fields and captured userErrors against the live fixture; synthetic IDs and timestamps remain covered by runtime integration tests because local staging intentionally does not reuse live Shopify identifiers.

Local staging intentionally uses stable synthetic IDs and timestamps instead of replaying live Shopify IDs. The raw original mutation body is retained in the meta log for successful staged lifecycle mutations so commit replay can preserve request order.

HAR-214 captures marketing engagement write evidence with `write_marketing_events`:

- `marketingEngagementCreate` accepts activity-level identifiers by either `marketingActivityId` or external activity `remoteId`; missing identifiers return `INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING`, multiple identifiers return `INVALID_MARKETING_ENGAGEMENT_ARGUMENTS`, and missing activity/remote IDs return `MARKETING_ACTIVITY_DOES_NOT_EXIST`
- duplicate same-day activity-level engagement writes are accepted and the latest returned metric values replace the local engagement record
- metric counts are not validated as non-negative by Shopify; negative counts are returned without userErrors in the captured activity-level branch
- unrecognized `channelHandle` values return `INVALID_CHANNEL_HANDLE`; this proxy only stages channel-level engagement records when the channel handle is already known from hydrated marketing event data
- `marketingEngagementsDelete` has no activity-level selector; missing delete selectors return `INVALID_DELETE_ENGAGEMENTS_ARGUMENTS`, `deleteEngagementsForAllChannels: true` returns the captured result string and removes known local channel-level engagement records, and activity-level engagement records are retained
- immediate downstream `marketingActivity.adSpend` reads remained `null` after captured activity-level engagement writes, so local staging records the engagement in meta state but does not invent activity/event aggregate attribution

## Local filtering and ordering

Local snapshot filtering is intentionally narrow and evidence-backed:

- `marketingActivities(query:)` supports default text, `title`, `app_name`, `id`, `created_at`, `updated_at`, scheduled date terms, and exact `tactic` terms.
- `marketingActivities(marketingActivityIds:)` and `marketingActivities(remoteIds:)` filter known local records by exact activity ID or nested external marketing-event remote ID.
- `marketingEvents(query:)` supports default text, `description`, `id`, `started_at`, and exact `type` terms.
- Activity sort keys currently modeled locally are `CREATED_AT`, `ID`, and `TITLE`.
- Event sort keys currently modeled locally are `ID` and `STARTED_AT`.

Unsupported marketing reads outside these registered roots continue through the generic unknown-operation path outside snapshot parity execution.
