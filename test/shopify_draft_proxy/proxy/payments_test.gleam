import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/payments
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, DraftProxy, Request, Response,
}
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CustomerPaymentMethodRecord,
  type CustomerPaymentMethodSubscriptionContractRecord, type CustomerRecord,
  type DraftOrderRecord, type Money, type OrderRecord,
  type PaymentScheduleRecord, type PaymentTermsRecord, type ShopRecord,
  CapturedArray, CapturedBool, CapturedNull, CapturedObject, CapturedString,
  CustomerPaymentMethodInstrumentRecord, CustomerPaymentMethodRecord,
  CustomerPaymentMethodSubscriptionContractRecord, CustomerRecord,
  DraftOrderRecord, Money, OrderRecord, PaymentScheduleRecord,
  PaymentSettingsRecord, PaymentTermsRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopFeaturesRecord,
  ShopPlanRecord, ShopRecord, ShopResourceLimitsRecord, ShopifyFunctionRecord,
}

const customer_id = "gid://shopify/Customer/1"

const payment_method_id = "gid://shopify/CustomerPaymentMethod/base-card"

fn empty_vars() {
  dict.new()
}

fn customer() {
  CustomerRecord(
    id: customer_id,
    first_name: Some("Draft"),
    last_name: Some("Buyer"),
    display_name: Some("Draft Buyer"),
    email: Some("buyer@example.com"),
    legacy_resource_id: Some("1"),
    locale: None,
    note: None,
    can_delete: None,
    verified_email: None,
    data_sale_opt_out: False,
    tax_exempt: None,
    tax_exemptions: [],
    state: Some("ENABLED"),
    tags: [],
    number_of_orders: None,
    amount_spent: None,
    default_email_address: None,
    default_phone_number: None,
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    account_activation_token: None,
    created_at: None,
    updated_at: None,
  )
}

fn contract() {
  CustomerPaymentMethodSubscriptionContractRecord(
    id: "gid://shopify/SubscriptionContract/active",
    cursor: None,
    data: dict.from_list([#("status", "ACTIVE")]),
  )
}

fn method_with(
  contracts: List(CustomerPaymentMethodSubscriptionContractRecord),
  revoked_at: Option(String),
  revoked_reason: Option(String),
) {
  CustomerPaymentMethodRecord(
    id: payment_method_id,
    customer_id: customer_id,
    cursor: None,
    instrument: Some(CustomerPaymentMethodInstrumentRecord(
      type_name: "CustomerCreditCard",
      data: dict.from_list([
        #("lastDigits", "4242"),
        #("maskedNumber", "**** **** **** 4242"),
      ]),
    )),
    revoked_at: revoked_at,
    revoked_reason: revoked_reason,
    subscription_contracts: contracts,
  )
}

fn seeded_store(method: CustomerPaymentMethodRecord) {
  store.new()
  |> store.upsert_base_customers([customer()])
  |> store.upsert_base_customer_payment_methods([method])
}

fn revoke_document(id: String) {
  "mutation { customerPaymentMethodRevoke(customerPaymentMethodId: \""
  <> id
  <> "\") { revokedCustomerPaymentMethodId userErrors { field message code } } }"
}

fn revoke(source: store.Store, id: String) {
  payments.process_mutation(
    source,
    synthetic_identity.new(),
    "/admin/api/2026-04/graphql.json",
    revoke_document(id),
    empty_vars(),
    upstream_query.empty_upstream_context(),
  )
}

fn graphql_seeded(source: store.Store, document: String) {
  let proxy = draft_proxy.new()
  let proxy = DraftProxy(..proxy, store: source)
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"" <> escape(document) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn graphql(proxy: DraftProxy, query: String) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"" <> escape(query) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn seed_shopify_function(proxy: DraftProxy, handle: String) -> DraftProxy {
  let record =
    ShopifyFunctionRecord(
      id: "gid://shopify/ShopifyFunction/" <> handle,
      title: Some("Function " <> handle),
      handle: Some(handle),
      api_type: Some("PAYMENT_CUSTOMIZATION"),
      description: None,
      app_key: None,
      app: None,
    )
  let #(_, next_store) =
    store.upsert_staged_shopify_function(proxy.store, record)
  DraftProxy(..proxy, store: next_store)
}

fn graphql_with_proxy(proxy: DraftProxy, query: String) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"" <> escape(query) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn meta_state_json(proxy: DraftProxy) {
  let #(Response(body: body, ..), _) =
    draft_proxy.process_request(
      proxy,
      Request(
        method: "GET",
        path: "/__meta/state",
        headers: dict.new(),
        body: "",
      ),
    )
  json.to_string(body)
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
  |> string.replace("\n", "\\n")
}

fn proxy_with_customer() {
  let proxy = draft_proxy.new()
  DraftProxy(
    ..proxy,
    store: store.upsert_base_customers(proxy.store, [
      CustomerRecord(
        id: "gid://shopify/Customer/1",
        first_name: None,
        last_name: None,
        display_name: Some("Payments Repro"),
        email: Some("payments-repro@example.test"),
        legacy_resource_id: Some("1"),
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
        amount_spent: None,
        default_email_address: None,
        default_phone_number: None,
        email_marketing_consent: None,
        sms_marketing_consent: None,
        default_address: None,
        account_activation_token: None,
        created_at: None,
        updated_at: None,
      ),
    ]),
  )
}

pub fn remote_payment_method_rejects_blank_stripe_customer_id_test() {
  let query =
    "mutation { customerPaymentMethodRemoteCreate(customerId: \"gid://shopify/Customer/1\", remoteReference: { stripePaymentMethod: { customerId: null, paymentMethodId: \"pm_x\" } }) { customerPaymentMethod { id } userErrors { field code message } } }"
  let #(Response(status: status, body: body, ..), _) =
    graphql(proxy_with_customer(), query)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"customerPaymentMethod\":null")
  assert string.contains(
    body_json,
    "\"field\":[\"remote_reference\",\"stripe_payment_method\",\"customer_id\"]",
  )
  assert string.contains(body_json, "\"code\":\"STRIPE_CUSTOMER_ID_BLANK\"")
  assert !string.contains(body_json, "CustomerPaymentMethod/3")
}

pub fn credit_card_create_requires_billing_address_fields_test() {
  let query =
    "mutation { customerPaymentMethodCreditCardCreate(customerId: \"gid://shopify/Customer/1\", sessionId: \"sess_valid\", billingAddress: { address1: null, city: null, zip: null, country: null, province: null }) { customerPaymentMethod { id } processing userErrors { field code message } } }"
  let #(Response(status: status, body: body, ..), _) =
    graphql(proxy_with_customer(), query)
  let response = json.to_string(body)

  assert status == 200
  assert string.contains(response, "\"customerPaymentMethod\":null")
  assert string.contains(response, "\"processing\":false")
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"address1\"]",
  )
  assert string.contains(response, "\"field\":[\"billing_address\",\"city\"]")
  assert string.contains(response, "\"field\":[\"billing_address\",\"zip\"]")
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"country_code\"]",
  )
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"province_code\"]",
  )
  assert string.contains(response, "\"code\":\"BLANK\"")
}

pub fn credit_card_create_requires_session_id_test() {
  let query =
    "mutation { customerPaymentMethodCreditCardCreate(customerId: \"gid://shopify/Customer/1\", billingAddress: { address1: \"1 Main St\", city: \"New York\", zip: \"10001\", country: \"US\", province: \"NY\" }) { customerPaymentMethod { id } processing userErrors { field code message } } }"
  let #(Response(status: status, body: body, ..), _) =
    graphql(proxy_with_customer(), query)
  let response = json.to_string(body)

  assert status == 200
  assert string.contains(response, "\"errors\"")
  assert string.contains(response, "sessionId")
}

