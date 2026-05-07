import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store as store_mod
import shopify_draft_proxy/state/types.{
  type CustomerOrderSummaryRecord, type CustomerRecord, type OrderRecord,
  B2BCompanyContactRecord, B2BCompanyRecord, CapturedNull, CapturedObject,
  CapturedString, CustomerDefaultEmailAddressRecord, CustomerOrderSummaryRecord,
  CustomerPaymentMethodRecord, CustomerPaymentMethodSubscriptionContractRecord,
  CustomerRecord, GiftCardRecord, Money, OrderRecord, StoreCreditAccountRecord,
  StorePropertyList, StorePropertyString,
}

fn graphql(proxy: draft_proxy.DraftProxy, query: String) {
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

fn setup_cross_customer_address() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_owner_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"address-owner@example.com\", firstName: \"Owner\", lastName: \"Customer\" }) { customer { id } userErrors { field message } } }",
    )
  assert create_owner_status == 200

  let #(Response(status: address_status, body: address_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { address1: \"1 Main St\", city: \"Ottawa\", countryCode: CA, provinceCode: ON, zip: \"K1A 0B1\" }, setAsDefault: true) { address { id city } userErrors { field message } } }",
    )
  assert address_status == 200
  assert string.contains(
    json.to_string(address_body),
    "\"id\":\"gid://shopify/MailingAddress/3\"",
  )

  let #(Response(status: create_other_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"address-other@example.com\", firstName: \"Other\", lastName: \"Customer\" }) { customer { id } userErrors { field message } } }",
    )
  assert create_other_status == 200

  #(
    proxy,
    "gid://shopify/Customer/1",
    "gid://shopify/MailingAddress/3",
    "gid://shopify/Customer/5",
  )
}

fn assert_address_ownership_error(body: json.Json) {
  assert string.contains(
    json.to_string(body),
    "\"userErrors\":[{\"field\":[\"addressId\"],\"message\":\"Address does not exist\"}]",
  )
}

fn assert_missing_customer_error(body: json.Json) {
  assert string.contains(
    json.to_string(body),
    "\"userErrors\":[{\"field\":[\"customerId\"],\"message\":\"Customer does not exist\"}]",
  )
}

pub fn order_customer_set_error_paths_test() {
  let customer_id = "gid://shopify/Customer/order-customer-errors"
  let order_id = "gid://shopify/Order/order-customer-errors"
  let proxy = order_customer_proxy(order_id, Some(customer_id), False)

  let #(Response(status: unknown_order_status, body: unknown_order_body, ..), _) =
    graphql(
      proxy,
      "mutation { orderCustomerSet(orderId: \"gid://shopify/Order/missing\", customerId: \""
        <> customer_id
        <> "\") { order { id } userErrors { field message code } } }",
    )
  assert unknown_order_status == 200
  assert json.to_string(unknown_order_body)
    == "{\"data\":{\"orderCustomerSet\":{\"order\":null,\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"Order does not exist\",\"code\":\"NOT_FOUND\"}]}}}"

  let #(
    Response(status: unknown_customer_status, body: unknown_customer_body, ..),
    _,
  ) =
    graphql(
      proxy,
      "mutation { orderCustomerSet(orderId: \""
        <> order_id
        <> "\", customerId: \"gid://shopify/Customer/missing\") { order { id } userErrors { field message code } } }",
    )
  assert unknown_customer_status == 200
  assert json.to_string(unknown_customer_body)
    == "{\"data\":{\"orderCustomerSet\":{\"order\":null,\"userErrors\":[{\"field\":[\"customerId\"],\"message\":\"Customer does not exist\",\"code\":\"NOT_FOUND\"}]}}}"
}

pub fn order_customer_set_rejects_b2b_contact_without_ordering_role_test() {
  let customer_id = "gid://shopify/Customer/order-customer-b2b"
  let order_id = "gid://shopify/Order/order-customer-b2b"
  let proxy =
    order_customer_proxy(order_id, Some(customer_id), False)
    |> seed_b2b_contact_without_ordering_role(customer_id)

  let #(Response(status: status, body: body, ..), _) =
    graphql(
      proxy,
      "mutation { orderCustomerSet(orderId: \""
        <> order_id
        <> "\", customerId: \""
        <> customer_id
        <> "\") { order { id } userErrors { field message code } } }",
    )
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"orderCustomerSet\":{\"order\":null,\"userErrors\":[{\"field\":[\"customerId\"],\"message\":\"no_customer_role_error\",\"code\":\"NOT_PERMITTED\"}]}}}"
}

pub fn order_customer_remove_rejects_cancelled_order_test() {
  let customer_id = "gid://shopify/Customer/order-customer-cancelled"
  let order_id = "gid://shopify/Order/order-customer-cancelled"
  let proxy = order_customer_proxy(order_id, Some(customer_id), True)

  let #(Response(status: status, body: body, ..), _) =
    graphql(
      proxy,
      "mutation { orderCustomerRemove(orderId: \""
        <> order_id
        <> "\") { order { id customer { id } } userErrors { field message code } } }",
    )
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"orderCustomerRemove\":{\"order\":null,\"userErrors\":[{\"field\":[\"orderId\"],\"message\":\"customer_cannot_be_removed\",\"code\":\"INVALID\"}]}}}"
}

pub fn order_customer_set_and_remove_happy_path_test() {
  let customer_id = "gid://shopify/Customer/order-customer-happy"
  let order_id = "gid://shopify/Order/order-customer-happy"
  let proxy = order_customer_proxy(order_id, None, False)

  let #(Response(status: set_status, body: set_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { orderCustomerSet(orderId: \""
        <> order_id
        <> "\", customerId: \""
        <> customer_id
        <> "\") { order { id customer { id email displayName } } userErrors { field message code } } }",
    )
  assert set_status == 200
  assert json.to_string(set_body)
    == "{\"data\":{\"orderCustomerSet\":{\"order\":{\"id\":\"gid://shopify/Order/order-customer-happy\",\"customer\":{\"id\":\"gid://shopify/Customer/order-customer-happy\",\"email\":\"enabled@example.com\",\"displayName\":\"ENABLED Customer\"}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(
      proxy,
      "query { order(id: \""
        <> order_id
        <> "\") { id customer { id } } customer(id: \""
        <> customer_id
        <> "\") { orders(first: 5) { nodes { id customer { id } } } } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"customer\":{\"id\":\"gid://shopify/Customer/order-customer-happy\"}",
  )

  let #(Response(status: remove_status, body: remove_body, ..), _) =
    graphql(
      proxy,
      "mutation { orderCustomerRemove(orderId: \""
        <> order_id
        <> "\") { order { id customer { id } } userErrors { field message code } } }",
    )
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"orderCustomerRemove\":{\"order\":{\"id\":\"gid://shopify/Order/order-customer-happy\",\"customer\":null},\"userErrors\":[]}}}"
}

pub fn customer_delete_blocks_when_customer_has_staged_order_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"blocked-delete@example.test\", firstName: \"Blocked\", lastName: \"Delete\" }) { customer { id email } userErrors { field message } } }",
    )
  assert create_status == 200

  let #(Response(status: order_status, body: order_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { orderCreate(order: { email: \"blocked-order@example.test\", customerId: \"gid://shopify/Customer/1\", currency: \"CAD\", lineItems: [{ title: \"Blocking line\", quantity: 1, priceSet: { shopMoney: { amount: \"9.99\", currencyCode: \"CAD\" } } }] }) { order { id customer { id email displayName } } userErrors { field message code } } }",
    )
  assert order_status == 200
  assert string.contains(
    json.to_string(order_body),
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\"",
  )

  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerDelete(input: { id: \"gid://shopify/Customer/1\" }) { deletedCustomerId userErrors { field message } } }",
    )
  assert delete_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"customerDelete\":{\"deletedCustomerId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Customer can’t be deleted because they have associated orders\"}]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id email } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"id\":\"gid://shopify/Customer/1\"",
  )
}

pub fn customer_delete_succeeds_when_customer_has_no_orders_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"delete-no-orders@example.test\" }) { customer { id } userErrors { field message } } }",
    )
  assert create_status == 200

  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerDelete(input: { id: \"gid://shopify/Customer/1\" }) { deletedCustomerId userErrors { field message } } }",
    )
  assert delete_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"customerDelete\":{\"deletedCustomerId\":\"gid://shopify/Customer/1\",\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id email } }",
    )
  assert read_status == 200
  assert json.to_string(read_body) == "{\"data\":{\"customer\":null}}"
}

pub fn customer_delete_ignores_staged_draft_orders_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"draft-only-delete@example.test\" }) { customer { id } userErrors { field message } } }",
    )
  assert create_status == 200

  let #(Response(status: draft_status, body: draft_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { draftOrderCreate(input: { purchasingEntity: { customerId: \"gid://shopify/Customer/1\" }, email: \"draft-only-delete@example.test\", lineItems: [{ title: \"Draft-only line\", quantity: 1, originalUnitPrice: \"5.00\" }] }) { draftOrder { id customer { id } } userErrors { field message } } }",
    )
  assert draft_status == 200
  assert string.contains(json.to_string(draft_body), "\"userErrors\":[]")

  let #(Response(status: delete_status, body: delete_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerDelete(input: { id: \"gid://shopify/Customer/1\" }) { deletedCustomerId userErrors { field message } } }",
    )
  assert delete_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"customerDelete\":{\"deletedCustomerId\":\"gid://shopify/Customer/1\",\"userErrors\":[]}}}"
}

fn customer_with_state(id: String, state: String) -> CustomerRecord {
  CustomerRecord(
    id: id,
    first_name: None,
    last_name: None,
    display_name: Some(state <> " Customer"),
    email: Some(string.lowercase(state) <> "@example.com"),
    legacy_resource_id: None,
    locale: Some("en"),
    note: None,
    can_delete: Some(True),
    verified_email: Some(True),
    data_sale_opt_out: False,
    tax_exempt: Some(False),
    tax_exemptions: [],
    state: Some(state),
    tags: [],
    number_of_orders: Some("0"),
    amount_spent: Some(Money(amount: "0.0", currency_code: "USD")),
    default_email_address: None,
    default_phone_number: None,
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    account_activation_token: None,
    created_at: Some("2024-01-01T00:00:00.000Z"),
    updated_at: Some("2024-01-01T00:00:00.000Z"),
  )
}

fn order_customer_proxy(
  order_id: String,
  customer_id: Option(String),
  cancelled: Bool,
) -> draft_proxy.DraftProxy {
  let proxy = draft_proxy.new()
  let customer =
    customer_with_state(
      option.unwrap(customer_id, "gid://shopify/Customer/order-customer-happy"),
      "ENABLED",
    )
  let order = order_customer_order(order_id, customer_id, cancelled)
  let summary = order_customer_summary(order_id, customer_id)
  let store =
    proxy.store
    |> store_mod.upsert_base_customers([customer])
    |> store_mod.upsert_base_orders([order])
    |> store_mod.upsert_base_customer_order_summaries([summary])
  proxy_state.DraftProxy(..proxy, store: store)
}

