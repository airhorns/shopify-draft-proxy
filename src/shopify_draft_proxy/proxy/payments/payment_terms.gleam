//// Payment terms and payment reminder mutation handling.

import gleam/dict.{type Dict}
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcNull, SrcString,
}
import shopify_draft_proxy/proxy/payments/serializers.{payment_terms_source}
import shopify_draft_proxy/proxy/payments/types.{
  type MutationFieldResult, type PaymentTermsSchedulePlan, type UserError,
  PaymentTermsSchedulePlan, UserError, gid_tail, is_shopify_gid,
  json_array_items, json_get, json_get_bool, json_get_int, json_get_string,
  multiple_payment_schedules_message, mutation_payload_result, non_null_json,
  payment_terms_creation_unsuccessful_code,
  payment_terms_delete_unsuccessful_code, payment_terms_templates,
  payment_terms_update_unsuccessful_code, project_payload, read_string_field,
  user_errors_source,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types as state_types

fn payment_terms_error(
  field: List(String),
  message: String,
  code: String,
) -> UserError {
  UserError(field: Some(field), message: message, code: Some(code))
}

@internal
pub fn maybe_hydrate_payment_terms_owner(
  store: Store,
  owner_id: String,
  upstream: UpstreamContext,
) -> Store {
  case payment_terms_owner_resource_type(owner_id) {
    Some(resource_type) -> {
      case payment_terms_owner_available(store, owner_id) {
        True -> store
        False -> {
          // Pattern 2: paymentTermsCreate needs the upstream order/draft-order
          // reference to exist before staging local payment terms. Snapshot or
          // no-cassette mode preserves Shopify-like local "does not exist" errors.
          let variables = json.object([#("id", json.string(owner_id))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "PaymentTermsOwnerHydrate",
              payment_terms_owner_hydrate_query(resource_type),
              variables,
            )
          {
            Ok(value) ->
              case payment_terms_owner_from_response(value, resource_type) {
                Some(owner) ->
                  hydrate_payment_terms_owner(
                    store,
                    owner_id,
                    resource_type,
                    owner,
                  )
                None -> store
              }
            Error(_) -> store
          }
        }
      }
    }
    _ -> store
  }
}

fn payment_terms_owner_resource_type(owner_id: String) -> Option(String) {
  case
    is_shopify_gid(Some(owner_id), "Order"),
    is_shopify_gid(Some(owner_id), "DraftOrder")
  {
    True, _ -> Some("Order")
    _, True -> Some("DraftOrder")
    _, _ -> None
  }
}

fn payment_terms_owner_available(store: Store, owner_id: String) -> Bool {
  case payment_terms_owner_resource_type(owner_id) {
    Some("Order") ->
      store.payment_terms_owner_exists(store, owner_id)
      || option.is_some(store.get_order_by_id(store, owner_id))
    Some("DraftOrder") ->
      store.payment_terms_owner_exists(store, owner_id)
      || option.is_some(store.get_draft_order_by_id(store, owner_id))
    _ -> False
  }
}

fn payment_terms_owner_hydrate_query(resource_type: String) -> String {
  case resource_type {
    "Order" ->
      "query PaymentTermsOwnerHydrate($id: ID!) {\n"
      <> "  order(id: $id) { id paymentTerms { id } totalOutstandingSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } currentTotalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } totalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } }\n"
      <> "}\n"
    _ ->
      "query PaymentTermsOwnerHydrate($id: ID!) {\n"
      <> "  draftOrder(id: $id) { id paymentTerms { id } subtotalPriceSet { shopMoney { amount currencyCode } } totalPriceSet { shopMoney { amount currencyCode } } }\n"
      <> "}\n"
  }
}

fn payment_terms_owner_from_response(
  value: commit.JsonValue,
  resource_type: String,
) -> Option(commit.JsonValue) {
  let field_name = case resource_type {
    "Order" -> "order"
    _ -> "draftOrder"
  }
  json_get(value, "data")
  |> option.then(fn(data) { json_get(data, field_name) })
  |> non_null_json
}

fn hydrate_payment_terms_owner(
  store: Store,
  owner_id: String,
  resource_type: String,
  owner: commit.JsonValue,
) -> Store {
  let id = json_get_string(owner, "id") |> option.unwrap(owner_id)
  let owner_data = captured_json_from_commit(owner)
  let with_owner = store.register_payment_terms_owner(store, id)
  case resource_type {
    "Order" ->
      store.upsert_base_orders(with_owner, [
        state_types.OrderRecord(id: id, cursor: None, data: owner_data),
      ])
    _ ->
      store.upsert_base_draft_orders(with_owner, [
        state_types.DraftOrderRecord(id: id, cursor: None, data: owner_data),
      ])
  }
}

@internal
pub fn hydrate_payment_schedule_context(
  store: Store,
  schedule_ids: List(String),
  upstream: UpstreamContext,
) -> Store {
  list.fold(schedule_ids, store, fn(current, id) {
    maybe_hydrate_payment_schedule(current, id, upstream)
  })
}

fn maybe_hydrate_payment_schedule(
  store: Store,
  schedule_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_shopify_gid(Some(schedule_id), "PaymentSchedule"),
    store.get_effective_payment_schedule_by_id(store, schedule_id)
  {
    True, None -> {
      let variables = json.object([#("id", json.string(schedule_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "PaymentScheduleReminderHydrate",
          payment_schedule_reminder_hydrate_query(),
          variables,
        )
      {
        Ok(value) -> hydrate_payment_schedule_from_response(store, value)
        Error(_) -> store
      }
    }
    _, _ -> store
  }
}

fn payment_schedule_reminder_hydrate_query() -> String {
  "query PaymentScheduleReminderHydrate($id: ID!) {\n"
  <> "  paymentSchedule: node(id: $id) {\n"
  <> "    ... on PaymentSchedule {\n"
  <> "      id dueAt issuedAt completedAt\n"
  <> "      paymentTerms {\n"
  <> "        id overdue dueInDays paymentTermsName paymentTermsType translatedName\n"
  <> "        order { id closed closedAt cancelledAt displayFinancialStatus }\n"
  <> "        draftOrder { id status completedAt }\n"
  <> "        paymentSchedules(first: 10) { nodes { id dueAt issuedAt completedAt } }\n"
  <> "      }\n"
  <> "    }\n"
  <> "  }\n"
  <> "}\n"
}

fn hydrate_payment_schedule_from_response(
  store: Store,
  value: commit.JsonValue,
) -> Store {
  let node =
    json_get(value, "data")
    |> option.then(fn(data) { json_get(data, "paymentSchedule") })
    |> non_null_json

  case node |> option.then(payment_schedule_context_from_node) {
    Some(#(terms, order, draft_order)) -> {
      let with_order = case order {
        Some(record) -> store.upsert_base_orders(store, [record])
        None -> store
      }
      let with_draft = case draft_order {
        Some(record) -> store.upsert_base_draft_orders(with_order, [record])
        None -> with_order
      }
      store.upsert_base_payment_terms(with_draft, terms)
    }
    None -> store
  }
}

fn payment_schedule_context_from_node(
  node: commit.JsonValue,
) -> Option(
  #(
    state_types.PaymentTermsRecord,
    Option(state_types.OrderRecord),
    Option(state_types.DraftOrderRecord),
  ),
) {
  use primary_schedule <- option.then(payment_schedule_from_hydrate_node(node))
  use terms_node <- option.then(json_get(node, "paymentTerms"))
  use terms_id <- option.then(json_get_string(terms_node, "id"))
  let order =
    json_get(terms_node, "order") |> option.then(order_from_terms_node)
  let draft_order =
    json_get(terms_node, "draftOrder")
    |> option.then(draft_order_from_terms_node)
  let owner_id = case order, draft_order {
    Some(record), _ -> Some(record.id)
    _, Some(record) -> Some(record.id)
    _, _ -> None
  }
  use owner <- option.then(owner_id)
  let schedules =
    json_get(terms_node, "paymentSchedules")
    |> option.then(fn(connection) { json_get(connection, "nodes") })
    |> json_array_items
    |> list.filter_map(fn(node) {
      case payment_schedule_from_hydrate_node(node) {
        Some(schedule) -> Ok(schedule)
        None -> Error(Nil)
      }
    })
    |> append_schedule_if_missing(primary_schedule)
  Some(#(
    state_types.PaymentTermsRecord(
      id: terms_id,
      owner_id: owner,
      due: False,
      overdue: json_get_bool(terms_node, "overdue") |> option.unwrap(False),
      due_in_days: json_get_int(terms_node, "dueInDays"),
      payment_terms_name: json_get_string(terms_node, "paymentTermsName")
        |> option.unwrap(""),
      payment_terms_type: json_get_string(terms_node, "paymentTermsType")
        |> option.unwrap(""),
      translated_name: json_get_string(terms_node, "translatedName")
        |> option.unwrap(""),
      payment_schedules: schedules,
    ),
    order,
    draft_order,
  ))
}

fn payment_schedule_from_hydrate_node(
  node: commit.JsonValue,
) -> Option(state_types.PaymentScheduleRecord) {
  use id <- option.then(json_get_string(node, "id"))
  Some(state_types.PaymentScheduleRecord(
    id: id,
    due_at: json_get_string(node, "dueAt"),
    issued_at: json_get_string(node, "issuedAt"),
    completed_at: json_get_string(node, "completedAt"),
    due: None,
    amount: None,
    balance_due: None,
    total_balance: None,
  ))
}

fn append_schedule_if_missing(
  schedules: List(state_types.PaymentScheduleRecord),
  primary: state_types.PaymentScheduleRecord,
) -> List(state_types.PaymentScheduleRecord) {
  case list.any(schedules, fn(schedule) { schedule.id == primary.id }) {
    True -> schedules
    False -> [primary, ..schedules]
  }
}

fn order_from_terms_node(
  node: commit.JsonValue,
) -> Option(state_types.OrderRecord) {
  use id <- option.then(json_get_string(node, "id"))
  Some(state_types.OrderRecord(
    id: id,
    cursor: None,
    data: state_types.CapturedObject([
      #("id", state_types.CapturedString(id)),
      #(
        "closed",
        state_types.CapturedBool(
          json_get_bool(node, "closed") |> option.unwrap(False),
        ),
      ),
      #("closedAt", optional_captured_string(json_get_string(node, "closedAt"))),
      #(
        "cancelledAt",
        optional_captured_string(json_get_string(node, "cancelledAt")),
      ),
      #(
        "displayFinancialStatus",
        optional_captured_string(json_get_string(node, "displayFinancialStatus")),
      ),
    ]),
  ))
}