pub fn credit_card_create_stores_billing_address_for_readback_test() {
  let create_query =
    "mutation { customerPaymentMethodCreditCardCreate(customerId: \"gid://shopify/Customer/1\", sessionId: \"sess_valid\", billingAddress: { firstName: \"Ada\", lastName: \"Lovelace\", address1: \"1 Main St\", city: \"New York\", zip: \"10001\", country: \"US\", province: \"NY\" }) { customerPaymentMethod { id instrument { __typename ... on CustomerCreditCard { billingAddress { firstName lastName address1 city zip countryCodeV2 provinceCode } } } } processing userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy_with_customer(), create_query)
  let create_response = json.to_string(create_body)

  assert create_status == 200
  assert string.contains(
    create_response,
    "\"id\":\"gid://shopify/CustomerPaymentMethod/1\"",
  )
  assert string.contains(create_response, "\"processing\":false")
  assert string.contains(create_response, "\"userErrors\":[]")
  assert string.contains(create_response, "\"address1\":\"1 Main St\"")
  assert string.contains(create_response, "\"countryCodeV2\":\"US\"")
  assert string.contains(create_response, "\"provinceCode\":\"NY\"")

  let read_query =
    "query { customerPaymentMethod(id: \"gid://shopify/CustomerPaymentMethod/1\") { id instrument { __typename ... on CustomerCreditCard { billingAddress { firstName lastName address1 city zip countryCodeV2 provinceCode } } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  let read_response = json.to_string(read_body)

  assert read_status == 200
  assert string.contains(read_response, "\"address1\":\"1 Main St\"")
  assert string.contains(read_response, "\"city\":\"New York\"")
  assert string.contains(read_response, "\"zip\":\"10001\"")
  assert string.contains(read_response, "\"countryCodeV2\":\"US\"")
  assert string.contains(read_response, "\"provinceCode\":\"NY\"")
}

pub fn credit_card_create_can_return_processing_state_test() {
  let query =
    "mutation { customerPaymentMethodCreditCardCreate(customerId: \"gid://shopify/Customer/1\", sessionId: \"shopify-draft-proxy:processing\", billingAddress: { address1: \"1 Main St\", city: \"New York\", zip: \"10001\", country: \"US\", province: \"NY\" }) { customerPaymentMethod { id } processing userErrors { field code message } } }"
  let #(Response(status: status, body: body, ..), _) =
    graphql(proxy_with_customer(), query)
  let response = json.to_string(body)

  assert status == 200
  assert string.contains(response, "\"customerPaymentMethod\":null")
  assert string.contains(response, "\"processing\":true")
  assert string.contains(response, "\"userErrors\":[]")
  assert !string.contains(response, "CustomerPaymentMethod/1")
}

pub fn credit_card_update_requires_billing_address_fields_test() {
  let query =
    "mutation { customerPaymentMethodCreditCardUpdate(id: \"gid://shopify/CustomerPaymentMethod/base-card\", sessionId: \"sess_valid\", billingAddress: { address1: null, city: null, zip: null, country: null, province: null }) { customerPaymentMethod { id } processing userErrors { field code message } } }"
  let #(Response(status: status, body: body, ..), _) =
    graphql_seeded(seeded_store(method_with([], None, None)), query)
  let response = json.to_string(body)

  assert status == 200
  assert string.contains(response, "\"customerPaymentMethod\":null")
  assert string.contains(response, "\"processing\":false")
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"address1\"]",
  )
  assert string.contains(response, "\"field\":[\"billing_address\",\"city\"]")
  assert string.contains(response, "\"field\":[\"billing_address\",\"zip\"]")
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"country_code\"]",
  )
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"province_code\"]",
  )
  assert string.contains(response, "\"code\":\"BLANK\"")
}

pub fn remote_payment_method_rejects_blank_gateway_fields_test() {
  let paypal_query =
    "mutation { customerPaymentMethodRemoteCreate(customerId: \"gid://shopify/Customer/1\", remoteReference: { paypalPaymentMethod: { } }) { customerPaymentMethod { id } userErrors { field code message } } }"
  let #(Response(status: paypal_status, body: paypal_body, ..), _) =
    graphql(proxy_with_customer(), paypal_query)
  assert paypal_status == 200
  let paypal_json = json.to_string(paypal_body)
  assert string.contains(paypal_json, "\"customerPaymentMethod\":null")
  assert string.contains(
    paypal_json,
    "\"field\":[\"remote_reference\",\"paypal_payment_method\",\"billing_agreement_id\"]",
  )
  assert string.contains(paypal_json, "\"code\":\"BILLING_AGREEMENT_ID_BLANK\"")

  let braintree_query =
    "mutation { customerPaymentMethodRemoteCreate(customerId: \"gid://shopify/Customer/1\", remoteReference: { braintreePaymentMethod: { customerId: \"\", paymentMethodToken: null } }) { customerPaymentMethod { id } userErrors { field code message } } }"
  let #(Response(status: braintree_status, body: braintree_body, ..), _) =
    graphql(proxy_with_customer(), braintree_query)
  assert braintree_status == 200
  let braintree_json = json.to_string(braintree_body)
  assert string.contains(
    braintree_json,
    "\"code\":\"BRAINTREE_CUSTOMER_ID_BLANK\"",
  )
  assert string.contains(
    braintree_json,
    "\"code\":\"PAYMENT_METHOD_TOKEN_BLANK\"",
  )

  let authorize_net_query =
    "mutation { customerPaymentMethodRemoteCreate(customerId: \"gid://shopify/Customer/1\", remoteReference: { authorizeNetCustomerPaymentProfile: { customerProfileId: \"\" } }) { customerPaymentMethod { id } userErrors { field code message } } }"
  let #(Response(status: authorize_net_status, body: authorize_net_body, ..), _) =
    graphql(proxy_with_customer(), authorize_net_query)
  assert authorize_net_status == 200
  let authorize_net_json = json.to_string(authorize_net_body)
  assert string.contains(
    authorize_net_json,
    "\"code\":\"AUTHORIZE_NET_CUSTOMER_PROFILE_ID_BLANK\"",
  )

  let adyen_query =
    "mutation { customerPaymentMethodRemoteCreate(customerId: \"gid://shopify/Customer/1\", remoteReference: { adyenPaymentMethod: { shopperReference: \"\", storedPaymentMethodId: null } }) { customerPaymentMethod { id } userErrors { field code message } } }"
  let #(Response(status: adyen_status, body: adyen_body, ..), _) =
    graphql(proxy_with_customer(), adyen_query)
  assert adyen_status == 200
  let adyen_json = json.to_string(adyen_body)
  assert string.contains(
    adyen_json,
    "\"code\":\"ADYEN_SHOPPER_REFERENCE_BLANK\"",
  )
  assert string.contains(
    adyen_json,
    "\"code\":\"ADYEN_STORED_PAYMENT_METHOD_ID_BLANK\"",
  )
}

pub fn remote_payment_method_rejects_multiple_gateways_with_invalid_test() {
  let query =
    "mutation { customerPaymentMethodRemoteCreate(customerId: \"gid://shopify/Customer/1\", remoteReference: { paypalPaymentMethod: { }, stripePaymentMethod: { } }) { customerPaymentMethod { id } userErrors { field code message } } }"
  let #(Response(status: status, body: body, ..), _) =
    graphql(proxy_with_customer(), query)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"customerPaymentMethod\":null")
  assert string.contains(body_json, "\"field\":[\"remote_reference\"]")
  assert string.contains(body_json, "\"code\":\"INVALID\"")
  assert !string.contains(body_json, "EXACTLY_ONE_REMOTE_REFERENCE_REQUIRED")
}

fn reminder_mutation(schedule_id: String) -> String {
  "mutation { paymentReminderSend(paymentScheduleId: \""
  <> schedule_id
  <> "\") { success userErrors { field message code } } }"
}

fn run_reminder(proxy: DraftProxy, schedule_id: String) {
  graphql_with_proxy(proxy, reminder_mutation(schedule_id))
}

pub fn payment_reminder_rejects_unknown_schedule_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: status, body: body, ..), proxy) =
    run_reminder(proxy, "gid://shopify/PaymentSchedule/9999999999")

  assert status == 200
  assert json.to_string(body) == not_found_response()
  assert dict.size(proxy.store.staged_state.payment_reminder_sends) == 0
}