fn order_customer_order(
  order_id: String,
  customer_id: Option(String),
  cancelled: Bool,
) -> OrderRecord {
  OrderRecord(
    id: order_id,
    cursor: None,
    data: CapturedObject([
      #("id", CapturedString(order_id)),
      #("name", CapturedString("#1001")),
      #("email", CapturedString("order-customer@example.com")),
      #("createdAt", CapturedString("2024-01-01T00:00:00.000Z")),
      #("customer", order_customer_captured(customer_id)),
      #("cancelledAt", case cancelled {
        True -> CapturedString("2024-01-02T00:00:00.000Z")
        False -> CapturedNull
      }),
      #(
        "currentTotalPriceSet",
        CapturedObject([
          #(
            "shopMoney",
            CapturedObject([
              #("amount", CapturedString("10.0")),
              #("currencyCode", CapturedString("USD")),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

fn order_customer_summary(
  order_id: String,
  customer_id: Option(String),
) -> CustomerOrderSummaryRecord {
  CustomerOrderSummaryRecord(
    id: order_id,
    customer_id: customer_id,
    cursor: None,
    name: Some("#1001"),
    email: Some("order-customer@example.com"),
    created_at: Some("2024-01-01T00:00:00.000Z"),
    current_total_price: Some(Money(amount: "10.0", currency_code: "USD")),
  )
}

fn order_customer_captured(customer_id: Option(String)) {
  case customer_id {
    Some(id) ->
      CapturedObject([
        #("id", CapturedString(id)),
        #("email", CapturedString("order-customer-happy@example.com")),
        #("displayName", CapturedString("ENABLED Customer")),
      ])
    None -> CapturedNull
  }
}

fn seed_b2b_contact_without_ordering_role(
  proxy: draft_proxy.DraftProxy,
  customer_id: String,
) -> draft_proxy.DraftProxy {
  let company_id = "gid://shopify/Company/order-customer-b2b"
  let contact_id = "gid://shopify/CompanyContact/order-customer-b2b"
  let company =
    B2BCompanyRecord(
      id: company_id,
      cursor: None,
      data: dict.from_list([
        #("id", StorePropertyString(company_id)),
        #("name", StorePropertyString("Order Customer B2B")),
      ]),
      main_contact_id: None,
      contact_ids: [contact_id],
      location_ids: [],
      contact_role_ids: [],
    )
  let contact =
    B2BCompanyContactRecord(
      id: contact_id,
      cursor: None,
      company_id: company_id,
      data: dict.from_list([
        #("id", StorePropertyString(contact_id)),
        #("customerId", StorePropertyString(customer_id)),
        #("roleAssignments", StorePropertyList([])),
      ]),
    )
  let store =
    proxy.store
    |> store_mod.upsert_base_b2b_company(company)
    |> store_mod.upsert_base_b2b_company_contact(contact)
  proxy_state.DraftProxy(..proxy, store: store)
}

fn customer_state_proxy(id: String, state: String) -> draft_proxy.DraftProxy {
  let proxy = draft_proxy.new()
  proxy_state.DraftProxy(
    ..proxy,
    store: store_mod.upsert_base_customers(proxy.store, [
      customer_with_state(id, state),
    ]),
  )
}

fn no_contact_customer_proxy() -> draft_proxy.DraftProxy {
  let proxy = draft_proxy.new()
  let customer =
    CustomerRecord(
      ..customer_with_state("gid://shopify/Customer/1", "Enabled"),
      display_name: Some("No Contact Customer"),
      email: None,
      default_email_address: None,
      default_phone_number: None,
      email_marketing_consent: None,
      sms_marketing_consent: None,
    )
  proxy_state.DraftProxy(
    ..proxy,
    store: store_mod.upsert_base_customers(proxy.store, [customer]),
  )
}

fn activation_mutation(customer_id: String) -> String {
  "mutation { customerGenerateAccountActivationUrl(customerId: \""
  <> customer_id
  <> "\") { accountActivationUrl userErrors { field message code } } }"
}

pub fn customer_activation_url_rejects_enabled_or_declined_customers_test() {
  assert_activation_rejects_state("ENABLED")
  assert_activation_rejects_state("DECLINED")
}

fn assert_activation_rejects_state(state: String) {
  let customer_id = "gid://shopify/Customer/" <> state
  let proxy = customer_state_proxy(customer_id, state)
  let #(Response(status: status, body: body, ..), _) =
    graphql(proxy, activation_mutation(customer_id))
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"accountActivationUrl\":null")
  assert string.contains(body_json, "\"code\":\"account_already_enabled\"")
}

pub fn customer_activation_url_generates_token_for_disabled_or_invited_customers_test() {
  assert_activation_generates_token("DISABLED")
  assert_activation_generates_token("INVITED")
}

fn assert_activation_generates_token(state: String) {
  let customer_id = "gid://shopify/Customer/" <> state
  let proxy = customer_state_proxy(customer_id, state)
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(proxy, activation_mutation(customer_id))
  assert first_status == 200
  let first_json = json.to_string(first_body)
  assert string.contains(first_json, "\"userErrors\":[]")
  assert string.contains(first_json, "account_activation_token=")

  let #(Response(status: second_status, body: second_body, ..), proxy) =
    graphql(proxy, activation_mutation(customer_id))
  assert second_status == 200
  assert json.to_string(second_body) == first_json

  let state_json =
    draft_proxy.get_state_snapshot(proxy)
    |> json.to_string
  assert string.contains(state_json, "\"accountActivationToken\":\"")
}

pub fn customer_activation_url_missing_customer_uses_snake_case_code_test() {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      draft_proxy.new(),
      activation_mutation("gid://shopify/Customer/999999"),
    )
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"accountActivationUrl\":null")
  assert string.contains(body_json, "\"code\":\"customer_does_not_exist\"")
}

pub fn customer_merge_required_argument_validation_test() {
  let proxy = draft_proxy.new()

  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql(
      proxy,
      "mutation CustomerMergeMissingArgument { customerMerge(customerOneId: \"gid://shopify/Customer/1\") { resultingCustomerId job { id done } userErrors { field message code } } }",
    )
  assert missing_status == 200
  let missing_json = json.to_string(missing_body)
  assert string.contains(
    missing_json,
    "\"message\":\"Field 'customerMerge' is missing required arguments: customerTwoId\"",
  )
  assert string.contains(
    missing_json,
    "\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"customerMerge\",\"arguments\":\"customerTwoId\"}",
  )

  let #(Response(status: blank_status, body: blank_body, ..), _) =
    graphql(
      proxy,
      "mutation CustomerMergeBlankIds { customerMerge(customerOneId: \"\", customerTwoId: \"\") { resultingCustomerId job { id done } userErrors { field message code } } }",
    )
  assert blank_status == 200
  let blank_json = json.to_string(blank_body)
  assert string.contains(blank_json, "\"message\":\"Invalid global id ''\"")
  assert string.contains(
    blank_json,
    "\"path\":[\"mutation CustomerMergeBlankIds\",\"customerMerge\",\"customerOneId\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"CoercionError\"}",
  )
  assert string.contains(
    blank_json,
    "\"path\":[\"mutation CustomerMergeBlankIds\",\"customerMerge\",\"customerTwoId\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"CoercionError\"}",
  )
}

pub fn customer_merge_job_and_result_nodes_readback_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: first_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"merge-one@example.com\", firstName: \"One\", lastName: \"Source\", tags: [\"source\"] }) { customer { id } userErrors { field message code } } }",
    )
  assert first_status == 200

  let #(Response(status: second_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"merge-two@example.com\", firstName: \"Two\", lastName: \"Result\", tags: [\"result\"] }) { customer { id } userErrors { field message code } } }",
    )
  assert second_status == 200

  let #(Response(status: merge_status, body: merge_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerMerge(customerOneId: \"gid://shopify/Customer/1\", customerTwoId: \"gid://shopify/Customer/3\") { resultingCustomerId job { id done } userErrors { field message code } } }",
    )
  assert merge_status == 200
  let merge_json = json.to_string(merge_body)
  assert string.contains(
    merge_json,
    "\"customerMerge\":{\"resultingCustomerId\":\"gid://shopify/Customer/3\",\"job\":{\"id\":\"gid://shopify/Job/5\",\"done\":false},\"userErrors\":[]}",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { job(id: \"gid://shopify/Job/5\") { __typename id done query { __typename } } jobNode: node(id: \"gid://shopify/Job/5\") { __typename id ... on Job { done query { __typename } } } resultNode: node(id: \"gid://shopify/Customer/3\") { __typename id ... on Customer { email tags displayName } } sourceNode: node(id: \"gid://shopify/Customer/1\") { __typename id } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"job\":{\"__typename\":\"Job\",\"id\":\"gid://shopify/Job/5\",\"done\":true,\"query\":{\"__typename\":\"QueryRoot\"}}",
  )
  assert string.contains(
    read_json,
    "\"jobNode\":{\"__typename\":\"Job\",\"id\":\"gid://shopify/Job/5\",\"done\":true,\"query\":{\"__typename\":\"QueryRoot\"}}",
  )
  assert string.contains(
    read_json,
    "\"resultNode\":{\"__typename\":\"Customer\",\"id\":\"gid://shopify/Customer/3\",\"email\":\"merge-two@example.com\",\"tags\":[\"result\",\"source\"],\"displayName\":\"Two Result\"}",
  )
  assert string.contains(read_json, "\"sourceNode\":null")
}

pub fn customer_merge_blocks_combined_tags_overflow_test() {
  let proxy =
    customer_merge_blocker_proxy(
      CustomerRecord(..merge_customer_one(), tags: numbered_tags("a", 126)),
      CustomerRecord(..merge_customer_two(), tags: numbered_tags("b", 125)),
    )

  let #(Response(status: status, body: body, ..), proxy_after) =
    graphql(proxy, merge_blocker_mutation())
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"customerMerge\":{\"resultingCustomerId\":null,\"job\":null,\"userErrors\":[{\"field\":[\"customerOneId\"],\"message\":\"Customers must have 250 tags or less.\",\"code\":\"INVALID_CUSTOMER\"},{\"field\":[\"customerTwoId\"],\"message\":\"Customers must have 250 tags or less.\",\"code\":\"INVALID_CUSTOMER\"}],\"customerMergeErrors\":[{\"field\":[\"customerOneId\"],\"code\":\"INVALID_CUSTOMER\",\"block_type\":\"HARD\"},{\"field\":[\"customerTwoId\"],\"code\":\"INVALID_CUSTOMER\",\"block_type\":\"HARD\"}]}}}"
  assert list.is_empty(store_mod.get_log(proxy_after.store))
}

pub fn customer_merge_blocks_combined_note_overflow_test() {
  let proxy =
    customer_merge_blocker_proxy(
      CustomerRecord(..merge_customer_one(), note: Some(repeat("A", 3000))),
      CustomerRecord(..merge_customer_two(), note: Some(repeat("B", 2501))),
    )

  let #(Response(status: status, body: body, ..), proxy_after) =
    graphql(proxy, merge_blocker_mutation())
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"customerMerge\":{\"resultingCustomerId\":null,\"job\":null,\"userErrors\":[{\"field\":[\"customerOneId\"],\"message\":\"Customer notes must be 5,000 characters or less.\",\"code\":\"INVALID_CUSTOMER\"},{\"field\":[\"customerTwoId\"],\"message\":\"Customer notes must be 5,000 characters or less.\",\"code\":\"INVALID_CUSTOMER\"}],\"customerMergeErrors\":[{\"field\":[\"customerOneId\"],\"code\":\"INVALID_CUSTOMER\",\"block_type\":\"HARD\"},{\"field\":[\"customerTwoId\"],\"code\":\"INVALID_CUSTOMER\",\"block_type\":\"HARD\"}]}}}"
  assert list.is_empty(store_mod.get_log(proxy_after.store))
}

pub fn customer_merge_blocks_subscription_contracts_test() {
  let proxy =
    customer_merge_blocker_proxy(merge_customer_one(), merge_customer_two())
  let method =
    CustomerPaymentMethodRecord(
      id: "gid://shopify/CustomerPaymentMethod/merge-contract",
      customer_id: "gid://shopify/Customer/merge-one",
      cursor: None,
      instrument: None,
      revoked_at: None,
      revoked_reason: None,
      subscription_contracts: [
        CustomerPaymentMethodSubscriptionContractRecord(
          id: "gid://shopify/SubscriptionContract/merge-contract",
          cursor: None,
          data: dict.from_list([#("status", "ACTIVE")]),
        ),
      ],
    )
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store_mod.upsert_base_customer_payment_methods(proxy.store, [
        method,
      ]),
    )

  let #(Response(status: status, body: body, ..), proxy_after) =
    graphql(proxy, merge_blocker_mutation())
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"customerMerge\":{\"resultingCustomerId\":null,\"job\":null,\"userErrors\":[{\"field\":[\"customerOneId\"],\"message\":\"Customer with email merge-one@example.test has subscription contracts and can’t be merged.\",\"code\":\"INVALID_CUSTOMER\"}],\"customerMergeErrors\":[{\"field\":[\"customerOneId\"],\"code\":\"INVALID_CUSTOMER\",\"block_type\":\"HARD\"}]}}}"
  assert list.is_empty(store_mod.get_log(proxy_after.store))
}

pub fn customer_merge_blocks_enabled_gift_cards_test() {
  let proxy =
    customer_merge_blocker_proxy(merge_customer_one(), merge_customer_two())
  let #(_, store_with_gift_card) =
    store_mod.stage_create_gift_card(
      proxy.store,
      GiftCardRecord(
        id: "gid://shopify/GiftCard/merge-gift",
        legacy_resource_id: "merge-gift",
        last_characters: "1234",
        masked_code: "**** **** **** 1234",
        code: None,
        enabled: True,
        notify: True,
        deactivated_at: None,
        expires_on: None,
        note: None,
        template_suffix: None,
        created_at: "2026-05-06T00:00:00Z",
        updated_at: "2026-05-06T00:00:00Z",
        initial_value: Money("5.0", "CAD"),
        balance: Money("5.0", "CAD"),
        customer_id: Some("gid://shopify/Customer/merge-one"),
        recipient_id: None,
        source: None,
        recipient_attributes: None,
        transactions: [],
      ),
    )
  let proxy = proxy_state.DraftProxy(..proxy, store: store_with_gift_card)

  let #(Response(status: status, body: body, ..), proxy_after) =
    graphql(proxy, merge_blocker_mutation())
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"customerMerge\":{\"resultingCustomerId\":null,\"job\":null,\"userErrors\":[{\"field\":[\"customerOneId\"],\"message\":\"Customer with email merge-one@example.test has gift cards and can’t be merged.\",\"code\":\"INVALID_CUSTOMER\"}],\"customerMergeErrors\":[{\"field\":[\"customerOneId\"],\"code\":\"INVALID_CUSTOMER\",\"block_type\":\"HARD\"}]}}}"
  assert list.is_empty(store_mod.get_log(proxy_after.store))
}

pub fn customer_create_readback_and_log_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { customerCreate(input: { email: \"draft@example.com\", firstName: \"Draft\", lastName: \"Customer\", phone: \"+14155550123\", tags: [\"vip\", \"draft\"], taxExempt: true }) { customer { id displayName email taxExempt tags defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } } userErrors { field message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"id\":\"gid://shopify/Customer/1\"")
  assert string.contains(create_json, "\"displayName\":\"Draft Customer\"")
  assert string.contains(create_json, "\"tags\":[\"draft\",\"vip\"]")

  let read_query =
    "query { customer(id: \"gid://shopify/Customer/1\") { id email displayName defaultPhoneNumber { phoneNumber } orders(first: 1) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } customers(first: 10) { nodes { id email tags } } customersCount { count precision } customerByIdentifier(identifier: { emailAddress: \"draft@example.com\" }) { id email } }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"customersCount\":{\"count\":1,\"precision\":\"EXACT\"}",
  )
  assert string.contains(
    read_json,
    "\"customerByIdentifier\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"draft@example.com\"}",
  )
  assert string.contains(
    read_json,
    "\"orders\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}",
  )

  let log_request =
    Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
  let #(Response(status: log_status, body: log_body, ..), _) =
    draft_proxy.process_request(proxy, log_request)
  assert log_status == 200
  let log_json = json.to_string(log_body)
  assert string.contains(log_json, "\"domain\":\"customers\"")
  assert string.contains(
    log_json,
    "\"query\":\"" <> escape(create_query) <> "\"",
  )
}

pub fn customer_create_sanitizes_email_before_storage_and_lookup_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"foo bar@example.com\" }) { customer { id email defaultEmailAddress { emailAddress } } userErrors { field message code } } }",
    )
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"customerCreate\":{\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"foobar@example.com\",\"defaultEmailAddress\":{\"emailAddress\":\"foobar@example.com\"}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customerByIdentifier(identifier: { emailAddress: \"Foo Bar@example.com \" }) { id email defaultEmailAddress { emailAddress } } }",
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"customerByIdentifier\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"foobar@example.com\",\"defaultEmailAddress\":{\"emailAddress\":\"foobar@example.com\"}}}}"
}

