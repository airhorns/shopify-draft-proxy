//// Incremental Orders-domain port.
////
//// This pass intentionally claims only the abandoned-checkout/abandonment
//// roots backed by checked-in executable parity fixtures. The broader order,
//// draft-order, fulfillment, refund, and return roots remain unported.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type ObjectField, type Selection, Field, NullValue, ObjectField, ObjectValue,
  SelectionSet, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, resolved_value_to_source,
  serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, RequiredArgument, find_argument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  type DraftOrderRecord, type DraftOrderVariantCatalogRecord,
  AbandonmentDeliveryActivityRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, DraftOrderRecord,
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
      "draftOrder",
    ],
    name,
  )
}

pub fn is_orders_mutation_root(name: String) -> Bool {
  list.contains(
    [
      "abandonmentUpdateActivitiesDeliveryStatuses",
      "draftOrderComplete",
      "draftOrderCreate",
      "fulfillmentCancel",
      "fulfillmentCreate",
      "fulfillmentTrackingInfoUpdate",
      "orderCreate",
      "orderCreateManualPayment",
      "orderEditAddVariant",
      "orderEditBegin",
      "orderEditCommit",
      "orderEditSetQuantity",
      "orderUpdate",
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
    "draftOrder" -> {
      let id = read_string_argument(field, "id", variables)
      case id {
        Some(id) ->
          case store.get_draft_order_by_id(store, id) {
            Some(draft_order) ->
              serialize_draft_order_node(field, draft_order, fragments)
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

fn serialize_draft_order_node(
  field: Selection,
  draft_order: DraftOrderRecord,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    captured_json_source(draft_order.data),
    selection_children(field),
    fragments,
  )
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
        Field(name: name, ..) if name.value == "draftOrderCreate" -> {
          let result =
            handle_draft_order_create(
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
        Field(name: name, ..) if name.value == "draftOrderComplete" -> {
          let #(key, payload, next_errors) =
            handle_draft_order_complete_guardrail(
              document,
              operation_path,
              field,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              drafts,
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..)
          if name.value == "fulfillmentCancel"
          || name.value == "fulfillmentTrackingInfoUpdate"
        -> {
          let #(key, payload, next_errors) =
            handle_fulfillment_validation_guardrail(
              name.value,
              document,
              operation_path,
              field,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              drafts,
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "fulfillmentCreate" -> {
          let #(key, payload, next_errors) =
            handle_fulfillment_create_invalid_id_guardrail(name.value)
          #(
            list.append(entries, [#(key, payload)]),
            list.append(errors, next_errors),
            current_store,
            current_identity,
            ids,
            drafts,
          )
        }
        Field(name: name, ..) if name.value == "orderCreate" -> {
          let #(key, payload, next_errors) =
            handle_order_create_validation_guardrail(
              document,
              operation_path,
              field,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              drafts,
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..) if name.value == "orderUpdate" -> {
          let #(key, payload, next_errors) =
            handle_order_update_validation_guardrail(
              operation_path,
              field,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              drafts,
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
              ids,
              drafts,
            )
          }
        }
        Field(name: name, ..)
          if name.value == "orderEditAddVariant"
          || name.value == "orderEditBegin"
          || name.value == "orderEditCommit"
          || name.value == "orderEditSetQuantity"
        -> {
          let #(key, payload, next_errors) =
            handle_order_edit_validation_guardrail(
              name.value,
              document,
              operation_path,
              field,
              variables,
            )
          case next_errors {
            [] -> #(
              list.append(entries, [#(key, payload)]),
              errors,
              current_store,
              current_identity,
              ids,
              drafts,
            )
            _ -> #(
              entries,
              list.append(errors, next_errors),
              current_store,
              current_identity,
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

fn handle_draft_order_complete_guardrail(
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderComplete",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> #(key, json.null(), [])
  }
}

fn handle_order_edit_validation_guardrail(
  root_name: String,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      root_name,
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> #(key, json.null(), [])
  }
}

fn handle_fulfillment_create_invalid_id_guardrail(
  root_name: String,
) -> #(String, Json, List(Json)) {
  #(root_name, json.null(), [
    json.object([
      #("message", json.string("invalid id")),
      #(
        "extensions",
        json.object([#("code", json.string("RESOURCE_NOT_FOUND"))]),
      ),
      #("path", json.array([root_name], json.string)),
    ]),
  ])
}

fn handle_order_update_validation_guardrail(
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let errors = case field {
    Field(arguments: arguments, ..) ->
      case find_argument(arguments, "input") {
        Some(input_argument) ->
          case input_argument.value {
            ObjectValue(fields: fields, ..) ->
              validate_order_update_inline_input(operation_path, fields)
            VariableValue(variable: variable) ->
              validate_order_update_variable_input(
                variable.name.value,
                variables,
              )
            _ -> []
          }
        None -> []
      }
    _ -> []
  }
  case errors {
    [_, ..] -> #(key, json.null(), errors)
    [] -> #(key, json.null(), [])
  }
}

fn validate_order_update_inline_input(
  operation_path: String,
  fields: List(ObjectField),
) -> List(Json) {
  case find_object_field(fields, "id") {
    None -> [build_order_update_missing_inline_id_error(operation_path)]
    Some(ObjectField(value: NullValue(..), ..)) -> [
      build_order_update_null_inline_id_error(operation_path),
    ]
    _ -> []
  }
}

fn validate_order_update_variable_input(
  variable_name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(Json) {
  case dict.get(variables, variable_name) {
    Ok(root_field.ObjectVal(input)) ->
      case dict.get(input, "id") {
        Ok(root_field.NullVal) | Error(_) -> [
          build_order_update_missing_variable_id_error(
            variable_name,
            root_field.ObjectVal(input),
          ),
        ]
        _ -> []
      }
    _ -> []
  }
}

fn find_object_field(
  fields: List(ObjectField),
  name: String,
) -> Option(ObjectField) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let ObjectField(name: field_name, ..) = first
      case field_name.value == name {
        True -> Some(first)
        False -> find_object_field(rest, name)
      }
    }
  }
}

fn build_order_update_missing_inline_id_error(operation_path: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Argument 'id' on InputObject 'OrderInput' is required. Expected type ID!",
      ),
    ),
    #(
      "path",
      json.array([operation_path, "orderUpdate", "input", "id"], json.string),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("missingRequiredInputObjectAttribute")),
        #("argumentName", json.string("id")),
        #("argumentType", json.string("ID!")),
        #("inputObjectType", json.string("OrderInput")),
      ]),
    ),
  ])
}

fn build_order_update_null_inline_id_error(operation_path: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Argument 'id' on InputObject 'OrderInput' has an invalid value (null). Expected type 'ID!'.",
      ),
    ),
    #(
      "path",
      json.array([operation_path, "orderUpdate", "input", "id"], json.string),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("argumentLiteralsIncompatible")),
        #("typeName", json.string("InputObject")),
        #("argumentName", json.string("id")),
      ]),
    ),
  ])
}

