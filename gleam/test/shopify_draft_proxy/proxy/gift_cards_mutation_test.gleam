//// Mutation-path tests for `proxy/gift_cards`.
////
//// Covers all 7 mutation roots
//// (`giftCardCreate`/`Update`/`Credit`/`Debit`/`Deactivate`,
//// `giftCardSendNotificationToCustomer`/`Recipient`), the
//// `process_mutation` `{"data": …}` envelope, the synthetic-id /
//// timestamp threading, and the user-error path on
//// `giftCardCreate { initialValue: 0 }`.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/gift_cards
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type GiftCardRecord, type Money, GiftCardRecord, Money,
}

// ----------- Helpers -----------

fn run_mutation_outcome(
  store_in: store.Store,
  document: String,
) -> mutation_helpers.MutationOutcome {
  let identity = synthetic_identity.new()
  let outcome =
    gift_cards.process_mutation(
      store_in,
      identity,
      "/admin/api/2025-01/graphql.json",
      document,
      dict.new(),
    )
  outcome
}

fn run_mutation(store_in: store.Store, document: String) -> String {
  json.to_string(run_mutation_outcome(store_in, document).data)
}

fn money(amount: String, currency: String) -> Money {
  Money(amount: amount, currency_code: currency)
}

fn seed_card(store_in: store.Store, record: GiftCardRecord) -> store.Store {
  let #(_, s) = store.stage_create_gift_card(store_in, record)
  s
}

// ----------- envelope -----------