pub fn payment_reminder_rejects_invalid_schedule_gid_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: status, body: body, ..), proxy) =
    run_reminder(proxy, "gid://shopify/Order/1")

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"paymentReminderSend\":{\"success\":false,\"userErrors\":[{\"field\":[\"paymentScheduleId\"],\"message\":\"Payment schedule ID is invalid\",\"code\":\"INVALID_PAYMENT_SCHEDULE_ID\"}]}}}"
  assert dict.size(proxy.store.staged_state.payment_reminder_sends) == 0
}

pub fn payment_reminder_stages_for_overdue_open_order_schedule_test() {
  let schedule_id = "gid://shopify/PaymentSchedule/123"
  let proxy =
    seeded_proxy(
      open_order("gid://shopify/Order/9001"),
      overdue_terms(
        "gid://shopify/PaymentTerms/901",
        "gid://shopify/Order/9001",
        [
          schedule(schedule_id, None),
        ],
      ),
    )
  let #(Response(status: status, body: body, ..), proxy) =
    run_reminder(proxy, schedule_id)

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"paymentReminderSend\":{\"success\":true,\"userErrors\":[]}}}"
  assert dict.size(proxy.store.staged_state.payment_reminder_sends) == 1
}

pub fn payment_reminder_rejects_customer_payment_method_selection_test() {
  let schedule_id = "gid://shopify/PaymentSchedule/shape"
  let proxy =
    seeded_proxy(
      open_order("gid://shopify/Order/shape-owner"),
      overdue_terms(
        "gid://shopify/PaymentTerms/shape",
        "gid://shopify/Order/shape-owner",
        [schedule(schedule_id, None)],
      ),
    )
  let query =
    "mutation { paymentReminderSend(paymentScheduleId: \""
    <> schedule_id
    <> "\") { customerPaymentMethod { id } success userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql_with_proxy(proxy, query)

  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"errors\":[")
  assert string.contains(body_json, "customerPaymentMethod")
  assert string.contains(body_json, "PaymentReminderSendPayload")
  assert !string.contains(body_json, "\"data\"")
  assert dict.size(proxy.store.staged_state.payment_reminder_sends) == 0
}

pub fn payment_reminder_rejects_paid_or_not_overdue_schedule_test() {
  let paid_schedule_id = "gid://shopify/PaymentSchedule/paid"
  let not_overdue_schedule_id = "gid://shopify/PaymentSchedule/current"
  let owner_id = "gid://shopify/Order/9002"
  let base = draft_proxy.new()
  let seeded_store =
    store.upsert_base_orders(store.new(), [open_order(owner_id)])
    |> store.upsert_staged_payment_terms(
      overdue_terms("gid://shopify/PaymentTerms/paid", owner_id, [
        schedule(paid_schedule_id, Some("2026-05-01T00:00:00Z")),
      ]),
    )
    |> store.upsert_staged_payment_terms(
      PaymentTermsRecord(
        ..overdue_terms("gid://shopify/PaymentTerms/current", owner_id, [
          schedule(not_overdue_schedule_id, None),
        ]),
        overdue: False,
      ),
    )
  let proxy = DraftProxy(..base, store: seeded_store)

  let #(Response(status: paid_status, body: paid_body, ..), proxy) =
    run_reminder(proxy, paid_schedule_id)
  let #(Response(status: current_status, body: current_body, ..), proxy) =
    run_reminder(proxy, not_overdue_schedule_id)

  assert paid_status == 200
  assert json.to_string(paid_body) == already_completed_response()
  assert current_status == 200
  assert json.to_string(current_body) == unsuccessful_response()
  assert dict.size(proxy.store.staged_state.payment_reminder_sends) == 0
}

pub fn payment_reminder_rejects_terminal_owner_resources_test() {
  let cancelled_schedule_id = "gid://shopify/PaymentSchedule/cancelled"
  let paid_schedule_id = "gid://shopify/PaymentSchedule/paid-owner"
  let completed_draft_schedule_id =
    "gid://shopify/PaymentSchedule/completed-draft"
  let cancelled_order_id = "gid://shopify/Order/9003"
  let paid_order_id = "gid://shopify/Order/9005"
  let completed_draft_id = "gid://shopify/DraftOrder/9004"
  let base_store =
    store.new()
    |> store.upsert_base_orders([cancelled_order(cancelled_order_id)])
    |> store.upsert_base_orders([paid_order(paid_order_id)])
    |> store.upsert_base_draft_orders([
      completed_draft_order(completed_draft_id),
    ])
    |> store.upsert_staged_payment_terms(
      overdue_terms("gid://shopify/PaymentTerms/cancelled", cancelled_order_id, [
        schedule(cancelled_schedule_id, None),
      ]),
    )
    |> store.upsert_staged_payment_terms(
      overdue_terms("gid://shopify/PaymentTerms/paid-owner", paid_order_id, [
        schedule(paid_schedule_id, None),
      ]),
    )
    |> store.upsert_staged_payment_terms(
      overdue_terms(
        "gid://shopify/PaymentTerms/completed-draft",
        completed_draft_id,
        [schedule(completed_draft_schedule_id, None)],
      ),
    )
  let proxy = DraftProxy(..draft_proxy.new(), store: base_store)

  let #(Response(status: cancelled_status, body: cancelled_body, ..), proxy) =
    run_reminder(proxy, cancelled_schedule_id)
  let #(Response(status: paid_status, body: paid_body, ..), proxy) =
    run_reminder(proxy, paid_schedule_id)
  let #(Response(status: draft_status, body: draft_body, ..), proxy) =
    run_reminder(proxy, completed_draft_schedule_id)

  assert cancelled_status == 200
  assert json.to_string(cancelled_body) == unsuccessful_response()
  assert paid_status == 200
  assert json.to_string(paid_body) == already_completed_response()
  assert draft_status == 200
  assert json.to_string(draft_body) == not_for_order_response()
  assert dict.size(proxy.store.staged_state.payment_reminder_sends) == 0
}

fn seeded_proxy(order: OrderRecord, terms: PaymentTermsRecord) -> DraftProxy {
  let proxy = draft_proxy.new()
  let seeded_store =
    store.new()
    |> store.upsert_base_orders([order])
    |> store.upsert_staged_payment_terms(terms)
  DraftProxy(..proxy, store: seeded_store)
}

fn overdue_terms(
  id: String,
  owner_id: String,
  schedules: List(PaymentScheduleRecord),
) -> PaymentTermsRecord {
  PaymentTermsRecord(
    id: id,
    owner_id: owner_id,
    due: True,
    overdue: True,
    due_in_days: Some(30),
    payment_terms_name: "Net 30",
    payment_terms_type: "NET",
    translated_name: "Net 30",
    payment_schedules: schedules,
  )
}

fn schedule(id: String, completed_at: Option(String)) -> PaymentScheduleRecord {
  let amount = Money(amount: "18.5", currency_code: "CAD")
  PaymentScheduleRecord(
    id: id,
    due_at: Some("2026-04-01T00:00:00Z"),
    issued_at: Some("2026-03-01T00:00:00Z"),
    completed_at: completed_at,
    due: Some(True),
    amount: Some(amount),
    balance_due: Some(amount),
    total_balance: Some(amount),
  )
}

fn open_order(id: String) -> OrderRecord {
  OrderRecord(
    id: id,
    cursor: None,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("closed", CapturedBool(False)),
      #("closedAt", CapturedNull),
      #("cancelledAt", CapturedNull),
      #("displayFinancialStatus", CapturedString("PENDING")),
    ]),
  )
}

fn cancelled_order(id: String) -> OrderRecord {
  OrderRecord(
    id: id,
    cursor: None,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("closed", CapturedBool(True)),
      #("closedAt", CapturedString("2026-05-01T00:00:00Z")),
      #("cancelledAt", CapturedString("2026-05-01T00:00:00Z")),
      #("displayFinancialStatus", CapturedString("PENDING")),
    ]),
  )
}

