//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

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
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, resolved_value_to_source,
  serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, RequiredArgument,
  find_argument, single_root_log_draft, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  captured_array_values, captured_field_or_null, captured_int_field,
  captured_json_source, captured_money_amount, captured_money_value,
  captured_object_field, captured_string_field, draft_order_gid_tail,
  field_arguments, format_decimal_amount, max_float, money_set, money_set_string,
  optional_captured_string, order_line_items, parse_amount, read_int,
  read_number, read_object, read_string, replace_captured_object_fields,
  selection_children, serialize_captured_selection, user_error,
}
import shopify_draft_proxy/proxy/orders/draft_order_builders.{discount_amount}
import shopify_draft_proxy/proxy/orders/draft_orders.{
  captured_field_or_int, captured_field_or_money,
}
import shopify_draft_proxy/proxy/orders/hydration.{
  maybe_hydrate_order_by_id, maybe_hydrate_product_variant_by_id,
}
import shopify_draft_proxy/proxy/orders/order_types.{
  type OrderEditUserError, OrderEditUserError,
}
import shopify_draft_proxy/proxy/orders/serializers.{serialize_order_node}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  type CustomerRecord, type DraftOrderRecord,
  type DraftOrderVariantCatalogRecord, type OrderRecord,
  type ProductMetafieldRecord, type ProductRecord, type ProductVariantRecord,
  AbandonmentDeliveryActivityRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
  CustomerOrderSummaryRecord, CustomerRecord, DraftOrderRecord,
  DraftOrderVariantCatalogRecord, OrderRecord, ProductVariantRecord,
}

@internal
pub fn handle_order_edit_begin_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
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
      "orderEditBegin",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let hydrated_store = case read_string(args, "id") {
        Some(id) -> maybe_hydrate_order_by_id(store, id, upstream)
        None -> store
      }
      // Pattern 2: orderEditBegin materializes a calculatedOrder from the
      // upstream/cassette order and stages an edit session locally.
      let order =
        read_string(args, "id")
        |> option.then(fn(id) { store.get_order_by_id(hydrated_store, id) })
      case order {
        Some(order) -> {
          case order_edit_order_not_editable(order) {
            True -> {
              let payload =
                serialize_order_edit_error_payload(field, [
                  order_edit_invalid_user_error(
                    ["base"],
                    "The order cannot be edited.",
                  ),
                ])
              #(key, payload, hydrated_store, identity, [], [], [])
            }
            False -> {
              case order_has_open_order_edit_session(order) {
                True -> {
                  let payload =
                    serialize_order_edit_error_payload(field, [
                      order_edit_invalid_user_error(
                        ["id"],
                        "An edit is already in progress for this order",
                      ),
                    ])
                  #(key, payload, hydrated_store, identity, [], [], [])
                }
                False -> {
                  let #(calculated_order, next_identity) =
                    build_calculated_order_from_order(order, identity)
                  let next_store =
                    stage_order_edit_session(
                      hydrated_store,
                      order,
                      calculated_order,
                    )
                  let payload =
                    serialize_order_edit_begin_payload(
                      field,
                      calculated_order,
                      fragments,
                    )
                  #(key, payload, next_store, next_identity, [], [], [])
                }
              }
            }
          }
        }
        None -> {
          let payload =
            serialize_order_edit_error_payload(field, [
              order_edit_invalid_user_error(["id"], "The order does not exist."),
            ])
          #(key, payload, hydrated_store, identity, [], [], [])
        }
      }
    }
  }
}

@internal
pub fn handle_order_edit_add_variant_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
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
      "orderEditAddVariant",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let calculated_order_id = read_string(args, "id")
      let variant_id = read_string(args, "variantId")
      case find_order_edit_session(store, calculated_order_id) {
        None -> {
          let payload =
            serialize_order_edit_error_payload(field, [
              order_edit_invalid_user_error(
                ["id"],
                "The calculated order does not exist.",
              ),
            ])
          #(key, payload, store, identity, [], [], [])
        }
        Some(_) -> {
          let hydrated_store = case variant_id {
            Some(id) -> maybe_hydrate_product_variant_by_id(store, id, upstream)
            None -> store
          }
          let variant =
            variant_id
            |> option.then(fn(id) {
              store.get_effective_variant_by_id(hydrated_store, id)
            })
          case variant {
            Some(variant) -> {
              let product =
                store.get_effective_product_by_id(
                  hydrated_store,
                  variant.product_id,
                )
              let quantity = read_int(args, "quantity", 1)
              let session_id =
                calculated_order_id
                |> option.map(order_edit_session_id_from_calculated_id)
                |> option.unwrap("")
              let #(calculated_line_item, next_identity) =
                build_added_calculated_line_item(
                  variant,
                  product,
                  quantity,
                  identity,
                )
              let #(next_store, calculated_order) =
                update_order_edit_session_with_line_item(
                  hydrated_store,
                  calculated_order_id,
                  calculated_line_item,
                )
              let payload =
                serialize_order_edit_add_variant_payload(
                  field,
                  calculated_line_item,
                  calculated_order,
                  session_id,
                  fragments,
                )
              #(key, payload, next_store, next_identity, [], [], [])
            }
            None -> {
              let user_error = case variant_id {
                Some(id) ->
                  case draft_order_gid_tail(id) == "0" {
                    True -> order_edit_invalid_variant_user_error()
                    False ->
                      order_edit_invalid_user_error(
                        ["variantId"],
                        "Variant does not exist",
                      )
                  }
                _ ->
                  order_edit_invalid_user_error(
                    ["variantId"],
                    "Variant does not exist",
                  )
              }
              let payload =
                serialize_order_edit_error_payload(field, [user_error])
              #(key, payload, hydrated_store, identity, [], [], [])
            }
          }
        }
      }
    }
  }
}