fn build_order_update_missing_variable_id_error(
  variable_name: String,
  value: root_field.ResolvedValue,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type OrderInput! was provided invalid value for id (Expected value to not be null)",
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", source_to_json(resolved_value_to_source(value))),
        #(
          "problems",
          json.array(
            [
              json.object([
                #("path", json.array(["id"], json.string)),
                #("explanation", json.string("Expected value to not be null")),
              ]),
            ],
            fn(problem) { problem },
          ),
        ),
      ]),
    ),
  ])
}

fn handle_order_create_validation_guardrail(
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderCreate",
      [
        RequiredArgument(name: "order", expected_type: "OrderCreateOrderInput!"),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> #(key, json.null(), [])
  }
}

fn handle_fulfillment_validation_guardrail(
  root_name: String,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let required = case root_name {
    "fulfillmentTrackingInfoUpdate" -> [
      RequiredArgument(name: "fulfillmentId", expected_type: "ID!"),
    ]
    _ -> [RequiredArgument(name: "id", expected_type: "ID!")]
  }
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      root_name,
      required,
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> #(key, json.null(), [])
  }
}

fn handle_draft_order_create(
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
      "draftOrderCreate",
      [RequiredArgument(name: "input", expected_type: "DraftOrderInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) -> {
          let #(draft_order, next_identity) =
            build_draft_order_from_input(store, identity, input)
          let next_store = store.stage_draft_order(store, draft_order)
          let payload =
            serialize_draft_order_create_payload(
              field,
              Some(draft_order),
              [],
              fragments,
            )
          let draft =
            single_root_log_draft(
              "draftOrderCreate",
              [draft_order.id],
              store.Staged,
              "orders",
              "stage-locally",
              Some("Locally staged draftOrderCreate in shopify-draft-proxy."),
            )
          #(key, payload, next_store, next_identity, [draft_order.id], [], [
            draft,
          ])
        }
        _ -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