fn paid_order(id: String) -> OrderRecord {
  OrderRecord(
    id: id,
    cursor: None,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("closed", CapturedBool(False)),
      #("closedAt", CapturedNull),
      #("cancelledAt", CapturedNull),
      #("displayFinancialStatus", CapturedString("PAID")),
    ]),
  )
}

fn completed_draft_order(id: String) -> DraftOrderRecord {
  DraftOrderRecord(
    id: id,
    cursor: None,
    data: CapturedObject([
      #("id", CapturedString(id)),
      #("status", CapturedString("COMPLETED")),
      #("completedAt", CapturedString("2026-05-01T00:00:00Z")),
    ]),
  )
}

fn unsuccessful_response() -> String {
  "{\"data\":{\"paymentReminderSend\":{\"success\":null,\"userErrors\":[{\"field\":null,\"message\":\"Payment reminder could not be sent\",\"code\":\"PAYMENT_REMINDER_SEND_UNSUCCESSFUL\"}]}}}"
}

fn not_found_response() -> String {
  "{\"data\":{\"paymentReminderSend\":{\"success\":null,\"userErrors\":[{\"field\":null,\"message\":\"Payment schedule does not exist\",\"code\":\"PAYMENT_REMINDER_SEND_UNSUCCESSFUL\"}]}}}"
}

fn already_completed_response() -> String {
  "{\"data\":{\"paymentReminderSend\":{\"success\":null,\"userErrors\":[{\"field\":null,\"message\":\"Payment schedule is already completed\",\"code\":\"PAYMENT_REMINDER_SEND_UNSUCCESSFUL\"}]}}}"
}

fn not_for_order_response() -> String {
  "{\"data\":{\"paymentReminderSend\":{\"success\":null,\"userErrors\":[{\"field\":null,\"message\":\"Payment schedule is not for an Order\",\"code\":\"PAYMENT_REMINDER_SEND_UNSUCCESSFUL\"}]}}}"
}

pub fn payment_customization_metafields_and_function_handle_readback_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionHandle: \"handle-before\", metafields: [{ namespace: \"$app:foo\", key: \"bar\", type: \"single_line_text_field\", value: \"baz\" }] }) { paymentCustomization { id title functionId functionHandle metafields(first: 5) { edges { node { namespace key type value } } } } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"userErrors\":[]")
  assert string.contains(
    create_json,
    "\"namespace\":\"app--347082227713--foo\"",
  )
  assert string.contains(create_json, "\"key\":\"bar\"")
  assert string.contains(create_json, "\"value\":\"baz\"")
  assert string.contains(create_json, "\"functionId\":null")
  assert string.contains(create_json, "\"functionHandle\":\"handle-before\"")

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { title: \"After\", functionHandle: \"handle-before\", metafields: [{ namespace: \"$app:foo\", key: \"bar\", type: \"single_line_text_field\", value: \"qux\" }] }) { paymentCustomization { id title functionId functionHandle metafields(first: 5) { edges { node { namespace key type value } } } } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"title\":\"After\"")
  assert string.contains(update_json, "\"functionId\":null")
  assert string.contains(update_json, "\"functionHandle\":\"handle-before\"")
  assert string.contains(update_json, "\"value\":\"qux\"")
  assert !string.contains(update_json, "\"value\":\"baz\"")

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/1\") { id title functionId functionHandle metafield(namespace: \"$app:foo\", key: \"bar\") { namespace key type value } metafields(first: 5) { edges { node { namespace key type value } } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"title\":\"After\"")
  assert string.contains(read_json, "\"functionId\":null")
  assert string.contains(read_json, "\"functionHandle\":\"handle-before\"")
  assert string.contains(
    read_json,
    "\"metafield\":{\"namespace\":\"app--347082227713--foo\"",
  )
  assert string.contains(read_json, "\"value\":\"qux\"")
  assert !string.contains(read_json, "\"value\":\"baz\"")
}

pub fn payment_customization_create_allows_missing_metafields_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Missing metafields\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"paymentCustomization\":{\"id\"")
  assert string.contains(create_json, "\"userErrors\":[]")
  assert list.length(store.list_effective_payment_customizations(proxy.store))
    == 1
}

pub fn payment_customization_create_rejects_multiple_function_identifiers_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Both identifiers\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\", functionHandle: \"payment-a\", metafields: [] }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"paymentCustomization\":null")
  assert string.contains(
    create_json,
    "\"field\":[\"paymentCustomization\",\"base\"]",
  )
  assert string.contains(
    create_json,
    "\"code\":\"MULTIPLE_FUNCTION_IDENTIFIERS\"",
  )
  assert list.is_empty(store.list_effective_payment_customizations(proxy.store))
}

pub fn payment_customization_create_rejects_missing_function_identifier_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Missing identifier\", enabled: true, metafields: [] }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"paymentCustomization\":null")
  assert string.contains(
    create_json,
    "\"field\":[\"paymentCustomization\",\"functionHandle\"]",
  )
  assert string.contains(
    create_json,
    "\"code\":\"MISSING_FUNCTION_IDENTIFIER\"",
  )
  assert list.is_empty(store.list_effective_payment_customizations(proxy.store))
}

pub fn payment_customization_create_allows_empty_metafields_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Empty metafields\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\", metafields: [] }) { paymentCustomization { id title } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"userErrors\":[]")
  assert string.contains(create_json, "\"title\":\"Empty metafields\"")
  assert list.length(store.list_effective_payment_customizations(proxy.store))
    == 1
}

pub fn payment_customization_create_allows_six_enabled_customizations_test() {
  let proxy =
    ["1", "2", "3", "4", "5", "6"]
    |> list.fold(draft_proxy.new(), fn(proxy, suffix) {
      let #(Response(status: create_status, body: create_body, ..), proxy) =
        graphql(proxy, valid_payment_customization_create_query(suffix))
      assert create_status == 200
      assert string.contains(json.to_string(create_body), "\"userErrors\":[]")
      proxy
    })

  assert list.length(store.list_effective_payment_customizations(proxy.store))
    == 6
  assert string.contains(meta_state_json(proxy), "Payment customization 6")
}

fn valid_payment_customization_create_query(suffix: String) -> String {
  "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Payment customization "
  <> suffix
  <> "\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-"
  <> suffix
  <> "\", metafields: [] }) { paymentCustomization { id title enabled } userErrors { field code message } } }"
}

pub fn payment_customization_update_rejects_different_existing_function_test() {
  let proxy =
    draft_proxy.new()
    |> seed_shopify_function("payment-a")
    |> seed_shopify_function("payment-b")

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\", metafields: [] }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { functionId: \"gid://shopify/ShopifyFunction/payment-b\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"paymentCustomization\":null")
  assert string.contains(
    update_json,
    "\"field\":[\"paymentCustomization\",\"functionId\"]",
  )
  assert string.contains(
    update_json,
    "\"code\":\"FUNCTION_ID_CANNOT_BE_CHANGED\"",
  )
  assert string.contains(
    update_json,
    "\"message\":\"Function ID cannot be changed.\"",
  )

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/1\") { id title functionId functionHandle } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
  assert !string.contains(read_json, "payment-b")
}

pub fn payment_customization_update_allows_equivalent_function_handle_test() {
  let proxy = draft_proxy.new() |> seed_shopify_function("payment-a")

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\", metafields: [] }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { title: \"After\", functionHandle: \"payment-a\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"title\":\"After\"")
  assert string.contains(
    update_json,
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
  assert string.contains(update_json, "\"functionHandle\":null")
}

pub fn payment_customization_update_allows_equivalent_function_id_gid_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"raw-payment-function\", metafields: [] }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { title: \"After\", functionId: \"gid://shopify/ShopifyFunction/raw-payment-function\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"title\":\"After\"")
  assert string.contains(update_json, "\"functionId\":\"raw-payment-function\"")
}