pub fn customer_update_sanitizes_email_before_storage_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"update-email@example.com\" }) { customer { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerUpdate(input: { id: \"gid://shopify/Customer/1\", email: \"new email@example.com \" }) { customer { id email defaultEmailAddress { emailAddress } } userErrors { field message code } } }",
    )
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"customerUpdate\":{\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"newemail@example.com\",\"defaultEmailAddress\":{\"emailAddress\":\"newemail@example.com\"}},\"userErrors\":[]}}}"
}

pub fn customer_email_validation_rejects_invalid_and_too_long_values_test() {
  assert_customer_create_invalid_email("foo@")
  assert_customer_create_invalid_email("@bar.com")
  assert_customer_create_invalid_email("foo@bar")
  assert_customer_create_invalid_email("foo@@bar.com")

  let too_long_email = string.repeat("a", times: 244) <> "@example.com"
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { customerCreate(input: { email: \""
        <> too_long_email
        <> "\" }) { customer { id } userErrors { field message code } } }",
    )
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"customerCreate\":{\"customer\":null,\"userErrors\":[{\"field\":[\"email\"],\"message\":\"Email is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\"},{\"field\":[\"email\"],\"message\":\"Email is invalid\",\"code\":\"INVALID\"}]}}}"
}

fn assert_customer_create_invalid_email(email: String) {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { customerCreate(input: { email: \""
        <> email
        <> "\" }) { customer { id } userErrors { field message code } } }",
    )
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"customerCreate\":{\"customer\":null,\"userErrors\":[{\"field\":[\"email\"],\"message\":\"Email is invalid\",\"code\":\"INVALID\"}]}}}"
}

pub fn customer_email_uniqueness_strips_whitespace_and_case_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"foo bar@example.com\" }) { customer { id email } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: duplicate_status, body: duplicate_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"FooBar@example.com \" }) { customer { id email } userErrors { field message code } } }",
    )
  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"customerCreate\":{\"customer\":null,\"userErrors\":[{\"field\":[\"email\"],\"message\":\"Email has already been taken\",\"code\":\"TAKEN\"}]}}}"
}

pub fn customer_set_email_validation_uses_input_field_path_test() {
  let #(Response(status: invalid_status, body: invalid_body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { customerSet(input: { email: \"foo@\" }) { customer { id } userErrors { field message code } } }",
    )
  assert invalid_status == 200
  assert json.to_string(invalid_body)
    == "{\"data\":{\"customerSet\":{\"customer\":null,\"userErrors\":[{\"field\":[\"input\",\"email\"],\"message\":\"Email is invalid\",\"code\":\"INVALID\"}]}}}"

  let too_long_email = string.repeat("a", times: 244) <> "@example.com"
  let #(Response(status: long_status, body: long_body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { customerSet(input: { email: \""
        <> too_long_email
        <> "\" }) { customer { id } userErrors { field message code } } }",
    )
  assert long_status == 200
  assert json.to_string(long_body)
    == "{\"data\":{\"customerSet\":{\"customer\":null,\"userErrors\":[{\"field\":[\"input\",\"email\"],\"message\":\"Email is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\"},{\"field\":[\"input\",\"email\"],\"message\":\"Email is invalid\",\"code\":\"INVALID\"}]}}}"
}

pub fn customer_set_sanitizes_and_dedupes_email_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSet(input: { email: \"set email@example.com\" }) { customer { id email defaultEmailAddress { emailAddress } } userErrors { field message code } } }",
    )
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"customerSet\":{\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"setemail@example.com\",\"defaultEmailAddress\":{\"emailAddress\":\"setemail@example.com\"}},\"userErrors\":[]}}}"

  let #(Response(status: duplicate_status, body: duplicate_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerSet(input: { email: \"SetEmail@example.com \" }) { customer { id email } userErrors { field message code } } }",
    )
  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"customerSet\":{\"customer\":null,\"userErrors\":[{\"field\":[\"input\",\"email\"],\"message\":\"Email has already been taken\",\"code\":\"TAKEN\"}]}}}"
}

pub fn customer_create_splits_comma_tags_and_dedupes_case_insensitively_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"tags-normalized@example.com\", tags: [\"a, b , c\", \"VIP\", \"vip\"] }) { customer { id tags } userErrors { field message code } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"tags\":[\"a\",\"b\",\"c\",\"VIP\"]")
  assert string.contains(create_json, "\"userErrors\":[]")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id tags } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"tags\":[\"a\",\"b\",\"c\",\"VIP\"]",
  )
}

pub fn customer_create_rejects_tag_count_after_comma_split_test() {
  let proxy = draft_proxy.new()
  let tag_csv = numbered_tags_csv(251)
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"too-many-tags@example.com\", tags: [\""
        <> tag_csv
        <> "\"] }) { customer { id tags } userErrors { field message code } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"customer\":null")
  assert string.contains(
    create_json,
    "\"userErrors\":[{\"field\":[\"tags\"],\"message\":\"Tags cannot be more than 250\",\"code\":\"TOO_MANY_TAGS\"}]",
  )
  assert_log_omits_root(proxy, "customerCreate")
}

