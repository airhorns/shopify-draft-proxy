import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/payments
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CustomerPaymentMethodRecord,
  type CustomerPaymentMethodSubscriptionContractRecord,
  CustomerPaymentMethodInstrumentRecord, CustomerPaymentMethodRecord,
  CustomerPaymentMethodSubscriptionContractRecord, CustomerRecord,
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

fn graphql(source: store.Store, document: String) {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: source)
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"" <> escape(document) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
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
    graphql(
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
