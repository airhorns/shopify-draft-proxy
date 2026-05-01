//// Incremental Orders-domain port.
////
//// This pass intentionally claims only the abandoned-checkout/abandonment
//// roots backed by checked-in executable parity fixtures. The broader order,
//// draft-order, fulfillment, refund, and return roots remain unported.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, RequiredArgument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  AbandonmentDeliveryActivityRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
}

pub type OrdersError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

pub fn is_orders_query_root(name: String) -> Bool {
  list.contains(
    [
      "abandonedCheckouts",
      "abandonedCheckoutsCount",
      "abandonment",
      "abandonmentByAbandonedCheckoutId",
    ],
    name,
  )
}

pub fn is_orders_mutation_root(name: String) -> Bool {
  list.contains(
    [
      "abandonmentUpdateActivitiesDeliveryStatuses",
      "orderCreateManualPayment",
      "taxSummaryCreate",
    ],
    name,
  )
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, OrdersError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let entries =
    list.filter_map(fields, fn(field) {
      case field {
        Field(name: name, ..) ->
          Ok(#(
            get_field_response_key(field),
            serialize_query_field(
              store,
              field,
              name.value,
              fragments,
              variables,
            ),
          ))
        _ -> Error(Nil)
      }
    })
  Ok(json.object([#("data", json.object(entries))]))
}

fn serialize_query_field(
  store: Store,
  field: Selection,
  name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case name {
    "abandonedCheckouts" ->
      serialize_abandoned_checkouts(store, field, fragments, variables)
    "abandonedCheckoutsCount" ->
      serialize_abandoned_checkouts_count(store, field, variables)
    "abandonment" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case store.get_abandonment_by_id(store, id) {
            Some(abandonment) ->
              serialize_abandonment_node(store, field, abandonment, fragments)
            None -> json.null()
          }
        None -> json.null()
      }
    }
    "abandonmentByAbandonedCheckoutId" -> {
      let id = read_string_argument(field, "abandonedCheckoutId", variables)
      case id {
        Some(id) ->
          case store.get_abandonment_by_abandoned_checkout_id(store, id) {
            Some(abandonment) ->
              serialize_abandonment_node(store, field, abandonment, fragments)
            None -> json.null()
          }
        None -> json.null()
      }
    }
    _ -> json.null()
  }
}

fn serialize_abandoned_checkouts(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let checkouts = store.list_effective_abandoned_checkouts(store)
  let args = field_arguments(field, variables)
  let ordered = case dict.get(args, "reverse") {
    Ok(root_field.BoolVal(False)) -> list.reverse(checkouts)
    _ -> checkouts
  }
  let window =
    paginate_connection_items(
      ordered,
      field,
      variables,
      abandoned_checkout_cursor,
      default_connection_window_options(),
    )
  let page_info_options =
    ConnectionPageInfoOptions(
      include_inline_fragments: True,
      prefix_cursors: False,
      include_cursors: True,
      fallback_start_cursor: None,
      fallback_end_cursor: None,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: abandoned_checkout_cursor,
      serialize_node: fn(checkout, selection, _index) {
        serialize_abandoned_checkout_node(selection, checkout, fragments)
      },
      selected_field_options: SelectedFieldOptions(True),
      page_info_options: page_info_options,
    ),
  )
}

fn abandoned_checkout_cursor(
  checkout: AbandonedCheckoutRecord,
  _index: Int,
) -> String {
  checkout.cursor |> option.unwrap(checkout.id)
}

fn serialize_abandoned_checkouts_count(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let checkouts = store.list_effective_abandoned_checkouts(store)
  let raw_count = list.length(checkouts)
  let limit = read_int_argument(field, "limit", variables)
  let count = case limit {
    Some(limit) if limit >= 0 -> min_int(raw_count, limit)
    _ -> raw_count
  }
  let precision = case limit {
    Some(limit) if limit >= 0 && raw_count > limit -> "AT_LEAST"
    _ -> "EXACT"
  }
  serialize_count_payload(field, count, precision)
}