fn draft_order_from_terms_node(
  node: commit.JsonValue,
) -> Option(state_types.DraftOrderRecord) {
  use id <- option.then(json_get_string(node, "id"))
  Some(state_types.DraftOrderRecord(
    id: id,
    cursor: None,
    data: state_types.CapturedObject([
      #("id", state_types.CapturedString(id)),
      #(
        "status",
        state_types.CapturedString(
          json_get_string(node, "status") |> option.unwrap("OPEN"),
        ),
      ),
      #(
        "completedAt",
        optional_captured_string(json_get_string(node, "completedAt")),
      ),
    ]),
  ))
}

fn optional_captured_string(
  value: Option(String),
) -> state_types.CapturedJsonValue {
  case value {
    Some(value) -> state_types.CapturedString(value)
    None -> state_types.CapturedNull
  }
}

@internal
pub fn create_payment_terms(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  let reference_id =
    graphql_helpers.read_arg_string_nonempty(args, "referenceId")
  let attrs =
    graphql_helpers.read_arg_object(args, "paymentTermsAttributes")
    |> option.unwrap(dict.new())
  case has_multiple_payment_schedules(attrs) {
    True ->
      payment_terms_result(
        store,
        identity,
        field,
        fragments,
        "paymentTermsCreate",
        None,
        [
          payment_terms_error(
            ["base"],
            multiple_payment_schedules_message,
            payment_terms_creation_unsuccessful_code,
          ),
        ],
        [],
      )
    False ->
      case reference_id {
        Some(owner_id) -> {
          case payment_terms_owner_available(store, owner_id) {
            True -> {
              let amount =
                payment_terms_owner_money(store, owner_id)
                |> option.unwrap(default_payment_terms_money())
              case
                build_payment_terms(
                  identity,
                  owner_id,
                  attrs,
                  None,
                  amount,
                  ["paymentTermsAttributes"],
                  payment_terms_creation_unsuccessful_code,
                )
              {
                Ok(#(record, next_identity)) ->
                  payment_terms_result(
                    store.upsert_staged_payment_terms(store, record),
                    next_identity,
                    field,
                    fragments,
                    "paymentTermsCreate",
                    Some(record),
                    [],
                    [record.id],
                  )
                Error(error) ->
                  payment_terms_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "paymentTermsCreate",
                    None,
                    [error],
                    [],
                  )
              }
            }
            False ->
              payment_terms_result(
                store,
                identity,
                field,
                fragments,
                "paymentTermsCreate",
                None,
                [
                  payment_terms_error(
                    ["referenceId"],
                    "Reference does not exist",
                    payment_terms_creation_unsuccessful_code,
                  ),
                ],
                [],
              )
          }
        }
        _ ->
          payment_terms_result(
            store,
            identity,
            field,
            fragments,
            "paymentTermsCreate",
            None,
            [
              payment_terms_error(
                ["referenceId"],
                "Reference does not exist",
                payment_terms_creation_unsuccessful_code,
              ),
            ],
            [],
          )
      }
  }
}

