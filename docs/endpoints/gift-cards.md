# Gift Cards Endpoint Group

HAR-310 adds local gift-card read and lifecycle staging for the Admin GraphQL gift-card roots. The implementation is intentionally local-first: supported gift-card mutations update normalized in-memory state and retain the original raw request for commit replay, without sending writes or notification side effects to Shopify during normal runtime handling.

## Current support and limitations

### Implemented Roots

Overlay reads:

- `giftCard(id:)`
- `giftCards(...)`
- `giftCardsCount(...)`
- `giftCardConfiguration`

Local staged mutations:

- `giftCardCreate(input:)`
- `giftCardUpdate(id:, input:)`
- `giftCardCredit(id:, creditInput:)`
- `giftCardDebit(id:, debitInput:)`
- `giftCardDeactivate(id:)`
- `giftCardSendNotificationToCustomer(id:)`
- `giftCardSendNotificationToRecipient(id:)`

### Local Read Behavior

- Gift-card reads are backed by normalized `giftCards` state plus `giftCardOrder`.
- Snapshot mode returns `null` for unknown `giftCard(id:)`, an empty `giftCards` connection, and `{ count: 0, precision: "EXACT" }` for `giftCardsCount` when no records are present.
- `giftCards` uses the shared connection helpers for `nodes`, `edges`, selected `pageInfo`, stable synthetic cursors, `first`/`last`, `before`/`after`, `sortKey: ID`, and `reverse`.
- Local query filtering covers `id` terms, documented `status:enabled` / `status:disabled`, documented `balance_status:full` / `balance_status:partial` / `balance_status:empty` / `balance_status:full_or_partial`, populated-data filters for `created_at`, `expires_on`, `customer_id`, `recipient_id`, `source`, and `initial_value`, and unfielded code-fragment searches against locally visible `lastCharacters` / `maskedCode` values. Date and money filters use the shared search helpers for comparator handling, so range filters such as `created_at:>=2026-04-29`, `expires_on:<2028-01-01`, and `initial_value:>=5` narrow local results and preserve empty connection/count behavior when no local record matches. `customer_id` and `recipient_id` match either the full customer GID or the numeric tail, mirroring the captured Shopify search form.
- Source filtering is represented as local metadata instead of a selected GiftCard field because Shopify does not expose `source` on the GiftCard object in the captured schema. Locally staged `giftCardCreate` records are tagged as `source:api_client`, and the LiveHybrid hydrate path tags the captured Admin API-created gift card the same way; unknown source values such as `source:manual` return no local matches when the record is known to be API-created.
- HAR-464 live evidence shows `updated_at` does not currently narrow results for the captured gift-card search path: an ID-filtered query with `updated_at:>=2099-01-01` still returned the live-created card. The local proxy therefore keeps `updated_at` in the unsupported/no-op bucket instead of claiming implemented date filtering for it. Fields such as `enabled`, `active`, and `last_characters` are also invalid search fields and leave Shopify results unfiltered with warnings.
- `giftCardConfiguration` exposes `issueLimit` and `purchaseLimit` money objects from normalized snapshot state. When no configuration fixture is present, snapshot mode returns zero-value CAD limits as a safe local placeholder.
- In LiveHybrid cassette parity, existing upstream gift cards referenced by supported mutation roots are hydrated with a narrow `GiftCardHydrate` read before local staging. The hydrate response persists the prior gift card and configuration into base state, then `giftCardUpdate`, `giftCardCredit`, `giftCardDebit`, and `giftCardDeactivate` stay local-only for their lifecycle effects and downstream reads.

### Local Mutation Behavior

- `giftCardCreate(input:)` stages a new normalized gift card with a proxy-synthetic `GiftCard` GID, lower-cased code echo, bullet-masked/last-character code metadata, initial value and balance, optional note/expiry/template/customer/recipient attributes metadata, and stable timestamps.
- `giftCardUpdate(id:, input:)` stages note, expiry, template suffix, customer, and recipient attributes metadata changes against base or staged gift cards.
- `giftCardCredit(id:, creditInput:)` and `giftCardDebit(id:, debitInput:)` stage balance changes and append local transaction nodes. They preserve explicit `processedAt` inputs on staged transactions; otherwise they use synthetic timestamps. Debit includes a local insufficient-balance guardrail.
- `giftCardDeactivate(id:)` stages `enabled: false` plus a synthetic `deactivatedAt` timestamp and keeps downstream reads visible.
- Notification roots return local payloads for existing gift cards and append mutation-log entries, but do not send customer-visible notifications at runtime. Recipient attributes are kept as local gift-card metadata; the original raw notification mutations are still retained for explicit commit replay.