@internal
pub fn handle_order_edit_set_quantity_mutation(
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
      "orderEditSetQuantity",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let calculated_order_id = read_string(args, "id")
      let quantity = read_int(args, "quantity", 0)
      case find_order_edit_session(store, calculated_order_id) {
        None -> {
          let payload =
            serialize_order_edit_error_payload(field, [
              order_edit_invalid_user_error(
                ["id"],
                "The calculated order does not exist.",
              ),
            ])
          #(key, payload, store, identity, [], [], [])
        }
        Some(_) -> {
          let line_item =
            find_order_edit_session_line_item(
              store,
              calculated_order_id,
              read_string(args, "lineItemId"),
            )
            |> option.or(
              read_string(args, "lineItemId")
              |> option.then(fn(id) {
                find_order_edit_line_item_by_calculated_id(store, id)
              }),
            )
          case line_item {
            Some(line_item) -> {
              let calculated_line_item =
                build_set_quantity_calculated_line_item(line_item, quantity)
              let #(next_store, calculated_order) =
                update_order_edit_session_line_item_quantity(
                  store,
                  calculated_order_id,
                  read_string(args, "lineItemId"),
                  quantity,
                )
              let payload =
                serialize_order_edit_set_quantity_payload(
                  field,
                  calculated_line_item,
                  calculated_order,
                  calculated_order_id,
                  fragments,
                )
              #(key, payload, next_store, identity, [], [], [])
            }
            None -> {
              let payload =
                serialize_order_edit_error_payload(field, [
                  order_edit_invalid_user_error(
                    ["lineItemId"],
                    "Line item does not exist",
                  ),
                ])
              #(key, payload, store, identity, [], [], [])
            }
          }
        }
      }
    }
  }
}

@internal
pub fn handle_order_edit_commit_mutation(
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
      "orderEditCommit",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let calculated_order_id = read_string(args, "id")
      case find_order_edit_session(store, calculated_order_id) {
        None -> {
          let payload =
            serialize_order_edit_error_payload(field, [
              order_edit_invalid_user_error(
                ["id"],
                "The calculated order does not exist.",
              ),
            ])
          #(key, payload, store, identity, [], [], [])
        }
        Some(match) -> {
          let #(order, session) = match
          let #(timestamp, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let committed_order =
            commit_order_edit_session(order, session, timestamp)
          let next_store =
            store.stage_order(
              store,
              remove_order_edit_session(committed_order, calculated_order_id),
            )
          let payload =
            serialize_order_edit_commit_payload(
              field,
              committed_order,
              fragments,
            )
          let draft =
            single_root_log_draft(
              "orderEditCommit",
              [order.id],
              store.Staged,
              "orders",
              "stage-locally",
              Some("Locally staged orderEditCommit in shopify-draft-proxy."),
            )
          #(key, payload, next_store, next_identity, [order.id], [], [draft])
        }
      }
    }
  }
}