@internal
pub fn update_payment_terms(store, identity, field, fragments, variables) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let id = read_string_field(input, "paymentTermsId")
  let attrs =
    graphql_helpers.read_arg_object(input, "paymentTermsAttributes")
    |> option.unwrap(dict.new())
  case has_multiple_payment_schedules(attrs) {
    True ->
      payment_terms_result(
        store,
        identity,
        field,
        fragments,
        "paymentTermsUpdate",
        None,
        [
          payment_terms_error(
            ["base"],
            multiple_payment_schedules_message,
            payment_terms_update_unsuccessful_code,
          ),
        ],
        [],
      )
    False ->
      case id {
        Some(payment_terms_id) ->
          case
            get_effective_payment_terms_by_input_id(store, payment_terms_id)
          {
            Some(current) -> {
              let amount =
                payment_terms_owner_money(store, current.owner_id)
                |> option.or(payment_terms_record_money(current))
                |> option.unwrap(default_payment_terms_money())
              case
                build_payment_terms(
                  identity,
                  current.owner_id,
                  attrs,
                  Some(current.id),
                  amount,
                  ["input", "paymentTermsAttributes"],
                  payment_terms_update_unsuccessful_code,
                )
              {
                Ok(#(record, next_identity)) ->
                  payment_terms_result(
                    store.upsert_staged_payment_terms(store, record),
                    next_identity,
                    field,
                    fragments,
                    "paymentTermsUpdate",
                    Some(record),
                    [],
                    [record.id],
                  )
                Error(error) ->
                  payment_terms_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "paymentTermsUpdate",
                    None,
                    [error],
                    [],
                  )
              }
            }
            None ->
              payment_terms_result(
                store,
                identity,
                field,
                fragments,
                "paymentTermsUpdate",
                None,
                [
                  payment_terms_error(
                    ["input", "paymentTermsId"],
                    "Payment terms do not exist",
                    payment_terms_update_unsuccessful_code,
                  ),
                ],
                [],
              )
          }
        None ->
          payment_terms_result(
            store,
            identity,
            field,
            fragments,
            "paymentTermsUpdate",
            None,
            [
              payment_terms_error(
                ["input", "paymentTermsId"],
                "Payment terms do not exist",
                payment_terms_update_unsuccessful_code,
              ),
            ],
            [],
          )
      }
  }
}

