# Marketing

## Supported read roots

- `marketingActivities`
- `marketingActivity`
- `marketingEvents`
- `marketingEvent`

This endpoint group is read-only. It does not stage marketing activity mutations.

## Snapshot behavior

- Snapshot mode serves marketing activity and event reads from normalized raw marketing records hydrated from conformance captures or seeded directly in tests.
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

The capture script also records an invalid-ID probe and schema inventory as evidence files. The current HAR-212 capture has `read_marketing_events` access and records Shopify's empty/no-data behavior because the dev store has no marketing rows. A representative non-empty live read was not captured: temporary seeding requires `write_marketing_events`, and the current conformance credential is denied that scope. Local snapshot tests cover non-empty serializer behavior from explicit fixture-backed records rather than inventing live conformance rows.

## Local filtering and ordering

Local snapshot filtering is intentionally narrow and evidence-backed:

- `marketingActivities(query:)` supports default text, `title`, `app_name`, `id`, `created_at`, `updated_at`, scheduled date terms, and exact `tactic` terms.
- `marketingActivities(marketingActivityIds:)` and `marketingActivities(remoteIds:)` filter known local records by exact ID.
- `marketingEvents(query:)` supports default text, `description`, `id`, `started_at`, and exact `type` terms.
- Activity sort keys currently modeled locally are `CREATED_AT`, `ID`, and `TITLE`.
- Event sort keys currently modeled locally are `ID` and `STARTED_AT`.

Unsupported marketing reads outside these registered roots continue through the generic unknown-operation path outside snapshot parity execution.