pub fn payment_customization_update_unknown_function_id_is_immutable_test() {
  let proxy = draft_proxy.new() |> seed_shopify_function("payment-a")

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\", metafields: [] }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { functionId: \"gid://shopify/ShopifyFunction/missing\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"paymentCustomization\":null")
  assert string.contains(
    update_json,
    "\"code\":\"FUNCTION_ID_CANNOT_BE_CHANGED\"",
  )
  assert string.contains(
    update_json,
    "\"message\":\"Function ID cannot be changed.\"",
  )

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/1\") { id title functionId } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
}

pub fn payment_customization_update_unknown_function_handle_returns_not_found_test() {
  let proxy = draft_proxy.new() |> seed_shopify_function("payment-a")

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\", metafields: [] }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { functionHandle: \"missing-payment-function\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"paymentCustomization\":null")
  assert string.contains(
    update_json,
    "\"field\":[\"paymentCustomization\",\"functionHandle\"]",
  )
  assert string.contains(update_json, "\"code\":\"FUNCTION_NOT_FOUND\"")
  assert string.contains(
    update_json,
    "\"message\":\"Could not find function with handle: missing-payment-function.\"",
  )

  let read_query =
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/1\") { id title functionId } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
}

pub fn payment_customization_invalid_metafield_shape_returns_user_error_test() {
  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Invalid\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/123\", metafields: [{ namespace: \"$app:foo\", key: \"bar\", value: \"baz\" }] }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), _) =
    graphql(draft_proxy.new(), create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"paymentCustomization\":null")
  assert string.contains(
    create_json,
    "\"field\":[\"paymentCustomization\",\"metafields\",\"0\",\"type\"]",
  )
  assert string.contains(create_json, "\"code\":\"INVALID_METAFIELDS\"")

  let seed_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Seed\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/123\", metafields: [] }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: seed_status, ..), proxy) =
    graphql(draft_proxy.new(), seed_query)
  assert seed_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/1\", paymentCustomization: { metafields: [{ key: \"bar\", type: \"single_line_text_field\", value: \"baz\" }] }) { paymentCustomization { id } userErrors { field code message } } }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(proxy, update_query)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"paymentCustomization\":null")
  assert string.contains(
    update_json,
    "\"field\":[\"paymentCustomization\",\"metafields\",\"0\",\"namespace\"]",
  )
  assert string.contains(update_json, "\"code\":\"INVALID_METAFIELDS\"")
}

const order_id = "gid://shopify/Order/637"

fn order_money_set(amount: String, currency_code: String) {
  CapturedObject([
    #(
      "shopMoney",
      CapturedObject([
        #("amount", CapturedString("42.50")),
        #("currencyCode", CapturedString("USD")),
      ]),
    ),
    #(
      "presentmentMoney",
      CapturedObject([
        #("amount", CapturedString(amount)),
        #("currencyCode", CapturedString(currency_code)),
      ]),
    ),
  ])
}

fn seeded_order_proxy(amount: String, currency_code: String) {
  let seeded_store =
    store.new()
    |> store.upsert_base_orders([
      OrderRecord(
        id: order_id,
        cursor: None,
        data: CapturedObject([
          #("id", CapturedString(order_id)),
          #("currentTotalPriceSet", order_money_set(amount, currency_code)),
          #("totalPriceSet", order_money_set(amount, currency_code)),
        ]),
      ),
    ])
  DraftProxy(..draft_proxy.new(), store: seeded_store)
}