@internal
pub fn delete_payment_terms(store, identity, field, fragments, variables) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let id = read_string_field(input, "paymentTermsId")
  case id {
    Some(payment_terms_id) ->
      case get_effective_payment_terms_by_input_id(store, payment_terms_id) {
        Some(record) -> {
          let deleted_id = payment_terms_gid_from_id(record.id)
          mutation_payload_result(
            store.delete_staged_payment_terms(store, record.id),
            identity,
            field,
            project_payload(field, fragments, [
              #("deletedId", SrcString(deleted_id)),
              #("userErrors", user_errors_source([])),
            ]),
            [record.id],
            "paymentTermsDelete",
            Some(
              "Staged payment terms deletion locally in the in-memory payment terms draft store.",
            ),
          )
        }
        None ->
          mutation_payload_result(
            store,
            identity,
            field,
            project_payload(field, fragments, [
              #("deletedId", SrcNull),
              #(
                "userErrors",
                user_errors_source([
                  payment_terms_error(
                    ["input", "paymentTermsId"],
                    "Payment terms do not exist",
                    payment_terms_delete_unsuccessful_code,
                  ),
                ]),
              ),
            ]),
            [],
            "paymentTermsDelete",
            Some(
              "Staged payment terms deletion locally in the in-memory payment terms draft store.",
            ),
          )
      }
    None ->
      mutation_payload_result(
        store,
        identity,
        field,
        project_payload(field, fragments, [
          #("deletedId", SrcNull),
          #(
            "userErrors",
            user_errors_source([
              payment_terms_error(
                ["input", "paymentTermsId"],
                "Payment terms do not exist",
                payment_terms_delete_unsuccessful_code,
              ),
            ]),
          ),
        ]),
        [],
        "paymentTermsDelete",
        Some(
          "Staged payment terms deletion locally in the in-memory payment terms draft store.",
        ),
      )
  }
}