@internal
pub fn handle_order_edit_residual_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  case find_order_edit_session(store, read_string(args, "id")) {
    None -> #(key, json.null(), store, identity)
    Some(match) -> {
      let #(order, session) = match
      case root_name {
        "orderEditAddCustomItem" ->
          order_edit_add_custom_item(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditAddLineItemDiscount" ->
          order_edit_add_line_item_discount(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditRemoveDiscount" ->
          order_edit_remove_discount(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditAddShippingLine" ->
          order_edit_add_shipping_line(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditUpdateShippingLine" ->
          order_edit_update_shipping_line(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        "orderEditRemoveShippingLine" ->
          order_edit_remove_shipping_line(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            session,
            args,
          )
        _ -> #(key, json.null(), store, identity)
      }
    }
  }
}

@internal
pub fn order_edit_add_custom_item(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let #(line_item, next_identity) =
    build_order_edit_custom_line_item(identity, args)
  let line_items =
    list.append(order_edit_session_line_items(session), [line_item])
  let added_line_items =
    list.append(order_edit_session_added_line_items(session), [line_item])
  let updated_session =
    replace_captured_object_fields(session, [
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
      #(
        "addedLineItems",
        CapturedObject([#("nodes", CapturedArray(added_line_items))]),
      ),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      Some(line_item),
      None,
      None,
      fragments,
    )
  #(key, payload, next_store, next_identity)
}

@internal
pub fn order_edit_add_line_item_discount(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let line_item_id = read_string(args, "lineItemId")
  let discount = read_object(args, "discount") |> option.unwrap(dict.new())
  let description = read_string(discount, "description") |> option.unwrap("")
  let fixed_value =
    read_object(discount, "fixedValue") |> option.unwrap(discount)
  let discount_amount = read_number(fixed_value, "amount") |> option.unwrap(0.0)
  let currency_code =
    read_string(fixed_value, "currencyCode") |> option.unwrap("CAD")
  let #(staged_change_id, identity_after_change) =
    synthetic_identity.make_synthetic_gid(
      identity,
      "OrderStagedChangeAddLineItemDiscount",
    )
  let #(discount_application_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_change,
      "CalculatedManualDiscountApplication",
    )
  let staged_change =
    CapturedObject([
      #("id", CapturedString(staged_change_id)),
      #("description", CapturedString(description)),
    ])
  let line_items =
    order_edit_session_line_items(session)
    |> list.map(fn(line_item) {
      case captured_string_field(line_item, "id") == line_item_id {
        True ->
          apply_order_edit_line_discount(
            line_item,
            discount_amount,
            currency_code,
            description,
            discount_application_id,
          )
        False -> line_item
      }
    })
  let updated_session =
    replace_captured_object_fields(session, [
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let calculated_line_item =
    line_item_id
    |> option.then(fn(id) { find_calculated_line_item(line_items, id) })
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      calculated_line_item,
      None,
      Some(staged_change),
      fragments,
    )
  #(key, payload, next_store, next_identity)
}

@internal
pub fn order_edit_remove_discount(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let discount_application_id = read_string(args, "discountApplicationId")
  let line_items =
    order_edit_session_line_items(session)
    |> list.map(fn(line_item) {
      case
        order_edit_line_item_has_discount(line_item, discount_application_id)
      {
        True -> remove_order_edit_line_discount(line_item)
        False -> line_item
      }
    })
  let updated_session =
    replace_captured_object_fields(session, [
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      None,
      None,
      None,
      fragments,
    )
  #(key, payload, next_store, identity)
}

@internal
pub fn order_edit_add_shipping_line(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let shipping_input =
    read_object(args, "shippingLine") |> option.unwrap(dict.new())
  let #(shipping_line, next_identity) =
    build_order_edit_shipping_line(identity, shipping_input, "ADDED")
  let shipping_lines =
    list.append(order_edit_session_shipping_lines(session), [shipping_line])
  let updated_session =
    replace_captured_object_fields(session, [
      #("shippingLines", CapturedArray(shipping_lines)),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      None,
      Some(shipping_line),
      None,
      fragments,
    )
  #(key, payload, next_store, next_identity)
}

@internal
pub fn order_edit_update_shipping_line(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let shipping_line_id = read_string(args, "shippingLineId")
  let shipping_input =
    read_object(args, "shippingLine") |> option.unwrap(dict.new())
  let shipping_lines =
    order_edit_session_shipping_lines(session)
    |> list.map(fn(shipping_line) {
      case captured_string_field(shipping_line, "id") == shipping_line_id {
        True -> update_order_edit_shipping_line(shipping_line, shipping_input)
        False -> shipping_line
      }
    })
  let updated_session =
    replace_captured_object_fields(session, [
      #("shippingLines", CapturedArray(shipping_lines)),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      None,
      None,
      None,
      fragments,
    )
  #(key, payload, next_store, identity)
}

@internal
pub fn order_edit_remove_shipping_line(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  session: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, SyntheticIdentityRegistry) {
  let shipping_line_id = read_string(args, "shippingLineId")
  let shipping_lines =
    order_edit_session_shipping_lines(session)
    |> list.filter(fn(shipping_line) {
      captured_string_field(shipping_line, "id") != shipping_line_id
    })
  let updated_session =
    replace_captured_object_fields(session, [
      #("shippingLines", CapturedArray(shipping_lines)),
    ])
  let #(next_store, calculated_order) =
    stage_updated_order_edit_session(store, order, updated_session)
  let payload =
    serialize_order_edit_residual_payload(
      field,
      Some(calculated_order),
      None,
      None,
      None,
      fragments,
    )
  #(key, payload, next_store, identity)
}

@internal
pub fn build_calculated_order_from_order(
  order: OrderRecord,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, identity_after_order) =
    synthetic_identity.make_synthetic_gid(identity, "CalculatedOrder")
  let #(line_items, next_identity) =
    build_calculated_line_items(
      order_line_items(order.data),
      identity_after_order,
    )
  let subtotal = order_edit_line_items_total(line_items)
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #(
        "originalOrder",
        CapturedObject([
          #("id", CapturedString(order.id)),
          #(
            "name",
            optional_captured_string(captured_string_field(order.data, "name")),
          ),
        ]),
      ),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
      #("addedLineItems", CapturedObject([#("nodes", CapturedArray([]))])),
      #("shippingLines", CapturedArray(order_edit_shipping_lines(order))),
      #(
        "subtotalLineItemsQuantity",
        CapturedInt(order_edit_line_items_quantity(line_items)),
      ),
      #("subtotalPriceSet", money_set(subtotal, "CAD")),
      #("totalPriceSet", money_set(subtotal, "CAD")),
    ]),
    next_identity,
  )
}

@internal
pub fn stage_order_edit_session(
  store: Store,
  order: OrderRecord,
  calculated_order: CapturedJsonValue,
) -> Store {
  let session = order_edit_session_record(order.id, calculated_order)
  store.stage_order(store, upsert_order_edit_session(order, session))
}

@internal
pub fn order_edit_session_record(
  order_id: String,
  calculated_order: CapturedJsonValue,
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "id",
      optional_captured_string(captured_string_field(calculated_order, "id")),
    ),
    #("originalOrderId", CapturedString(order_id)),
    #(
      "lineItems",
      captured_object_field(calculated_order, "lineItems")
        |> option.unwrap(CapturedObject([#("nodes", CapturedArray([]))])),
    ),
    #(
      "addedLineItems",
      captured_object_field(calculated_order, "addedLineItems")
        |> option.unwrap(CapturedObject([#("nodes", CapturedArray([]))])),
    ),
    #(
      "shippingLines",
      captured_object_field(calculated_order, "shippingLines")
        |> option.unwrap(CapturedArray([])),
    ),
  ])
}

@internal
pub fn upsert_order_edit_session(
  order: OrderRecord,
  session: CapturedJsonValue,
) -> OrderRecord {
  let session_id = captured_string_field(session, "id") |> option.unwrap("")
  let existing =
    order_edit_sessions(order)
    |> list.filter(fn(existing_session) {
      captured_string_field(existing_session, "id") != Some(session_id)
    })
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("orderEditSessions", CapturedArray(list.append(existing, [session]))),
    ]),
  )
}

