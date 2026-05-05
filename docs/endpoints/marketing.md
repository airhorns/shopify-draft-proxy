# Marketing

## Current support and limitations

### Supported read roots

- `marketingActivities`
- `marketingActivity`
- `marketingEvents`
- `marketingEvent`

### Supported mutation roots

- `marketingActivityCreate`
- `marketingActivityUpdate`
- `marketingActivityCreateExternal`
- `marketingActivityUpdateExternal`
- `marketingActivityUpsertExternal`
- `marketingActivityDeleteExternal`
- `marketingActivitiesDeleteAllExternal`
- `marketingEngagementCreate`
- `marketingEngagementsDelete`

Native/deprecated `marketingActivityCreate` and `marketingActivityUpdate` are modeled separately from the external activity roots. They stage non-external `MarketingActivity` records locally and do not synthesize nested `MarketingEvent` rows unless future capture proves native events materialize for that branch.

### Snapshot behavior

- Snapshot mode serves marketing activity and event reads from normalized raw marketing records hydrated from conformance captures or seeded directly in tests.
- Staged external activity creates, updates, upserts, deletes, and bulk delete-all overlays are applied to snapshot/local reads immediately.
- Staged native activity creates and updates overlay snapshot/local activity reads immediately. Current native create payloads expose only `userErrors`, so downstream reads and meta state are the observable local activity surface.
- Missing singular lookups return `null`.
- Absent catalogs return non-null empty connections with empty `nodes`/`edges`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors.
- Local connection serialization preserves selected `nodes`, `edges`, `cursor`, and `pageInfo` fields. Captured Shopify cursors are reused when present; locally seeded records without captured cursors use stable synthetic `cursor:<gid>` cursors.

### Captured scope

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
- local integration coverage also exercises Shopify's documented `marketingActivityUpdateExternal(utm:)` selector path; the proxy resolves that selector against the staged/effective activity `utmParameters` and keeps the update local
- upsertExternal create and update behavior keyed by `remoteId`
- deleteExternal by activity ID and remote ID, including missing-activity userErrors
- deleteAllExternal asynchronous `Job` payload with `done: false`
- userErrors for missing non-hierarchical attribution and immutable UTM changes

HAR-681 extends existing external activity update/upsert validation:

- existing external activities reject immutable `channelHandle`, `urlParameterValue`, UTM, invalid `parentRemoteId`, and `hierarchyLevel` changes with Shopify's captured `MarketingActivityUserError.code` values and `marketingActivity: null`
- the shared local validator also rejects non-external activity records, external records whose nested marketing event is absent, and parent changes to a different resolved marketing event before staging any update
- the live parity capture covers the branches the current disposable shop can create: channel-handle, URL-parameter, UTM, invalid-parent-remote-id, and hierarchy-level rejections. The immutable-parent-id branch is runtime-test-backed because the conformance app/store has no recognized channel handle, while Shopify requires one to create the campaign-level parent activity needed for that live branch.

The HAR-213 parity spec replays the external lifecycle through the local proxy parity harness. It compares stable selected mutation/read fields and captured userErrors against the live fixture; synthetic IDs and timestamps remain covered by runtime integration tests because local staging intentionally does not reuse live Shopify identifiers.

Local staging intentionally uses stable synthetic IDs and timestamps instead of replaying live Shopify IDs. The raw original mutation body is retained in the meta log for successful staged lifecycle mutations so commit replay can preserve request order.

HAR-214 captures marketing engagement write evidence with `write_marketing_events`:

- `marketingEngagementCreate` accepts activity-level identifiers by either `marketingActivityId` or external activity `remoteId`; missing identifiers return `INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING`, multiple identifiers return `INVALID_MARKETING_ENGAGEMENT_ARGUMENTS`, and missing activity/remote IDs return `MARKETING_ACTIVITY_DOES_NOT_EXIST`
- `marketingEngagementCreate` rejects mixed `adSpend`/`sales` currencies with `CURRENCY_CODE_MISMATCH_INPUT` and rejects engagement money in a currency that differs from the resolved activity's staged `budget.total`/`adSpend` currency with `MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH`; both validation branches return `marketingEngagement: null` and do not stage an engagement record
- duplicate same-day activity-level engagement writes are accepted and the latest returned metric values replace the local engagement record
- metric counts are not validated as non-negative by Shopify; negative counts are returned without userErrors in the captured activity-level branch, and HAR-453 replays that branch in the executable parity request
- HAR-463 refreshes the executable engagement fixture against Admin GraphQL 2026-04; `primaryConversions` and `allConversions` are now live-capture-backed decimal-string fields in both the input inventory and the activity-level success payload
- unrecognized `channelHandle` values return `INVALID_CHANNEL_HANDLE`; this proxy only stages channel-level engagement records when the channel handle is already known from hydrated marketing event data. HAR-463 probed the current conformance app handle plus common channel handles and found no recognized success branch in the disposable shop.
- `marketingEngagementsDelete` has no activity-level selector; missing delete selectors return `INVALID_DELETE_ENGAGEMENTS_ARGUMENTS`, `deleteEngagementsForAllChannels: true` returns the captured result string and removes known local channel-level engagement records, and activity-level engagement records are retained
- immediate downstream `marketingActivity.adSpend` reads remained `null` after captured activity-level engagement writes, so local staging records the engagement in meta state but does not invent activity/event aggregate attribution