fn build_payment_terms(
  identity: SyntheticIdentityRegistry,
  owner_id: String,
  attrs: Dict(String, root_field.ResolvedValue),
  existing_id: Option(String),
  amount: state_types.Money,
  field_prefix: List(String),
  unsuccessful_code: String,
) -> Result(
  #(state_types.PaymentTermsRecord, SyntheticIdentityRegistry),
  UserError,
) {
  use template <- result.try(resolve_payment_terms_template(
    attrs,
    field_prefix,
    unsuccessful_code,
  ))
  use schedule_plan <- result.try(resolve_payment_terms_schedule(
    attrs,
    template,
    unsuccessful_code,
  ))
  let #(id, identity_after_terms) = case existing_id {
    Some(value) -> #(value, identity)
    None -> synthetic_identity.make_synthetic_gid(identity, "PaymentTerms")
  }
  let #(payment_schedules, next_identity) = case
    schedule_plan.include_schedule
  {
    True -> {
      let #(schedule_id, next_identity) =
        synthetic_identity.make_synthetic_gid(
          identity_after_terms,
          "PaymentSchedule",
        )
      #(
        [
          state_types.PaymentScheduleRecord(
            id: schedule_id,
            due_at: schedule_plan.due_at,
            issued_at: schedule_plan.issued_at,
            completed_at: None,
            due: Some(False),
            amount: Some(amount),
            balance_due: Some(amount),
            total_balance: Some(amount),
          ),
        ],
        next_identity,
      )
    }
    False -> #([], identity_after_terms)
  }
  Ok(#(
    state_types.PaymentTermsRecord(
      id: id,
      owner_id: owner_id,
      due: False,
      overdue: False,
      due_in_days: template.due_in_days,
      payment_terms_name: template.name,
      payment_terms_type: template.payment_terms_type,
      translated_name: template.translated_name,
      payment_schedules: payment_schedules,
    ),
    next_identity,
  ))
}

fn resolve_payment_terms_template(
  attrs: Dict(String, root_field.ResolvedValue),
  field_prefix: List(String),
  unsuccessful_code: String,
) -> Result(state_types.PaymentTermsTemplateRecord, UserError) {
  case read_string_field(attrs, "paymentTermsTemplateId") {
    Some(template_id) ->
      case find_payment_terms_template(template_id) {
        Some(template) -> Ok(template)
        None ->
          Error(UserError(
            field: None,
            message: "Could not find payment terms template.",
            code: Some(unsuccessful_code),
          ))
      }
    None ->
      Error(UserError(
        field: Some(list.append(field_prefix, ["paymentTermsTemplateId"])),
        message: "Payment terms template is required.",
        code: Some("REQUIRED"),
      ))
  }
}

fn resolve_payment_terms_schedule(
  attrs: Dict(String, root_field.ResolvedValue),
  template: state_types.PaymentTermsTemplateRecord,
  unsuccessful_code: String,
) -> Result(PaymentTermsSchedulePlan, UserError) {
  let schedule_attrs = first_schedule_attrs(attrs)
  let issued_at = read_string_field(schedule_attrs, "issuedAt")
  let due_at = read_string_field(schedule_attrs, "dueAt")
  case template.payment_terms_type {
    "FIXED" -> fixed_payment_terms_schedule(due_at, unsuccessful_code)
    "NET" ->
      net_payment_terms_schedule(
        due_at,
        issued_at,
        template.due_in_days,
        unsuccessful_code,
      )
    "RECEIPT" -> event_payment_terms_schedule(due_at, unsuccessful_code)
    "FULFILLMENT" -> event_payment_terms_schedule(due_at, unsuccessful_code)
    _ ->
      Ok(PaymentTermsSchedulePlan(
        issued_at: issued_at,
        due_at: due_at,
        include_schedule: True,
      ))
  }
}