@internal
pub fn remove_order_edit_session(
  order: OrderRecord,
  calculated_order_id: Option(String),
) -> OrderRecord {
  let remaining =
    order_edit_sessions(order)
    |> list.filter(fn(session) {
      captured_string_field(session, "id") != calculated_order_id
    })
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("orderEditSessions", CapturedArray(remaining)),
    ]),
  )
}

@internal
pub fn order_edit_sessions(order: OrderRecord) -> List(CapturedJsonValue) {
  case captured_object_field(order.data, "orderEditSessions") {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

@internal
pub fn order_has_open_order_edit_session(order: OrderRecord) -> Bool {
  case order_edit_sessions(order) {
    [] -> False
    [_, ..] -> True
  }
}

@internal
pub fn order_edit_order_not_editable(order: OrderRecord) -> Bool {
  case captured_string_field(order.data, "displayFinancialStatus") {
    Some("REFUNDED") | Some("VOIDED") -> True
    _ -> order_cancelled_at_is_set(order)
  }
}

@internal
pub fn order_cancelled_at_is_set(order: OrderRecord) -> Bool {
  case captured_object_field(order.data, "cancelledAt") {
    Some(CapturedNull) | None -> False
    Some(_) -> True
  }
}

@internal
pub fn find_order_edit_session(
  store: Store,
  calculated_order_id: Option(String),
) -> Option(#(OrderRecord, CapturedJsonValue)) {
  case calculated_order_id {
    None -> None
    Some(id) ->
      store.list_effective_orders(store)
      |> list.find_map(fn(order) {
        case
          order_edit_sessions(order)
          |> list.find(fn(session) {
            captured_string_field(session, "id") == Some(id)
          })
        {
          Ok(session) -> Ok(#(order, session))
          Error(_) -> Error(Nil)
        }
      })
      |> option.from_result
  }
}

@internal
pub fn find_order_edit_session_line_item(
  store: Store,
  calculated_order_id: Option(String),
  line_item_id: Option(String),
) -> Option(CapturedJsonValue) {
  case find_order_edit_session(store, calculated_order_id), line_item_id {
    Some(match), Some(line_item_id) -> {
      let #(_, session) = match
      order_edit_session_line_items(session)
      |> list.find(fn(line_item) {
        captured_string_field(line_item, "id") == Some(line_item_id)
      })
      |> option.from_result
    }
    _, _ -> None
  }
}

@internal
pub fn order_edit_session_line_items(
  session: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(session, "lineItems") {
    Some(line_items) ->
      case captured_object_field(line_items, "nodes") {
        Some(CapturedArray(items)) -> items
        _ -> []
      }
    None -> []
  }
}

@internal
pub fn order_edit_session_added_line_items(
  session: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(session, "addedLineItems") {
    Some(line_items) ->
      case captured_object_field(line_items, "nodes") {
        Some(CapturedArray(items)) -> items
        _ -> []
      }
    None -> []
  }
}

@internal
pub fn order_edit_session_shipping_lines(
  session: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(session, "shippingLines") {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

@internal
pub fn stage_updated_order_edit_session(
  store: Store,
  order: OrderRecord,
  session: CapturedJsonValue,
) -> #(Store, CapturedJsonValue) {
  let updated_order = upsert_order_edit_session(order, session)
  #(
    store.stage_order(store, updated_order),
    calculated_order_from_session(session, updated_order),
  )
}

@internal
pub fn build_order_edit_custom_line_item(
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "CalculatedLineItem")
  let price = read_object(args, "price") |> option.unwrap(dict.new())
  let amount = read_number(price, "amount") |> option.unwrap(0.0)
  let currency_code = read_string(price, "currencyCode") |> option.unwrap("CAD")
  let quantity = read_int(args, "quantity", 1)
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("title", optional_captured_string(read_string(args, "title"))),
      #("quantity", CapturedInt(quantity)),
      #("currentQuantity", CapturedInt(quantity)),
      #("sku", CapturedNull),
      #("variant", CapturedNull),
      #("originalUnitPriceSet", money_set(amount, currency_code)),
      #("discountedUnitPriceSet", money_set(amount, currency_code)),
    ]),
    next_identity,
  )
}