pub fn customer_note_length_codes_apply_to_create_update_and_set_test() {
  let long_note = string.repeat("N", times: 5001)
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"too-long-note-create@example.com\", note: \""
        <> long_note
        <> "\" }) { customer { id note } userErrors { field message code } } }",
    )
  assert create_status == 200
  assert string.contains(
    json.to_string(create_body),
    "\"userErrors\":[{\"field\":[\"note\"],\"message\":\"Note is too long (maximum is 5000 characters)\",\"code\":\"TOO_LONG\"}]",
  )
  assert_log_omits_root(proxy, "customerCreate")

  let #(Response(status: base_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"note-base@example.com\" }) { customer { id } userErrors { field message code } } }",
    )
  assert base_status == 200

  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerUpdate(input: { id: \"gid://shopify/Customer/1\", note: \""
        <> long_note
        <> "\" }) { customer { id note } userErrors { field message code } } }",
    )
  assert update_status == 200
  assert string.contains(
    json.to_string(update_body),
    "\"userErrors\":[{\"field\":[\"note\"],\"message\":\"Note is too long (maximum is 5000 characters)\",\"code\":\"TOO_LONG\"}]",
  )
  assert_log_occurrences(proxy, "customerCreate", 1)
  assert_log_omits_root(proxy, "customerUpdate")

  let #(Response(status: set_status, body: set_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSet(identifier: { id: \"gid://shopify/Customer/1\" }, input: { note: \""
        <> long_note
        <> "\" }) { customer { id note } userErrors { field message code } } }",
    )
  assert set_status == 200
  assert string.contains(
    json.to_string(set_body),
    "\"userErrors\":[{\"field\":[\"input\",\"note\"],\"message\":\"Note is too long (maximum is 5000 characters)\",\"code\":\"TOO_LONG\"}]",
  )
  assert_log_occurrences(proxy, "customerCreate", 1)
  assert_log_omits_root(proxy, "customerSet")
}

pub fn customer_update_rejects_clearing_last_email_identity_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"identity-only@example.com\" }) { customer { id email } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerUpdate(input: { id: \"gid://shopify/Customer/1\", email: null }) { customer { id email firstName lastName displayName defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } } userErrors { field message code } } }",
    )
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"customer\":null")
  assert_required_identity_user_error(update_json)
  assert_log_omits_root(proxy, "customerUpdate")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id email defaultEmailAddress { emailAddress } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"email\":\"identity-only@example.com\"")
  assert string.contains(
    read_json,
    "\"defaultEmailAddress\":{\"emailAddress\":\"identity-only@example.com\"}",
  )
}

pub fn customer_update_rejects_clearing_last_phone_identity_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { phone: \"+14155550123\" }) { customer { id defaultPhoneNumber { phoneNumber } } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerUpdate(input: { id: \"gid://shopify/Customer/1\", phone: null }) { customer { id defaultPhoneNumber { phoneNumber } } userErrors { field message code } } }",
    )
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"customer\":null")
  assert_required_identity_user_error(update_json)
  assert_log_omits_root(proxy, "customerUpdate")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id email defaultPhoneNumber { phoneNumber } } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"defaultPhoneNumber\":{\"phoneNumber\":\"+14155550123\"}",
  )
}

pub fn customer_create_normalizes_formatted_phone_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { phone: \"+1 (613) 450-4538\" }) { customer { id phone defaultPhoneNumber { phoneNumber } } userErrors { field message code } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"userErrors\":[]")
  assert string.contains(create_json, "\"phone\":\"+16134504538\"")
  assert string.contains(
    create_json,
    "\"defaultPhoneNumber\":{\"phoneNumber\":\"+16134504538\"}",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { phone defaultPhoneNumber { phoneNumber } } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"phone\":\"+16134504538\"",
  )
  assert string.contains(
    json.to_string(read_body),
    "\"defaultPhoneNumber\":{\"phoneNumber\":\"+16134504538\"}",
  )
}

pub fn customer_phone_validation_and_uniqueness_use_normalized_values_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { phone: \"+1-613-450-4538\" }) { customer { id defaultPhoneNumber { phoneNumber } } userErrors { field message code } } }",
    )
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let #(Response(status: duplicate_status, body: duplicate_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { phone: \"+16134504538\" }) { customer { id } userErrors { field message code } } }",
    )
  assert duplicate_status == 200
  let duplicate_json = json.to_string(duplicate_body)
  assert string.contains(duplicate_json, "\"customer\":null")
  assert string.contains(
    duplicate_json,
    "\"userErrors\":[{\"field\":[\"phone\"],\"message\":\"Phone has already been taken\"",
  )

  let long_phone = "+" <> string.repeat("1", times: 255)
  let #(Response(status: long_status, body: long_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { phone: \""
        <> long_phone
        <> "\" }) { customer { id } userErrors { field message code } } }",
    )
  assert long_status == 200
  let long_json = json.to_string(long_body)
  assert string.contains(long_json, "\"customer\":null")
  assert string.contains(
    long_json,
    "\"userErrors\":[{\"field\":[\"phone\"],\"message\":\"Phone is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\"},{\"field\":[\"phone\"],\"message\":\"Phone is invalid\",\"code\":\"INVALID\"}]",
  )

  let #(Response(status: set_status, body: set_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerSet(identifier: { email: \"phone-set-invalid@example.com\" }, input: { email: \"phone-set-invalid@example.com\", phone: \"+1234abcd\" }) { customer { id } userErrors { field message code } } }",
    )
  assert set_status == 200
  assert string.contains(
    json.to_string(set_body),
    "\"userErrors\":[{\"field\":[\"input\",\"phone\"],\"message\":\"Phone is invalid\",\"code\":\"INVALID\"}]",
  )
}

pub fn customer_update_rejects_clearing_last_name_identity_after_control_update_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"name-control@example.com\", firstName: \"Hermes\", lastName: \"Identity\" }) { customer { id firstName lastName email } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: control_status, body: control_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerUpdate(input: { id: \"gid://shopify/Customer/1\", email: null }) { customer { id firstName lastName email defaultEmailAddress { emailAddress } } userErrors { field message code } } }",
    )
  assert control_status == 200
  let control_json = json.to_string(control_body)
  assert string.contains(control_json, "\"userErrors\":[]")
  assert string.contains(control_json, "\"email\":null")
  assert_log_contains_root(proxy, "customerUpdate")

  let #(Response(status: reject_status, body: reject_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerUpdate(input: { id: \"gid://shopify/Customer/1\", firstName: null, lastName: null }) { customer { id firstName lastName email } userErrors { field message code } } }",
    )
  assert reject_status == 200
  let reject_json = json.to_string(reject_body)
  assert string.contains(reject_json, "\"customer\":null")
  assert_required_identity_user_error(reject_json)
  assert_log_occurrences(proxy, "customerUpdate", 1)

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id firstName lastName email defaultEmailAddress { emailAddress } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"firstName\":\"Hermes\"")
  assert string.contains(read_json, "\"lastName\":\"Identity\"")
  assert string.contains(read_json, "\"email\":null")
}

pub fn customer_account_invite_stages_only_invitable_customers_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"invite@example.com\" }) { customer { id state } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: invite_status, body: invite_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSendAccountInviteEmail(customerId: \"gid://shopify/Customer/1\") { customer { id state } userErrors { field message code } } }",
    )
  assert invite_status == 200
  let invite_json = json.to_string(invite_body)
  assert string.contains(invite_json, "\"state\":\"INVITED\"")
  assert string.contains(invite_json, "\"userErrors\":[]")

  let enabled_proxy =
    proxy
    |> set_customer_state("gid://shopify/Customer/1", "ENABLED")
  let #(Response(status: enabled_status, body: enabled_body, ..), next_proxy) =
    graphql(
      enabled_proxy,
      "mutation { customerSendAccountInviteEmail(customerId: \"gid://shopify/Customer/1\") { customer { id state } userErrors { field message code } } }",
    )
  assert enabled_status == 200
  let enabled_json = json.to_string(enabled_body)
  assert string.contains(
    enabled_json,
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"state\":\"ENABLED\"}",
  )
  assert string.contains(
    enabled_json,
    "\"userErrors\":[{\"field\":[\"customerId\"],\"message\":\"Account already enabled\",\"code\":\"ACCOUNT_ALREADY_ENABLED\"}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      next_proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id state } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"state\":\"ENABLED\"}",
  )
}

pub fn customer_account_invite_validates_email_overrides_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"invite-validation@example.com\" }) { customer { id state } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: invite_status, body: invite_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSendAccountInviteEmail(customerId: \"gid://shopify/Customer/1\", email: { subject: \"\", to: \"not-an-email\", from: \"\", bcc: [\"bad\", \"ok@example.com\"], customMessage: \"<script>bad</script>\" }) { customer { id state } userErrors { field message code } } }",
    )
  assert invite_status == 200
  let invite_json = json.to_string(invite_body)
  assert string.contains(invite_json, "\"customer\":null,\"userErrors\":[")
  assert string.contains(
    invite_json,
    "{\"field\":[\"email\",\"subject\"],\"message\":\"Subject can't be blank\",\"code\":\"INVALID\"}",
  )
  assert string.contains(
    invite_json,
    "{\"field\":[\"email\",\"to\"],\"message\":\"To must be blank when the customer has an email address\",\"code\":\"INVALID\"}",
  )
  assert string.contains(
    invite_json,
    "{\"field\":[\"email\",\"from\"],\"message\":\"From Sender is invalid\",\"code\":\"INVALID\"}",
  )
  assert string.contains(
    invite_json,
    "{\"field\":[\"email\",\"bcc\"],\"message\":\"Bcc bad is not a valid bcc address and ok@example.com is not a valid bcc address\",\"code\":\"INVALID\"}",
  )
  assert string.contains(
    invite_json,
    "{\"field\":[\"customerId\"],\"message\":\"Error sending account invite to customer.\",\"code\":\"INVALID\"}",
  )

  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id state } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"state\":\"DISABLED\"}",
  )

  let log_request =
    Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
  let #(Response(status: log_status, body: log_body, ..), _) =
    draft_proxy.process_request(proxy, log_request)
  assert log_status == 200
  let log_json = json.to_string(log_body)
  assert string.contains(log_json, "\"primaryRootField\":\"customerCreate\"")
  assert !string.contains(log_json, "customerSendAccountInviteEmail")
}

pub fn customer_account_invite_accepts_valid_to_override_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { phone: \"+14155550123\" }) { customer { id state } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: invite_status, body: invite_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerSendAccountInviteEmail(customerId: \"gid://shopify/Customer/1\", email: { subject: \"Account invite\", to: \"valid@example.com\" }) { customer { id state } userErrors { field message code } } }",
    )
  assert invite_status == 200
  let invite_json = json.to_string(invite_body)
  assert string.contains(
    invite_json,
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"state\":\"INVITED\"}",
  )
  assert string.contains(invite_json, "\"userErrors\":[]")
}

pub fn customer_account_invite_rejects_oversized_subject_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"invite-long-subject@example.com\" }) { customer { id state } userErrors { field message code } } }",
    )
  assert create_status == 200

  let long_subject = string.repeat("s", 1001)
  let #(Response(status: invite_status, body: invite_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSendAccountInviteEmail(customerId: \"gid://shopify/Customer/1\", email: { subject: \""
        <> long_subject
        <> "\" }) { customer { id state } userErrors { field message code } } }",
    )
  assert invite_status == 200
  assert string.contains(
    json.to_string(invite_body),
    "\"userErrors\":[{\"field\":[\"customerId\"],\"message\":\"Error sending account invite to customer.\",\"code\":\"INVALID\"}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { id state } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"state\":\"DISABLED\"}",
  )
}

pub fn customer_create_rejects_client_supplied_id_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { id: \"gid://shopify/Customer/999\", email: \"client-id@example.com\" }) { customer { id email } userErrors { field message } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"customer\":null")
  assert string.contains(
    create_json,
    "\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot specify ID on creation\"}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, "query { customersCount { count precision } }")
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"customersCount\":{\"count\":0,\"precision\":\"EXACT\"}",
  )
}