pub fn payment_terms_create_accepts_order_owner_and_uses_presentment_money_test() {
  let proxy = seeded_order_proxy("57.00", "CAD")
  let mutation =
    "mutation {
      paymentTermsCreate(
        referenceId: \"gid://shopify/Order/637\",
        paymentTermsAttributes: {
          paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/4\",
          paymentSchedules: [{ issuedAt: \"2026-05-05T00:00:00Z\" }]
        }
      ) {
        paymentTerms {
          id
          paymentSchedules(first: 1) {
            nodes {
              amount { amount currencyCode }
              balanceDue { amount currencyCode }
              totalBalance { amount currencyCode }
            }
          }
        }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), _) = graphql(proxy, mutation)
  let body = json.to_string(body)

  assert status == 200
  assert string.contains(body, "\"userErrors\":[]")
  assert string.contains(body, "\"amount\":\"57.00\"")
  assert string.contains(body, "\"currencyCode\":\"CAD\"")
  assert !string.contains(body, "REFERENCE_DOES_NOT_EXIST")
}

pub fn payment_terms_create_rejects_multiple_schedules_with_shopify_code_test() {
  let proxy = seeded_order_proxy("57.00", "CAD")
  let mutation =
    "mutation {
      paymentTermsCreate(
        referenceId: \"gid://shopify/Order/637\",
        paymentTermsAttributes: {
          paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/4\",
          paymentSchedules: [
            { issuedAt: \"2026-05-05T00:00:00Z\" },
            { issuedAt: \"2026-05-06T00:00:00Z\" }
          ]
        }
      ) {
        paymentTerms { id }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), _) = graphql(proxy, mutation)
  let body = json.to_string(body)

  assert status == 200
  assert string.contains(body, "\"paymentTerms\":null")
  assert string.contains(body, "\"field\":[\"base\"]")
  assert string.contains(
    body,
    "\"message\":\"Cannot create payment terms with multiple schedules.\"",
  )
  assert string.contains(
    body,
    "\"code\":\"PAYMENT_TERMS_CREATION_UNSUCCESSFUL\"",
  )
}

pub fn payment_terms_create_rejects_unknown_template_without_staging_test() {
  let proxy = seeded_order_proxy("57.00", "CAD")
  let mutation =
    "mutation {
      paymentTermsCreate(
        referenceId: \"gid://shopify/Order/637\",
        paymentTermsAttributes: {
          paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/9999\",
          paymentSchedules: [{ issuedAt: \"2026-01-01T00:00:00Z\" }]
        }
      ) {
        paymentTerms { id paymentTermsName paymentTermsType }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), proxy_after) =
    graphql(proxy, mutation)
  let body = json.to_string(body)
  let state = meta_state_json(proxy_after)

  assert status == 200
  assert string.contains(body, "\"paymentTerms\":null")
  assert string.contains(body, "\"field\":null")
  assert string.contains(
    body,
    "\"message\":\"Could not find payment terms template.\"",
  )
  assert string.contains(
    body,
    "\"code\":\"PAYMENT_TERMS_CREATION_UNSUCCESSFUL\"",
  )
  assert !string.contains(state, "PaymentTerms/")
}

pub fn payment_terms_create_requires_template_id_instead_of_defaulting_test() {
  let proxy = seeded_order_proxy("57.00", "CAD")
  let mutation =
    "mutation {
      paymentTermsCreate(
        referenceId: \"gid://shopify/Order/637\",
        paymentTermsAttributes: {
          paymentSchedules: [{ issuedAt: \"2026-01-01T00:00:00Z\" }]
        }
      ) {
        paymentTerms { id paymentTermsName }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), proxy_after) =
    graphql(proxy, mutation)
  let body = json.to_string(body)
  let state = meta_state_json(proxy_after)

  assert status == 200
  assert string.contains(body, "\"paymentTerms\":null")
  assert string.contains(
    body,
    "\"field\":[\"paymentTermsAttributes\",\"paymentTermsTemplateId\"]",
  )
  assert string.contains(
    body,
    "\"message\":\"Payment terms template is required.\"",
  )
  assert string.contains(body, "\"code\":\"REQUIRED\"")
  assert !string.contains(state, "PaymentTerms/")
}

pub fn payment_terms_create_rejects_fixed_template_without_due_at_test() {
  let proxy = seeded_order_proxy("57.00", "CAD")
  let mutation =
    "mutation {
      paymentTermsCreate(
        referenceId: \"gid://shopify/Order/637\",
        paymentTermsAttributes: {
          paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/7\",
          paymentSchedules: [{}]
        }
      ) {
        paymentTerms { id paymentTermsName paymentTermsType }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), proxy_after) =
    graphql(proxy, mutation)
  let body = json.to_string(body)
  let state = meta_state_json(proxy_after)

  assert status == 200
  assert string.contains(body, "\"paymentTerms\":null")
  assert string.contains(body, "\"field\":null")
  assert string.contains(
    body,
    "\"message\":\"A due date is required with fixed or net payment terms.\"",
  )
  assert string.contains(
    body,
    "\"code\":\"PAYMENT_TERMS_CREATION_UNSUCCESSFUL\"",
  )
  assert !string.contains(state, "PaymentTerms/")
}

pub fn payment_terms_create_rejects_receipt_template_with_due_at_test() {
  let proxy = seeded_order_proxy("57.00", "CAD")
  let mutation =
    "mutation {
      paymentTermsCreate(
        referenceId: \"gid://shopify/Order/637\",
        paymentTermsAttributes: {
          paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/1\",
          paymentSchedules: [{ dueAt: \"2026-01-01T00:00:00Z\" }]
        }
      ) {
        paymentTerms { id paymentTermsName paymentTermsType }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), proxy_after) =
    graphql(proxy, mutation)
  let body = json.to_string(body)
  let state = meta_state_json(proxy_after)

  assert status == 200
  assert string.contains(body, "\"paymentTerms\":null")
  assert string.contains(body, "\"field\":null")
  assert string.contains(
    body,
    "\"message\":\"A due date cannot be set with event payment terms.\"",
  )
  assert string.contains(
    body,
    "\"code\":\"PAYMENT_TERMS_CREATION_UNSUCCESSFUL\"",
  )
  assert !string.contains(state, "PaymentTerms/")
}

pub fn payment_terms_create_accepts_receipt_issued_at_without_schedule_test() {
  let proxy = seeded_order_proxy("57.00", "CAD")
  let mutation =
    "mutation {
      paymentTermsCreate(
        referenceId: \"gid://shopify/Order/637\",
        paymentTermsAttributes: {
          paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/1\",
          paymentSchedules: [{ issuedAt: \"2026-01-01T00:00:00Z\" }]
        }
      ) {
        paymentTerms {
          paymentTermsName
          paymentTermsType
          paymentSchedules(first: 1) { nodes { issuedAt dueAt } }
        }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), _) = graphql(proxy, mutation)
  let body = json.to_string(body)

  assert status == 200
  assert string.contains(body, "\"userErrors\":[]")
  assert string.contains(body, "\"paymentTermsName\":\"Due on receipt\"")
  assert string.contains(body, "\"paymentTermsType\":\"RECEIPT\"")
  assert string.contains(body, "\"nodes\":[]")
}

pub fn payment_terms_update_and_delete_missing_ids_use_shopify_codes_test() {
  let proxy = draft_proxy.new()
  let update =
    "mutation {
      paymentTermsUpdate(input: {
        paymentTermsId: \"gid://shopify/PaymentTerms/999999\",
        paymentTermsAttributes: {
          paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/4\",
          paymentSchedules: [{ issuedAt: \"2026-05-05T00:00:00Z\" }]
        }
      }) {
        paymentTerms { id }
        userErrors { field message code }
      }
    }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update)
  let delete =
    "mutation {
      paymentTermsDelete(input: {
        paymentTermsId: \"gid://shopify/PaymentTerms/999999\"
      }) {
        deletedId
        userErrors { field message code }
      }
    }"
  let #(Response(status: delete_status, body: delete_body, ..), _) =
    graphql(proxy, delete)
  let update_body = json.to_string(update_body)
  let delete_body = json.to_string(delete_body)

  assert update_status == 200
  assert delete_status == 200
  assert string.contains(
    update_body,
    "\"code\":\"PAYMENT_TERMS_UPDATE_UNSUCCESSFUL\"",
  )
  assert string.contains(
    delete_body,
    "\"code\":\"payment_terms_deletion_unsuccessful\"",
  )
  assert !string.contains(update_body, "PAYMENT_TERMS_NOT_FOUND")
  assert !string.contains(delete_body, "PAYMENT_TERMS_NOT_FOUND")
}

pub fn payment_terms_delete_clears_stale_order_owner_read_test() {
  let terms_id = "gid://shopify/PaymentTerms/order-owner-cascade"
  let schedule_id = "gid://shopify/PaymentSchedule/order-owner-cascade"
  let money = Money(amount: "57.00", currency_code: "CAD")
  let seeded_store =
    store.new()
    |> store.upsert_base_orders([
      OrderRecord(
        id: order_id,
        cursor: None,
        data: CapturedObject([
          #("id", CapturedString(order_id)),
          #("name", CapturedString("#637")),
          #("paymentTerms", stale_payment_terms_node(terms_id)),
        ]),
      ),
    ])
    |> store.upsert_base_payment_terms(payment_terms_with_schedule(
      terms_id,
      order_id,
      schedule_id,
      money,
    ))
  let proxy = DraftProxy(..draft_proxy.new(), store: seeded_store)
  let delete =
    "mutation {
      paymentTermsDelete(input: {
        paymentTermsId: \"gid://shopify/PaymentTerms/order-owner-cascade\"
      }) {
        deletedId
        userErrors { field message code }
      }
    }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(proxy, delete)
  let read =
    "query {
      order(id: \"gid://shopify/Order/637\") {
        id
        paymentTerms { id paymentTermsName }
      }
    }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(proxy, read)
  let reminder =
    "mutation {
      paymentReminderSend(paymentScheduleId: \"gid://shopify/PaymentSchedule/order-owner-cascade\") {
        success
        userErrors { field message code }
      }
    }"
  let #(Response(status: reminder_status, body: reminder_body, ..), _) =
    graphql(proxy, reminder)
  let delete_body = json.to_string(delete_body)
  let read_body = json.to_string(read_body)
  let reminder_body = json.to_string(reminder_body)

  assert delete_status == 200
  assert string.contains(delete_body, "\"userErrors\":[]")
  assert read_status == 200
  assert string.contains(read_body, "\"paymentTerms\":null")
  assert !string.contains(read_body, "order-owner-cascade")
  assert reminder_status == 200
  assert string.contains(reminder_body, "\"success\":null")
  assert string.contains(
    reminder_body,
    "\"code\":\"PAYMENT_REMINDER_SEND_UNSUCCESSFUL\"",
  )
}

pub fn payment_terms_delete_clears_stale_draft_order_owner_read_test() {
  let draft_order_id = "gid://shopify/DraftOrder/delete-cascade"
  let terms_id = "gid://shopify/PaymentTerms/draft-owner-cascade"
  let schedule_id = "gid://shopify/PaymentSchedule/draft-owner-cascade"
  let money = Money(amount: "57.00", currency_code: "CAD")
  let seeded_store =
    store.new()
    |> store.upsert_base_draft_orders([
      DraftOrderRecord(
        id: draft_order_id,
        cursor: None,
        data: CapturedObject([
          #("id", CapturedString(draft_order_id)),
          #("name", CapturedString("#D1")),
          #("paymentTerms", stale_payment_terms_node(terms_id)),
        ]),
      ),
    ])
    |> store.upsert_base_payment_terms(payment_terms_with_schedule(
      terms_id,
      draft_order_id,
      schedule_id,
      money,
    ))
  let proxy = DraftProxy(..draft_proxy.new(), store: seeded_store)
  let delete =
    "mutation {
      paymentTermsDelete(input: {
        paymentTermsId: \"gid://shopify/PaymentTerms/draft-owner-cascade\"
      }) {
        deletedId
        userErrors { field message code }
      }
    }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(proxy, delete)
  let read =
    "query {
      draftOrder(id: \"gid://shopify/DraftOrder/delete-cascade\") {
        id
        name
        paymentTerms { id paymentTermsName }
      }
    }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read)
  let delete_body = json.to_string(delete_body)
  let read_body = json.to_string(read_body)

  assert delete_status == 200
  assert string.contains(delete_body, "\"userErrors\":[]")
  assert read_status == 200
  assert string.contains(read_body, "\"paymentTerms\":null")
  assert !string.contains(read_body, "draft-owner-cascade")
}