@internal
pub fn apply_order_edit_line_discount(
  line_item: CapturedJsonValue,
  discount_amount: Float,
  currency_code: String,
  description: String,
  discount_application_id: String,
) -> CapturedJsonValue {
  let quantity = captured_int_field(line_item, "quantity") |> option.unwrap(1)
  let original = captured_money_amount(line_item, "originalUnitPriceSet")
  let discounted = max_float(0.0, original -. discount_amount)
  let allocated = discount_amount *. int.to_float(quantity)
  replace_captured_object_fields(line_item, [
    #("hasStagedLineItemDiscount", CapturedBool(True)),
    #("discountedUnitPriceSet", money_set(discounted, currency_code)),
    #(
      "calculatedDiscountAllocations",
      CapturedArray([
        CapturedObject([
          #("allocatedAmountSet", money_set(allocated, currency_code)),
          #(
            "discountApplication",
            CapturedObject([
              #("id", CapturedString(discount_application_id)),
              #("description", CapturedString(description)),
            ]),
          ),
        ]),
      ]),
    ),
  ])
}

@internal
pub fn remove_order_edit_line_discount(
  line_item: CapturedJsonValue,
) -> CapturedJsonValue {
  let original =
    captured_object_field(line_item, "originalUnitPriceSet")
    |> option.unwrap(money_set(0.0, "CAD"))
  replace_captured_object_fields(line_item, [
    #("hasStagedLineItemDiscount", CapturedBool(False)),
    #("calculatedDiscountAllocations", CapturedArray([])),
    #("discountedUnitPriceSet", original),
  ])
}

@internal
pub fn order_edit_line_item_has_discount(
  line_item: CapturedJsonValue,
  discount_application_id: Option(String),
) -> Bool {
  case discount_application_id {
    None -> False
    Some(id) ->
      case captured_object_field(line_item, "calculatedDiscountAllocations") {
        Some(CapturedArray(allocations)) ->
          allocations
          |> list.any(fn(allocation) {
            captured_object_field(allocation, "discountApplication")
            |> option.then(fn(application) {
              captured_string_field(application, "id")
            })
            == Some(id)
          })
        _ -> False
      }
  }
}

@internal
pub fn find_calculated_line_item(
  line_items: List(CapturedJsonValue),
  id: String,
) -> Option(CapturedJsonValue) {
  line_items
  |> list.find(fn(line_item) {
    captured_string_field(line_item, "id") == Some(id)
  })
  |> option.from_result
}

@internal
pub fn build_order_edit_shipping_line(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  staged_status: String,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "CalculatedShippingLine")
  let price = read_object(input, "price") |> option.unwrap(dict.new())
  let amount = read_number(price, "amount") |> option.unwrap(0.0)
  let currency_code = read_string(price, "currencyCode") |> option.unwrap("CAD")
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("title", optional_captured_string(read_string(input, "title"))),
      #("stagedStatus", CapturedString(staged_status)),
      #("price", money_set(amount, currency_code)),
    ]),
    next_identity,
  )
}

@internal
pub fn update_order_edit_shipping_line(
  shipping_line: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let price = read_object(input, "price") |> option.unwrap(dict.new())
  let amount = read_number(price, "amount") |> option.unwrap(0.0)
  let currency_code = read_string(price, "currencyCode") |> option.unwrap("CAD")
  replace_captured_object_fields(shipping_line, [
    #("title", optional_captured_string(read_string(input, "title"))),
    #("price", money_set(amount, currency_code)),
  ])
}

@internal
pub fn order_edit_shipping_lines(
  order: OrderRecord,
) -> List(CapturedJsonValue) {
  case captured_object_field(order.data, "shippingLines") {
    Some(CapturedObject(fields)) ->
      dict.from_list(fields)
      |> dict.get("nodes")
      |> result.unwrap(CapturedArray([]))
      |> captured_array_values
    Some(CapturedArray(items)) -> items
    _ -> []
  }
}

@internal
pub fn order_edit_line_items_quantity(
  line_items: List(CapturedJsonValue),
) -> Int {
  line_items
  |> list.fold(0, fn(sum, line_item) {
    let quantity = captured_int_field(line_item, "quantity") |> option.unwrap(0)
    sum + quantity
  })
}