pub fn customer_set_unknown_identifier_id_errors_without_staging_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: set_status, body: set_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSet(identifier: { id: \"gid://shopify/Customer/999999999\" }, input: { email: \"buyer@example.com\" }) { customer { id email } userErrors { field message code } } }",
    )
  assert set_status == 200
  let set_json = json.to_string(set_body)
  assert string.contains(set_json, "\"customer\":null")
  assert string.contains(
    set_json,
    "\"userErrors\":[{\"field\":[\"input\"],\"message\":\"Resource matching the identifier was not found.\",\"code\":\"INVALID\"}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, "query { customers(first: 5) { nodes { id email } } }")
  assert read_status == 200
  assert string.contains(json.to_string(read_body), "\"nodes\":[]")
  assert_log_omits_root(proxy, "customerSet")
  assert_next_customer_create_uses_first_customer_id(proxy)
}

pub fn customer_set_mixed_identifier_unknown_id_wins_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: set_status, body: set_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSet(identifier: { id: \"gid://shopify/Customer/999999999\", email: \"mixed@example.com\" }, input: { email: \"mixed@example.com\", firstName: \"Mixed\" }) { customer { id email firstName } userErrors { field message code } } }",
    )
  assert set_status == 200
  let set_json = json.to_string(set_body)
  assert string.contains(set_json, "\"customer\":null")
  assert string.contains(
    set_json,
    "\"userErrors\":[{\"field\":[\"input\"],\"message\":\"Resource matching the identifier was not found.\",\"code\":\"INVALID\"}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, "query { customers(first: 5) { nodes { id email } } }")
  assert read_status == 200
  assert string.contains(json.to_string(read_body), "\"nodes\":[]")
}

pub fn customer_set_email_and_phone_identifiers_create_without_id_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: email_status, body: email_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSet(identifier: { email: \"set-email@example.com\" }, input: { email: \"set-email@example.com\", firstName: \"Email\" }) { customer { id email firstName } userErrors { field message code } } }",
    )
  assert email_status == 200
  let email_json = json.to_string(email_body)
  assert string.contains(
    email_json,
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"set-email@example.com\",\"firstName\":\"Email\"}",
  )
  assert string.contains(email_json, "\"userErrors\":[]")

  let #(Response(status: phone_status, body: phone_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerSet(identifier: { phone: \"+14155550123\" }, input: { phone: \"+14155550123\", firstName: \"Phone\" }) { customer { id defaultPhoneNumber { phoneNumber } firstName } userErrors { field message code } } }",
    )
  assert phone_status == 200
  let phone_json = json.to_string(phone_body)
  assert string.contains(
    phone_json,
    "\"customer\":{\"id\":\"gid://shopify/Customer/3\",\"defaultPhoneNumber\":{\"phoneNumber\":\"+14155550123\"},\"firstName\":\"Phone\"}",
  )
  assert string.contains(phone_json, "\"userErrors\":[]")
}

pub fn customer_create_rejects_nested_client_supplied_ids_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"nested-client-id@example.com\", addresses: [{ id: \"gid://shopify/MailingAddress/999\", address1: \"1 Spear St\" }], metafields: [{ id: \"gid://shopify/Metafield/999\", namespace: \"ns\", key: \"k\", value: \"v\", type: \"single_line_text_field\" }] }) { customer { id email } userErrors { field message code } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"customer\":null")
  assert string.contains(
    create_json,
    "\"userErrors\":[{\"field\":[\"addresses\",\"0\",\"id\"],\"message\":\"Cannot specify address ID on creation\",\"code\":\"INVALID\"}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, "query { customersCount { count precision } }")
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"customersCount\":{\"count\":0,\"precision\":\"EXACT\"}",
  )
  assert_log_omits_root(proxy, "customerCreate")
  assert_next_customer_create_uses_first_customer_id(proxy)
}

pub fn customer_create_rejects_nested_metafield_client_supplied_id_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"nested-metafield-client-id@example.com\", metafields: [{ id: \"gid://shopify/Metafield/999\", namespace: \"ns\", key: \"k\", value: \"v\", type: \"single_line_text_field\" }] }) { customer { id email } userErrors { field message code } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"customer\":null")
  assert string.contains(
    create_json,
    "\"userErrors\":[{\"field\":[\"metafields\",\"0\",\"id\"],\"message\":\"Cannot specify metafield ID on creation\",\"code\":\"INVALID\"}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, "query { customersCount { count precision } }")
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"customersCount\":{\"count\":0,\"precision\":\"EXACT\"}",
  )
  assert_log_omits_root(proxy, "customerCreate")
  assert_next_customer_create_uses_first_customer_id(proxy)
}

pub fn customer_address_lifecycle_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"address@example.com\", firstName: \"Ada\", lastName: \"Lovelace\" }) { customer { id } userErrors { field message } } }",
    )
  assert create_status == 200

  let #(Response(status: address_status, body: address_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { address1: \"1 Main St\", city: \"Ottawa\", countryCode: CA, provinceCode: ON, zip: \"K1A 0B1\" }, setAsDefault: true) { address { id address1 city country countryCodeV2 province provinceCode formattedArea } userErrors { field message } } }",
    )
  assert address_status == 200
  let address_json = json.to_string(address_body)
  assert string.contains(
    address_json,
    "\"id\":\"gid://shopify/MailingAddress/3\"",
  )
  assert string.contains(address_json, "\"country\":\"Canada\"")
  assert string.contains(address_json, "\"province\":\"Ontario\"")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { defaultAddress { id address1 formattedArea } addressesV2(first: 5) { nodes { id address1 city } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"defaultAddress\":{\"id\":\"gid://shopify/MailingAddress/3\"",
  )
  assert string.contains(
    read_json,
    "\"addressesV2\":{\"nodes\":[{\"id\":\"gid://shopify/MailingAddress/3\"",
  )
}

pub fn customer_address_update_rejects_mismatched_nested_address_id_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"address-id-mismatch@example.com\" }) { customer { id } userErrors { field message } } }",
    )
  assert create_status == 200

  let #(Response(status: first_address_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { address1: \"1 Main St\", city: \"Ottawa\", countryCode: CA, provinceCode: ON, zip: \"K1A 0B1\" }) { address { id address1 } userErrors { field message } } }",
    )
  assert first_address_status == 200

  let #(Response(status: second_address_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { address1: \"2 Side St\", city: \"Toronto\", countryCode: CA, provinceCode: ON, zip: \"M5H 2N2\" }) { address { id address1 } userErrors { field message } } }",
    )
  assert second_address_status == 200

  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressUpdate(customerId: \"gid://shopify/Customer/1\", addressId: \"gid://shopify/MailingAddress/3\", address: { id: \"gid://shopify/MailingAddress/5\", address1: \"999 Bryant\" }) { address { id address1 } userErrors { field message code } } }",
    )
  assert update_status == 200
  assert string.contains(json.to_string(update_body), "\"address\":null")
  assert string.contains(
    json.to_string(update_body),
    "\"userErrors\":[{\"field\":[\"addressId\"],\"message\":\"The id of the address does not match the id in the input\",\"code\":null}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { addressesV2(first: 5) { nodes { id address1 city } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"address1\":\"1 Main St\"")
  assert string.contains(read_json, "\"address1\":\"2 Side St\"")
  assert !string.contains(read_json, "999 Bryant")
}

pub fn customer_address_country_province_validation_test() {
  let proxy = draft_proxy.new()

  let #(
    Response(status: invalid_create_status, body: invalid_create_body, ..),
    proxy,
  ) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"invalid-address@example.com\", addresses: [{ address1: \"1 Spear\", country: \"Atlantis\" }] }) { customer { id } userErrors { field message code } } }",
    )
  assert invalid_create_status == 200
  let invalid_create_json = json.to_string(invalid_create_body)
  assert string.contains(invalid_create_json, "\"customer\":null")
  assert string.contains(
    invalid_create_json,
    "\"userErrors\":[{\"field\":[\"addresses\",\"0\",\"country\"],\"message\":\"Country is invalid\",\"code\":\"INVALID\"}]",
  )
  assert_log_omits_root(proxy, "customerCreate")

  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"valid-address@example.com\", addresses: [{ address1: \"1 Valid St\", city: \"San Francisco\", countryCode: US, country: \"Canada\", provinceCode: CA, zip: \"94105\" }] }) { customer { id defaultAddress { country countryCodeV2 province provinceCode formattedArea } } userErrors { field message code } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"userErrors\":[]")
  assert string.contains(create_json, "\"country\":\"United States\"")
  assert string.contains(create_json, "\"countryCodeV2\":\"US\"")
  assert string.contains(create_json, "\"province\":\"California\"")

  let #(
    Response(status: create_address_status, body: create_address_body, ..),
    proxy,
  ) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { address1: \"2 Bad Country\", countryCode: ZZ, provinceCode: ON }) { address { id } userErrors { field message code } } }",
    )
  assert create_address_status == 200
  let create_address_json = json.to_string(create_address_body)
  assert string.contains(create_address_json, "\"address\":null")
  assert string.contains(
    create_address_json,
    "\"userErrors\":[{\"field\":[\"address\",\"country\"],\"message\":\"Country is invalid\",\"code\":\"INVALID\"}]",
  )
  assert_log_omits_root(proxy, "customerAddressCreate")

  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerUpdate(input: { id: \"gid://shopify/Customer/1\", addresses: [{ address1: \"3 Bad Province\", city: \"Chicago\", countryCode: US, provinceCode: ON }] }) { customer { id } userErrors { field message code } } }",
    )
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"customer\":null")
  assert string.contains(
    update_json,
    "\"userErrors\":[{\"field\":[\"addresses\",\"0\",\"province\"],\"message\":\"Province is invalid\",\"code\":\"INVALID\"}]",
  )

  let #(Response(status: set_status, body: set_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSet(identifier: { id: \"gid://shopify/Customer/1\" }, input: { email: \"valid-address@example.com\", addresses: [{ address1: \"4 Bad Province\", city: \"Chicago\", countryCode: US, provinceCode: ON }] }) { customer { id } userErrors { field message code } } }",
    )
  assert set_status == 200
  let set_json = json.to_string(set_body)
  assert string.contains(set_json, "\"customer\":null")
  assert string.contains(
    set_json,
    "\"userErrors\":[{\"field\":[\"input\",\"addresses\",\"0\",\"province\"],\"message\":\"Province is invalid\",\"code\":\"INVALID\"}]",
  )

  let #(
    Response(status: second_address_status, body: second_address_body, ..),
    proxy,
  ) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { address1: \"5 Valid St\", city: \"New York\", countryCode: US, provinceCode: NY }) { address { id province provinceCode } userErrors { field message code } } }",
    )
  assert second_address_status == 200
  let second_address_json = json.to_string(second_address_body)
  assert string.contains(second_address_json, "\"province\":\"New York\"")

  let #(
    Response(status: address_update_status, body: address_update_body, ..),
    proxy,
  ) =
    graphql(
      proxy,
      "mutation { customerAddressUpdate(customerId: \"gid://shopify/Customer/1\", addressId: \"gid://shopify/MailingAddress/3\", address: { countryCode: US, provinceCode: ON }) { address { id } userErrors { field message code } } }",
    )
  assert address_update_status == 200
  let address_update_json = json.to_string(address_update_body)
  assert string.contains(address_update_json, "\"address\":null")
  assert string.contains(
    address_update_json,
    "\"userErrors\":[{\"field\":[\"address\",\"province\"],\"message\":\"Province is invalid\",\"code\":\"INVALID\"}]",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { defaultAddress { country countryCodeV2 province provinceCode } addressesV2(first: 5) { nodes { address1 province provinceCode } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"province\":\"California\"")
  assert !string.contains(read_json, "Bad Province")
  assert !string.contains(read_json, "Bad Country")
}

pub fn customer_address_input_rejects_string_guardrails_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"address-validation@example.com\" }) { customer { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let too_long = string.repeat("x", times: 256)
  let #(Response(status: address_status, body: address_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { address1: \""
        <> too_long
        <> "\", address2: \"<b>Suite</b>\", city: \"https://evil.example\", company: \"<i>Acme</i>\", zip: \"H0H 0H0 https://x\", phone: \"<a>+1 613\", countryCode: CA, provinceCode: ON }) { address { id } userErrors { field message code } } }",
    )
  assert address_status == 200
  let address_json = json.to_string(address_body)
  assert string.contains(address_json, "\"address\":null")
  assert string.contains(
    address_json,
    "\"field\":[\"address\",\"address1\"],\"message\":\"Address1 is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\"",
  )
  assert string.contains(
    address_json,
    "\"field\":[\"address\",\"address2\"],\"message\":\"Address2 cannot contain HTML tags\",\"code\":\"INVALID\"",
  )
  assert string.contains(
    address_json,
    "\"field\":[\"address\",\"city\"],\"message\":\"City cannot contain URL\",\"code\":\"INVALID\"",
  )
  assert string.contains(
    address_json,
    "\"field\":[\"address\",\"company\"],\"message\":\"Company cannot contain HTML tags\",\"code\":\"INVALID\"",
  )
  assert string.contains(
    address_json,
    "\"field\":[\"address\",\"zip\"],\"message\":\"Zip cannot contain URL\",\"code\":\"INVALID\"",
  )
  assert string.contains(
    address_json,
    "\"field\":[\"address\",\"phone\"],\"message\":\"Phone cannot contain HTML tags\",\"code\":\"INVALID\"",
  )
  let #(Response(status: html_city_status, body: html_city_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { city: \"<script>\", countryCode: CA, provinceCode: ON }) { address { id } userErrors { field message code } } }",
    )
  assert html_city_status == 200
  assert string.contains(
    json.to_string(html_city_body),
    "\"field\":[\"address\",\"city\"],\"message\":\"City cannot contain HTML tags\",\"code\":\"INVALID\"",
  )

  let #(Response(status: phone_url_status, body: phone_url_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { phone: \"https://evil.example\", countryCode: CA, provinceCode: ON }) { address { id } userErrors { field message code } } }",
    )
  assert phone_url_status == 200
  assert string.contains(
    json.to_string(phone_url_body),
    "\"field\":[\"address\",\"phone\"],\"message\":\"Phone cannot contain URL\",\"code\":\"INVALID\"",
  )

  let #(Response(status: emoji_status, body: emoji_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"address-emoji@example.com\", addresses: [{ address1: \"100 Main \u{1F600}\", countryCode: CA, provinceCode: ON }] }) { customer { id } userErrors { field message code } } }",
    )
  assert emoji_status == 200
  let emoji_json = json.to_string(emoji_body)
  assert string.contains(emoji_json, "\"customer\":null")
  assert string.contains(
    emoji_json,
    "\"field\":[\"addresses\",\"0\",\"address1\"],\"message\":\"Address1 cannot contain emojis\",\"code\":\"INVALID\"",
  )

  let #(Response(status: blank_create_status, body: blank_create_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"address-blank@example.com\", addresses: [{ address1: \" \", address2: \" \", city: \" \", company: \" \", zip: \" \", phone: \" \" }] }) { customer { id } userErrors { field message code } } }",
    )
  assert blank_create_status == 200
  let blank_create_json = json.to_string(blank_create_body)
  assert string.contains(blank_create_json, "\"customer\":null")
  assert string.contains(
    blank_create_json,
    "\"field\":[\"addresses\",\"0\"],\"message\":\"Customer address cannot be blank.\",\"code\":\"INVALID\"",
  )
}

