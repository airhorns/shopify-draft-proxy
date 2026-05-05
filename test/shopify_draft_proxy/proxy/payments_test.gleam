import gleam/dict
import gleam/json
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, DraftProxy, Request, Response,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type DraftOrderRecord, type OrderRecord, type PaymentScheduleRecord,
  type PaymentTermsRecord, CapturedBool, CapturedNull, CapturedObject,
  CapturedString, CustomerRecord, DraftOrderRecord, Money, OrderRecord,
  PaymentScheduleRecord, PaymentTermsRecord,
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

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
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

pub fn payment_customization_update_rejects_different_existing_function_test() {
  let seed_a =
    "mutation { validationCreate(validation: { title: \"Function A\", functionHandle: \"payment-a\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_a_status, ..), proxy) =
    graphql(draft_proxy.new(), seed_a)
  assert seed_a_status == 200
  let seed_b =
    "mutation { validationCreate(validation: { title: \"Function B\", functionHandle: \"payment-b\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_b_status, ..), proxy) = graphql(proxy, seed_b)
  assert seed_b_status == 200

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/5\", paymentCustomization: { functionId: \"gid://shopify/ShopifyFunction/payment-b\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
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
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/5\") { id title functionId functionHandle } }"
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
  let seed =
    "mutation { validationCreate(validation: { title: \"Function A\", functionHandle: \"payment-a\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_status, ..), proxy) =
    graphql(draft_proxy.new(), seed)
  assert seed_status == 200

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/3\", paymentCustomization: { title: \"After\", functionHandle: \"payment-a\" }) { paymentCustomization { id title functionId functionHandle } userErrors { field code message } } }"
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
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"raw-payment-function\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
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
  let seed =
    "mutation { validationCreate(validation: { title: \"Function A\", functionHandle: \"payment-a\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_status, ..), proxy) =
    graphql(draft_proxy.new(), seed)
  assert seed_status == 200

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/3\", paymentCustomization: { functionId: \"gid://shopify/ShopifyFunction/missing\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
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
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/3\") { id title functionId } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"functionId\":\"gid://shopify/ShopifyFunction/payment-a\"",
  )
}

pub fn payment_customization_update_unknown_function_handle_returns_not_found_test() {
  let seed =
    "mutation { validationCreate(validation: { title: \"Function A\", functionHandle: \"payment-a\" }) { validation { id } userErrors { field code message } } }"
  let #(Response(status: seed_status, ..), proxy) =
    graphql(draft_proxy.new(), seed)
  assert seed_status == 200

  let create_query =
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Before\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/payment-a\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200

  let update_query =
    "mutation { paymentCustomizationUpdate(id: \"gid://shopify/PaymentCustomization/3\", paymentCustomization: { functionHandle: \"missing-payment-function\" }) { paymentCustomization { id title functionId } userErrors { field code message } } }"
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
    "query { paymentCustomization(id: \"gid://shopify/PaymentCustomization/3\") { id title functionId } }"
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
    "mutation { paymentCustomizationCreate(paymentCustomization: { title: \"Seed\", enabled: true, functionId: \"gid://shopify/ShopifyFunction/123\" }) { paymentCustomization { id } userErrors { field code message } } }"
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
