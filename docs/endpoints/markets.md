# Markets Endpoint Group

The markets group has read-only local slices for captured Shopify Markets data. Keep Markets-specific capture details, coverage boundaries, and field behavior here instead of in `docs/architecture.md`.

## Implemented roots

Overlay reads:

- `market`
- `markets`
- `catalogs`
- `webPresences`
- `marketsResolvedValues`

## Unsupported roots still tracked by the registry

- `marketCreate`
- `marketUpdate`
- `marketDelete`
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
- Markets mutations remain unsupported runtime scope. Registry entries are declared gaps for future local staging and must not be read as current supported behavior.

## Validation anchors

- Runtime reads: `tests/integration/markets-query-shapes.test.ts`
- Conformance parity: `tests/unit/conformance-parity-scenarios.test.ts`
- Conformance fixtures and requests: `config/parity-specs/market*.json`, `config/parity-specs/markets*.json`, and matching files under `config/parity-requests/`