pub fn customer_address_inputs_are_trimmed_before_staging_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"address-trim@example.com\", firstName: \"Ada\", lastName: \"Lovelace\" }) { customer { id } userErrors { field message } } }",
    )
  assert create_status == 200

  let #(Response(status: address_status, body: address_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressCreate(customerId: \"gid://shopify/Customer/1\", address: { address1: \" 100 Main \", address2: \" Suite 4 \", city: \" Ottawa \", company: \" Acme \", countryCode: CA, provinceCode: ON, zip: \" K1A 0B1 \", phone: \" +14155550123 \" }, setAsDefault: true) { address { id address1 address2 city company zip phone formattedArea } userErrors { field message } } }",
    )
  assert address_status == 200
  let address_json = json.to_string(address_body)
  assert string.contains(address_json, "\"userErrors\":[]")
  assert string.contains(address_json, "\"address1\":\"100 Main\"")
  assert string.contains(address_json, "\"address2\":\"Suite 4\"")
  assert string.contains(address_json, "\"city\":\"Ottawa\"")
  assert string.contains(address_json, "\"company\":\"Acme\"")
  assert string.contains(address_json, "\"zip\":\"K1A 0B1\"")
  assert string.contains(address_json, "\"phone\":\"+14155550123\"")
  assert !string.contains(address_json, " 100 Main ")

  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressUpdate(customerId: \"gid://shopify/Customer/1\", addressId: \"gid://shopify/MailingAddress/3\", address: { address1: \" 200 Side \", city: \" Toronto \", countryCode: CA, provinceCode: ON, zip: \" M5H 2N2 \" }) { address { id address1 city zip formattedArea } userErrors { field message } } }",
    )
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"address1\":\"200 Side\"")
  assert string.contains(update_json, "\"city\":\"Toronto\"")
  assert string.contains(update_json, "\"zip\":\"M5H 2N2\"")

  let #(Response(status: set_status, body: set_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerSet(identifier: { id: \"gid://shopify/Customer/1\" }, input: { email: \"address-trim@example.com\", addresses: [{ address1: \" 300 Set \", city: \" Vancouver \", countryCode: CA, provinceCode: BC, zip: \" V6B 1A1 \" }] }) { customer { id defaultAddress { address1 city zip } addressesV2(first: 5) { nodes { address1 city zip } } } userErrors { field message } } }",
    )
  assert set_status == 200
  let set_json = json.to_string(set_body)
  assert string.contains(set_json, "\"userErrors\":[]")
  assert string.contains(set_json, "\"address1\":\"300 Set\"")
  assert string.contains(set_json, "\"city\":\"Vancouver\"")
  assert string.contains(set_json, "\"zip\":\"V6B 1A1\"")
  assert !string.contains(set_json, " 300 Set ")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { defaultAddress { address1 city zip } addressesV2(first: 5) { nodes { address1 city zip } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"address1\":\"300 Set\"")
  assert string.contains(read_json, "\"city\":\"Vancouver\"")
  assert string.contains(read_json, "\"zip\":\"V6B 1A1\"")
  assert !string.contains(read_json, " 300 Set ")
}

pub fn customer_address_update_requires_address_owner_test() {
  let #(proxy, owner_id, address_id, other_id) = setup_cross_customer_address()

  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressUpdate(customerId: \""
        <> other_id
        <> "\", addressId: \""
        <> address_id
        <> "\", address: { city: \"Cross Customer\" }, setAsDefault: true) { address { id city } userErrors { field message } } }",
    )
  assert update_status == 200
  assert string.contains(json.to_string(update_body), "\"address\":null")
  assert_address_ownership_error(update_body)

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \""
        <> owner_id
        <> "\") { defaultAddress { id city } addressesV2(first: 5) { nodes { id city } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"city\":\"Ottawa\"")
  assert !string.contains(read_json, "Cross Customer")
}

pub fn customer_address_delete_requires_address_owner_test() {
  let #(proxy, owner_id, address_id, other_id) = setup_cross_customer_address()

  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerAddressDelete(customerId: \""
        <> other_id
        <> "\", addressId: \""
        <> address_id
        <> "\") { deletedAddressId userErrors { field message } } }",
    )
  assert delete_status == 200
  assert string.contains(
    json.to_string(delete_body),
    "\"deletedAddressId\":null",
  )
  assert_address_ownership_error(delete_body)

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \""
        <> owner_id
        <> "\") { addressesV2(first: 5) { nodes { id city } } } }",
    )
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"id\":\"" <> address_id <> "\"",
  )
}

pub fn customer_update_default_address_requires_address_owner_test() {
  let #(proxy, _owner_id, address_id, other_id) = setup_cross_customer_address()

  let #(Response(status: default_status, body: default_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerUpdateDefaultAddress(customerId: \""
        <> other_id
        <> "\", addressId: \""
        <> address_id
        <> "\") { customer { id defaultAddress { id city } } userErrors { field message } } }",
    )
  assert default_status == 200
  assert string.contains(
    json.to_string(default_body),
    "\"defaultAddress\":null",
  )
  assert_address_ownership_error(default_body)

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \""
        <> other_id
        <> "\") { defaultAddress { id city } addressesV2(first: 5) { nodes { id city } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"defaultAddress\":null")
  assert !string.contains(read_json, address_id)
}

pub fn customer_address_update_missing_customer_precedes_address_lookup_test() {
  let #(proxy, _owner_id, address_id, _other_id) =
    setup_cross_customer_address()

  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerAddressUpdate(customerId: \"gid://shopify/Customer/999999\", addressId: \""
        <> address_id
        <> "\", address: { city: \"Cross Customer\" }) { address { id city } userErrors { field message } } }",
    )
  assert update_status == 200
  assert string.contains(json.to_string(update_body), "\"address\":null")
  assert_missing_customer_error(update_body)
}

pub fn customer_address_delete_missing_customer_precedes_address_lookup_test() {
  let #(proxy, _owner_id, address_id, _other_id) =
    setup_cross_customer_address()

  let #(Response(status: delete_status, body: delete_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerAddressDelete(customerId: \"gid://shopify/Customer/999999\", addressId: \""
        <> address_id
        <> "\") { deletedAddressId userErrors { field message } } }",
    )
  assert delete_status == 200
  assert string.contains(
    json.to_string(delete_body),
    "\"deletedAddressId\":null",
  )
  assert_missing_customer_error(delete_body)
}

pub fn customer_update_default_address_missing_customer_precedes_address_lookup_test() {
  let #(proxy, _owner_id, address_id, _other_id) =
    setup_cross_customer_address()

  let #(Response(status: default_status, body: default_body, ..), _) =
    graphql(
      proxy,
      "mutation { customerUpdateDefaultAddress(customerId: \"gid://shopify/Customer/999999\", addressId: \""
        <> address_id
        <> "\") { customer { id defaultAddress { id } } userErrors { field message } } }",
    )
  assert default_status == 200
  assert string.contains(json.to_string(default_body), "\"customer\":null")
  assert_missing_customer_error(default_body)
}