@internal
pub fn order_edit_line_items_total(
  line_items: List(CapturedJsonValue),
) -> Float {
  line_items
  |> list.fold(0.0, fn(sum, line_item) {
    let quantity = captured_int_field(line_item, "quantity") |> option.unwrap(0)
    let unit =
      captured_object_field(line_item, "discountedUnitPriceSet")
      |> option.map(captured_money_value)
      |> option.unwrap(captured_money_amount(line_item, "originalUnitPriceSet"))
    sum +. unit *. int.to_float(quantity)
  })
}

@internal
pub fn order_edit_shipping_lines_total(
  shipping_lines: List(CapturedJsonValue),
) -> Float {
  shipping_lines
  |> list.fold(0.0, fn(sum, shipping_line) {
    sum +. captured_money_amount(shipping_line, "price")
  })
}

@internal
pub fn update_order_edit_session_with_line_item(
  store: Store,
  calculated_order_id: Option(String),
  calculated_line_item: CapturedJsonValue,
) -> #(Store, Option(CapturedJsonValue)) {
  case find_order_edit_session(store, calculated_order_id) {
    None -> #(store, None)
    Some(match) -> {
      let #(order, session) = match
      let line_items =
        list.append(order_edit_session_line_items(session), [
          calculated_line_item,
        ])
      let added_line_items =
        list.append(order_edit_session_added_line_items(session), [
          calculated_line_item,
        ])
      let updated_session =
        replace_captured_object_fields(session, [
          #(
            "lineItems",
            CapturedObject([#("nodes", CapturedArray(line_items))]),
          ),
          #(
            "addedLineItems",
            CapturedObject([#("nodes", CapturedArray(added_line_items))]),
          ),
        ])
      let updated_order = upsert_order_edit_session(order, updated_session)
      #(
        store.stage_order(store, updated_order),
        Some(calculated_order_from_session(updated_session, updated_order)),
      )
    }
  }
}

@internal
pub fn update_order_edit_session_line_item_quantity(
  store: Store,
  calculated_order_id: Option(String),
  line_item_id: Option(String),
  quantity: Int,
) -> #(Store, Option(CapturedJsonValue)) {
  case find_order_edit_session(store, calculated_order_id), line_item_id {
    Some(match), Some(line_item_id) -> {
      let #(order, session) = match
      let line_items =
        order_edit_session_line_items(session)
        |> list.map(fn(line_item) {
          case captured_string_field(line_item, "id") == Some(line_item_id) {
            True ->
              replace_captured_object_fields(line_item, [
                #("quantity", CapturedInt(quantity)),
                #("currentQuantity", CapturedInt(quantity)),
              ])
            False -> line_item
          }
        })
      let updated_session =
        replace_captured_object_fields(session, [
          #(
            "lineItems",
            CapturedObject([#("nodes", CapturedArray(line_items))]),
          ),
        ])
      let updated_order = upsert_order_edit_session(order, updated_session)
      #(
        store.stage_order(store, updated_order),
        Some(calculated_order_from_session(updated_session, updated_order)),
      )
    }
    _, _ -> #(store, None)
  }
}

@internal
pub fn calculated_order_from_session(
  session: CapturedJsonValue,
  order: OrderRecord,
) -> CapturedJsonValue {
  let line_items = order_edit_session_line_items(session)
  let shipping_lines = order_edit_session_shipping_lines(session)
  let subtotal = order_edit_line_items_total(line_items)
  let shipping_total = order_edit_shipping_lines_total(shipping_lines)
  CapturedObject([
    #("id", captured_field_or_null(session, "id")),
    #(
      "originalOrder",
      CapturedObject([
        #("id", CapturedString(order.id)),
        #("name", captured_field_or_null(order.data, "name")),
      ]),
    ),
    #(
      "lineItems",
      captured_object_field(session, "lineItems")
        |> option.unwrap(CapturedObject([#("nodes", CapturedArray([]))])),
    ),
    #(
      "addedLineItems",
      captured_object_field(session, "addedLineItems")
        |> option.unwrap(CapturedObject([#("nodes", CapturedArray([]))])),
    ),
    #("shippingLines", CapturedArray(shipping_lines)),
    #(
      "subtotalLineItemsQuantity",
      CapturedInt(order_edit_line_items_quantity(line_items)),
    ),
    #("subtotalPriceSet", money_set(subtotal, "CAD")),
    #("totalPriceSet", money_set(subtotal +. shipping_total, "CAD")),
  ])
}

@internal
pub fn commit_order_edit_session(
  order: OrderRecord,
  session: CapturedJsonValue,
  updated_at: String,
) -> OrderRecord {
  let committed_line_items =
    order_edit_session_line_items(session)
    |> list.map(fn(line_item) { commit_order_edit_line_item(order, line_item) })
  let current_quantity =
    committed_line_items
    |> list.fold(0, fn(sum, line_item) {
      let quantity =
        captured_int_field(line_item, "currentQuantity") |> option.unwrap(0)
      sum + quantity
    })
  OrderRecord(
    ..order,
    data: replace_captured_object_fields(order.data, [
      #("updatedAt", CapturedString(updated_at)),
      #("currentSubtotalLineItemsQuantity", CapturedInt(current_quantity)),
      #(
        "lineItems",
        CapturedObject([#("nodes", CapturedArray(committed_line_items))]),
      ),
    ]),
  )
}

@internal
pub fn commit_order_edit_line_item(
  order: OrderRecord,
  calculated_line_item: CapturedJsonValue,
) -> CapturedJsonValue {
  let calculated_id = captured_string_field(calculated_line_item, "id")
  let original_line_item =
    calculated_id
    |> option.then(fn(id) {
      find_order_edit_line_item_by_calculated_id_in_order(order, id)
    })
  case original_line_item {
    Some(original) ->
      replace_captured_object_fields(original, [
        #(
          "currentQuantity",
          captured_field_or_int(calculated_line_item, "currentQuantity", 0),
        ),
      ])
    None ->
      CapturedObject([
        #("id", optional_captured_string(calculated_id)),
        #("title", captured_field_or_null(calculated_line_item, "title")),
        #(
          "quantity",
          captured_field_or_int(calculated_line_item, "quantity", 0),
        ),
        #(
          "currentQuantity",
          captured_field_or_int(calculated_line_item, "currentQuantity", 0),
        ),
        #("sku", captured_field_or_null(calculated_line_item, "sku")),
        #("variant", captured_field_or_null(calculated_line_item, "variant")),
        #(
          "originalUnitPriceSet",
          captured_field_or_money(
            calculated_line_item,
            "originalUnitPriceSet",
            "CAD",
          ),
        ),
      ])
  }
}

