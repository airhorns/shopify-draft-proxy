# Gift Cards Endpoint Group

HAR-310 adds local gift-card read and lifecycle staging for the Admin GraphQL gift-card roots. The implementation is intentionally local-first: supported gift-card mutations update normalized in-memory state and retain the original raw request for commit replay, without sending writes or notification side effects to Shopify during normal runtime handling.

## Implemented Roots

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

## Local Read Behavior

- Gift-card reads are backed by normalized `giftCards` state plus `giftCardOrder`.
- Snapshot mode returns `null` for unknown `giftCard(id:)`, an empty `giftCards` connection, and `{ count: 0, precision: "EXACT" }` for `giftCardsCount` when no records are present.
- `giftCards` uses the shared connection helpers for `nodes`, `edges`, selected `pageInfo`, stable synthetic cursors, `first`/`last`, `before`/`after`, `sortKey: ID`, and `reverse`.
- Local query filtering covers `id` terms. Live evidence shows Shopify accepts `id:<numeric>` for gift-card search; fields such as `enabled`, `active`, and `last_characters` are invalid search fields and leave Shopify results unfiltered with warnings.
- `giftCardConfiguration` exposes `issueLimit` and `purchaseLimit` money objects from normalized snapshot state. When no configuration fixture is present, snapshot mode returns zero-value CAD limits as a safe local placeholder.

## Local Mutation Behavior

- `giftCardCreate(input:)` stages a new normalized gift card with a proxy-synthetic `GiftCard` GID, masked/last-character code metadata, initial value and balance, optional note/expiry/template/customer/recipient metadata, and stable timestamps.
- `giftCardUpdate(id:, input:)` stages note, expiry, template suffix, customer, and recipient metadata changes against base or staged gift cards.
- `giftCardCredit(id:, creditInput:)` and `giftCardDebit(id:, debitInput:)` stage balance changes and append local transaction nodes. Debit includes a local insufficient-balance guardrail.
- `giftCardDeactivate(id:)` stages `enabled: false` plus a synthetic `deactivatedAt` timestamp and keeps downstream reads visible.
- Notification roots return local payloads for existing gift cards and append mutation-log entries, but do not send customer-visible notifications at runtime. The original raw notification mutations are still retained for explicit commit replay.

## Captured Evidence

`fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/gift-card-lifecycle.json` records:

- Admin GraphQL gift-card schema shape for the modeled fields and lifecycle payloads
- active conformance access scopes
- readable gift-card configuration limits
- unknown-id and filtered empty read behavior, including empty `giftCards` and `{ count: 0, precision: "EXACT" }` for an `id:` search miss
- non-empty filtered `giftCards` / `giftCardsCount` behavior for a live-created gift card
- successful `giftCardCreate`, `giftCardUpdate`, `giftCardCredit`, `giftCardDebit`, and `giftCardDeactivate` payloads
- downstream `giftCard.transactions` read-after-write behavior after staged credit/debit lifecycle steps
- explicit non-execution of notification roots because those roots send customer-visible side effects

The fixture shows the current conformance credential can read gift cards, perform the core gift-card lifecycle with `read_gift_cards` and `write_gift_cards`, and exercise transaction reads/writes with `read_gift_card_transactions` and `write_gift_card_transactions`.

`config/parity-specs/gift-card-lifecycle.json` now runs as captured-vs-proxy parity. The parity request seeds the live-created gift card and configuration from the capture, replays update/credit/debit/deactivate against the local proxy, and strictly compares stable payload, filtered empty read, transaction read-after-write, and filtered non-empty downstream read fields. Runtime integration coverage still verifies synthetic ID/timestamp behavior, meta logging, raw mutation retention, and notification short-circuiting.

## Validation

- `corepack pnpm vitest run tests/integration/gift-card-flow.test.ts`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
- `corepack pnpm typecheck`
