# Markets Endpoint Group

The markets group has local slices for captured Shopify Markets reads and stage-local lifecycle mutations. Keep Markets-specific capture details, coverage boundaries, and field behavior here instead of in `docs/architecture.md`.

## Implemented roots

Overlay reads:

- `market`
- `markets`
- `catalogs`
- `webPresences`
- `marketsResolvedValues`

Stage-local mutations:

- `marketCreate`
- `marketUpdate`
- `marketDelete`
- `webPresenceCreate`
- `webPresenceUpdate`

## Unsupported roots still tracked by the registry

- `webPresenceDelete`
- `marketLocalizationsRegister`
- `marketLocalizationsRemove`

## Behavior notes

- Captured Markets reads hydrate normalized Market records keyed by ID, with captured cursor and order metadata preserved for connection responses.
- Snapshot `market(id:)` and `markets(...)` reads resolve from the normalized Market bucket. The local serializer preserves selected-field behavior, unknown-id `null`, empty connections, `nodes`, `edges`, `pageInfo`, `first`, `last`, `before`, `after`, `reverse`, sort keys, root `type` and `status`, and captured-safe `query` filters such as `name`, `id`, `market_type`, and `market_condition_types`.
- Nested Market connection fields such as `conditions.regions`, `catalogs`, and `webPresences` are projected from captured nested payloads with local connection windowing. The proxy does not invent arbitrary nested Markets values when fixture data is absent.
- Top-level `webPresences`, nested `Market.webPresences`, and `MarketsResolvedValues.webPresences` hydrate normalized `MarketWebPresence` records from captured payloads and apply local connection windowing. `catalogs` still replays captured root payload slices until a deeper normalized catalog model is added.
- Supported lifecycle mutations are staged locally and are not sent upstream during normal runtime handling. The mutation log keeps the original raw request body and route path so commit can replay the exact mutation order later.
- `marketCreate` generates stable synthetic `Market` IDs, handles, status/enabled values, timestamps, conditions, currency settings, price inclusions, catalog references, and web presence references from the input.
- `marketUpdate` resolves staged or captured markets by ID, preserves existing fields when inputs omit them, and stages merged changes for downstream `market` and `markets` reads.
- `marketDelete` marks the market deleted in staged state. Deleted staged/captured markets return `null` from `market(id:)` and are removed from `markets(...)` connections while the deleted ID remains visible in meta state.
- `webPresenceCreate` and `webPresenceUpdate` stage local `MarketWebPresence` records using the schema-current 2026-04 input slice: `domainId`, `defaultLocale`, `alternateLocales`, and `subfolderSuffix`. The local model enforces domain/subfolder mutual exclusion, basic locale format and uniqueness checks, duplicate domain/subfolder checks against effective state, and update-by-ID existence checks.
- Web presence root URL derivation is local-only. Subfolder presences derive locale URLs from the captured shop/web-presence domain when available and fall back to the proxy's synthetic shop URL; domain-ID-only presences synthesize a stable domain object because `WebPresenceCreateInput` does not carry a host.
- Current-schema `webPresenceCreate` does not take a market ID. Market association remains modeled through market-side web presence references such as `marketUpdate` inputs that add web presence IDs. When a staged market references a modeled web presence, downstream top-level `webPresences`, nested `Market.webPresences`, `MarketsResolvedValues.webPresences` with a captured baseline, meta state, and the mutation log expose the local-only change.
- Captured validation parity currently covers safe no-side-effect branches for blank `marketCreate` names and unknown IDs for `marketUpdate`/`marketDelete`. Additional success-path conformance should use disposable market setup/cleanup before touching shared buyer-facing market configuration.
- `webPresenceDelete` is schema-current in 2026-04, and deprecated `marketWebPresenceCreate` / `marketWebPresenceUpdate` / `marketWebPresenceDelete` aliases remain visible in Shopify docs, but this repo does not mark them implemented without fixture-backed behavior for payload shape, association cleanup, and validation errors.

## Validation anchors

- Runtime reads: `tests/integration/markets-query-shapes.test.ts`
- Runtime lifecycle staging: `tests/integration/markets-lifecycle-flow.test.ts`
- Conformance parity: `tests/unit/conformance-parity-scenarios.test.ts`
- Conformance fixtures and requests: `config/parity-specs/market*.json`, `config/parity-specs/markets*.json`, and matching files under `config/parity-requests/`
