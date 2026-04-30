import gleam/dict
import gleam/json
import gleam/option.{None}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy.{Request, Response}
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
    draft_proxy.DraftProxy(
      ..proxy,
      store: store_mod.stage_store_credit_account(proxy.store, account),
    )

  let #(Response(status: credit_status, body: credit_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { storeCreditAccountCredit(id: \"gid://shopify/StoreCreditAccount/900\", creditInput: { amount: { amount: \"2.50\", currencyCode: USD } }) { storeCreditAccountTransaction { amount { amount currencyCode } balanceAfterTransaction { amount currencyCode } account { id balance { amount currencyCode } owner { ... on Customer { id email } } } event origin } userErrors { field message code } } }",
    )
  assert credit_status == 200
  assert string.contains(
    json.to_string(credit_body),
    "\"balanceAfterTransaction\":{\"amount\":\"12.50\",\"currencyCode\":\"USD\"}",
  )

  let #(Response(status: debit_status, body: debit_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { storeCreditAccountDebit(id: \"gid://shopify/StoreCreditAccount/900\", debitInput: { amount: { amount: \"1.25\", currencyCode: USD } }) { storeCreditAccountTransaction { amount { amount currencyCode } balanceAfterTransaction { amount currencyCode } account { id balance { amount currencyCode } } event } userErrors { field message code } } }",
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