fn serialize_draft_order_create_payload(
  field: Selection,
  draft_order: Option(DraftOrderRecord),
  user_errors: List(#(List(String), String)),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "draftOrder" -> #(key, case draft_order {
              Some(record) ->
                serialize_draft_order_node(child, record, fragments)
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

fn build_draft_order_from_input(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(draft_order_id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "DraftOrder")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let #(line_items, identity_after_lines) =
    build_draft_order_line_items(
      store,
      identity_after_time,
      read_object_list(input, "lineItems"),
    )
  let currency_code = draft_order_currency(input, line_items)
  let applied_discount =
    build_draft_order_applied_discount(
      read_object(input, "appliedDiscount"),
      currency_code,
    )
  let shipping_line =
    build_draft_order_shipping_line(read_object(input, "shippingLine"))
  let line_discount_total =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum +. captured_money_amount(item, "totalDiscountSet")
    })
  let discounted_line_subtotal =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum +. captured_money_amount(item, "discountedTotalSet")
    })
  let order_discount_total =
    discount_amount(applied_discount, discounted_line_subtotal)
  let subtotal =
    max_float(0.0, discounted_line_subtotal -. order_discount_total)
  let shipping_total = captured_money_amount(shipping_line, "originalPriceSet")
  let total_discount = line_discount_total +. order_discount_total
  let total = subtotal +. shipping_total
  let data =
    CapturedObject([
      #("id", CapturedString(draft_order_id)),
      #(
        "name",
        CapturedString(
          "#D"
          <> int.to_string(
            list.length(store.list_effective_draft_orders(store)) + 1,
          ),
        ),
      ),
      #("status", CapturedString("OPEN")),
      #("ready", CapturedBool(True)),
      #("email", optional_captured_string(read_string(input, "email"))),
      #("note", optional_captured_string(read_string(input, "note"))),
      #("customer", build_draft_order_customer(store, input)),
      #("taxExempt", CapturedBool(read_bool(input, "taxExempt", False))),
      #("taxesIncluded", CapturedBool(read_bool(input, "taxesIncluded", False))),
      #(
        "reserveInventoryUntil",
        optional_captured_string(read_string(input, "reserveInventoryUntil")),
      ),
      #("paymentTerms", CapturedNull),
      #(
        "tags",
        CapturedArray(
          read_string_list(input, "tags")
          |> list.sort(by: string.compare)
          |> list.map(CapturedString),
        ),
      ),
      #(
        "invoiceUrl",
        CapturedString(
          "https://shopify-draft-proxy.local/draft_orders/"
          <> draft_order_id
          <> "/invoice",
        ),
      ),
      #(
        "customAttributes",
        captured_attributes(read_object_list(input, "customAttributes")),
      ),
      #("appliedDiscount", applied_discount),
      #(
        "billingAddress",
        build_draft_order_address(read_object(input, "billingAddress")),
      ),
      #(
        "shippingAddress",
        build_draft_order_address(read_object(input, "shippingAddress")),
      ),
      #("shippingLine", shipping_line),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("subtotalPriceSet", money_set(subtotal, currency_code)),
      #("totalDiscountsSet", money_set(total_discount, currency_code)),
      #("totalShippingPriceSet", money_set(shipping_total, currency_code)),
      #("totalPriceSet", money_set(total, currency_code)),
      #("totalQuantityOfLineItems", CapturedInt(total_quantity(line_items))),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
  #(
    DraftOrderRecord(id: draft_order_id, cursor: None, data: data),
    identity_after_lines,
  )
}