## Historical and developer notes

### Captured Evidence

`fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/gift-cards/gift-card-lifecycle.json` records:

- Admin GraphQL gift-card schema shape for the modeled fields and lifecycle payloads
- active conformance access scopes
- disposable customer-backed gift-card setup and cleanup for populated-data search filters
- readable gift-card configuration limits
- unknown-id and filtered empty read behavior, including empty `giftCards` and `{ count: 0, precision: "EXACT" }` for an `id:` search miss
- non-empty filtered `giftCards` / `giftCardsCount` behavior for a live-created gift card
- post-balance-write `balance_status` and default code-fragment search behavior after Shopify search indexing has materialized the live-created gift card
- post-balance-write advanced search behavior for `created_at`, `expires_on`, `customer_id`, `recipient_id`, `source:api_client`, `source:manual`, and `initial_value`
- observed no-op behavior for `updated_at` even with a future range value
- successful `giftCardCreate`, `giftCardUpdate`, `giftCardCredit`, `giftCardDebit`, and `giftCardDeactivate` payloads
- downstream `giftCard.transactions` read-after-write behavior after staged credit/debit lifecycle steps
- explicit non-execution of notification roots because those roots send customer-visible side effects
- create payload quirks used by the local serializer: Shopify returned a lower-case `giftCardCode` echo and bullet-masked `maskedCode` value for the captured explicit code

The fixture shows the current conformance credential can read gift cards, perform the core gift-card lifecycle with `read_gift_cards` and `write_gift_cards`, and exercise transaction reads/writes with `read_gift_card_transactions` and `write_gift_card_transactions`.

`config/parity-specs/gift-cards/gift-card-lifecycle.json` now runs as cassette-backed captured-vs-proxy parity. The proxy starts cold, fetches the prior gift card/configuration through the checked-in `GiftCardHydrate` cassette entry, replays update/credit/debit/deactivate locally, and strictly compares stable payload, filtered empty read, transaction read-after-write, and filtered non-empty downstream read fields. `config/parity-specs/gift-cards/gift-card-search-filters.json` replays the same update/credit/debit setup and strictly compares the captured pre-deactivation `balance_status`, visible code-fragment search, populated advanced filters, and captured `updated_at` no-op behavior. Runtime integration coverage still verifies synthetic ID/timestamp behavior, explicit transaction `processedAt` preservation, `status:` read-after-write filters, advanced gift-card search filters, recipient attributes projection, meta logging, raw mutation retention, local userErrors, and notification short-circuiting.

### HAR-457 Fidelity Review Notes

- Shopify's current Admin GraphQL gift-card examples emphasize create/update lifecycle inputs, balance transaction mutations, `giftCards` search filters, configuration reads, and explicit notification mutations. The local implementation covers those high-risk paths without runtime Shopify writes.
- Notification roots are intentionally modeled as local acknowledgement/logging boundaries only. The proxy cannot verify or emulate Shopify email delivery, template rendering, customer notification preferences, bounce handling, or recipient inbox state, so tests assert that supported runtime handling does not call upstream Shopify and docs/conformance notes identify delivery as non-emulatable.
- Remaining search-fidelity gaps include full-code default search after the original code is no longer locally stored, source values other than the captured Admin API `api_client` path, and any undocumented search fields not backed by executable conformance evidence. HAR-464 specifically captured `updated_at` as a no-op for the selected API/version rather than a supported local filter.
- The current executable evidence is strict captured-vs-proxy lifecycle parity plus strict captured-vs-proxy search-filter parity, with integration coverage for snapshot empty reads, staged lifecycle read-after-write, balance effects, local validation guardrails, raw mutation retention, and notification side-effect boundaries.

### Validation

- `corepack pnpm vitest run gleam/test/parity_test.gleam`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
- `corepack pnpm typecheck`
