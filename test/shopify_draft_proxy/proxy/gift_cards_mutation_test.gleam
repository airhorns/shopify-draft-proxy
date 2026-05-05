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
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/proxy/gift_cards
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CustomerRecord, type GiftCardRecord, type Money, type ShopRecord,
  CustomerDefaultEmailAddressRecord, CustomerDefaultPhoneNumberRecord,
  CustomerRecord, GiftCardRecord, Money, PaymentSettingsRecord,
  ShopAddressRecord, ShopBundlesFeatureRecord,
  ShopCartTransformEligibleOperationsRecord, ShopCartTransformFeatureRecord,
  ShopDomainRecord, ShopFeaturesRecord, ShopPlanRecord, ShopRecord,
  ShopResourceLimitsRecord,
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
      empty_upstream_context(),
    )
  outcome
}

fn run_mutation(store_in: store.Store, document: String) -> String {
  json.to_string(run_mutation_outcome(store_in, document).data)
}

fn run_customer_notification(store_in: store.Store, id: String) -> String {
  run_mutation(
    store_in,
    "mutation { giftCardSendNotificationToCustomer(giftCardId: \""
      <> id
      <> "\") { giftCard { id } userErrors { field code message } } }",
  )
}

fn run_recipient_notification(store_in: store.Store, id: String) -> String {
  run_mutation(
    store_in,
    "mutation { giftCardSendNotificationToRecipient(giftCardId: \""
      <> id
      <> "\") { giftCard { id } userErrors { field code message } } }",
  )
}

fn money(amount: String, currency: String) -> Money {
  Money(amount: amount, currency_code: currency)
}

fn seed_card(store_in: store.Store, record: GiftCardRecord) -> store.Store {
  let #(_, s) = store.stage_create_gift_card(store_in, record)
  s
}

fn notification_card(
  id: String,
  customer_id: Option(String),
  recipient_id: Option(String),
) -> GiftCardRecord {
  GiftCardRecord(
    id: id,
    legacy_resource_id: "notification",
    last_characters: "5555",
    masked_code: "•••• •••• •••• 5555",
    enabled: True,
    notify: True,
    deactivated_at: None,
    expires_on: None,
    note: None,
    template_suffix: None,
    created_at: "2024-01-01T00:00:00.000Z",
    updated_at: "2024-01-01T00:00:00.000Z",
    initial_value: money("100.0", "CAD"),
    balance: money("100.0", "CAD"),
    customer_id: customer_id,
    recipient_id: recipient_id,
    source: None,
    recipient_attributes: None,
    transactions: [],
  )
}

fn transaction_card(
  id: String,
  enabled: Bool,
  expires_on: Option(String),
  balance: String,
  currency: String,
) -> GiftCardRecord {
  GiftCardRecord(
    id: id,
    legacy_resource_id: "transaction",
    last_characters: "7777",
    masked_code: "•••• •••• •••• 7777",
    enabled: enabled,
    notify: True,
    deactivated_at: case enabled {
      True -> None
      False -> Some("2024-01-02T00:00:00.000Z")
    },
    expires_on: expires_on,
    note: None,
    template_suffix: None,
    created_at: "2024-01-01T00:00:00.000Z",
    updated_at: "2024-01-01T00:00:00.000Z",
    initial_value: money(balance, currency),
    balance: money(balance, currency),
    customer_id: None,
    recipient_id: None,
    source: None,
    recipient_attributes: None,
    transactions: [],
  )
}

fn seed_customer(store_in: store.Store, record: CustomerRecord) -> store.Store {
  let #(_, s) = store.stage_create_customer(store_in, record)
  s
}