@internal
pub fn find_order_edit_line_item_by_calculated_id_in_order(
  order: OrderRecord,
  calculated_line_item_id: String,
) -> Option(CapturedJsonValue) {
  let index = calculated_line_item_index(calculated_line_item_id)
  case index {
    Some(index) -> list_item_at(order_line_items(order.data), index)
    None -> None
  }
}

@internal
pub fn build_calculated_line_items(
  line_items: List(CapturedJsonValue),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  line_items
  |> list.fold(#([], identity), fn(acc, item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "CalculatedLineItem",
      )
    let quantity = captured_int_field(item, "quantity") |> option.unwrap(0)
    let current_quantity =
      captured_int_field(item, "currentQuantity") |> option.unwrap(quantity)
    let calculated_item =
      CapturedObject([
        #("id", CapturedString(id)),
        #("title", captured_field_or_null(item, "title")),
        #("quantity", CapturedInt(quantity)),
        #("currentQuantity", CapturedInt(current_quantity)),
        #("sku", captured_field_or_null(item, "sku")),
        #("variant", captured_field_or_null(item, "variant")),
        #(
          "originalUnitPriceSet",
          captured_field_or_money(item, "originalUnitPriceSet", "CAD"),
        ),
      ])
    #(list.append(items, [calculated_item]), next_identity)
  })
}

@internal
pub fn build_added_calculated_line_item(
  variant: ProductVariantRecord,
  product: Option(ProductRecord),
  quantity: Int,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "CalculatedLineItem")
  let title =
    product
    |> option.map(fn(product) { product.title })
    |> option.unwrap(variant.title)
  let amount =
    variant.price
    |> option.map(parse_amount)
    |> option.map(format_decimal_amount)
    |> option.unwrap("0.0")
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("title", CapturedString(title)),
      #("quantity", CapturedInt(quantity)),
      #("currentQuantity", CapturedInt(quantity)),
      #("sku", optional_captured_string(variant.sku)),
      #(
        "variant",
        CapturedObject([
          #("id", CapturedString(variant.id)),
        ]),
      ),
      #("originalUnitPriceSet", money_set_string(amount, "CAD")),
    ]),
    next_identity,
  )
}

@internal
pub fn build_set_quantity_calculated_line_item(
  line_item: CapturedJsonValue,
  quantity: Int,
) -> CapturedJsonValue {
  CapturedObject([
    #("title", captured_field_or_null(line_item, "title")),
    #("quantity", CapturedInt(quantity)),
    #("currentQuantity", CapturedInt(quantity)),
    #("sku", captured_field_or_null(line_item, "sku")),
    #("variant", captured_field_or_null(line_item, "variant")),
    #(
      "originalUnitPriceSet",
      captured_field_or_money(line_item, "originalUnitPriceSet", "CAD"),
    ),
  ])
}

@internal
pub fn find_order_edit_line_item_by_calculated_id(
  store: Store,
  calculated_line_item_id: String,
) -> Option(CapturedJsonValue) {
  let index = calculated_line_item_index(calculated_line_item_id)
  case index {
    Some(index) ->
      store.list_effective_orders(store)
      |> list.find_map(fn(order) {
        case list_item_at(order_line_items(order.data), index) {
          Some(item) -> Ok(item)
          None -> Error(Nil)
        }
      })
      |> option.from_result
    None -> None
  }
}

