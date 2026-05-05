import gleam/dict
import gleam/json
import gleam/option.{None}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store as store_mod
import shopify_draft_proxy/state/types.{Money, StoreCreditAccountRecord}

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
    "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"2.00\", currencyCode: CAD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "mismatching_currency",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"0.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "negative_or_zero_amount",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountDebit(id: \"gid://shopify/Customer/1\", debitInput: { debitAmount: { amount: \"-1.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "negative_or_zero_amount",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"1.00\", currencyCode: USD }, expiresAt: \"2000-01-01T00:00:00Z\" }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "expires_at_in_past",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountCredit(id: \"gid://shopify/Customer/1\", creditInput: { creditAmount: { amount: \"1.00\", currencyCode: XXX } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "unsupported_currency",
  )
  assert_store_credit_error(
    proxy,
    "mutation { storeCreditAccountDebit(id: \"gid://shopify/Customer/1\", debitInput: { debitAmount: { amount: \"99.00\", currencyCode: USD } }) { storeCreditAccountTransaction { account { id } } userErrors { field message code } } }",
    "credit_limit_exceeded",
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

fn opt_in_level_for_state(state: String) -> String {
  case state {
    "PENDING" -> "CONFIRMED_OPT_IN"
    _ -> "SINGLE_OPT_IN"
  }
}