pub fn store_credit_credit_debit_readback_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"credit@example.com\", firstName: \"Credit\", lastName: \"Customer\" }) { customer { id } userErrors { field message } } }",
    )
  assert create_status == 200

  let account =
    StoreCreditAccountRecord(
      id: "gid://shopify/StoreCreditAccount/900",
      customer_id: "gid://shopify/Customer/1",
      cursor: None,
      balance: Money(amount: "10.0", currency_code: "USD"),
    )
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: store_mod.stage_store_credit_account(proxy.store, account),
    )

  let #(Response(status: credit_status, body: credit_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { storeCreditAccountCredit(id: \"gid://shopify/StoreCreditAccount/900\", creditInput: { creditAmount: { amount: \"2.50\", currencyCode: USD } }) { storeCreditAccountTransaction { amount { amount currencyCode } balanceAfterTransaction { amount currencyCode } account { id balance { amount currencyCode } owner { ... on Customer { id email } } } event origin } userErrors { field message code } } }",
    )
  assert credit_status == 200
  assert string.contains(
    json.to_string(credit_body),
    "\"balanceAfterTransaction\":{\"amount\":\"12.50\",\"currencyCode\":\"USD\"}",
  )

  let #(Response(status: debit_status, body: debit_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { storeCreditAccountDebit(id: \"gid://shopify/StoreCreditAccount/900\", debitInput: { debitAmount: { amount: \"1.25\", currencyCode: USD } }) { storeCreditAccountTransaction { amount { amount currencyCode } balanceAfterTransaction { amount currencyCode } account { id balance { amount currencyCode } } event } userErrors { field message code } } }",
    )
  assert debit_status == 200
  assert string.contains(
    json.to_string(debit_body),
    "\"balanceAfterTransaction\":{\"amount\":\"11.25\",\"currencyCode\":\"USD\"}",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { storeCreditAccounts(first: 5) { nodes { id balance { amount currencyCode } } pageInfo { hasNextPage hasPreviousPage } } } storeCreditAccount(id: \"gid://shopify/StoreCreditAccount/900\") { id balance { amount currencyCode } owner { ... on Customer { id email } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"balance\":{\"amount\":\"11.25\",\"currencyCode\":\"USD\"}",
  )
  assert string.contains(
    read_json,
    "\"owner\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"credit@example.com\"}",
  )
}

pub fn store_credit_credit_customer_id_creates_account_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"credit-owner@example.com\", firstName: \"Credit\", lastName: \"Owner\" }) { customer { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: credit_status, body: credit_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"50.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { id balance { amount currencyCode } owner { ... on Customer { id email } } } balanceAfterTransaction { amount currencyCode } } userErrors { field message code } } }",
    )
  assert credit_status == 200
  let credit_json = json.to_string(credit_body)
  assert string.contains(credit_json, "\"userErrors\":[]")
  assert string.contains(
    credit_json,
    "\"balanceAfterTransaction\":{\"amount\":\"50.0\",\"currencyCode\":\"USD\"}",
  )
  assert string.contains(
    credit_json,
    "\"owner\":{\"id\":\"gid://shopify/Customer/1\",\"email\":\"credit-owner@example.com\"}",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { storeCreditAccounts(first: 5) { nodes { id balance { amount currencyCode } owner { ... on Customer { id email } } } } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"balance\":{\"amount\":\"50.0\",\"currencyCode\":\"USD\"}",
  )
}

pub fn store_credit_credit_company_location_id_creates_account_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: company_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyCreate(input: { company: { name: \"Credit B2B\" }, companyLocation: { name: \"HQ\" } }) { company { id locations(first: 5) { nodes { id name } } } userErrors { field message code } } }",
    )
  assert company_status == 200

  let #(Response(status: credit_status, body: credit_body, ..), _) =
    graphql(
      proxy,
      "mutation { storeCreditAccountCredit(id: \"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\", creditInput: { creditAmount: { amount: \"30.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { balance { amount currencyCode } owner { ... on CompanyLocation { id } } } } userErrors { field message code } } }",
    )
  assert credit_status == 200
  let credit_json = json.to_string(credit_body)
  assert string.contains(credit_json, "\"userErrors\":[]")
  assert string.contains(
    credit_json,
    "\"owner\":{\"id\":\"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\"}",
  )
}

pub fn store_credit_adjustments_validate_currency_amount_expiry_and_limits_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"credit-validation@example.com\" }) { customer { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: seed_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"10.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    )
  assert seed_status == 200

  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountCredit(id: \"gid://shopify/StoreCreditAccount/3\", creditInput: { creditAmount: { amount: \"2.00\", currencyCode: CAD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "MISMATCHING_CURRENCY",
  )
  let #(Response(status: cad_status, body: cad_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"2.00\", currencyCode: CAD } }) { storeCreditAccountTransaction { account { balance { amount currencyCode } owner { ... on Customer { id } } } balanceAfterTransaction { amount currencyCode } } userErrors { field message code } } }",
    )
  assert cad_status == 200
  let cad_json = json.to_string(cad_body)
  assert string.contains(cad_json, "\"userErrors\":[]")
  assert string.contains(
    cad_json,
    "\"balanceAfterTransaction\":{\"amount\":\"2.0\",\"currencyCode\":\"CAD\"}",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"0.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "NEGATIVE_OR_ZERO_AMOUNT",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountDebit(id: \"gid://shopify/Customer/1\", debitInput: { debitAmount: { amount: \"-1.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "NEGATIVE_OR_ZERO_AMOUNT",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"1.00\", currencyCode: USD }, expiresAt: \"2000-01-01T00:00:00Z\" }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "EXPIRES_AT_IN_PAST",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"1.00\", currencyCode: XXX } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "UNSUPPORTED_CURRENCY",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountDebit(id: \"gid://shopify/Customer/1\", debitInput: { debitAmount: { amount: \"99.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "INSUFFICIENT_FUNDS",
  )
}

fn assert_store_credit_error(proxy, query, code) {
  let #(Response(status: status, body: body, ..), _) = graphql(proxy, query)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"storeCreditAccountTransaction\":null")
  assert string.contains(body_json, "\"code\":\"" <> code <> "\"")
}

pub fn email_marketing_consent_update_rejects_disallowed_states_test() {
  assert_email_consent_state_rejected("NOT_SUBSCRIBED")
  assert_email_consent_state_rejected("REDACTED")
  assert_email_consent_state_rejected("INVALID")
}

pub fn sms_marketing_consent_update_rejects_disallowed_states_test() {
  assert_sms_consent_state_rejected("NOT_SUBSCRIBED")
  assert_sms_consent_state_rejected("REDACTED")
  assert_sms_consent_state_rejected("INVALID")
}

pub fn marketing_consent_update_allowed_states_still_stage_test() {
  assert_email_consent_state_stages("SUBSCRIBED")
  assert_email_consent_state_stages("UNSUBSCRIBED")
  assert_email_consent_state_stages("PENDING")
  assert_sms_consent_state_stages("SUBSCRIBED")
  assert_sms_consent_state_stages("UNSUBSCRIBED")
  assert_sms_consent_state_stages("PENDING")
}

pub fn email_marketing_consent_update_without_email_noops_test() {
  let proxy = no_contact_customer_proxy()
  let mutation =
    "mutation { customerEmailMarketingConsentUpdate(input: { customerId: \"gid://shopify/Customer/1\", emailMarketingConsent: { marketingState: SUBSCRIBED, marketingOptInLevel: SINGLE_OPT_IN } }) { customer { id email defaultEmailAddress { emailAddress marketingState } emailMarketingConsent { marketingState } defaultPhoneNumber { phoneNumber marketingState } smsMarketingConsent { marketingState } } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"userErrors\":[]")
  assert string.contains(body_json, "\"email\":null")
  assert string.contains(body_json, "\"defaultEmailAddress\":null")
  assert string.contains(body_json, "\"emailMarketingConsent\":null")
  assert string.contains(body_json, "\"defaultPhoneNumber\":null")
  assert string.contains(body_json, "\"smsMarketingConsent\":null")
  assert_no_contact_consent_readback(proxy)
}

pub fn sms_marketing_consent_update_without_phone_errors_test() {
  let proxy = no_contact_customer_proxy()
  let mutation =
    "mutation { customerSmsMarketingConsentUpdate(input: { customerId: \"gid://shopify/Customer/1\", smsMarketingConsent: { marketingState: SUBSCRIBED, marketingOptInLevel: SINGLE_OPT_IN } }) { customer { id defaultPhoneNumber { phoneNumber marketingState } smsMarketingConsent { marketingState } } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"customer\":null")
  assert string.contains(
    body_json,
    "\"userErrors\":[{\"field\":[\"input\",\"smsMarketingConsent\"],\"message\":\"A phone number is required to set the SMS consent state.\",\"code\":\"INVALID\"}]",
  )
  assert_no_contact_consent_readback(proxy)
}

pub fn customer_create_requires_email_for_inline_email_consent_test() {
  let proxy = draft_proxy.new()
  let mutation =
    "mutation { customerCreate(input: { phone: \"+14155550123\", emailMarketingConsent: { marketingState: SUBSCRIBED } }) { customer { id email } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"customer\":null")
  assert string.contains(
    body_json,
    "\"userErrors\":[{\"field\":[\"emailMarketingConsent\"],\"message\":\"An email address is required to set the email marketing consent state.\"}]",
  )
  assert_log_omits_root(proxy, "customerCreate")
  assert_next_customer_create_uses_first_customer_id(proxy)
}

pub fn customer_create_requires_phone_for_inline_sms_consent_test() {
  let proxy = draft_proxy.new()
  let mutation =
    "mutation { customerCreate(input: { email: \"missing-phone@example.com\", smsMarketingConsent: { marketingState: SUBSCRIBED } }) { customer { id phone } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"customer\":null")
  assert string.contains(
    body_json,
    "\"userErrors\":[{\"field\":[\"smsMarketingConsent\"],\"message\":\"A phone number is required to set the SMS consent state.\"}]",
  )
  assert_log_omits_root(proxy, "customerCreate")
  assert_next_customer_create_uses_first_customer_id(proxy)
}

pub fn customer_create_rejects_disallowed_inline_consent_state_test() {
  let proxy = draft_proxy.new()
  let mutation =
    "mutation { customerCreate(input: { email: \"redacted-create@example.com\", emailMarketingConsent: { marketingState: REDACTED } }) { customer { id email } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"errors\":[")
  assert string.contains(
    body_json,
    "\"message\":\"Cannot specify REDACTED as a marketing state input\"",
  )
  assert string.contains(body_json, "\"extensions\":{\"code\":\"INVALID\"}")
  assert string.contains(body_json, "\"path\":[\"customerCreate\"]")
  assert string.contains(body_json, "\"data\":{\"customerCreate\":null}")
  assert_log_omits_root(proxy, "customerCreate")
  assert_next_customer_create_uses_first_customer_id(proxy)
}

pub fn customer_create_allows_not_subscribed_inline_consent_test() {
  let proxy = draft_proxy.new()
  let mutation =
    "mutation { customerCreate(input: { email: \"not-subscribed@example.com\", phone: \"+13127004572\", emailMarketingConsent: { marketingState: NOT_SUBSCRIBED }, smsMarketingConsent: { marketingState: NOT_SUBSCRIBED } }) { customer { id emailMarketingConsent { marketingState } smsMarketingConsent { marketingState } } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  let body_json = json.to_string(body)
  assert string.contains(body_json, "\"userErrors\":[]")
  assert string.contains(
    body_json,
    "\"emailMarketingConsent\":{\"marketingState\":\"NOT_SUBSCRIBED\"}",
  )
  assert string.contains(
    body_json,
    "\"smsMarketingConsent\":{\"marketingState\":\"NOT_SUBSCRIBED\"}",
  )
  assert_log_contains_root(proxy, "customerCreate")
}