fn build_draft_order_line_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  inputs
  |> list.fold(initial, fn(acc, input) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "DraftOrderLineItem",
      )
    let item = build_draft_order_line_item(store, id, input)
    #(list.append(items, [item]), next_identity)
  })
}

fn build_draft_order_line_item(
  store: Store,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let quantity = read_int(input, "quantity", 1)
  case read_string(input, "variantId") {
    Some(variant_id) -> {
      let catalog =
        store.get_draft_order_variant_catalog_by_id(store, variant_id)
      build_variant_draft_order_line_item(id, variant_id, quantity, catalog)
    }
    None -> build_custom_draft_order_line_item(id, quantity, input)
  }
}

fn build_variant_draft_order_line_item(
  id: String,
  variant_id: String,
  quantity: Int,
  catalog: Option(DraftOrderVariantCatalogRecord),
) -> CapturedJsonValue {
  let title = case catalog {
    Some(record) -> record.title
    None -> "Variant"
  }
  let name = case catalog {
    Some(record) -> record.name
    None -> title
  }
  let variant_title = case catalog {
    Some(record) -> record.variant_title
    None -> None
  }
  let sku = case catalog {
    Some(record) -> record.sku
    None -> None
  }
  let line_variant_title = case variant_title {
    Some("Default Title") -> None
    other -> other
  }
  let nested_variant_sku = case sku {
    Some("") -> None
    other -> other
  }
  let unit_price = case catalog {
    Some(record) -> parse_amount(record.unit_price)
    None -> 0.0
  }
  let currency_code = case catalog {
    Some(record) -> record.currency_code
    None -> "CAD"
  }
  let original_total = unit_price *. int.to_float(quantity)
  CapturedObject([
    #("id", CapturedString(id)),
    #("title", CapturedString(title)),
    #("name", CapturedString(name)),
    #("quantity", CapturedInt(quantity)),
    #("sku", optional_captured_string(sku)),
    #("variantTitle", optional_captured_string(line_variant_title)),
    #("custom", CapturedBool(False)),
    #("requiresShipping", CapturedBool(catalog_requires_shipping(catalog))),
    #("taxable", CapturedBool(catalog_taxable(catalog))),
    #("customAttributes", CapturedArray([])),
    #("appliedDiscount", CapturedNull),
    #("originalUnitPriceSet", money_set(unit_price, currency_code)),
    #("originalTotalSet", money_set(original_total, currency_code)),
    #("discountedTotalSet", money_set(original_total, currency_code)),
    #("totalDiscountSet", money_set(0.0, currency_code)),
    #(
      "variant",
      CapturedObject([
        #("id", CapturedString(variant_id)),
        #("title", optional_captured_string(variant_title)),
        #("sku", optional_captured_string(nested_variant_sku)),
      ]),
    ),
  ])
}

fn build_custom_draft_order_line_item(
  id: String,
  quantity: Int,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let currency_code = "CAD"
  let title = read_string(input, "title") |> option.unwrap("Custom item")
  let unit_price = read_string(input, "originalUnitPrice") |> option.unwrap("0")
  let unit_price = parse_amount(unit_price)
  let original_total = unit_price *. int.to_float(quantity)
  let applied_discount =
    build_draft_order_applied_discount(
      read_object(input, "appliedDiscount"),
      currency_code,
    )
  let discount_total = discount_amount(applied_discount, original_total)
  let discounted_total = max_float(0.0, original_total -. discount_total)
  CapturedObject([
    #("id", CapturedString(id)),
    #("title", CapturedString(title)),
    #("name", CapturedString(title)),
    #("quantity", CapturedInt(quantity)),
    #("sku", optional_captured_string(read_string(input, "sku"))),
    #("variantTitle", CapturedNull),
    #("custom", CapturedBool(True)),
    #(
      "requiresShipping",
      CapturedBool(read_bool(input, "requiresShipping", True)),
    ),
    #("taxable", CapturedBool(read_bool(input, "taxable", True))),
    #(
      "customAttributes",
      captured_attributes(read_object_list(input, "customAttributes")),
    ),
    #("appliedDiscount", applied_discount),
    #("originalUnitPriceSet", money_set(unit_price, currency_code)),
    #("originalTotalSet", money_set(original_total, currency_code)),
    #("discountedTotalSet", money_set(discounted_total, currency_code)),
    #("totalDiscountSet", money_set(discount_total, currency_code)),
    #("variant", CapturedNull),
  ])
}