fn customer(id: String, email: Option(String), phone: Option(String)) {
  CustomerRecord(
    id: id,
    first_name: Some("Ada"),
    last_name: Some("Lovelace"),
    display_name: Some("Ada Lovelace"),
    email: email,
    legacy_resource_id: None,
    locale: Some("en"),
    note: None,
    can_delete: Some(True),
    verified_email: Some(True),
    data_sale_opt_out: False,
    tax_exempt: Some(False),
    tax_exemptions: [],
    state: Some("DISABLED"),
    tags: [],
    number_of_orders: Some("0"),
    amount_spent: Some(money("0.0", "CAD")),
    default_email_address: Some(CustomerDefaultEmailAddressRecord(
      email_address: email,
      marketing_state: None,
      marketing_opt_in_level: None,
      marketing_updated_at: None,
    )),
    default_phone_number: Some(CustomerDefaultPhoneNumberRecord(
      phone_number: phone,
      marketing_state: None,
      marketing_opt_in_level: None,
      marketing_updated_at: None,
      marketing_collected_from: None,
    )),
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    created_at: Some("2024-01-01T00:00:00.000Z"),
    updated_at: Some("2024-01-01T00:00:00.000Z"),
  )
}

fn trial_shop() -> ShopRecord {
  ShopRecord(
    id: "gid://shopify/Shop/1",
    name: "Trial Shop",
    myshopify_domain: "trial-shop.myshopify.com",
    url: "https://trial-shop.myshopify.com",
    primary_domain: ShopDomainRecord(
      id: "gid://shopify/Domain/1",
      host: "trial-shop.myshopify.com",
      url: "https://trial-shop.myshopify.com",
      ssl_enabled: True,
    ),
    contact_email: "shop@example.com",
    email: "shop@example.com",
    currency_code: "CAD",
    enabled_presentment_currencies: ["CAD"],
    iana_timezone: "America/Toronto",
    timezone_abbreviation: "EST",
    timezone_offset: "-0500",
    timezone_offset_minutes: -300,
    taxes_included: False,
    tax_shipping: False,
    unit_system: "METRIC_SYSTEM",
    weight_unit: "KILOGRAMS",
    shop_address: ShopAddressRecord(
      id: "gid://shopify/ShopAddress/1",
      address1: None,
      address2: None,
      city: None,
      company: None,
      coordinates_validated: False,
      country: None,
      country_code_v2: None,
      formatted: [],
      formatted_area: None,
      latitude: None,
      longitude: None,
      phone: None,
      province: None,
      province_code: None,
      zip: None,
    ),
    plan: ShopPlanRecord(
      partner_development: False,
      public_display_name: "Trial",
      shopify_plus: False,
    ),
    resource_limits: ShopResourceLimitsRecord(
      location_limit: 1,
      max_product_options: 3,
      max_product_variants: 100,
      redirect_limit_reached: False,
    ),
    features: ShopFeaturesRecord(
      avalara_avatax: False,
      branding: "SHOPIFY",
      bundles: ShopBundlesFeatureRecord(
        eligible_for_bundles: False,
        ineligibility_reason: None,
        sells_bundles: False,
      ),
      captcha: False,
      cart_transform: ShopCartTransformFeatureRecord(
        eligible_operations: ShopCartTransformEligibleOperationsRecord(
          expand_operation: False,
          merge_operation: False,
          update_operation: False,
        ),
      ),
      dynamic_remarketing: False,
      eligible_for_subscription_migration: False,
      eligible_for_subscriptions: False,
      gift_cards: True,
      harmonized_system_code: False,
      legacy_subscription_gateway_enabled: False,
      live_view: False,
      paypal_express_subscription_gateway_status: "DISABLED",
      reports: False,
      sells_subscriptions: False,
      show_metrics: False,
      storefront: True,
      unified_markets: False,
    ),
    payment_settings: PaymentSettingsRecord(supported_digital_wallets: []),
    shop_policies: [],
  )
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
      notify: True,
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
  let s =
    store.new()
    |> seed_card(existing)
    |> seed_customer(customer(
      "gid://shopify/Customer/1",
      Some("ada@example.com"),
      None,
    ))
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
      notify: True,
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
  let s =
    store.new()
    |> seed_card(existing)
    |> seed_customer(customer(
      "gid://shopify/Customer/2",
      Some("ada@example.com"),
      None,
    ))
  let body =
    run_mutation(
      s,
      "mutation { giftCardCredit(id: \"gid://shopify/GiftCard/200?shopify-draft-proxy=synthetic\", creditInput: { creditAmount: { amount: \"25\", currencyCode: \"CAD\" } }) { giftCard { id balance { amount currencyCode } } giftCardCreditTransaction { __typename amount { amount currencyCode } } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardCredit\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/200?shopify-draft-proxy=synthetic\",\"balance\":{\"amount\":\"75.0\",\"currencyCode\":\"CAD\"}},\"giftCardCreditTransaction\":{\"__typename\":\"GiftCardCreditTransaction\",\"amount\":{\"amount\":\"25.0\",\"currencyCode\":\"CAD\"}},\"userErrors\":[]}}}"
}

pub fn gift_card_credit_rejects_expired_card_without_mutating_test() {
  let id = "gid://shopify/GiftCard/credit-expired"
  let s =
    store.new()
    |> seed_card(transaction_card(id, True, Some("2000-01-01"), "10.0", "USD"))
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { giftCardCredit(id: \"gid://shopify/GiftCard/credit-expired\", creditInput: { creditAmount: { amount: \"5\", currencyCode: \"USD\" } }) { giftCardCreditTransaction { __typename } userErrors { field code message } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"giftCardCredit\":{\"giftCardCreditTransaction\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"INVALID\",\"message\":\"The gift card has expired.\"}]}}}"
  let assert Some(after) =
    store.get_effective_gift_card_by_id(outcome.store, id)
  assert after.balance == money("10.0", "USD")
  assert after.transactions == []
}

pub fn gift_card_credit_rejects_currency_mismatch_without_mutating_test() {
  let id = "gid://shopify/GiftCard/credit-currency"
  let s =
    store.new()
    |> seed_card(transaction_card(id, True, Some("2099-01-01"), "10.0", "USD"))
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { giftCardCredit(id: \"gid://shopify/GiftCard/credit-currency\", creditInput: { creditAmount: { amount: \"5\", currencyCode: \"EUR\" } }) { giftCard { balance { amount currencyCode } } giftCardCreditTransaction { __typename amount { amount currencyCode } } userErrors { field code message } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"giftCardCredit\":{\"giftCard\":null,\"giftCardCreditTransaction\":null,\"userErrors\":[{\"field\":[\"creditInput\",\"creditAmount\",\"currencyCode\"],\"code\":\"MISMATCHING_CURRENCY\",\"message\":\"The currency provided does not match the currency of the gift card.\"}]}}}"
  let assert Some(after) =
    store.get_effective_gift_card_by_id(outcome.store, id)
  assert after.balance == money("10.0", "USD")
  assert after.transactions == []
}

pub fn gift_card_credit_rejects_processed_at_bounds_test() {
  let id = "gid://shopify/GiftCard/credit-processed-at"
  let s =
    store.new()
    |> seed_card(transaction_card(id, True, Some("2099-01-01"), "10.0", "USD"))
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { future: giftCardCredit(id: \"gid://shopify/GiftCard/credit-processed-at\", creditInput: { processedAt: \"2099-01-01T00:00:00Z\", creditAmount: { amount: \"5\", currencyCode: \"USD\" } }) { giftCardCreditTransaction { __typename } userErrors { field code message } } preEpoch: giftCardCredit(id: \"gid://shopify/GiftCard/credit-processed-at\", creditInput: { processedAt: \"1969-12-31T23:59:59Z\", creditAmount: { amount: \"5\", currencyCode: \"USD\" } }) { giftCardCreditTransaction { __typename } userErrors { field code message } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"future\":{\"giftCardCreditTransaction\":null,\"userErrors\":[{\"field\":[\"creditInput\",\"processedAt\"],\"code\":\"INVALID\",\"message\":\"The processed date must not be in the future.\"}]},\"preEpoch\":{\"giftCardCreditTransaction\":null,\"userErrors\":[{\"field\":[\"creditInput\",\"processedAt\"],\"code\":\"INVALID\",\"message\":\"A valid processed date must be used.\"}]}}}"
  let assert Some(after) =
    store.get_effective_gift_card_by_id(outcome.store, id)
  assert after.balance == money("10.0", "USD")
  assert after.transactions == []
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
      notify: True,
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
  let s =
    store.new()
    |> seed_card(existing)
    |> seed_customer(customer(
      "gid://shopify/Customer/1",
      Some("ada@example.com"),
      None,
    ))
  let body =
    run_mutation(
      s,
      "mutation { giftCardDebit(id: \"gid://shopify/GiftCard/300?shopify-draft-proxy=synthetic\", debitInput: { debitAmount: { amount: \"40\", currencyCode: \"CAD\" } }) { giftCard { balance { amount currencyCode } } giftCardDebitTransaction { __typename amount { amount currencyCode } } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardDebit\":{\"giftCard\":{\"balance\":{\"amount\":\"60.0\",\"currencyCode\":\"CAD\"}},\"giftCardDebitTransaction\":{\"__typename\":\"GiftCardDebitTransaction\",\"amount\":{\"amount\":\"-40.0\",\"currencyCode\":\"CAD\"}},\"userErrors\":[]}}}"
}

pub fn gift_card_debit_rejects_deactivated_card_without_mutating_test() {
  let id = "gid://shopify/GiftCard/debit-deactivated"
  let s =
    store.new()
    |> seed_card(transaction_card(id, False, Some("2099-01-01"), "10.0", "USD"))
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { giftCardDebit(id: \"gid://shopify/GiftCard/debit-deactivated\", debitInput: { debitAmount: { amount: \"5\", currencyCode: \"USD\" } }) { giftCardDebitTransaction { __typename } userErrors { field code message } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"giftCardDebit\":{\"giftCardDebitTransaction\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"INVALID\",\"message\":\"The gift card is deactivated.\"}]}}}"
  let assert Some(after) =
    store.get_effective_gift_card_by_id(outcome.store, id)
  assert after.balance == money("10.0", "USD")
  assert after.transactions == []
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
      notify: True,
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
  let s =
    store.new()
    |> seed_card(existing)
    |> seed_customer(customer(
      "gid://shopify/Customer/2",
      Some("ada@example.com"),
      None,
    ))
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
      notify: True,
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
  let s =
    store.new()
    |> seed_card(existing)
    |> seed_customer(customer(
      "gid://shopify/Customer/1",
      Some("ada@example.com"),
      None,
    ))
  let body =
    run_mutation(
      s,
      "mutation { giftCardSendNotificationToCustomer(giftCardId: \"gid://shopify/GiftCard/500?shopify-draft-proxy=synthetic\") { giftCard { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/500?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
}

pub fn gift_card_send_notification_rejects_trial_shop_test() {
  let id = "gid://shopify/GiftCard/notification-trial"
  let s =
    store.new()
    |> store.upsert_base_shop(trial_shop())
    |> seed_card(notification_card(
      id,
      Some("gid://shopify/Customer/trial"),
      None,
    ))
    |> seed_customer(customer(
      "gid://shopify/Customer/trial",
      Some("ada@example.com"),
      None,
    ))

  assert run_customer_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"base\"],\"code\":\"INVALID\",\"message\":\"Gift card notifications are not available for trial shops.\"}]}}}"
}

pub fn gift_card_send_notification_to_customer_rejects_missing_card_test() {
  assert run_customer_notification(
      store.new(),
      "gid://shopify/GiftCard/notification-missing",
    )
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"GIFT_CARD_NOT_FOUND\",\"message\":\"The gift card could not be found.\"}]}}}"
}

pub fn gift_card_send_notification_rejects_notify_false_test() {
  let id = "gid://shopify/GiftCard/notification-disabled"
  let card =
    GiftCardRecord(
      ..notification_card(id, Some("gid://shopify/Customer/notify"), None),
      notify: False,
    )
  let s =
    store.new()
    |> seed_card(card)
    |> seed_customer(customer(
      "gid://shopify/Customer/notify",
      Some("ada@example.com"),
      None,
    ))

  assert run_customer_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"INVALID\",\"message\":\"Gift card notifications are disabled.\"}]}}}"
}

pub fn gift_card_send_notification_rejects_expired_before_deactivated_test() {
  let id = "gid://shopify/GiftCard/notification-expired"
  let card =
    GiftCardRecord(
      ..notification_card(id, Some("gid://shopify/Customer/expired"), None),
      enabled: False,
      deactivated_at: Some("2024-01-02T00:00:00.000Z"),
      expires_on: Some("2000-01-01"),
    )
  let s =
    store.new()
    |> seed_card(card)
    |> seed_customer(customer(
      "gid://shopify/Customer/expired",
      Some("ada@example.com"),
      None,
    ))

  assert run_customer_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"INVALID\",\"message\":\"The gift card has expired.\"}]}}}"
}

pub fn gift_card_send_notification_rejects_deactivated_test() {
  let id = "gid://shopify/GiftCard/notification-deactivated"
  let card =
    GiftCardRecord(
      ..notification_card(id, Some("gid://shopify/Customer/deactivated"), None),
      enabled: False,
      deactivated_at: Some("2024-01-02T00:00:00.000Z"),
    )
  let s =
    store.new()
    |> seed_card(card)
    |> seed_customer(customer(
      "gid://shopify/Customer/deactivated",
      Some("ada@example.com"),
      None,
    ))

  assert run_customer_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"INVALID\",\"message\":\"The gift card is deactivated.\"}]}}}"
}

pub fn gift_card_send_notification_to_customer_requires_customer_id_test() {
  let id = "gid://shopify/GiftCard/notification-no-customer"
  let s = seed_card(store.new(), notification_card(id, None, None))

  assert run_customer_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"base\"],\"code\":\"INVALID\",\"message\":\"The gift card has no customer.\"}]}}}"
}

pub fn gift_card_send_notification_to_customer_rejects_missing_customer_test() {
  let id = "gid://shopify/GiftCard/notification-missing-customer"
  let s =
    seed_card(
      store.new(),
      notification_card(id, Some("gid://shopify/Customer/missing"), None),
    )

  assert run_customer_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"base\"],\"code\":\"CUSTOMER_NOT_FOUND\",\"message\":\"The customer could not be found.\"}]}}}"
}

pub fn gift_card_send_notification_to_customer_requires_contact_info_test() {
  let id = "gid://shopify/GiftCard/notification-no-contact"
  let s =
    store.new()
    |> seed_card(notification_card(
      id,
      Some("gid://shopify/Customer/no-contact"),
      None,
    ))
    |> seed_customer(customer("gid://shopify/Customer/no-contact", None, None))

  assert run_customer_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToCustomer\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"base\"],\"code\":\"INVALID\",\"message\":\"The customer has no contact information (e.g. email address or phone number).\"}]}}}"
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
      notify: True,
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
  let s =
    store.new()
    |> seed_card(existing)
    |> seed_customer(customer(
      "gid://shopify/Customer/2",
      Some("ada@example.com"),
      None,
    ))
  let body =
    run_mutation(
      s,
      "mutation { giftCardSendNotificationToRecipient(giftCardId: \"gid://shopify/GiftCard/600?shopify-draft-proxy=synthetic\") { giftCard { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":{\"id\":\"gid://shopify/GiftCard/600?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"
}

pub fn gift_card_send_notification_to_recipient_rejects_missing_card_test() {
  assert run_recipient_notification(
      store.new(),
      "gid://shopify/GiftCard/notification-missing",
    )
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"GIFT_CARD_NOT_FOUND\",\"message\":\"The gift card could not be found.\"}]}}}"
}

pub fn gift_card_send_notification_to_recipient_rejects_notify_false_test() {
  let id = "gid://shopify/GiftCard/notification-recipient-disabled"
  let card =
    GiftCardRecord(
      ..notification_card(id, None, Some("gid://shopify/Customer/notify")),
      notify: False,
    )
  let s =
    store.new()
    |> seed_card(card)
    |> seed_customer(customer(
      "gid://shopify/Customer/notify",
      Some("ada@example.com"),
      None,
    ))

  assert run_recipient_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"INVALID\",\"message\":\"Gift card notifications are disabled.\"}]}}}"
}

pub fn gift_card_send_notification_to_recipient_rejects_expired_test() {
  let id = "gid://shopify/GiftCard/notification-recipient-expired"
  let card =
    GiftCardRecord(
      ..notification_card(id, None, Some("gid://shopify/Customer/expired")),
      expires_on: Some("2000-01-01"),
    )
  let s =
    store.new()
    |> seed_card(card)
    |> seed_customer(customer(
      "gid://shopify/Customer/expired",
      Some("ada@example.com"),
      None,
    ))

  assert run_recipient_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"INVALID\",\"message\":\"The gift card has expired.\"}]}}}"
}

pub fn gift_card_send_notification_to_recipient_rejects_deactivated_test() {
  let id = "gid://shopify/GiftCard/notification-recipient-deactivated"
  let card =
    GiftCardRecord(
      ..notification_card(id, None, Some("gid://shopify/Customer/deactivated")),
      enabled: False,
      deactivated_at: Some("2024-01-02T00:00:00.000Z"),
    )
  let s =
    store.new()
    |> seed_card(card)
    |> seed_customer(customer(
      "gid://shopify/Customer/deactivated",
      Some("ada@example.com"),
      None,
    ))

  assert run_recipient_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"id\"],\"code\":\"INVALID\",\"message\":\"The gift card is deactivated.\"}]}}}"
}

pub fn gift_card_send_notification_to_recipient_requires_recipient_id_test() {
  let id = "gid://shopify/GiftCard/notification-no-recipient"
  let s = seed_card(store.new(), notification_card(id, None, None))

  assert run_recipient_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"base\"],\"code\":\"INVALID\",\"message\":\"The gift card has no recipient.\"}]}}}"
}

pub fn gift_card_send_notification_to_recipient_rejects_missing_recipient_test() {
  let id = "gid://shopify/GiftCard/notification-missing-recipient"
  let s =
    seed_card(
      store.new(),
      notification_card(id, None, Some("gid://shopify/Customer/missing")),
    )

  assert run_recipient_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"base\"],\"code\":\"RECIPIENT_NOT_FOUND\",\"message\":\"The recipient could not be found.\"}]}}}"
}

pub fn gift_card_send_notification_to_recipient_requires_contact_info_test() {
  let id = "gid://shopify/GiftCard/notification-recipient-no-contact"
  let s =
    store.new()
    |> seed_card(notification_card(
      id,
      None,
      Some("gid://shopify/Customer/recipient-no-contact"),
    ))
    |> seed_customer(customer(
      "gid://shopify/Customer/recipient-no-contact",
      None,
      None,
    ))

  assert run_recipient_notification(s, id)
    == "{\"data\":{\"giftCardSendNotificationToRecipient\":{\"giftCard\":null,\"userErrors\":[{\"field\":[\"base\"],\"code\":\"INVALID\",\"message\":\"The recipient has no contact information (e.g. email address or phone number).\"}]}}}"
}

// ----------- is_gift_card_mutation_root -----------

pub fn is_gift_card_mutation_root_predicate_test() {
  assert gift_cards.is_gift_card_mutation_root("giftCardCreate")
  assert !gift_cards.is_gift_card_mutation_root("giftCard")
}
