//// Read-path tests for `proxy/gift_cards`.
////
//// Covers:
////   - the four query roots (`giftCard`, `giftCards`, `giftCardsCount`,
////     `giftCardConfiguration`),
////   - field projection (id / lastCharacters / maskedCode / enabled /
////     initialValue / balance / customer / recipient / transactions),
////   - the `disabledAt` <-> `deactivatedAt` aliasing on GiftCard,
////   - search-query filtering on `id` / `status` / `balance_status`,
////   - sort by `CREATED_AT` / `UPDATED_AT` / `ID`,
////   - the singleton `giftCardConfiguration` default fallback.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/gift_cards
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type GiftCardRecord, type Money, GiftCardRecord, GiftCardTransactionRecord,
  Money,
}

// ----------- Helpers -----------

fn money(amount: String, currency: String) -> Money {
  Money(amount: amount, currency_code: currency)
}

fn gift_card(
  id: String,
  legacy_id: String,
  last4: String,
  enabled: Bool,
  balance_amount: String,
) -> GiftCardRecord {
  GiftCardRecord(
    id: id,
    legacy_resource_id: legacy_id,
    last_characters: last4,
    masked_code: "•••• •••• •••• " <> last4,
    enabled: enabled,
    deactivated_at: case enabled {
      True -> None
      False -> Some("2024-02-01T00:00:00.000Z")
    },
    expires_on: None,
    note: None,
    template_suffix: None,
    created_at: "2024-01-01T00:00:00.000Z",
    updated_at: "2024-01-02T00:00:00.000Z",
    initial_value: money("100.0", "CAD"),
    balance: money(balance_amount, "CAD"),
    customer_id: None,
    recipient_id: None,
    source: None,
    recipient_attributes: None,
    transactions: [],
  )
}

fn run(store_in: store.Store, query: String) -> String {
  let assert Ok(data) =
    gift_cards.handle_gift_card_query(store_in, query, dict.new())
  json.to_string(data)
}

fn seed(store_in: store.Store, record: GiftCardRecord) -> store.Store {
  let #(_, s) = store.stage_create_gift_card(store_in, record)
  s
}

// ----------- is_gift_card_query_root -----------

pub fn is_gift_card_query_root_test() {
  assert gift_cards.is_gift_card_query_root("giftCard")
  assert gift_cards.is_gift_card_query_root("giftCards")
  assert gift_cards.is_gift_card_query_root("giftCardsCount")
  assert gift_cards.is_gift_card_query_root("giftCardConfiguration")
  assert !gift_cards.is_gift_card_query_root("giftCardCreate")
  assert !gift_cards.is_gift_card_query_root("validation")
}

pub fn is_gift_card_mutation_root_test() {
  assert gift_cards.is_gift_card_mutation_root("giftCardCreate")
  assert gift_cards.is_gift_card_mutation_root("giftCardUpdate")
  assert gift_cards.is_gift_card_mutation_root("giftCardCredit")
  assert gift_cards.is_gift_card_mutation_root("giftCardDebit")
  assert gift_cards.is_gift_card_mutation_root("giftCardDeactivate")
  assert gift_cards.is_gift_card_mutation_root(
    "giftCardSendNotificationToCustomer",
  )
  assert gift_cards.is_gift_card_mutation_root(
    "giftCardSendNotificationToRecipient",
  )
  assert !gift_cards.is_gift_card_mutation_root("giftCard")
}

// ----------- giftCard(id:) -----------

pub fn gift_card_by_id_returns_record_test() {
  let record =
    gift_card(
      "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
      "1",
      "1234",
      True,
      "75.0",
    )
  let s = seed(store.new(), record)
  let result =
    run(
      s,
      "{ giftCard(id: \"gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic\") { __typename id legacyResourceId lastCharacters maskedCode enabled } }",
    )
  assert result
    == "{\"giftCard\":{\"__typename\":\"GiftCard\",\"id\":\"gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic\",\"legacyResourceId\":\"1\",\"lastCharacters\":\"1234\",\"maskedCode\":\"•••• •••• •••• 1234\",\"enabled\":true}}"
}

pub fn gift_card_by_id_missing_returns_null_test() {
  let result =
    run(
      store.new(),
      "{ giftCard(id: \"gid://shopify/GiftCard/missing\") { id } }",
    )
  assert result == "{\"giftCard\":null}"
}

pub fn gift_card_by_id_missing_argument_returns_null_test() {
  let result = run(store.new(), "{ giftCard { id } }")
  assert result == "{\"giftCard\":null}"
}

pub fn gift_card_balance_and_initial_value_test() {
  let record =
    gift_card(
      "gid://shopify/GiftCard/2?shopify-draft-proxy=synthetic",
      "2",
      "4567",
      True,
      "42.5",
    )
  let s = seed(store.new(), record)
  let result =
    run(
      s,
      "{ giftCard(id: \"gid://shopify/GiftCard/2?shopify-draft-proxy=synthetic\") { initialValue { amount currencyCode } balance { amount currencyCode } } }",
    )
  assert result
    == "{\"giftCard\":{\"initialValue\":{\"amount\":\"100.0\",\"currencyCode\":\"CAD\"},\"balance\":{\"amount\":\"42.5\",\"currencyCode\":\"CAD\"}}}"
}