fn catalog_requires_shipping(
  catalog: Option(DraftOrderVariantCatalogRecord),
) -> Bool {
  case catalog {
    Some(record) -> record.requires_shipping
    None -> True
  }
}

fn catalog_taxable(catalog: Option(DraftOrderVariantCatalogRecord)) -> Bool {
  case catalog {
    Some(record) -> record.taxable
    None -> True
  }
}

fn build_draft_order_customer(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let customer_id = case read_object(input, "purchasingEntity") {
    Some(entity) -> read_string(entity, "customerId")
    None -> None
  }
  case customer_id {
    None -> CapturedNull
    Some(id) -> {
      let customer = store.get_effective_customer_by_id(store, id)
      CapturedObject([
        #("id", CapturedString(id)),
        #(
          "email",
          optional_captured_string(case customer {
            Some(record) -> record.email
            None -> None
          }),
        ),
        #(
          "displayName",
          optional_captured_string(case customer {
            Some(record) -> record.display_name
            None -> None
          }),
        ),
      ])
    }
  }
}

fn build_draft_order_address(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) ->
      CapturedObject([
        #(
          "firstName",
          optional_captured_string(read_string(input, "firstName")),
        ),
        #("lastName", optional_captured_string(read_string(input, "lastName"))),
        #("address1", optional_captured_string(read_string(input, "address1"))),
        #("city", optional_captured_string(read_string(input, "city"))),
        #(
          "provinceCode",
          optional_captured_string(read_string(input, "provinceCode")),
        ),
        #(
          "countryCodeV2",
          optional_captured_string(
            read_string(input, "countryCodeV2")
            |> option.or(read_string(input, "countryCode")),
          ),
        ),
        #("zip", optional_captured_string(read_string(input, "zip"))),
      ])
  }
}

fn build_draft_order_shipping_line(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) -> {
      let money = read_object(input, "priceWithCurrency")
      let amount = case money {
        Some(money) -> read_string(money, "amount") |> option.unwrap("0")
        None -> "0"
      }
      let currency_code = case money {
        Some(money) ->
          read_string(money, "currencyCode") |> option.unwrap("CAD")
        None -> "CAD"
      }
      let amount = parse_amount(amount)
      CapturedObject([
        #("title", optional_captured_string(read_string(input, "title"))),
        #("code", CapturedString("custom")),
        #("custom", CapturedBool(True)),
        #("originalPriceSet", money_set(amount, currency_code)),
        #("discountedPriceSet", money_set(amount, currency_code)),
      ])
    }
  }
}

fn build_draft_order_applied_discount(
  input: Option(Dict(String, root_field.ResolvedValue)),
  currency_code: String,
) -> CapturedJsonValue {
  case input {
    None -> CapturedNull
    Some(input) -> {
      let amount =
        read_number(input, "amount")
        |> option.or(read_number(input, "value"))
        |> option.unwrap(0.0)
      CapturedObject([
        #("title", optional_captured_string(read_string(input, "title"))),
        #(
          "description",
          optional_captured_string(read_string(input, "description")),
        ),
        #("value", captured_number(input, "value")),
        #(
          "valueType",
          optional_captured_string(read_string(input, "valueType")),
        ),
        #("amountSet", money_set(amount, currency_code)),
      ])
    }
  }
}