fn assert_email_consent_state_rejected(state: String) {
  let proxy = consent_customer_proxy()
  let mutation =
    "mutation { customerEmailMarketingConsentUpdate(input: { customerId: \"gid://shopify/Customer/1\", emailMarketingConsent: { marketingState: "
    <> state
    <> ", marketingOptInLevel: "
    <> opt_in_level_for_state(state)
    <> " } }) { customer { id defaultEmailAddress { marketingState } emailMarketingConsent { marketingState } } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  assert_disallowed_consent_response(
    json.to_string(body),
    "customerEmailMarketingConsentUpdate",
    state,
  )
  assert_email_consent_readback(proxy, "SUBSCRIBED")
  assert_log_omits_root(proxy, "customerEmailMarketingConsentUpdate")
}

fn assert_sms_consent_state_rejected(state: String) {
  let proxy = consent_customer_proxy()
  let mutation =
    "mutation { customerSmsMarketingConsentUpdate(input: { customerId: \"gid://shopify/Customer/1\", smsMarketingConsent: { marketingState: "
    <> state
    <> ", marketingOptInLevel: "
    <> opt_in_level_for_state(state)
    <> " } }) { customer { id defaultPhoneNumber { marketingState } smsMarketingConsent { marketingState } } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  assert_disallowed_consent_response(
    json.to_string(body),
    "customerSmsMarketingConsentUpdate",
    state,
  )
  assert_sms_consent_readback(proxy, "SUBSCRIBED")
  assert_log_omits_root(proxy, "customerSmsMarketingConsentUpdate")
}

fn assert_email_consent_state_stages(state: String) {
  let proxy = consent_customer_proxy()
  let mutation =
    "mutation { customerEmailMarketingConsentUpdate(input: { customerId: \"gid://shopify/Customer/1\", emailMarketingConsent: { marketingState: "
    <> state
    <> ", marketingOptInLevel: "
    <> opt_in_level_for_state(state)
    <> " } }) { customer { id defaultEmailAddress { marketingState } emailMarketingConsent { marketingState } } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  let body_json = json.to_string(body)
  assert !string.contains(body_json, "\"errors\"")
  assert string.contains(body_json, "\"userErrors\":[]")
  assert_email_consent_readback(proxy, state)
}

fn assert_sms_consent_state_stages(state: String) {
  let proxy = consent_customer_proxy()
  let mutation =
    "mutation { customerSmsMarketingConsentUpdate(input: { customerId: \"gid://shopify/Customer/1\", smsMarketingConsent: { marketingState: "
    <> state
    <> ", marketingOptInLevel: "
    <> opt_in_level_for_state(state)
    <> " } }) { customer { id defaultPhoneNumber { marketingState } smsMarketingConsent { marketingState } } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, mutation)
  assert status == 200
  let body_json = json.to_string(body)
  assert !string.contains(body_json, "\"errors\"")
  assert string.contains(body_json, "\"userErrors\":[]")
  assert_sms_consent_readback(proxy, state)
}

fn consent_customer_proxy() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"consent@example.com\", phone: \"+14155550123\", emailMarketingConsent: { marketingState: SUBSCRIBED, marketingOptInLevel: SINGLE_OPT_IN }, smsMarketingConsent: { marketingState: SUBSCRIBED, marketingOptInLevel: SINGLE_OPT_IN } }) { customer { id } userErrors { field message code } } }",
    )
  assert create_status == 200
  proxy
}

fn assert_disallowed_consent_response(
  body_json: String,
  root_name: String,
  state: String,
) {
  assert string.contains(body_json, "\"errors\":[")
  case root_name, state {
    "customerSmsMarketingConsentUpdate", "INVALID" -> {
      assert string.contains(
        body_json,
        "\"message\":\"Variable $input of type CustomerSmsMarketingConsentUpdateInput! was provided invalid value for smsMarketingConsent.marketingState",
      )
      assert string.contains(body_json, "\"code\":\"INVALID_VARIABLE\"")
      assert string.contains(
        body_json,
        "Expected \\\"INVALID\\\" to be one of: NOT_SUBSCRIBED, PENDING, SUBSCRIBED, UNSUBSCRIBED, REDACTED",
      )
      assert !string.contains(body_json, "\"" <> root_name <> "\":null")
    }
    _, _ -> {
      assert string.contains(
        body_json,
        "\"message\":\"Cannot specify "
          <> state
          <> " as a marketing state input\"",
      )
      assert string.contains(body_json, "\"extensions\":{\"code\":\"INVALID\"}")
      assert string.contains(body_json, "\"path\":[\"" <> root_name <> "\"]")
      assert string.contains(body_json, "\"" <> root_name <> "\":null")
    }
  }
  assert !string.contains(body_json, "userErrors")
}

fn assert_email_consent_readback(proxy: draft_proxy.DraftProxy, state: String) {
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { defaultEmailAddress { marketingState } emailMarketingConsent { marketingState } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"defaultEmailAddress\":{\"marketingState\":\"" <> state <> "\"}",
  )
  assert string.contains(
    read_json,
    "\"emailMarketingConsent\":{\"marketingState\":\"" <> state <> "\"}",
  )
}

fn assert_sms_consent_readback(proxy: draft_proxy.DraftProxy, state: String) {
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { defaultPhoneNumber { marketingState } smsMarketingConsent { marketingState } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"defaultPhoneNumber\":{\"marketingState\":\"" <> state <> "\"}",
  )
  assert string.contains(
    read_json,
    "\"smsMarketingConsent\":{\"marketingState\":\"" <> state <> "\"}",
  )
}

fn assert_no_contact_consent_readback(proxy: draft_proxy.DraftProxy) {
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { customer(id: \"gid://shopify/Customer/1\") { email defaultEmailAddress { emailAddress marketingState } emailMarketingConsent { marketingState } defaultPhoneNumber { phoneNumber marketingState } smsMarketingConsent { marketingState } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"email\":null")
  assert string.contains(read_json, "\"defaultEmailAddress\":null")
  assert string.contains(read_json, "\"emailMarketingConsent\":null")
  assert string.contains(read_json, "\"defaultPhoneNumber\":null")
  assert string.contains(read_json, "\"smsMarketingConsent\":null")
  assert !string.contains(read_json, "\"marketingState\":\"SUBSCRIBED\"")
}

fn assert_log_omits_root(proxy: draft_proxy.DraftProxy, root_name: String) {
  let log_request =
    Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
  let #(Response(status: log_status, body: log_body, ..), _) =
    draft_proxy.process_request(proxy, log_request)
  assert log_status == 200
  assert !string.contains(json.to_string(log_body), root_name)
}

fn assert_log_contains_root(proxy: draft_proxy.DraftProxy, root_name: String) {
  let log_request =
    Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
  let #(Response(status: log_status, body: log_body, ..), _) =
    draft_proxy.process_request(proxy, log_request)
  assert log_status == 200
  assert string.contains(json.to_string(log_body), root_name)
}

fn assert_log_occurrences(
  proxy: draft_proxy.DraftProxy,
  root_name: String,
  expected: Int,
) {
  let log_request =
    Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
  let #(Response(status: log_status, body: log_body, ..), _) =
    draft_proxy.process_request(proxy, log_request)
  assert log_status == 200
  assert substring_occurrences(
      json.to_string(log_body),
      "\"primaryRootField\":\"" <> root_name <> "\"",
    )
    == expected
}

fn substring_occurrences(haystack: String, needle: String) -> Int {
  case needle {
    "" -> 0
    _ -> list.length(string.split(haystack, needle)) - 1
  }
}

fn assert_required_identity_user_error(body_json: String) {
  assert string.contains(
    body_json,
    "\"userErrors\":[{\"field\":null,\"message\":\"A name, phone number, or email address must be present\",\"code\":\"INVALID\"}]",
  )
}

fn assert_next_customer_create_uses_first_customer_id(
  proxy: draft_proxy.DraftProxy,
) {
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      proxy,
      "mutation { customerCreate(input: { email: \"after-validation@example.com\" }) { customer { id } userErrors { field message } } }",
    )
  assert status == 200
  assert string.contains(
    json.to_string(body),
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\"}",
  )
}

fn set_customer_state(
  proxy: draft_proxy.DraftProxy,
  customer_id: String,
  state: String,
) -> draft_proxy.DraftProxy {
  let customer = case
    store_mod.get_effective_customer_by_id(proxy.store, customer_id)
  {
    Some(record) -> record
    None -> panic as "expected seeded customer"
  }
  let updated = CustomerRecord(..customer, state: Some(state))
  let #(_, next_store) = store_mod.stage_update_customer(proxy.store, updated)
  proxy_state.DraftProxy(..proxy, store: next_store)
}

fn opt_in_level_for_state(state: String) -> String {
  case state {
    "PENDING" -> "CONFIRMED_OPT_IN"
    _ -> "SINGLE_OPT_IN"
  }
}

fn merge_customer_one() -> CustomerRecord {
  customer_with_state("gid://shopify/Customer/merge-one", "ENABLED")
  |> merge_customer_with_email("merge-one@example.test")
}

fn merge_customer_two() -> CustomerRecord {
  customer_with_state("gid://shopify/Customer/merge-two", "ENABLED")
  |> merge_customer_with_email("merge-two@example.test")
}

fn merge_customer_with_email(
  customer: CustomerRecord,
  email: String,
) -> CustomerRecord {
  CustomerRecord(
    ..customer,
    email: Some(email),
    display_name: Some(email),
    default_email_address: Some(CustomerDefaultEmailAddressRecord(
      email_address: Some(email),
      marketing_state: None,
      marketing_opt_in_level: None,
      marketing_updated_at: None,
    )),
  )
}

fn customer_merge_blocker_proxy(
  one: CustomerRecord,
  two: CustomerRecord,
) -> draft_proxy.DraftProxy {
  let proxy = draft_proxy.new()
  let store = store_mod.upsert_base_customers(proxy.store, [one, two])
  proxy_state.DraftProxy(..proxy, store: store)
}

fn merge_blocker_mutation() -> String {
  "mutation { customerMerge(customerOneId: \"gid://shopify/Customer/merge-one\", customerTwoId: \"gid://shopify/Customer/merge-two\") { resultingCustomerId job { id done } userErrors { field message code } customerMergeErrors { field code block_type } } }"
}

fn numbered_tags(prefix: String, count: Int) -> List(String) {
  do_numbered_tags(prefix, 0, count, [])
}

fn do_numbered_tags(
  prefix: String,
  index: Int,
  count: Int,
  acc: List(String),
) -> List(String) {
  case index >= count {
    True -> list.reverse(acc)
    False ->
      do_numbered_tags(prefix, index + 1, count, [
        prefix <> "-" <> int.to_string(index),
        ..acc
      ])
  }
}

fn repeat(value: String, count: Int) -> String {
  do_repeat(value, count, "")
}

fn do_repeat(value: String, remaining: Int, acc: String) -> String {
  case remaining <= 0 {
    True -> acc
    False -> do_repeat(value, remaining - 1, acc <> value)
  }
}

fn numbered_tags_csv(count: Int) -> String {
  int_range(from: 0, to: count - 1)
  |> list.map(fn(index) { "tag-" <> int.to_string(index) })
  |> string.join(",")
}

fn int_range(from start: Int, to stop: Int) -> List(Int) {
  case start > stop {
    True -> []
    False -> [start, ..int_range(from: start + 1, to: stop)]
  }
}