fn fixed_payment_terms_schedule(
  due_at: Option(String),
  unsuccessful_code: String,
) -> Result(PaymentTermsSchedulePlan, UserError) {
  case due_at {
    Some(due_at) ->
      Ok(PaymentTermsSchedulePlan(
        issued_at: None,
        due_at: Some(due_at),
        include_schedule: True,
      ))
    None -> Error(required_due_date_payment_terms_error(unsuccessful_code))
  }
}

fn net_payment_terms_schedule(
  due_at: Option(String),
  issued_at: Option(String),
  due_in_days: Option(Int),
  unsuccessful_code: String,
) -> Result(PaymentTermsSchedulePlan, UserError) {
  let resolved_due_at =
    due_at
    |> option.or(case due_in_days, issued_at {
      Some(days), Some(issued) -> add_days(issued, days)
      _, _ -> None
    })
  case resolved_due_at {
    Some(resolved_due_at) ->
      Ok(PaymentTermsSchedulePlan(
        issued_at: issued_at,
        due_at: Some(resolved_due_at),
        include_schedule: True,
      ))
    None -> Error(required_due_date_payment_terms_error(unsuccessful_code))
  }
}

fn event_payment_terms_schedule(
  due_at: Option(String),
  unsuccessful_code: String,
) -> Result(PaymentTermsSchedulePlan, UserError) {
  case due_at {
    Some(_) ->
      Error(UserError(
        field: None,
        message: "A due date cannot be set with event payment terms.",
        code: Some(unsuccessful_code),
      ))
    None ->
      Ok(PaymentTermsSchedulePlan(
        issued_at: None,
        due_at: None,
        include_schedule: False,
      ))
  }
}

fn required_due_date_payment_terms_error(
  unsuccessful_code: String,
) -> UserError {
  UserError(
    field: None,
    message: "A due date is required with fixed or net payment terms.",
    code: Some(unsuccessful_code),
  )
}

fn first_schedule_attrs(
  attrs: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(attrs, "paymentSchedules") {
    Ok(root_field.ListVal([root_field.ObjectVal(first), ..])) -> first
    _ -> dict.new()
  }
}

fn has_multiple_payment_schedules(
  attrs: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case dict.get(attrs, "paymentSchedules") {
    Ok(root_field.ListVal([_, _, ..])) -> True
    _ -> False
  }
}

fn default_payment_terms_money() -> state_types.Money {
  state_types.Money(amount: "0.0", currency_code: "CAD")
}

fn payment_terms_owner_money(
  store: Store,
  owner_id: String,
) -> Option(state_types.Money) {
  case payment_terms_owner_resource_type(owner_id) {
    Some("Order") ->
      case store.get_order_by_id(store, owner_id) {
        Some(record) ->
          captured_money_from_fields(record.data, [
            "totalOutstandingSet",
            "currentTotalPriceSet",
            "totalPriceSet",
            "subtotalPriceSet",
          ])
        None -> None
      }
    Some("DraftOrder") ->
      case store.get_draft_order_by_id(store, owner_id) {
        Some(record) ->
          captured_money_from_fields(record.data, [
            "totalOutstandingSet",
            "currentTotalPriceSet",
            "totalPriceSet",
            "subtotalPriceSet",
          ])
        None -> None
      }
    _ -> None
  }
}

fn payment_terms_record_money(
  record: state_types.PaymentTermsRecord,
) -> Option(state_types.Money) {
  case record.payment_schedules {
    [schedule, ..] ->
      schedule.amount
      |> option.or(schedule.balance_due)
      |> option.or(schedule.total_balance)
    [] -> None
  }
}

fn captured_money_from_fields(
  data: state_types.CapturedJsonValue,
  fields: List(String),
) -> Option(state_types.Money) {
  case fields {
    [] -> None
    [field, ..rest] ->
      case
        captured_object_field(data, field)
        |> option.then(captured_money_from_money_bag)
      {
        Some(money) -> Some(money)
        None -> captured_money_from_fields(data, rest)
      }
  }
}

fn captured_money_from_money_bag(
  value: state_types.CapturedJsonValue,
) -> Option(state_types.Money) {
  case
    captured_object_field(value, "presentmentMoney")
    |> option.then(captured_money_value)
  {
    Some(money) -> Some(money)
    None ->
      captured_object_field(value, "shopMoney")
      |> option.then(captured_money_value)
  }
}

