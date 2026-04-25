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

## Unsupported roots still tracked by the registry

- `webPresenceCreate`
- `webPresenceUpdate`
- `webPresenceDelete`
- `marketLocalizationsRegister`
- `marketLocalizationsRemove`

## Behavior notes

- Captured Markets reads hydrate normalized Market records keyed by ID, with captured cursor and order metadata preserved for connection responses.
- Snapshot `market(id:)` and `markets(...)` reads resolve from the normalized Market bucket. The local serializer preserves selected-field behavior, unknown-id `null`, empty connections, `nodes`, `edges`, `pageInfo`, `first`, `last`, `before`, `after`, `reverse`, sort keys, root `type` and `status`, and captured-safe `query` filters such as `name`, `id`, `market_type`, and `market_condition_types`.
- Nested Market connection fields such as `conditions.regions`, `catalogs`, and `webPresences` are projected from captured nested payloads with local connection windowing. The proxy does not invent arbitrary nested Markets values when fixture data is absent.
- Adjacent Markets roots such as top-level `catalogs`, `webPresences`, and `marketsResolvedValues` still replay captured root payload slices until deeper normalized models are added for those resources.
- Supported lifecycle mutations are staged locally and are not sent upstream during normal runtime handling. The mutation log keeps the original raw request body and route path so commit can replay the exact mutation order later.
- `marketCreate` generates stable synthetic `Market` IDs, handles, status/enabled values, timestamps, conditions, currency settings, price inclusions, catalog references, and web presence references from the input.
- `marketUpdate` resolves staged or captured markets by ID, preserves existing fields when inputs omit them, and stages merged changes for downstream `market` and `markets` reads.
- `marketDelete` marks the market deleted in staged state. Deleted staged/captured markets return `null` from `market(id:)` and are removed from `markets(...)` connections while the deleted ID remains visible in meta state.
- Captured validation parity currently covers safe no-side-effect branches for blank `marketCreate` names and unknown IDs for `marketUpdate`/`marketDelete`. Additional success-path conformance should use disposable market setup/cleanup before touching shared buyer-facing market configuration.

## Validation anchors

- Runtime reads: `tests/integration/markets-query-shapes.test.ts`
- Runtime lifecycle staging: `tests/integration/markets-lifecycle-flow.test.ts`
- Conformance parity: `tests/unit/conformance-parity-scenarios.test.ts`
- Conformance fixtures and requests: `config/parity-specs/market*.json`, `config/parity-specs/markets*.json`, and matching files under `config/parity-requests/`