pub fn process_mutation_returns_data_envelope_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { giftCardCreate(input: { initialValue: \"50\" }) { giftCard { id initialValue { amount currencyCode } } userErrors { field } } }",
    )
  // Always wraps in `{"data": {...}}`.
  assert body
    == "{\"data\":{\"giftCardCreate\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic\",\"initialValue\":{\"amount\":\"50.0\",\"currencyCode\":\"CAD\"}},\"userErrors\":[]}}}"
}

// ----------- giftCardCreate -----------

pub fn gift_card_create_mints_record_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { giftCardCreate(input: { initialValue: \"75\", note: \"hello\" }) { giftCard { id legacyResourceId enabled balance { amount currencyCode } note } giftCardCode userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  // Synthetic id is #1, default code is `proxy00000001`, last4 is `0001`.
  assert body
    == "{\"data\":{\"giftCardCreate\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic\",\"legacyResourceId\":\"1\",\"enabled\":true,\"balance\":{\"amount\":\"75.0\",\"currencyCode\":\"CAD\"},\"note\":\"hello\"},\"giftCardCode\":\"proxy00000001\",\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids
    == ["gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic"]
  let assert Some(_) =
    store.get_effective_gift_card_by_id(
      outcome.store,
      "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic",
    )
}

pub fn gift_card_create_zero_initial_value_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { giftCardCreate(input: { initialValue: \"0\" }) { giftCard { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardCreate\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"input\",\"initialValue\"],\"message\":\"Initial value must be greater than zero\"}]}}}"
}

// ----------- giftCardUpdate -----------

pub fn gift_card_update_changes_note_test() {
  let existing =
    GiftCardRecord(
      id: "gid://shopify/GiftCard/100?shopify-draft-proxy=synthetic",
      legacy_resource_id: "100",
      last_characters: "1234",
      masked_code: "•••• •••• •••• 1234",
      enabled: True,
      deactivated_at: None,
      expires_on: None,
      note: Some("old note"),
      template_suffix: None,
      created_at: "2024-01-01T00:00:00.000Z",
      updated_at: "2024-01-01T00:00:00.000Z",
      initial_value: money("100.0", "CAD"),
      balance: money("100.0", "CAD"),
      customer_id: None,
      recipient_id: None,
      source: None,
      recipient_attributes: None,
      transactions: [],
    )
  let s = seed_card(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { giftCardUpdate(id: \"gid://shopify/GiftCard/100?shopify-draft-proxy=synthetic\", input: { note: \"new note\" }) { giftCard { id note } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardUpdate\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/100?shopify-draft-proxy=synthetic\",\"note\":\"new note\"},\"userErrors\":[]}}}"
}

// ----------- giftCardCredit -----------

pub fn gift_card_credit_increases_balance_test() {
  let existing =
    GiftCardRecord(
      id: "gid://shopify/GiftCard/200?shopify-draft-proxy=synthetic",
      legacy_resource_id: "200",
      last_characters: "5678",
      masked_code: "•••• •••• •••• 5678",
      enabled: True,
      deactivated_at: None,
      expires_on: None,
      note: None,
      template_suffix: None,
      created_at: "2024-01-01T00:00:00.000Z",
      updated_at: "2024-01-01T00:00:00.000Z",
      initial_value: money("50.0", "CAD"),
      balance: money("50.0", "CAD"),
      customer_id: None,
      recipient_id: None,
      source: None,
      recipient_attributes: None,
      transactions: [],
    )
  let s = seed_card(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { giftCardCredit(id: \"gid://shopify/GiftCard/200?shopify-draft-proxy=synthetic\", creditInput: { creditAmount: { amount: \"25\", currencyCode: \"CAD\" } }) { giftCard { id balance { amount currencyCode } } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardCredit\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/200?shopify-draft-proxy=synthetic\",\"balance\":{\"amount\":\"75.0\",\"currencyCode\":\"CAD\"}},\"userErrors\":[]}}}"
}

// ----------- giftCardDebit -----------

pub fn gift_card_debit_decreases_balance_test() {
  let existing =
    GiftCardRecord(
      id: "gid://shopify/GiftCard/300?shopify-draft-proxy=synthetic",
      legacy_resource_id: "300",
      last_characters: "9999",
      masked_code: "•••• •••• •••• 9999",
      enabled: True,
      deactivated_at: None,
      expires_on: None,
      note: None,
      template_suffix: None,
      created_at: "2024-01-01T00:00:00.000Z",
      updated_at: "2024-01-01T00:00:00.000Z",
      initial_value: money("100.0", "CAD"),
      balance: money("100.0", "CAD"),
      customer_id: None,
      recipient_id: None,
      source: None,
      recipient_attributes: None,
      transactions: [],
    )
  let s = seed_card(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { giftCardDebit(id: \"gid://shopify/GiftCard/300?shopify-draft-proxy=synthetic\", debitInput: { debitAmount: { amount: \"40\", currencyCode: \"CAD\" } }) { giftCard { balance { amount currencyCode } } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardDebit\":{\"giftCard\":{\"balance\":{\"amount\":\"60.0\",\"currencyCode\":\"CAD\"}},\"userErrors\":[]}}}"
}

// ----------- giftCardDeactivate -----------

pub fn gift_card_deactivate_disables_card_test() {
  let existing =
    GiftCardRecord(
      id: "gid://shopify/GiftCard/400?shopify-draft-proxy=synthetic",
      legacy_resource_id: "400",
      last_characters: "1010",
      masked_code: "•••• •••• •••• 1010",
      enabled: True,
      deactivated_at: None,
      expires_on: None,
      note: None,
      template_suffix: None,
      created_at: "2024-01-01T00:00:00.000Z",
      updated_at: "2024-01-01T00:00:00.000Z",
      initial_value: money("25.0", "CAD"),
      balance: money("25.0", "CAD"),
      customer_id: None,
      recipient_id: None,
      source: None,
      recipient_attributes: None,
      transactions: [],
    )
  let s = seed_card(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { giftCardDeactivate(id: \"gid://shopify/GiftCard/400?shopify-draft-proxy=synthetic\") { giftCard { id enabled disabledAt } userErrors { field } } }",
    )
  // First synthetic timestamp consumed during deactivate.
  assert body
    == "{\"data\":{\"giftCardDeactivate\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/400?shopify-draft-proxy=synthetic\",\"enabled\":false,\"disabledAt\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"
}

// ----------- giftCardSendNotificationToCustomer -----------

pub fn gift_card_send_notification_to_customer_returns_card_test() {
  let existing =
    GiftCardRecord(
      id: "gid://shopify/GiftCard/500?shopify-draft-proxy=synthetic",
      legacy_resource_id: "500",
      last_characters: "5555",
      masked_code: "•••• •••• •••• 5555",
      enabled: True,
      deactivated_at: None,
      expires_on: None,
      note: None,
      template_suffix: None,
      created_at: "2024-01-01T00:00:00.000Z",
      updated_at: "2024-01-01T00:00:00.000Z",
      initial_value: money("100.0", "CAD"),
      balance: money("100.0", "CAD"),
      customer_id: Some("gid://shopify/Customer/1"),
      recipient_id: None,
      source: None,
      recipient_attributes: None,
      transactions: [],
    )
  let s = seed_card(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { giftCardSendNotificationToCustomer(giftCardId: \"gid://shopify/GiftCard/500?shopify-draft-proxy=synthetic\") { giftCard { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/500?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
}

// ----------- giftCardSendNotificationToRecipient -----------

pub fn gift_card_send_notification_to_recipient_returns_card_test() {
  let existing =
    GiftCardRecord(
      id: "gid://shopify/GiftCard/600?shopify-draft-proxy=synthetic",
      legacy_resource_id: "600",
      last_characters: "6666",
      masked_code: "•••• •••• •••• 6666",
      enabled: True,
      deactivated_at: None,
      expires_on: None,
      note: None,
      template_suffix: None,
      created_at: "2024-01-01T00:00:00.000Z",
      updated_at: "2024-01-01T00:00:00.000Z",
      initial_value: money("100.0", "CAD"),
      balance: money("100.0", "CAD"),
      customer_id: None,
      recipient_id: Some("gid://shopify/Customer/2"),
      source: None,
      recipient_attributes: None,
      transactions: [],
    )
  let s = seed_card(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { giftCardSendNotificationToRecipient(giftCardId: \"gid://shopify/GiftCard/600?shopify-draft-proxy=synthetic\") { giftCard { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/600?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
}

// ----------- is_gift_card_mutation_root -----------

pub fn is_gift_card_mutation_root_predicate_test() {
  assert gift_cards.is_gift_card_mutation_root("giftCardCreate")
  assert !gift_cards.is_gift_card_mutation_root("giftCard")
}