fn captured_money_value(
  value: state_types.CapturedJsonValue,
) -> Option(state_types.Money) {
  case
    captured_string_field(value, "amount"),
    captured_string_field(value, "currencyCode")
  {
    Some(amount), Some(currency_code) ->
      Some(state_types.Money(amount: amount, currency_code: currency_code))
    _, _ -> None
  }
}

fn captured_object_field(
  value: state_types.CapturedJsonValue,
  key: String,
) -> Option(state_types.CapturedJsonValue) {
  case value {
    state_types.CapturedObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(name, child) if name == key -> Ok(child)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn captured_string_field(
  value: state_types.CapturedJsonValue,
  key: String,
) -> Option(String) {
  captured_object_field(value, key)
  |> option.then(fn(child) {
    case child {
      state_types.CapturedString(value) -> Some(value)
      _ -> None
    }
  })
}

fn captured_json_from_commit(
  value: commit.JsonValue,
) -> state_types.CapturedJsonValue {
  case value {
    commit.JsonNull -> state_types.CapturedNull
    commit.JsonBool(value) -> state_types.CapturedBool(value)
    commit.JsonInt(value) -> state_types.CapturedInt(value)
    commit.JsonFloat(value) -> state_types.CapturedFloat(value)
    commit.JsonString(value) -> state_types.CapturedString(value)
    commit.JsonArray(items) ->
      state_types.CapturedArray(list.map(items, captured_json_from_commit))
    commit.JsonObject(fields) ->
      state_types.CapturedObject(
        list.map(fields, fn(pair) {
          #(pair.0, captured_json_from_commit(pair.1))
        }),
      )
  }
}

fn get_effective_payment_terms_by_input_id(
  store: Store,
  id: String,
) -> Option(state_types.PaymentTermsRecord) {
  payment_terms_lookup_ids(id)
  |> list.find_map(fn(candidate) {
    case store.get_effective_payment_terms_by_id(store, candidate) {
      Some(record) -> Ok(record)
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

fn payment_terms_lookup_ids(id: String) -> List(String) {
  let gid = payment_terms_gid_from_id(id)
  case gid == id {
    True -> [id]
    False -> [id, gid]
  }
}

fn payment_terms_gid_from_id(id: String) -> String {
  "gid://shopify/PaymentTerms/" <> gid_tail(id)
}

fn find_payment_terms_template(
  id: String,
) -> Option(state_types.PaymentTermsTemplateRecord) {
  case
    payment_terms_templates() |> list.find(fn(template) { template.id == id })
  {
    Ok(template) -> Some(template)
    Error(_) -> None
  }
}

fn add_days(iso: String, days: Int) -> Option(String) {
  case iso_timestamp.parse_iso(iso) {
    Ok(ms) ->
      Some(
        iso_timestamp.format_iso(ms + days * 86_400_000)
        |> string.replace(".000Z", "Z"),
      )
    Error(_) -> None
  }
}

fn payment_terms_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  terms: Option(state_types.PaymentTermsRecord),
  errors: List(UserError),
  staged_ids: List(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  mutation_payload_result(
    store,
    identity,
    field,
    project_payload(field, fragments, [
      #("paymentTerms", case terms {
        Some(record) -> payment_terms_source(record)
        None -> SrcNull
      }),
      #("userErrors", user_errors_source(errors)),
    ]),
    staged_ids,
    root_name,
    Some(
      "Staged payment terms locally in the in-memory payment terms draft store.",
    ),
  )
}

@internal
pub fn send_payment_reminder(store, identity, field, fragments, variables) {
  let payment_schedule_id =
    graphql_helpers.read_arg_string_nonempty(
      graphql_helpers.field_args(field, variables),
      "paymentScheduleId",
    )
  case
    is_shopify_gid(payment_schedule_id, "PaymentSchedule"),
    payment_schedule_id
  {
    True, Some(schedule_id) ->
      case store.get_effective_payment_schedule_by_id(store, schedule_id) {
        Some(#(terms, schedule)) ->
          case payment_schedule_reminder_error(store, terms, schedule) {
            None -> {
              let #(id, identity_after_id) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "PaymentReminderSend",
                )
              let #(sent_at, next_identity) =
                synthetic_identity.make_synthetic_timestamp(identity_after_id)
              let record =
                state_types.PaymentReminderSendRecord(
                  id: id,
                  payment_schedule_id: schedule_id,
                  sent_at: sent_at,
                )
              payment_reminder_result(
                store.stage_payment_reminder_send(store, record),
                next_identity,
                field,
                fragments,
                [],
                SrcBool(True),
              )
            }
            Some(error) ->
              payment_reminder_error_result(
                store,
                identity,
                field,
                fragments,
                [
                  error,
                ],
                SrcNull,
              )
          }
        _ ->
          payment_reminder_error_result(
            store,
            identity,
            field,
            fragments,
            [
              payment_reminder_not_found_error(),
            ],
            SrcNull,
          )
      }
    _, _ ->
      payment_reminder_error_result(
        store,
        identity,
        field,
        fragments,
        [
          UserError(
            field: Some(["paymentScheduleId"]),
            message: "Payment schedule ID is invalid",
            code: Some("INVALID_PAYMENT_SCHEDULE_ID"),
          ),
        ],
        SrcBool(False),
      )
  }
}

fn payment_schedule_reminder_error(
  store: Store,
  terms: state_types.PaymentTermsRecord,
  schedule: state_types.PaymentScheduleRecord,
) -> Option(UserError) {
  case schedule.completed_at {
    Some(_) -> Some(payment_reminder_already_completed_error())
    None ->
      case terms.overdue {
        False -> Some(payment_reminder_unsuccessful_error())
        True -> payment_terms_owner_error(store, terms.owner_id)
      }
  }
}

fn payment_terms_owner_error(
  store: Store,
  owner_id: String,
) -> Option(UserError) {
  case is_shopify_gid(Some(owner_id), "Order") {
    True ->
      case store.get_order_by_id(store, owner_id) {
        Some(order) -> order_terminal_error(order)
        None -> Some(payment_reminder_unsuccessful_error())
      }
    False ->
      case is_shopify_gid(Some(owner_id), "DraftOrder") {
        True -> Some(payment_reminder_not_for_order_error())
        False -> Some(payment_reminder_unsuccessful_error())
      }
  }
}

fn order_terminal_error(order: state_types.OrderRecord) -> Option(UserError) {
  case captured_string_field(order.data, "displayFinancialStatus") {
    Some("PAID") -> Some(payment_reminder_already_completed_error())
    _ ->
      case
        captured_present(order.data, "cancelledAt")
        || captured_present(order.data, "closedAt")
        || case captured_bool_field(order.data, "closed") {
          Some(True) -> True
          _ -> False
        }
      {
        True -> Some(payment_reminder_unsuccessful_error())
        False -> None
      }
  }
}

fn captured_present(
  value: state_types.CapturedJsonValue,
  name: String,
) -> Bool {
  case captured_object_field(value, name) {
    Some(state_types.CapturedNull) | None -> False
    Some(_) -> True
  }
}

fn captured_bool_field(
  value: state_types.CapturedJsonValue,
  name: String,
) -> Option(Bool) {
  case captured_object_field(value, name) {
    Some(state_types.CapturedBool(value)) -> Some(value)
    _ -> None
  }
}

fn payment_reminder_unsuccessful_error() -> UserError {
  UserError(
    field: None,
    message: "Payment reminder could not be sent",
    code: Some("PAYMENT_REMINDER_SEND_UNSUCCESSFUL"),
  )
}

fn payment_reminder_not_found_error() -> UserError {
  UserError(
    field: None,
    message: "Payment schedule does not exist",
    code: Some("PAYMENT_REMINDER_SEND_UNSUCCESSFUL"),
  )
}

fn payment_reminder_already_completed_error() -> UserError {
  UserError(
    field: None,
    message: "Payment schedule is already completed",
    code: Some("PAYMENT_REMINDER_SEND_UNSUCCESSFUL"),
  )
}

fn payment_reminder_not_for_order_error() -> UserError {
  UserError(
    field: None,
    message: "Payment schedule is not for an Order",
    code: Some("PAYMENT_REMINDER_SEND_UNSUCCESSFUL"),
  )
}

fn payment_reminder_error_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  errors: List(UserError),
  success: SourceValue,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  payment_reminder_result(store, identity, field, fragments, errors, success)
}

fn payment_reminder_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  errors: List(UserError),
  success: SourceValue,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  mutation_payload_result(
    store,
    identity,
    field,
    project_payload(field, fragments, [
      #("success", success),
      #("userErrors", user_errors_source(errors)),
    ]),
    [],
    "paymentReminderSend",
    Some(
      "Staged a local payment reminder intent only; no customer email is sent at runtime.",
    ),
  )
}
