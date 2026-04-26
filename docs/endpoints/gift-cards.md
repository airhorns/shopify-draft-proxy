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
- Local query filtering currently covers simple `id`, `enabled`/`active`, and `last_characters` terms.
- `giftCardConfiguration` exposes `issueLimit` and `purchaseLimit` money objects from normalized snapshot state. When no configuration fixture is present, snapshot mode returns zero-value CAD limits as a safe local placeholder until a readable live fixture is available.

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
- read/configuration blocker payloads requiring `read_gift_cards`
- create/lifecycle blocker payloads requiring `write_gift_cards`
- explicit non-execution of notification roots because those roots send customer-visible side effects

The fixture shows the current conformance credential cannot read or mutate live gift cards, so full non-empty Shopify payload parity is blocked on adding `read_gift_cards` and `write_gift_cards` to the conformance grant. The local runtime test remains the executable evidence for staged read-after-write behavior until those scopes are available.

## Validation

- `corepack pnpm vitest run tests/integration/gift-card-flow.test.ts`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
- `corepack pnpm typecheck`