HAR-373 captures native/deprecated activity evidence against Admin GraphQL 2026-04, and HAR-463 reconfirmed the current app topology still lacks a usable deprecated extension:

- `MarketingActivityCreateInput` exposes only `marketingActivityExtensionId` and `status`
- `MarketingActivityCreatePayload` exposes only `userErrors`
- `MarketingActivityUpdateInput` exposes only `id`, while `MarketingActivityUpdatePayload` still exposes `marketingActivity`, `redirectPath`, and `userErrors`
- invalid create attempts with an unknown `MarketingActivityExtension` return `userErrors[{ field: ["input", "marketingActivityExtensionId"], message: "Could not find the marketing extension" }]`
- live success-path create/update capture is blocked in the current conformance app because no deprecated marketing activity app extension is installed or discoverable; update probes outside extension context return a top-level `ACCESS_DENIED` error despite the app having `write_marketing_events`
- local runtime tests cover the intended draft-proxy behavior for staged native create/update, downstream activity reads, engagement by staged native activity ID, meta state, and mutation log retention without runtime Shopify writes

### External/native boundary

- External activity roots create/update both `MarketingActivity` and nested `MarketingEvent` records from remote ID / UTM attribution evidence.
- Native activity roots create/update non-external activity records only. They preserve extension/context/form fields in local state when supplied by older clients, but the current 2026-04 schema does not expose those deprecated inputs.
- Native success-path conformance should be refreshed if a future conformance app install includes a deprecated marketing activity extension. Until then, local support intentionally remains narrow and runtime-test-backed for draft-proxy staging semantics.
- Public GitHub search during HAR-394 found mostly generated schema/type artifacts rather than production app implementations, so local behavior continues to lean on Shopify docs plus checked-in live captures instead of inferring extra app-specific semantics.

### Local filtering and ordering

Local snapshot filtering is intentionally narrow and evidence-backed:

- `marketingActivities(query:)` supports default text, `title`, `app_name`, `id`, `created_at`, `updated_at`, scheduled date terms, and exact `tactic` terms.
- `marketingActivities(marketingActivityIds:)` and `marketingActivities(remoteIds:)` filter known local records by exact activity ID or nested external marketing-event remote ID.
- `marketingEvents(query:)` supports default text, `description`, `id`, `started_at`, and exact `type` terms.
- Activity sort keys currently modeled locally are `CREATED_AT`, `ID`, and `TITLE`.
- Event sort keys currently modeled locally are `ID` and `STARTED_AT`.

Unsupported marketing reads outside these registered roots continue through the generic unknown-operation path outside snapshot parity execution.

## Historical and developer notes

- Historical capture notes are embedded in the current behavior descriptions above for the HAR-212, HAR-213, and HAR-214 slices; keep future validation anchors or fixture-specific notes here.
- HAR-453 reviewed Shopify docs/examples and public Admin GraphQL examples for marketing activity, event, and engagement roots. Public examples remain sparse and mostly generated from Shopify's schema/docs, so local fidelity should continue to be driven by checked-in conformance captures and focused runtime tests rather than inferred app-specific behavior.
- HAR-463 adds the `marketing-engagement` aggregate capture path and refreshes `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/marketing/marketing-engagement-lifecycle.json` with setup/cleanup evidence for disposable external activity-level engagement writes. The fixture backs `primaryConversions` and `allConversions` through executable parity rather than runtime tests alone.
- HAR-684 adds `marketing-engagement-currency-validation` live parity for `marketingEngagementCreate` currency guardrails. Shopify returns `field: ["marketingEngagement"]` for the captured currency userErrors; an unrecognized `channelHandle` returns `INVALID_CHANNEL_HANDLE` before currency validation, so recognized channel-handle currency behavior remains runtime-test-backed until the conformance shop exposes a valid handle.
- HAR-463 did not find a live evidence path for native/deprecated activity success or recognized channel-handle engagement success in the current disposable shop. Native success remains blocked on installing or discovering a deprecated `MarketingActivityExtension`; channel-level engagement success remains blocked because the live marketing event catalog has no non-null `channelHandle`, and probes for the conformance app handle plus common channel handles all returned `INVALID_CHANNEL_HANDLE`.
- HAR-453 added focused local coverage that `marketingActivitiesDeleteAllExternal` removes staged external activities and events without deleting staged native activities. The operation still returns Shopify's captured asynchronous `Job` shape (`done: false`), and downstream local reads reflect the delete-all effect immediately so tests can observe deterministic draft state.
