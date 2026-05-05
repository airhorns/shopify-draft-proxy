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
  CapturedString, DraftOrderRecord, Money, OrderRecord, PaymentScheduleRecord,
  PaymentTermsRecord,
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
