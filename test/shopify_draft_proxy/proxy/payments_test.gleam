import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{DraftProxy, Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  CapturedObject, CapturedString, Money, OrderRecord, PaymentScheduleRecord,
  PaymentTermsRecord,
}

const order_id = "gid://shopify/Order/637"

fn graphql(proxy: draft_proxy.DraftProxy, query: String) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"" <> escape(query) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
  |> string.replace("\n", "\\n")
}

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
    "\"code\":\"PAYMENT_TERMS_DELETE_UNSUCCESSFUL\"",
  )
  assert !string.contains(update_body, "PAYMENT_TERMS_NOT_FOUND")
  assert !string.contains(delete_body, "PAYMENT_TERMS_NOT_FOUND")
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