fn serialize_count_payload(
  field: Selection,
  count: Int,
  precision: String,
) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "count" -> #(key, json.int(count))
              "precision" -> #(key, json.string(precision))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn serialize_abandoned_checkout_node(
  field: Selection,
  checkout: AbandonedCheckoutRecord,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    captured_json_source(checkout.data),
    selection_children(field),
    fragments,
  )
}

fn serialize_abandonment_node(
  store: Store,
  field: Selection,
  abandonment: AbandonmentRecord,
  fragments: FragmentMap,
) -> Json {
  let source = captured_json_source(abandonment.data)
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "abandonedCheckoutPayload" -> #(
              key,
              serialize_abandoned_checkout_payload(
                store,
                child,
                abandonment,
                fragments,
              ),
            )
            _ -> #(key, graphql_helpers_project_field(source, child, fragments))
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_abandoned_checkout_payload(
  store: Store,
  field: Selection,
  abandonment: AbandonmentRecord,
  fragments: FragmentMap,
) -> Json {
  case abandonment.abandoned_checkout_id {
    Some(checkout_id) ->
      case store.get_abandoned_checkout_by_id(store, checkout_id) {
        Some(checkout) ->
          serialize_abandoned_checkout_node(field, checkout, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn graphql_helpers_project_field(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_field_value(source, field, fragments)
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, OrdersError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let operation_path = get_operation_path_label(document)
  let initial = #([], [], store, identity, [], [])
  let #(
    data_entries,
    all_errors,
    final_store,
    final_identity,
    staged_ids,
    log_drafts,
  ) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, errors, current_store, current_identity, ids, drafts) = acc
      case field {
        Field(name: name, ..)
          if name.value == "abandonmentUpdateActivitiesDeliveryStatuses"
        -> {
          let result =
            handle_abandonment_delivery_status(
              current_store,
              current_identity,
              document,
              operation_path,
              field,
              fragments,
              variables,
            )
          let #(
            key,
            payload,
            next_store,
            next_identity,
            next_ids,
            next_errors,
            next_drafts,
          ) = result
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              next_store,
              next_identity,
              list.append(ids, next_ids),
              list.append(drafts, next_drafts),
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              next_store,
              next_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..)
          if name.value == "orderCreateManualPayment"
          || name.value == "taxSummaryCreate"
        -> {
          let #(key, payload, next_errors, next_drafts) =
            handle_access_denied_guardrail(name.value, field)
          #(
            list.append(entries, [#(key, payload)]),
            list.append(errors, next_errors),
            current_store,
            current_identity,
            ids,
            list.append(drafts, next_drafts),
          )
        }
        _ -> acc
      }
    })
  let envelope = case all_errors {
    [] -> json.object([#("data", json.object(data_entries))])
    _ ->
      case data_entries {
        [] -> json.object([#("errors", json.preprocessed_array(all_errors))])
        _ ->
          json.object([
            #("errors", json.preprocessed_array(all_errors)),
            #("data", json.object(data_entries)),
          ])
      }
  }
  Ok(MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: log_drafts,
  ))
}

fn handle_access_denied_guardrail(
  root_name: String,
  field: Selection,
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let required_access = access_denied_required_access(root_name)
  let error = access_denied_error(root_name, required_access)
  let draft =
    single_root_log_draft(
      root_name,
      [],
      store.Failed,
      "orders",
      "stage-locally",
      Some(root_name <> " failed local access-denied guardrail."),
    )
  #(key, json.null(), [error], [draft])
}

fn access_denied_required_access(root_name: String) -> String {
  case root_name {
    "orderCreateManualPayment" ->
      "`write_orders` access scope. Also: The user must have mark_orders_as_paid permission. The API client must be installed on a Shopify Plus store to use the amount field."
    "taxSummaryCreate" ->
      "`write_taxes` access scope. Also: The caller must be a tax calculations app and the relevant feature must be on."
    _ -> "`write_orders` access scope."
  }
}

fn access_denied_error(root_name: String, required_access: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Access denied for "
        <> root_name
        <> " field. Required access: "
        <> required_access,
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("ACCESS_DENIED")),
        #(
          "documentation",
          json.string("https://shopify.dev/api/usage/access-scopes"),
        ),
        #("requiredAccess", json.string(required_access)),
      ]),
    ),
    #("path", json.array([root_name], json.string)),
  ])
}

