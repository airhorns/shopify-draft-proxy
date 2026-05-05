import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store as store_mod
import shopify_draft_proxy/state/types.{
  type CustomerRecord, CustomerRecord, Money, StoreCreditAccountRecord,
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

fn customer_state_proxy(id: String, state: String) -> draft_proxy.DraftProxy {
  let proxy = draft_proxy.new()
  proxy_state.DraftProxy(
    ..proxy,
    store: store_mod.upsert_base_customers(proxy.store, [
      customer_with_state(id, state),
    ]),
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