@internal
pub fn calculated_line_item_index(
  calculated_line_item_id: String,
) -> Option(Int) {
  let tail = draft_order_gid_tail(calculated_line_item_id)
  case int.parse(tail) {
    Ok(value) if value >= 2 -> Some(value - 2)
    _ -> None
  }
}

@internal
pub fn list_item_at(items: List(a), index: Int) -> Option(a) {
  case items, index {
    [], _ -> None
    [item, ..], 0 -> Some(item)
    [_, ..rest], n if n > 0 -> list_item_at(rest, n - 1)
    _, _ -> None
  }
}

@internal
pub fn serialize_order_edit_begin_payload(
  field: Selection,
  calculated_order: CapturedJsonValue,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(
              key,
              project_graphql_value(
                captured_json_source(calculated_order),
                selection_children(child),
                fragments,
              ),
            )
            "orderEditSession" -> #(
              key,
              serialize_order_edit_session(
                child,
                captured_string_field(calculated_order, "id")
                  |> option.map(order_edit_session_id_from_calculated_id)
                  |> option.unwrap(""),
              ),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn serialize_order_edit_add_variant_payload(
  field: Selection,
  calculated_line_item: CapturedJsonValue,
  calculated_order: Option(CapturedJsonValue),
  session_id: String,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(
              key,
              serialize_captured_selection(child, calculated_order, fragments),
            )
            "calculatedLineItem" -> #(
              key,
              project_graphql_value(
                captured_json_source(calculated_line_item),
                selection_children(child),
                fragments,
              ),
            )
            "orderEditSession" -> #(
              key,
              serialize_order_edit_session(child, session_id),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn serialize_order_edit_error_payload(
  field: Selection,
  user_errors: List(OrderEditUserError),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(key, json.null())
            "calculatedLineItem" -> #(key, json.null())
            "orderEditSession" -> #(key, json.null())
            "order" -> #(key, json.null())
            "successMessages" -> #(
              key,
              json.array([], fn(message) { json.string(message) }),
            )
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_order_edit_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn order_edit_invalid_user_error(
  field_path: List(String),
  message: String,
) -> OrderEditUserError {
  OrderEditUserError(
    field_path: field_path,
    message: message,
    code: Some(user_error_codes.invalid),
  )
}

@internal
pub fn order_edit_invalid_variant_user_error() -> OrderEditUserError {
  order_edit_invalid_user_error(
    ["variantId"],
    "can't convert Integer[0] to a positive Integer to use as an untrusted id",
  )
}

@internal
pub fn serialize_order_edit_user_error(
  field: Selection,
  error: OrderEditUserError,
) -> Json {
  let code = case error.code {
    Some(value) -> SrcString(value)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("field", SrcList(list.map(error.field_path, SrcString))),
      #("message", SrcString(error.message)),
      #("code", code),
    ]),
    selection_children(field),
    dict.new(),
  )
}

@internal
pub fn serialize_order_edit_set_quantity_payload(
  field: Selection,
  calculated_line_item: CapturedJsonValue,
  calculated_order: Option(CapturedJsonValue),
  calculated_order_id: Option(String),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(
              key,
              serialize_captured_selection(child, calculated_order, fragments),
            )
            "calculatedLineItem" -> #(
              key,
              project_graphql_value(
                captured_json_source(calculated_line_item),
                selection_children(child),
                fragments,
              ),
            )
            "orderEditSession" -> #(
              key,
              serialize_order_edit_session(
                child,
                calculated_order_id
                  |> option.map(order_edit_session_id_from_calculated_id)
                  |> option.unwrap(""),
              ),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn serialize_order_edit_commit_payload(
  field: Selection,
  order: OrderRecord,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(
              key,
              serialize_order_node(None, child, order, fragments, dict.new()),
            )
            "successMessages" -> #(
              key,
              json.array(["Order updated"], json.string),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn serialize_order_edit_residual_payload(
  field: Selection,
  calculated_order: Option(CapturedJsonValue),
  calculated_line_item: Option(CapturedJsonValue),
  calculated_shipping_line: Option(CapturedJsonValue),
  staged_change: Option(CapturedJsonValue),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedOrder" -> #(
              key,
              serialize_captured_selection(child, calculated_order, fragments),
            )
            "calculatedLineItem" -> #(
              key,
              serialize_captured_selection(
                child,
                calculated_line_item,
                fragments,
              ),
            )
            "calculatedShippingLine" -> #(
              key,
              serialize_captured_selection(
                child,
                calculated_shipping_line,
                fragments,
              ),
            )
            "addedDiscountStagedChange" -> #(
              key,
              serialize_captured_selection(child, staged_change, fragments),
            )
            "userErrors" -> #(key, json.array([], fn(error) { error }))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn serialize_order_edit_session(
  field: Selection,
  session_id: String,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "id" -> #(key, json.string(session_id))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn order_edit_session_id_from_calculated_id(id: String) -> String {
  string.replace(id, "/CalculatedOrder/", "/OrderEditSession/")
}