fn captured_attributes(
  attributes: List(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  CapturedArray(
    attributes
    |> list.map(fn(attribute) {
      CapturedObject([
        #("key", optional_captured_string(read_string(attribute, "key"))),
        #("value", optional_captured_string(read_string(attribute, "value"))),
      ])
    }),
  )
}

fn money_set(amount: Float, currency_code: String) -> CapturedJsonValue {
  CapturedObject([
    #(
      "shopMoney",
      CapturedObject([
        #("amount", CapturedString(float.to_string(amount))),
        #("currencyCode", CapturedString(currency_code)),
      ]),
    ),
  ])
}

fn captured_money_amount(value: CapturedJsonValue, name: String) -> Float {
  case captured_object_field(value, name) {
    Some(money) -> captured_money_value(money)
    None -> 0.0
  }
}

fn captured_money_value(value: CapturedJsonValue) -> Float {
  case captured_object_field(value, "shopMoney") {
    Some(shop_money) ->
      case captured_object_field(shop_money, "amount") {
        Some(CapturedString(amount)) -> parse_amount(amount)
        _ -> 0.0
      }
    None -> 0.0
  }
}

fn discount_amount(discount: CapturedJsonValue, base: Float) -> Float {
  case discount {
    CapturedNull -> 0.0
    _ -> {
      let amount = captured_money_amount(discount, "amountSet")
      case captured_string_field(discount, "valueType") {
        Some("PERCENTAGE") ->
          case captured_number_field(discount, "value") {
            Some(percent) -> base *. percent /. 100.0
            None -> amount
          }
        _ -> amount
      }
    }
  }
}

fn draft_order_currency(
  input: Dict(String, root_field.ResolvedValue),
  line_items: List(CapturedJsonValue),
) -> String {
  case read_object(input, "shippingLine") {
    Some(shipping) ->
      case read_object(shipping, "priceWithCurrency") {
        Some(money) ->
          read_string(money, "currencyCode") |> option.unwrap("CAD")
        None -> line_item_currency(line_items)
      }
    None -> line_item_currency(line_items)
  }
}

fn line_item_currency(line_items: List(CapturedJsonValue)) -> String {
  line_items
  |> list.find_map(fn(item) {
    case captured_object_field(item, "originalUnitPriceSet") {
      Some(money) ->
        case captured_object_field(money, "shopMoney") {
          Some(shop_money) ->
            case captured_object_field(shop_money, "currencyCode") {
              Some(CapturedString(value)) -> Ok(value)
              _ -> Error(Nil)
            }
          None -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> result.unwrap("CAD")
}

fn total_quantity(line_items: List(CapturedJsonValue)) -> Int {
  line_items
  |> list.fold(0, fn(sum, item) {
    sum
    + case captured_object_field(item, "quantity") {
      Some(CapturedInt(quantity)) -> quantity
      _ -> 0
    }
  })
}

fn read_object(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, name) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_object_list(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.ObjectVal(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_string(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_string_list(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_int(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Int,
) -> Int {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> value
    _ -> fallback
  }
}

fn read_bool(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Bool,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> value
    _ -> fallback
  }
}

fn read_number(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Float) {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> Some(int.to_float(value))
    Ok(root_field.FloatVal(value)) -> Some(value)
    Ok(root_field.StringVal(value)) -> Some(parse_amount(value))
    _ -> None
  }
}

fn captured_number(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> CapturedJsonValue {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> CapturedInt(value)
    Ok(root_field.FloatVal(value)) -> CapturedFloat(value)
    Ok(root_field.StringVal(value)) -> CapturedString(value)
    _ -> CapturedNull
  }
}

fn parse_amount(value: String) -> Float {
  float.parse(value) |> result.unwrap(0.0)
}

fn optional_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

fn max_float(left: Float, right: Float) -> Float {
  case left >. right {
    True -> left
    False -> right
  }
}

fn captured_object_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      fields
      |> list.find_map(fn(pair) {
        let #(key, item) = pair
        case key == name {
          True -> Ok(item)
          False -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn captured_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

fn captured_number_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(Float) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(int.to_float(value))
    Some(CapturedFloat(value)) -> Some(value)
    Some(CapturedString(value)) -> Some(parse_amount(value))
    _ -> None
  }
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