pub fn payment_terms_update_uses_update_code_for_invalid_attributes_test() {
  let terms_id = "gid://shopify/PaymentTerms/123"
  let money = Money(amount: "57.00", currency_code: "CAD")
  let seeded_store =
    store.new()
    |> store.upsert_staged_payment_terms(
      PaymentTermsRecord(
        id: terms_id,
        owner_id: order_id,
        due: False,
        overdue: False,
        due_in_days: Some(30),
        payment_terms_name: "Net 30",
        payment_terms_type: "NET",
        translated_name: "Net 30",
        payment_schedules: [
          PaymentScheduleRecord(
            id: "gid://shopify/PaymentSchedule/456",
            due_at: Some("2026-06-04T00:00:00Z"),
            issued_at: Some("2026-05-05T00:00:00Z"),
            completed_at: None,
            due: Some(False),
            amount: Some(money),
            balance_due: Some(money),
            total_balance: Some(money),
          ),
        ],
      ),
    )
  let proxy = DraftProxy(..draft_proxy.new(), store: seeded_store)
  let mutation =
    "mutation {
      paymentTermsUpdate(input: {
        paymentTermsId: \"gid://shopify/PaymentTerms/123\",
        paymentTermsAttributes: {
          paymentTermsTemplateId: \"gid://shopify/PaymentTermsTemplate/7\",
          paymentSchedules: [{}]
        }
      }) {
        paymentTerms { id paymentTermsName paymentTermsType }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), _) = graphql(proxy, mutation)
  let body = json.to_string(body)

  assert status == 200
  assert string.contains(body, "\"paymentTerms\":null")
  assert string.contains(body, "\"field\":null")
  assert string.contains(
    body,
    "\"message\":\"A due date is required with fixed or net payment terms.\"",
  )
  assert string.contains(body, "\"code\":\"PAYMENT_TERMS_UPDATE_UNSUCCESSFUL\"")
}

pub fn payment_terms_delete_normalizes_numeric_input_to_gid_test() {
  let terms_id = "gid://shopify/PaymentTerms/123"
  let money = Money(amount: "57.00", currency_code: "CAD")
  let seeded_store =
    store.new()
    |> store.upsert_staged_payment_terms(
      PaymentTermsRecord(
        id: terms_id,
        owner_id: order_id,
        due: False,
        overdue: False,
        due_in_days: Some(30),
        payment_terms_name: "Net 30",
        payment_terms_type: "NET",
        translated_name: "Net 30",
        payment_schedules: [
          PaymentScheduleRecord(
            id: "gid://shopify/PaymentSchedule/456",
            due_at: Some("2026-06-04T00:00:00Z"),
            issued_at: Some("2026-05-05T00:00:00Z"),
            completed_at: None,
            due: Some(False),
            amount: Some(money),
            balance_due: Some(money),
            total_balance: Some(money),
          ),
        ],
      ),
    )
  let proxy = DraftProxy(..draft_proxy.new(), store: seeded_store)
  let mutation =
    "mutation {
      paymentTermsDelete(input: { paymentTermsId: \"123\" }) {
        deletedId
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), _) = graphql(proxy, mutation)
  let body = json.to_string(body)

  assert status == 200
  assert string.contains(
    body,
    "\"deletedId\":\"gid://shopify/PaymentTerms/123\"",
  )
  assert string.contains(body, "\"userErrors\":[]")
}

fn stale_payment_terms_node(terms_id: String) {
  CapturedObject([
    #("id", CapturedString(terms_id)),
    #("paymentTermsName", CapturedString("Net 30")),
    #("paymentTermsType", CapturedString("NET")),
    #("translatedName", CapturedString("Net 30")),
    #("paymentSchedules", CapturedObject([#("nodes", CapturedArray([]))])),
  ])
}

fn payment_terms_with_schedule(
  terms_id: String,
  owner_id: String,
  schedule_id: String,
  money: Money,
) {
  PaymentTermsRecord(
    id: terms_id,
    owner_id: owner_id,
    due: False,
    overdue: False,
    due_in_days: Some(30),
    payment_terms_name: "Net 30",
    payment_terms_type: "NET",
    translated_name: "Net 30",
    payment_schedules: [
      PaymentScheduleRecord(
        id: schedule_id,
        due_at: Some("2026-06-04T00:00:00Z"),
        issued_at: Some("2026-05-05T00:00:00Z"),
        completed_at: None,
        due: Some(False),
        amount: Some(money),
        balance_due: Some(money),
        total_balance: Some(money),
      ),
    ],
  )
}

pub fn customer_payment_method_revoke_active_contract_does_not_mutate_test() {
  let source = seeded_store(method_with([contract()], None, None))
  let outcome = revoke(source, payment_method_id)
  let response = json.to_string(outcome.data)

  assert string.contains(response, "\"revokedCustomerPaymentMethodId\":null")
  assert string.contains(response, "\"code\":\"ACTIVE_CONTRACT\"")

  let assert Some(method) =
    store.get_effective_customer_payment_method_by_id(
      outcome.store,
      payment_method_id,
      True,
    )
  assert method.revoked_at == None
  assert method.revoked_reason == None
  assert list.length(method.subscription_contracts) == 1

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql_seeded(
      outcome.store,
      "query { customerPaymentMethod(id: \"gid://shopify/CustomerPaymentMethod/base-card\", showRevoked: true) { id revokedAt revokedReason } }",
    )
  assert read_status == 200
  let read_response = json.to_string(read_body)
  assert string.contains(read_response, "\"revokedAt\":null")
  assert string.contains(read_response, "\"revokedReason\":null")
}

pub fn customer_payment_method_revoke_success_stages_revocation_test() {
  let source = seeded_store(method_with([], None, None))
  let outcome = revoke(source, payment_method_id)
  let response = json.to_string(outcome.data)

  assert string.contains(
    response,
    "\"revokedCustomerPaymentMethodId\":\"gid://shopify/CustomerPaymentMethod/base-card\"",
  )
  assert string.contains(response, "\"userErrors\":[]")

  let assert Some(method) =
    store.get_effective_customer_payment_method_by_id(
      outcome.store,
      payment_method_id,
      True,
    )
  assert method.revoked_at == Some("2024-01-01T00:00:01.000Z")
  assert method.revoked_reason == Some("CUSTOMER_REVOKED")
}

pub fn customer_payment_method_revoke_missing_uses_shopify_enum_code_test() {
  let outcome =
    revoke(
      store.new() |> store.upsert_base_customers([customer()]),
      "gid://shopify/CustomerPaymentMethod/missing",
    )
  let response = json.to_string(outcome.data)

  assert string.contains(response, "\"revokedCustomerPaymentMethodId\":null")
  assert string.contains(response, "\"code\":\"PAYMENT_METHOD_DOES_NOT_EXIST\"")
}

pub fn customer_payment_method_revoke_already_revoked_is_idempotent_test() {
  let existing_revoked_at = "2026-05-01T00:00:00.000Z"
  let source =
    seeded_store(method_with(
      [],
      Some(existing_revoked_at),
      Some("CUSTOMER_REVOKED"),
    ))
  let outcome = revoke(source, payment_method_id)
  let response = json.to_string(outcome.data)

  assert string.contains(
    response,
    "\"revokedCustomerPaymentMethodId\":\"gid://shopify/CustomerPaymentMethod/base-card\"",
  )
  assert string.contains(response, "\"userErrors\":[]")

  let assert Some(method) =
    store.get_effective_customer_payment_method_by_id(
      outcome.store,
      payment_method_id,
      True,
    )
  assert method.revoked_at == Some(existing_revoked_at)
  assert method.revoked_reason == Some("CUSTOMER_REVOKED")
}

fn payment_guard_customer(id: String, email: String) -> CustomerRecord {
  CustomerRecord(
    id: id,
    first_name: None,
    last_name: None,
    display_name: Some("Payment Guard Customer"),
    email: Some(email),
    legacy_resource_id: Some(string.replace(id, "gid://shopify/Customer/", "")),
    locale: Some("en"),
    note: None,
    can_delete: Some(True),
    verified_email: Some(True),
    data_sale_opt_out: False,
    tax_exempt: Some(False),
    tax_exemptions: [],
    state: Some("ENABLED"),
    tags: [],
    number_of_orders: Some("0"),
    amount_spent: None,
    default_email_address: None,
    default_phone_number: None,
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    account_activation_token: None,
    created_at: None,
    updated_at: None,
  )
}