fn handle_abandonment_delivery_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "abandonmentUpdateActivitiesDeliveryStatuses",
      [
        RequiredArgument(name: "abandonmentId", expected_type: "ID!"),
        RequiredArgument(name: "marketingActivityId", expected_type: "ID!"),
        RequiredArgument(
          name: "deliveryStatus",
          expected_type: "AbandonmentDeliveryState!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let abandonment_id = read_string_arg(args, "abandonmentId")
      let marketing_activity_id = read_string_arg(args, "marketingActivityId")
      let delivery_status = read_string_arg(args, "deliveryStatus")
      case abandonment_id, marketing_activity_id, delivery_status {
        Some(abandonment_id), Some(marketing_activity_id), Some(delivery_status)
        -> {
          let activity =
            AbandonmentDeliveryActivityRecord(
              marketing_activity_id: marketing_activity_id,
              delivery_status: delivery_status,
              delivered_at: read_string_arg(args, "deliveredAt"),
              delivery_status_change_reason: read_string_arg(
                args,
                "deliveryStatusChangeReason",
              ),
            )
          let #(next_store, updated) =
            store.stage_abandonment_delivery_activity(
              store,
              abandonment_id,
              activity,
            )
          case updated {
            Some(abandonment) -> {
              let payload =
                serialize_abandonment_mutation_payload(
                  next_store,
                  field,
                  Some(abandonment),
                  [],
                  fragments,
                )
              let draft =
                abandonment_log_draft(
                  [abandonment.id],
                  store.Staged,
                  Some(
                    "Locally staged abandonmentUpdateActivitiesDeliveryStatuses in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, identity, [abandonment.id], [], [
                draft,
              ])
            }
            None ->
              unknown_abandonment_result(key, store, identity, field, fragments)
          }
        }
        _, _, _ ->
          unknown_abandonment_result(key, store, identity, field, fragments)
      }
    }
  }
}

fn unknown_abandonment_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let user_errors = [
    #(["abandonmentId"], "abandonment_not_found"),
  ]
  let payload =
    serialize_abandonment_mutation_payload(
      store,
      field,
      None,
      user_errors,
      fragments,
    )
  let draft =
    abandonment_log_draft(
      [],
      store.Failed,
      Some(
        "abandonmentUpdateActivitiesDeliveryStatuses failed local validation.",
      ),
    )
  #(key, payload, store, identity, [], [], [draft])
}

fn abandonment_log_draft(
  staged_resource_ids: List(String),
  status: store.EntryStatus,
  notes: Option(String),
) -> LogDraft {
  single_root_log_draft(
    "abandonmentUpdateActivitiesDeliveryStatuses",
    staged_resource_ids,
    status,
    "orders",
    "stage-locally",
    notes,
  )
}

fn serialize_abandonment_mutation_payload(
  store: Store,
  field: Selection,
  abandonment: Option(AbandonmentRecord),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "abandonment" -> #(key, case abandonment {
              Some(record) ->
                serialize_abandonment_node(store, child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_user_error(
  field: Selection,
  error: #(List(String), String),
) -> Json {
  let #(field_path, message) = error
  let source =
    src_object([
      #("field", SrcList(list.map(field_path, SrcString))),
      #("message", SrcString(message)),
    ])
  project_graphql_value(source, selection_children(field), dict.new())
}

fn captured_json_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_json_source))
    CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.fold(dict.new(), fn(acc, pair) {
          let #(key, item) = pair
          dict.insert(acc, key, captured_json_source(item))
        }),
      )
  }
}

fn selection_children(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

fn field_arguments(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  root_field.get_field_arguments(field, variables)
  |> result.unwrap(dict.new())
}

fn read_string_argument(
  field: Selection,
  name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  field_arguments(field, variables) |> read_string_arg(name)
}

fn read_string_arg(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_int_argument(
  field: Selection,
  name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Int) {
  case dict.get(field_arguments(field, variables), name) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
  }
}

fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

fn min_int(left: Int, right: Int) -> Int {
  case left < right {
    True -> left
    False -> right
  }
}