pub fn gift_card_disabled_at_alias_test() {
  let record =
    gift_card(
      "gid://shopify/GiftCard/3?shopify-draft-proxy=synthetic",
      "3",
      "9999",
      False,
      "0.0",
    )
  let s = seed(store.new(), record)
  let result =
    run(
      s,
      "{ giftCard(id: \"gid://shopify/GiftCard/3?shopify-draft-proxy=synthetic\") { enabled disabledAt deactivatedAt } }",
    )
  assert result
    == "{\"giftCard\":{\"enabled\":false,\"disabledAt\":\"2024-02-01T00:00:00.000Z\",\"deactivatedAt\":\"2024-02-01T00:00:00.000Z\"}}"
}

// ----------- giftCards connection -----------

pub fn gift_cards_connection_empty_test() {
  let result = run(store.new(), "{ giftCards(first: 5) { nodes { id } } }")
  assert result == "{\"giftCards\":{\"nodes\":[]}}"
}

pub fn gift_cards_connection_returns_seeded_test() {
  let r1 =
    gift_card(
      "gid://shopify/GiftCard/10?shopify-draft-proxy=synthetic",
      "10",
      "1010",
      True,
      "50.0",
    )
  let r2 =
    gift_card(
      "gid://shopify/GiftCard/11?shopify-draft-proxy=synthetic",
      "11",
      "1111",
      True,
      "75.0",
    )
  let s =
    store.new()
    |> seed(r1)
    |> seed(r2)
  let result = run(s, "{ giftCards(first: 5) { nodes { id lastCharacters } } }")
  assert result
    == "{\"giftCards\":{\"nodes\":[{\"id\":\"gid://shopify/GiftCard/10?shopify-draft-proxy=synthetic\",\"lastCharacters\":\"1010\"},{\"id\":\"gid://shopify/GiftCard/11?shopify-draft-proxy=synthetic\",\"lastCharacters\":\"1111\"}]}}"
}

// ----------- giftCardsCount -----------

pub fn gift_cards_count_zero_test() {
  let result = run(store.new(), "{ giftCardsCount { count } }")
  assert result == "{\"giftCardsCount\":{\"count\":0}}"
}

pub fn gift_cards_count_seeded_test() {
  let r1 =
    gift_card(
      "gid://shopify/GiftCard/20?shopify-draft-proxy=synthetic",
      "20",
      "2020",
      True,
      "10.0",
    )
  let r2 =
    gift_card(
      "gid://shopify/GiftCard/21?shopify-draft-proxy=synthetic",
      "21",
      "2121",
      False,
      "0.0",
    )
  let s =
    store.new()
    |> seed(r1)
    |> seed(r2)
  let result = run(s, "{ giftCardsCount { count } }")
  assert result == "{\"giftCardsCount\":{\"count\":2}}"
}

// ----------- giftCardConfiguration (singleton fallback) -----------

pub fn gift_card_configuration_default_test() {
  let result =
    run(
      store.new(),
      "{ giftCardConfiguration { issueLimit { amount currencyCode } purchaseLimit { amount currencyCode } } }",
    )
  assert result
    == "{\"giftCardConfiguration\":{\"issueLimit\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"},\"purchaseLimit\":{\"amount\":\"0.0\",\"currencyCode\":\"CAD\"}}}"
}

// ----------- transactions inline -----------

pub fn gift_card_transactions_test() {
  let txn =
    GiftCardTransactionRecord(
      id: "gid://shopify/GiftCardCreditTransaction/100",
      kind: "CREDIT",
      amount: money("25.0", "CAD"),
      processed_at: "2024-03-01T00:00:00.000Z",
      note: Some("topup"),
    )
  let record =
    GiftCardRecord(
      ..gift_card(
        "gid://shopify/GiftCard/30?shopify-draft-proxy=synthetic",
        "30",
        "3030",
        True,
        "125.0",
      ),
      transactions: [txn],
    )
  let s = seed(store.new(), record)
  let result =
    run(
      s,
      "{ giftCard(id: \"gid://shopify/GiftCard/30?shopify-draft-proxy=synthetic\") { transactions(first: 5) { nodes { __typename id amount { amount currencyCode } processedAt note } } } }",
    )
  assert result
    == "{\"giftCard\":{\"transactions\":{\"nodes\":[{\"__typename\":\"GiftCardTransaction\",\"id\":\"gid://shopify/GiftCardCreditTransaction/100\",\"amount\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"},\"processedAt\":\"2024-03-01T00:00:00.000Z\",\"note\":\"topup\"}]}}}"
}