fn payment_guard_method(
  id: String,
  customer_id: String,
  type_name: String,
) -> CustomerPaymentMethodRecord {
  CustomerPaymentMethodRecord(
    id: id,
    customer_id: customer_id,
    cursor: None,
    instrument: Some(CustomerPaymentMethodInstrumentRecord(
      type_name: type_name,
      data: dict.new(),
    )),
    revoked_at: None,
    revoked_reason: None,
    subscription_contracts: [],
  )
}

fn payment_guard_shop() -> ShopRecord {
  ShopRecord(
    id: "gid://shopify/Shop/1000",
    name: "Payments Guard Shop",
    myshopify_domain: "payments-guard.myshopify.com",
    url: "https://payments-guard.myshopify.com",
    primary_domain: ShopDomainRecord(
      id: "gid://shopify/Domain/1000",
      host: "payments-guard.myshopify.com",
      url: "https://payments-guard.myshopify.com",
      ssl_enabled: True,
    ),
    contact_email: "shop@example.com",
    email: "shop@example.com",
    currency_code: "USD",
    enabled_presentment_currencies: ["USD"],
    iana_timezone: "America/New_York",
    timezone_abbreviation: "EST",
    timezone_offset: "-0500",
    timezone_offset_minutes: -300,
    taxes_included: False,
    tax_shipping: False,
    unit_system: "IMPERIAL_SYSTEM",
    weight_unit: "POUNDS",
    shop_address: ShopAddressRecord(
      id: "gid://shopify/ShopAddress/1000",
      address1: Some("1 Main St"),
      address2: None,
      city: Some("New York"),
      company: None,
      coordinates_validated: False,
      country: Some("United States"),
      country_code_v2: Some("US"),
      formatted: ["1 Main St", "New York NY 10001", "United States"],
      formatted_area: Some("New York NY, United States"),
      latitude: None,
      longitude: None,
      phone: None,
      province: Some("New York"),
      province_code: Some("NY"),
      zip: Some("10001"),
    ),
    plan: ShopPlanRecord(
      partner_development: True,
      public_display_name: "Development",
      shopify_plus: False,
    ),
    resource_limits: ShopResourceLimitsRecord(
      location_limit: 1000,
      max_product_options: 3,
      max_product_variants: 2048,
      redirect_limit_reached: False,
    ),
    features: ShopFeaturesRecord(
      avalara_avatax: False,
      branding: "SHOPIFY",
      bundles: ShopBundlesFeatureRecord(
        eligible_for_bundles: True,
        ineligibility_reason: None,
        sells_bundles: False,
      ),
      captcha: True,
      cart_transform: ShopCartTransformFeatureRecord(
        eligible_operations: ShopCartTransformEligibleOperationsRecord(
          expand_operation: True,
          merge_operation: True,
          update_operation: True,
        ),
      ),
      dynamic_remarketing: False,
      eligible_for_subscription_migration: False,
      eligible_for_subscriptions: False,
      gift_cards: True,
      harmonized_system_code: True,
      legacy_subscription_gateway_enabled: False,
      live_view: True,
      paypal_express_subscription_gateway_status: "DISABLED",
      reports: True,
      discounts_by_market_enabled: False,
      sells_subscriptions: False,
      show_metrics: True,
      storefront: True,
      unified_markets: True,
    ),
    payment_settings: PaymentSettingsRecord(
      supported_digital_wallets: [],
      payment_gateways: [],
    ),
    shop_policies: [],
  )
}

fn payment_guard_proxy() -> DraftProxy {
  let source_customer =
    payment_guard_customer(
      "gid://shopify/Customer/8801",
      "payment-one@example.com",
    )
  let target_customer =
    payment_guard_customer(
      "gid://shopify/Customer/8802",
      "payment-two@example.com",
    )
  let card =
    payment_guard_method(
      "gid://shopify/CustomerPaymentMethod/base-card",
      source_customer.id,
      "CustomerCreditCard",
    )
  let shop_pay =
    payment_guard_method(
      "gid://shopify/CustomerPaymentMethod/base-shop-pay",
      source_customer.id,
      "CustomerShopPayAgreement",
    )
  let base_store =
    store.new()
    |> store.upsert_base_shop(payment_guard_shop())
    |> store.upsert_base_customers([source_customer, target_customer])
    |> store.upsert_base_customer_payment_methods([card, shop_pay])

  DraftProxy(..draft_proxy.new(), store: base_store)
}

pub fn customer_payment_method_shop_pay_guards_reject_credit_cards_test() {
  let mutation =
    "mutation {
      duplication: customerPaymentMethodGetDuplicationData(
        customerPaymentMethodId: \"gid://shopify/CustomerPaymentMethod/base-card\",
        targetShopId: \"gid://shopify/Shop/target\",
        targetCustomerId: \"gid://shopify/Customer/8802\"
      ) {
        encryptedDuplicationData
        userErrors { field message code }
      }
      updateUrl: customerPaymentMethodGetUpdateUrl(
        customerPaymentMethodId: \"gid://shopify/CustomerPaymentMethod/base-card\"
      ) {
        updatePaymentMethodUrl
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), _) =
    graphql(payment_guard_proxy(), mutation)
  let response = json.to_string(body)

  assert status == 200
  assert string.contains(response, "\"encryptedDuplicationData\":null")
  assert string.contains(response, "\"updatePaymentMethodUrl\":null")
  assert string.contains(response, "\"field\":[\"customerPaymentMethodId\"]")
  assert string.contains(response, "\"code\":\"INVALID_INSTRUMENT\"")
  assert !string.contains(
    response,
    "encryptedDuplicationData\":\"shopify-draft",
  )
  assert !string.contains(response, "updatePaymentMethodUrl\":\"https://")
}

pub fn customer_payment_method_shop_pay_guards_reject_same_shop_test() {
  let mutation =
    "mutation {
      customerPaymentMethodGetDuplicationData(
        customerPaymentMethodId: \"gid://shopify/CustomerPaymentMethod/base-shop-pay\",
        targetShopId: \"gid://shopify/Shop/1000\",
        targetCustomerId: \"gid://shopify/Customer/8802\"
      ) {
        encryptedDuplicationData
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), _) =
    graphql(payment_guard_proxy(), mutation)
  let response = json.to_string(body)

  assert status == 200
  assert string.contains(response, "\"encryptedDuplicationData\":null")
  assert string.contains(response, "\"field\":[\"targetShopId\"]")
  assert string.contains(response, "\"code\":\"SAME_SHOP\"")
  assert !string.contains(
    response,
    "encryptedDuplicationData\":\"shopify-draft",
  )
}

pub fn customer_payment_method_create_from_duplication_data_requires_billing_address_fields_test() {
  let mutation =
    "mutation {
      customerPaymentMethodCreateFromDuplicationData(
        customerId: \"gid://shopify/Customer/8802\",
        billingAddress: {},
        encryptedDuplicationData: \"shopify-draft-proxy:customer-payment-method-duplication:not-used-before-validation\"
      ) {
        customerPaymentMethod { id }
        userErrors { field message code }
      }
    }"
  let #(Response(status: status, body: body, ..), _) =
    graphql(payment_guard_proxy(), mutation)
  let response = json.to_string(body)

  assert status == 200
  assert string.contains(response, "\"customerPaymentMethod\":null")
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"address1\"]",
  )
  assert string.contains(response, "\"field\":[\"billing_address\",\"city\"]")
  assert string.contains(response, "\"field\":[\"billing_address\",\"zip\"]")
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"country_code\"]",
  )
  assert string.contains(
    response,
    "\"field\":[\"billing_address\",\"province_code\"]",
  )
  assert string.contains(response, "\"code\":\"BLANK\"")
}
