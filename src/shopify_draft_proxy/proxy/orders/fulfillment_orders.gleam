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
  captured_bool_field, captured_int_field, captured_string_field,
  captured_supported_actions, computed_fulfillment_order_supported_actions,
  field_arguments, find_order_with_fulfillment_order,
  fulfillment_order_line_items, fulfillment_order_merchant_requests,
  fulfillment_order_supported_actions_with_merge,
  fulfillment_order_supports_split, fulfillment_source_line_item_id,
  has_pending_cancellation_request, inferred_nullable_user_error,
  max_fulfillment_holds_per_api_client, nullable_user_error, option_is_in,
  option_to_result, optional_captured_string, order_fulfillment_holds,
  order_fulfillment_orders, read_object_list, read_string, read_string_argument,
  read_string_list, replace_captured_object_fields, selection_children,
  serialize_captured_selection, user_error_field_source,
}
import shopify_draft_proxy/proxy/orders/draft_order_builders.{total_quantity}
import shopify_draft_proxy/proxy/orders/fulfillments.{
  build_replacement_fulfillment_order,
  fulfillment_order_active_requesting_app_holds, fulfillment_order_hold_handle,
  fulfillment_order_hold_input_from_variables,
  fulfillment_order_hold_validation_errors,
  read_fulfillment_order_line_item_inputs, replace_fulfillment_order_line_items,
  replace_order_fulfillment_order, replace_order_fulfillment_order_with_extras,
  requested_fulfillment_quantity, split_fulfillment_order_line_items,
}
import shopify_draft_proxy/proxy/orders/order_types.{
  type FulfillmentOrderMergeInput, type FulfillmentOrderMergeResult,
  type FulfillmentOrderSplitInput, type FulfillmentOrderSplitResult,
  type RequestedFulfillmentLineItem, FulfillmentOrderMergeInput,
  FulfillmentOrderMergeResult, FulfillmentOrderSplitInput,
  FulfillmentOrderSplitResult, RequestedFulfillmentLineItem,
}
import shopify_draft_proxy/proxy/orders/serializers.{
  serialize_order_fulfillment_order,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
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
pub fn handle_fulfillment_order_bulk_mutation(
  root_name: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
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
  case root_name {
    "fulfillmentOrderSplit" ->
      handle_fulfillment_order_split_mutation(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "fulfillmentOrderMerge" ->
      handle_fulfillment_order_merge_mutation(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "fulfillmentOrdersSetFulfillmentDeadline" ->
      handle_fulfillment_orders_set_deadline_mutation(
        store,
        identity,
        field,
        variables,
      )
    _ -> #(
      get_field_response_key(field),
      json.null(),
      store,
      identity,
      [],
      [],
      [],
    )
  }
}

@internal
pub fn handle_fulfillment_order_split_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
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
  let inputs = read_fulfillment_order_split_inputs(variables)
  case inputs {
    [] -> #(
      key,
      json.null(),
      store,
      identity,
      [],
      [fulfillment_order_bulk_invalid_id_error("fulfillmentOrderSplit", key)],
      [],
    )
    [_, ..] ->
      case
        apply_fulfillment_order_split_inputs(store, identity, inputs, [], [])
      {
        Ok(result) -> {
          let #(results, next_store, next_identity, staged_ids) = result
          #(
            key,
            serialize_fulfillment_order_split_payload(
              field,
              results,
              [],
              fragments,
            ),
            next_store,
            next_identity,
            staged_ids,
            [],
            [
              fulfillment_order_log_draft("fulfillmentOrderSplit", staged_ids),
            ],
          )
        }
        Error(_) -> #(
          key,
          json.null(),
          store,
          identity,
          [],
          [
            fulfillment_order_bulk_invalid_id_error(
              "fulfillmentOrderSplit",
              key,
            ),
          ],
          [],
        )
      }
  }
}

@internal
pub fn handle_fulfillment_order_merge_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
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
  let inputs = read_fulfillment_order_merge_inputs(variables)
  case inputs {
    [] -> #(
      key,
      json.null(),
      store,
      identity,
      [],
      [fulfillment_order_bulk_invalid_id_error("fulfillmentOrderMerge", key)],
      [],
    )
    [_, ..] ->
      case
        apply_fulfillment_order_merge_inputs(store, identity, inputs, [], [])
      {
        Ok(result) -> {
          let #(results, next_store, next_identity, staged_ids) = result
          #(
            key,
            serialize_fulfillment_order_merge_payload(
              field,
              results,
              [],
              fragments,
            ),
            next_store,
            next_identity,
            staged_ids,
            [],
            [
              fulfillment_order_log_draft("fulfillmentOrderMerge", staged_ids),
            ],
          )
        }
        Error(_) -> #(
          key,
          json.null(),
          store,
          identity,
          [],
          [
            fulfillment_order_bulk_invalid_id_error(
              "fulfillmentOrderMerge",
              key,
            ),
          ],
          [],
        )
      }
  }
}

@internal
pub fn handle_fulfillment_orders_set_deadline_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
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
  let ids = read_string_list(variables, "fulfillmentOrderIds")
  let deadline = read_string(variables, "fulfillmentDeadline")
  case ids, deadline {
    [], _ | _, None -> #(
      key,
      json.null(),
      store,
      identity,
      [],
      [
        fulfillment_order_bulk_invalid_id_error(
          "fulfillmentOrdersSetFulfillmentDeadline",
          key,
        ),
      ],
      [],
    )
    [_, ..], Some(deadline) ->
      case apply_fulfillment_order_deadline(store, identity, ids, deadline) {
        Ok(result) -> {
          let #(next_store, next_identity, staged_ids) = result
          #(
            key,
            serialize_fulfillment_orders_set_deadline_payload(field, True, []),
            next_store,
            next_identity,
            staged_ids,
            [],
            [
              fulfillment_order_log_draft(
                "fulfillmentOrdersSetFulfillmentDeadline",
                staged_ids,
              ),
            ],
          )
        }
        Error(_) -> #(
          key,
          json.null(),
          store,
          identity,
          [],
          [
            fulfillment_order_bulk_invalid_id_error(
              "fulfillmentOrdersSetFulfillmentDeadline",
              key,
            ),
          ],
          [],
        )
      }
  }
}

@internal
pub fn read_fulfillment_order_split_inputs(
  variables: Dict(String, root_field.ResolvedValue),
) -> List(FulfillmentOrderSplitInput) {
  read_object_list(variables, "fulfillmentOrderSplits")
  |> list.filter_map(fn(input) {
    case read_string(input, "fulfillmentOrderId") {
      Some(id) ->
        Ok(FulfillmentOrderSplitInput(
          fulfillment_order_id: id,
          line_items: read_fulfillment_order_line_item_inputs(input),
        ))
      None -> Error(Nil)
    }
  })
}

@internal
pub fn read_fulfillment_order_merge_inputs(
  variables: Dict(String, root_field.ResolvedValue),
) -> List(FulfillmentOrderMergeInput) {
  read_object_list(variables, "fulfillmentOrderMergeInputs")
  |> list.filter_map(fn(input) {
    let ids =
      read_object_list(input, "mergeIntents")
      |> list.filter_map(fn(intent) {
        read_string(intent, "fulfillmentOrderId") |> option_to_result
      })
    case ids {
      [] -> Error(Nil)
      [_, ..] -> Ok(FulfillmentOrderMergeInput(ids: ids))
    }
  })
}

@internal
pub fn apply_fulfillment_order_split_inputs(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(FulfillmentOrderSplitInput),
  results: List(FulfillmentOrderSplitResult),
  staged_ids: List(String),
) -> Result(
  #(
    List(FulfillmentOrderSplitResult),
    Store,
    SyntheticIdentityRegistry,
    List(String),
  ),
  Nil,
) {
  case inputs {
    [] -> Ok(#(results, store, identity, staged_ids))
    [input, ..rest] -> {
      let FulfillmentOrderSplitInput(fulfillment_order_id:, line_items:) = input
      case find_order_with_fulfillment_order(store, fulfillment_order_id) {
        None -> Error(Nil)
        Some(match) -> {
          let #(order, fulfillment_order) = match
          let #(result, next_order, next_identity) =
            apply_fulfillment_order_split(
              order,
              fulfillment_order,
              identity,
              line_items,
            )
          let next_store = store.stage_order(store, next_order)
          apply_fulfillment_order_split_inputs(
            next_store,
            next_identity,
            rest,
            list.append(results, [result]),
            list.append(staged_ids, [next_order.id]),
          )
        }
      }
    }
  }
}

@internal
pub fn apply_fulfillment_order_merge_inputs(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(FulfillmentOrderMergeInput),
  results: List(FulfillmentOrderMergeResult),
  staged_ids: List(String),
) -> Result(
  #(
    List(FulfillmentOrderMergeResult),
    Store,
    SyntheticIdentityRegistry,
    List(String),
  ),
  Nil,
) {
  case inputs {
    [] -> Ok(#(results, store, identity, staged_ids))
    [input, ..rest] -> {
      let FulfillmentOrderMergeInput(ids:) = input
      case find_fulfillment_orders_for_merge(store, ids) {
        Error(_) -> Error(Nil)
        Ok(match) -> {
          let #(order, fulfillment_orders) = match
          let #(result, next_order, next_identity) =
            apply_fulfillment_order_merge(order, fulfillment_orders, identity)
          let next_store = store.stage_order(store, next_order)
          apply_fulfillment_order_merge_inputs(
            next_store,
            next_identity,
            rest,
            list.append(results, [result]),
            list.append(staged_ids, [next_order.id]),
          )
        }
      }
    }
  }
}

@internal
pub fn apply_fulfillment_order_deadline(
  store: Store,
  identity: SyntheticIdentityRegistry,
  ids: List(String),
  deadline: String,
) -> Result(#(Store, SyntheticIdentityRegistry, List(String)), Nil) {
  case all_fulfillment_order_ids_exist(store, ids) {
    False -> Error(Nil)
    True ->
      apply_fulfillment_order_deadline_updates(
        store,
        identity,
        ids,
        deadline,
        [],
      )
  }
}

@internal
pub fn apply_fulfillment_order_deadline_updates(
  store: Store,
  identity: SyntheticIdentityRegistry,
  ids: List(String),
  deadline: String,
  staged_ids: List(String),
) -> Result(#(Store, SyntheticIdentityRegistry, List(String)), Nil) {
  case ids {
    [] -> Ok(#(store, identity, staged_ids))
    [id, ..rest] ->
      case find_order_with_fulfillment_order(store, id) {
        None -> Error(Nil)
        Some(match) -> {
          let #(order, fulfillment_order) = match
          let #(updated_at, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let updated =
            replace_captured_object_fields(fulfillment_order, [
              #("fulfillBy", CapturedString(deadline)),
              #("updatedAt", CapturedString(updated_at)),
            ])
          let next_order = replace_order_fulfillment_order(order, id, updated)
          let next_store = store.stage_order(store, next_order)
          apply_fulfillment_order_deadline_updates(
            next_store,
            next_identity,
            rest,
            deadline,
            list.append(staged_ids, [next_order.id]),
          )
        }
      }
  }
}

@internal
pub fn all_fulfillment_order_ids_exist(
  store: Store,
  ids: List(String),
) -> Bool {
  !list.any(ids, fn(id) { find_order_with_fulfillment_order(store, id) == None })
}

@internal
pub fn find_fulfillment_orders_for_merge(
  store: Store,
  ids: List(String),
) -> Result(#(OrderRecord, List(CapturedJsonValue)), Nil) {
  case ids {
    [] -> Error(Nil)
    [first_id, ..] ->
      case find_order_with_fulfillment_order(store, first_id) {
        None -> Error(Nil)
        Some(first_match) -> {
          let #(first_order, _) = first_match
          let matches =
            ids
            |> list.filter_map(fn(id) {
              find_order_with_fulfillment_order(store, id) |> option_to_result
            })
          case list.length(matches) == list.length(ids) {
            False -> Error(Nil)
            True -> {
              let same_order =
                !list.any(matches, fn(match) {
                  let #(order, _) = match
                  order.id != first_order.id
                })
              case same_order {
                False -> Error(Nil)
                True ->
                  Ok(#(
                    first_order,
                    list.map(matches, fn(match) {
                      let #(_, fulfillment_order) = match
                      fulfillment_order
                    }),
                  ))
              }
            }
          }
        }
      }
  }
}

@internal
pub fn apply_fulfillment_order_split(
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
  requested: List(RequestedFulfillmentLineItem),
) -> #(FulfillmentOrderSplitResult, OrderRecord, SyntheticIdentityRegistry) {
  let #(original_line_items, split_line_items, identity_after_line_items) =
    split_fulfillment_order_for_split(
      fulfillment_order_line_items(fulfillment_order),
      requested,
      identity,
      [],
      [],
    )
  let #(updated_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity_after_line_items)
  let original =
    fulfillment_order
    |> replace_fulfillment_order_line_items(original_line_items)
    |> replace_captured_object_fields([
      #("updatedAt", CapturedString(updated_at)),
      #(
        "supportedActions",
        captured_supported_actions(
          fulfillment_order_supported_actions_with_merge(
            captured_string_field(fulfillment_order, "status"),
            original_line_items,
            True,
          ),
        ),
      ),
    ])
  let #(remaining, next_identity) =
    build_replacement_fulfillment_order(
      identity_after_timestamp,
      fulfillment_order,
      split_line_items,
      [
        #(
          "supportedActions",
          captured_supported_actions(
            fulfillment_order_supported_actions_with_merge(
              captured_string_field(fulfillment_order, "status"),
              split_line_items,
              True,
            )
            |> list.filter(fn(action) {
              action != "SPLIT"
              || fulfillment_order_supports_split(split_line_items)
            }),
          ),
        ),
      ],
    )
  let next_order =
    replace_order_fulfillment_order_with_extras(
      order,
      captured_string_field(fulfillment_order, "id") |> option.unwrap(""),
      original,
      [remaining],
    )
  #(
    FulfillmentOrderSplitResult(
      fulfillment_order: original,
      remaining_fulfillment_order: remaining,
      replacement_fulfillment_order: None,
    ),
    next_order,
    next_identity,
  )
}

@internal
pub fn split_fulfillment_order_for_split(
  line_items: List(CapturedJsonValue),
  requested: List(RequestedFulfillmentLineItem),
  identity: SyntheticIdentityRegistry,
  original_line_items: List(CapturedJsonValue),
  split_line_items: List(CapturedJsonValue),
) -> #(
  List(CapturedJsonValue),
  List(CapturedJsonValue),
  SyntheticIdentityRegistry,
) {
  case line_items {
    [] -> #(original_line_items, split_line_items, identity)
    [line_item, ..rest] -> {
      let line_item_id =
        captured_string_field(line_item, "id") |> option.unwrap("")
      let requested_quantity =
        requested_fulfillment_quantity(line_item_id, requested)
        |> option.unwrap(0)
      let total_quantity =
        captured_int_field(line_item, "totalQuantity") |> option.unwrap(0)
      let remaining_quantity =
        captured_int_field(line_item, "remainingQuantity")
        |> option.unwrap(total_quantity)
      let selected_quantity = int.min(requested_quantity, remaining_quantity)
      let original_quantity = total_quantity - selected_quantity
      let fulfillable_quantity =
        captured_int_field(line_item, "lineItemFulfillableQuantity")
        |> option.or(captured_int_field(line_item, "lineItemQuantity"))
        |> option.unwrap(remaining_quantity)
      let next_original_line_items = case original_quantity > 0 {
        True ->
          list.append(original_line_items, [
            replace_captured_object_fields(line_item, [
              #("totalQuantity", CapturedInt(original_quantity)),
              #(
                "remainingQuantity",
                CapturedInt(int.min(
                  original_quantity,
                  int.max(0, remaining_quantity - selected_quantity),
                )),
              ),
              #(
                "lineItemFulfillableQuantity",
                CapturedInt(fulfillable_quantity),
              ),
            ]),
          ])
        False -> original_line_items
      }
      let #(next_split_line_items, next_identity) = case selected_quantity > 0 {
        False -> #(split_line_items, identity)
        True -> {
          let #(split_line_item_id, identity_after_id) = case
            original_quantity > 0
          {
            True ->
              synthetic_identity.make_synthetic_gid(
                identity,
                "FulfillmentOrderLineItem",
              )
            False -> #(line_item_id, identity)
          }
          #(
            list.append(split_line_items, [
              replace_captured_object_fields(line_item, [
                #("id", CapturedString(split_line_item_id)),
                #("totalQuantity", CapturedInt(selected_quantity)),
                #("remainingQuantity", CapturedInt(selected_quantity)),
                #(
                  "lineItemFulfillableQuantity",
                  CapturedInt(fulfillable_quantity),
                ),
              ]),
            ]),
            identity_after_id,
          )
        }
      }
      split_fulfillment_order_for_split(
        rest,
        requested,
        next_identity,
        next_original_line_items,
        next_split_line_items,
      )
    }
  }
}

@internal
pub fn apply_fulfillment_order_merge(
  order: OrderRecord,
  fulfillment_orders: List(CapturedJsonValue),
  identity: SyntheticIdentityRegistry,
) -> #(FulfillmentOrderMergeResult, OrderRecord, SyntheticIdentityRegistry) {
  let target = case fulfillment_orders {
    [first, ..] -> first
    [] -> CapturedNull
  }
  let merged_line_items =
    fulfillment_orders
    |> list.flat_map(fulfillment_order_line_items)
    |> list.fold([], merge_fulfillment_order_line_item)
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let merged =
    target
    |> replace_fulfillment_order_line_items(merged_line_items)
    |> replace_captured_object_fields([
      #("updatedAt", CapturedString(updated_at)),
      #(
        "fulfillBy",
        optional_captured_string(first_fulfillment_order_fulfill_by(
          fulfillment_orders,
        )),
      ),
      #(
        "supportedActions",
        captured_supported_actions(computed_fulfillment_order_supported_actions(
          captured_string_field(target, "status"),
          merged_line_items,
        )),
      ),
    ])
  let target_id = captured_string_field(target, "id")
  let merged_ids =
    fulfillment_orders
    |> list.filter_map(fn(fulfillment_order) {
      captured_string_field(fulfillment_order, "id") |> option_to_result
    })
  let updated_fulfillment_orders =
    order_fulfillment_orders(order.data)
    |> list.map(fn(candidate) {
      let candidate_id = captured_string_field(candidate, "id")
      case candidate_id == target_id {
        True -> merged
        False ->
          case option_is_in(candidate_id, merged_ids) {
            True ->
              candidate
              |> replace_fulfillment_order_line_items(
                zero_fulfillment_order_line_items(candidate),
              )
              |> replace_captured_object_fields([
                #("status", CapturedString("CLOSED")),
                #("updatedAt", CapturedString(updated_at)),
                #("supportedActions", CapturedArray([])),
              ])
            False -> candidate
          }
      }
    })
  let next_order =
    OrderRecord(
      ..order,
      data: replace_captured_object_fields(order.data, [
        #("fulfillmentOrders", CapturedArray(updated_fulfillment_orders)),
      ]),
    )
  #(
    FulfillmentOrderMergeResult(fulfillment_order: merged),
    next_order,
    next_identity,
  )
}

@internal
pub fn merge_fulfillment_order_line_item(
  merged: List(CapturedJsonValue),
  line_item: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  let source_id = fulfillment_source_line_item_id(line_item)
  case merged {
    [] -> [line_item]
    [first, ..rest] ->
      case fulfillment_source_line_item_id(first) == source_id {
        True -> [
          merge_fulfillment_order_line_item_values(first, line_item),
          ..rest
        ]
        False -> [first, ..merge_fulfillment_order_line_item(rest, line_item)]
      }
  }
}

@internal
pub fn merge_fulfillment_order_line_item_values(
  existing: CapturedJsonValue,
  line_item: CapturedJsonValue,
) -> CapturedJsonValue {
  let existing_remaining =
    captured_int_field(existing, "remainingQuantity") |> option.unwrap(0)
  replace_captured_object_fields(existing, [
    #(
      "totalQuantity",
      CapturedInt(
        { captured_int_field(existing, "totalQuantity") |> option.unwrap(0) }
        + { captured_int_field(line_item, "totalQuantity") |> option.unwrap(0) },
      ),
    ),
    #(
      "remainingQuantity",
      CapturedInt(
        existing_remaining
        + {
          captured_int_field(line_item, "remainingQuantity") |> option.unwrap(0)
        },
      ),
    ),
    #(
      "lineItemFulfillableQuantity",
      CapturedInt(
        captured_int_field(existing, "lineItemFulfillableQuantity")
        |> option.or(captured_int_field(
          line_item,
          "lineItemFulfillableQuantity",
        ))
        |> option.unwrap(existing_remaining),
      ),
    ),
  ])
}

@internal
pub fn first_fulfillment_order_fulfill_by(
  fulfillment_orders: List(CapturedJsonValue),
) -> Option(String) {
  fulfillment_orders
  |> list.find_map(fn(fulfillment_order) {
    captured_string_field(fulfillment_order, "fulfillBy") |> option_to_result
  })
  |> option.from_result
}

@internal
pub fn serialize_fulfillment_order_split_payload(
  field: Selection,
  results: List(FulfillmentOrderSplitResult),
  user_errors: List(#(Option(List(String)), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "fulfillmentOrderSplits" -> #(
              key,
              json.array(results, fn(result) {
                serialize_fulfillment_order_split_result(
                  child,
                  result,
                  fragments,
                )
              }),
            )
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_field_user_error(child, error)
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
pub fn serialize_fulfillment_order_split_result(
  field: Selection,
  result: FulfillmentOrderSplitResult,
  fragments: FragmentMap,
) -> Json {
  let FulfillmentOrderSplitResult(
    fulfillment_order:,
    remaining_fulfillment_order:,
    replacement_fulfillment_order:,
  ) = result
  serialize_fulfillment_order_result_fields(
    field,
    [
      #("fulfillmentOrder", Some(fulfillment_order)),
      #("remainingFulfillmentOrder", Some(remaining_fulfillment_order)),
      #("replacementFulfillmentOrder", replacement_fulfillment_order),
    ],
    fragments,
  )
}

@internal
pub fn serialize_fulfillment_order_merge_payload(
  field: Selection,
  results: List(FulfillmentOrderMergeResult),
  user_errors: List(#(Option(List(String)), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "fulfillmentOrderMerges" -> #(
              key,
              json.array(results, fn(result) {
                let FulfillmentOrderMergeResult(fulfillment_order:) = result
                serialize_fulfillment_order_result_fields(
                  child,
                  [#("fulfillmentOrder", Some(fulfillment_order))],
                  fragments,
                )
              }),
            )
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_field_user_error(child, error)
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
pub fn serialize_fulfillment_orders_set_deadline_payload(
  field: Selection,
  success: Bool,
  user_errors: List(#(Option(List(String)), String, Option(String))),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "success" -> #(key, json.bool(success))
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_field_user_error(child, error)
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
pub fn serialize_fulfillment_order_result_fields(
  field: Selection,
  values: List(#(String, Option(CapturedJsonValue))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case find_named_captured_value(values, name.value) {
            Some(value) -> #(
              key,
              serialize_order_fulfillment_order(child, value, fragments),
            )
            None -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn fulfillment_order_bulk_invalid_id_error(
  root_name: String,
  response_key: String,
) -> Json {
  json.object([
    #("message", json.string("invalid id")),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
    #(
      "path",
      json.array(
        [
          case response_key == "" {
            True -> root_name
            False -> response_key
          },
        ],
        json.string,
      ),
    ),
  ])
}

@internal
pub fn handle_fulfillment_order_request_mutation(
  root_name: String,
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
      root_name,
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      case read_string_argument(field, "id", variables) {
        None -> #(key, json.null(), store, identity, [], [], [])
        Some(id) -> {
          case find_order_with_fulfillment_order(store, id) {
            None -> #(
              key,
              json.null(),
              store,
              identity,
              [],
              [fulfillment_order_invalid_id_error(root_name, key, id)],
              [],
            )
            Some(match) -> {
              let #(order, fulfillment_order) = match
              case root_name {
                "fulfillmentOrderSubmitFulfillmentRequest" ->
                  handle_fulfillment_order_submit_request(
                    key,
                    root_name,
                    id,
                    order,
                    fulfillment_order,
                    store,
                    identity,
                    field,
                    fragments,
                    variables,
                  )
                _ ->
                  handle_fulfillment_order_request_status(
                    key,
                    root_name,
                    id,
                    order,
                    fulfillment_order,
                    store,
                    identity,
                    field,
                    fragments,
                    variables,
                  )
              }
            }
          }
        }
      }
    }
  }
}

@internal
pub fn handle_fulfillment_order_submit_request(
  key: String,
  root_name: String,
  id: String,
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  store: Store,
  identity: SyntheticIdentityRegistry,
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
  let args = field_arguments(field, variables)
  case
    build_submit_fulfillment_request_result(identity, fulfillment_order, args)
  {
    Error(user_errors) -> #(
      key,
      serialize_submit_fulfillment_request_payload(
        field,
        None,
        None,
        None,
        user_errors,
        fragments,
      ),
      store,
      identity,
      [],
      [],
      [],
    )
    Ok(result) -> {
      let #(submitted, unsubmitted, next_identity) = result
      let next_order =
        replace_order_fulfillment_order_with_extras(
          order,
          id,
          submitted,
          case unsubmitted {
            Some(unsubmitted) -> [unsubmitted]
            None -> []
          },
        )
      let next_store = store.stage_order(store, next_order)
      let payload =
        serialize_submit_fulfillment_request_payload(
          field,
          Some(submitted),
          Some(submitted),
          unsubmitted,
          [],
          fragments,
        )
      #(key, payload, next_store, next_identity, [next_order.id], [], [
        fulfillment_order_log_draft(root_name, [id]),
      ])
    }
  }
}

@internal
pub fn handle_fulfillment_order_request_status(
  key: String,
  root_name: String,
  id: String,
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  store: Store,
  identity: SyntheticIdentityRegistry,
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
  case
    apply_fulfillment_order_request_status(
      root_name,
      order,
      fulfillment_order,
      identity,
      field_arguments(field, variables),
    )
  {
    Error(message) -> #(
      key,
      serialize_fulfillment_order_mutation_payload(
        field,
        [],
        [nullable_user_error(None, message, None)],
        fragments,
      ),
      store,
      identity,
      [],
      [],
      [],
    )
    Ok(result) -> {
      let #(updated, next_order, next_identity) = result
      let next_store = store.stage_order(store, next_order)
      #(
        key,
        serialize_fulfillment_order_mutation_payload(
          field,
          [#("fulfillmentOrder", Some(updated))],
          [],
          fragments,
        ),
        next_store,
        next_identity,
        [next_order.id],
        [],
        [fulfillment_order_log_draft(root_name, [id])],
      )
    }
  }
}

@internal
pub fn build_submit_fulfillment_request_result(
  identity: SyntheticIdentityRegistry,
  fulfillment_order: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> Result(
  #(CapturedJsonValue, Option(CapturedJsonValue), SyntheticIdentityRegistry),
  List(#(Option(List(String)), String, Option(String))),
) {
  case captured_string_field(fulfillment_order, "requestStatus") {
    Some(status) if status != "UNSUBMITTED" ->
      Error([
        inferred_nullable_user_error(
          None,
          "Cannot request fulfillment for the fulfillment order.",
        ),
      ])
    _ -> {
      let line_items = fulfillment_order_line_items(fulfillment_order)
      let requested = read_fulfillment_order_line_item_inputs(args)
      case fulfillment_request_line_items_are_valid(line_items, requested) {
        False ->
          Error([
            inferred_nullable_user_error(
              Some(["fulfillmentOrderLineItems"]),
              "Quantity must be greater than 0 and less than or equal to the remaining quantity.",
            ),
          ])
        True -> {
          let #(
            submitted_line_items,
            unsubmitted_line_items,
            identity_after_line_items,
          ) =
            split_fulfillment_request_line_items(
              identity,
              line_items,
              requested,
              [],
              [],
            )
          let #(merchant_request, identity_after_request) =
            make_fulfillment_order_merchant_request(
              identity_after_line_items,
              "FULFILLMENT_REQUEST",
              read_string(args, "message"),
              fulfillment_request_options(args),
            )
          let submitted =
            fulfillment_order
            |> replace_fulfillment_order_line_items(submitted_line_items)
            |> replace_captured_object_fields([
              #("status", CapturedString("OPEN")),
              #("requestStatus", CapturedString("SUBMITTED")),
              #(
                "merchantRequests",
                CapturedArray(
                  list.append(
                    fulfillment_order_merchant_requests(fulfillment_order),
                    [merchant_request],
                  ),
                ),
              ),
            ])
          let #(unsubmitted, next_identity) = case unsubmitted_line_items {
            [] -> #(None, identity_after_request)
            [_, ..] -> {
              let #(unsubmitted_id, identity_after_unsubmitted) =
                synthetic_identity.make_synthetic_gid(
                  identity_after_request,
                  "FulfillmentOrder",
                )
              #(
                Some(
                  fulfillment_order
                  |> replace_fulfillment_order_line_items(
                    unsubmitted_line_items,
                  )
                  |> replace_captured_object_fields([
                    #("id", CapturedString(unsubmitted_id)),
                    #("status", CapturedString("OPEN")),
                    #("requestStatus", CapturedString("UNSUBMITTED")),
                    #("merchantRequests", CapturedArray([])),
                  ]),
                ),
                identity_after_unsubmitted,
              )
            }
          }
          Ok(#(submitted, unsubmitted, next_identity))
        }
      }
    }
  }
}

@internal
pub fn fulfillment_request_line_items_are_valid(
  line_items: List(CapturedJsonValue),
  requested: List(RequestedFulfillmentLineItem),
) -> Bool {
  !list.any(requested, fn(request) {
    let RequestedFulfillmentLineItem(id:, quantity:) = request
    case quantity {
      None -> True
      Some(quantity) ->
        case find_fulfillment_order_line_item(line_items, id) {
          None -> True
          Some(line_item) -> {
            let remaining =
              captured_int_field(line_item, "remainingQuantity")
              |> option.or(captured_int_field(line_item, "totalQuantity"))
              |> option.unwrap(0)
            quantity < 1 || quantity > remaining
          }
        }
    }
  })
}

@internal
pub fn find_fulfillment_order_line_item(
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
pub fn split_fulfillment_request_line_items(
  identity: SyntheticIdentityRegistry,
  line_items: List(CapturedJsonValue),
  requested: List(RequestedFulfillmentLineItem),
  submitted: List(CapturedJsonValue),
  unsubmitted: List(CapturedJsonValue),
) -> #(
  List(CapturedJsonValue),
  List(CapturedJsonValue),
  SyntheticIdentityRegistry,
) {
  case line_items {
    [] -> #(submitted, unsubmitted, identity)
    [line_item, ..rest] -> {
      let request_all = list.is_empty(requested)
      let line_item_id =
        captured_string_field(line_item, "id") |> option.unwrap("")
      let requested_quantity = case request_all {
        True ->
          captured_int_field(line_item, "remainingQuantity")
          |> option.or(captured_int_field(line_item, "totalQuantity"))
          |> option.unwrap(0)
        False ->
          requested_fulfillment_quantity(line_item_id, requested)
          |> option.unwrap(0)
      }
      let remaining_quantity =
        captured_int_field(line_item, "remainingQuantity")
        |> option.or(captured_int_field(line_item, "totalQuantity"))
        |> option.unwrap(0)
      let selected_quantity = int.min(requested_quantity, remaining_quantity)
      let leftover_quantity = remaining_quantity - selected_quantity
      let next_submitted = case selected_quantity > 0 {
        True ->
          list.append(submitted, [
            replace_captured_object_fields(line_item, [
              #("totalQuantity", CapturedInt(selected_quantity)),
              #("remainingQuantity", CapturedInt(selected_quantity)),
              #("lineItemFulfillableQuantity", CapturedInt(selected_quantity)),
            ]),
          ])
        False -> submitted
      }
      let #(next_unsubmitted, next_identity) = case leftover_quantity > 0 {
        True -> {
          let #(unsubmitted_line_item_id, identity_after_line_item) =
            synthetic_identity.make_synthetic_gid(
              identity,
              "FulfillmentOrderLineItem",
            )
          #(
            list.append(unsubmitted, [
              replace_captured_object_fields(line_item, [
                #("id", CapturedString(unsubmitted_line_item_id)),
                #("totalQuantity", CapturedInt(leftover_quantity)),
                #("remainingQuantity", CapturedInt(leftover_quantity)),
                #("lineItemFulfillableQuantity", CapturedInt(leftover_quantity)),
              ]),
            ]),
            identity_after_line_item,
          )
        }
        False -> #(unsubmitted, identity)
      }
      split_fulfillment_request_line_items(
        next_identity,
        rest,
        requested,
        next_submitted,
        next_unsubmitted,
      )
    }
  }
}

@internal
pub fn fulfillment_request_options(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case dict.get(args, "notifyCustomer") {
    Ok(root_field.BoolVal(value)) ->
      CapturedObject([#("notify_customer", CapturedBool(value))])
    _ -> CapturedObject([])
  }
}

@internal
pub fn make_fulfillment_order_merchant_request(
  identity: SyntheticIdentityRegistry,
  kind: String,
  message: Option(String),
  request_options: CapturedJsonValue,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(
      identity,
      "FulfillmentOrderMerchantRequest",
    )
  let #(sent_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("kind", CapturedString(kind)),
      #("message", optional_captured_string(message)),
      #("requestOptions", request_options),
      #("responseData", CapturedNull),
      #("sentAt", CapturedString(sent_at)),
    ]),
    next_identity,
  )
}

@internal
pub fn apply_fulfillment_order_request_status(
  root_name: String,
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
) -> Result(
  #(CapturedJsonValue, OrderRecord, SyntheticIdentityRegistry),
  String,
) {
  case root_name {
    "fulfillmentOrderAcceptFulfillmentRequest" ->
      case captured_string_field(fulfillment_order, "requestStatus") {
        Some("SUBMITTED") ->
          Ok(update_fulfillment_order_request_status(
            order,
            fulfillment_order,
            identity,
            "IN_PROGRESS",
            "ACCEPTED",
            False,
          ))
        _ ->
          Error("Cannot accept fulfillment request for the fulfillment order.")
      }
    "fulfillmentOrderRejectFulfillmentRequest" ->
      case captured_string_field(fulfillment_order, "requestStatus") {
        Some("SUBMITTED") ->
          Ok(update_fulfillment_order_request_status(
            order,
            fulfillment_order,
            identity,
            "OPEN",
            "REJECTED",
            False,
          ))
        _ ->
          Error("Cannot reject fulfillment request for the fulfillment order.")
      }
    "fulfillmentOrderSubmitCancellationRequest" ->
      case captured_string_field(fulfillment_order, "requestStatus") {
        Some("ACCEPTED") ->
          Ok(append_fulfillment_order_cancellation_request(
            order,
            fulfillment_order,
            identity,
            read_string(args, "message"),
          ))
        _ -> Error("Cannot request cancellation for the fulfillment order.")
      }
    "fulfillmentOrderAcceptCancellationRequest" ->
      case
        captured_string_field(fulfillment_order, "requestStatus"),
        has_pending_cancellation_request(fulfillment_order)
      {
        Some("ACCEPTED"), True ->
          Ok(update_fulfillment_order_request_status(
            order,
            fulfillment_order,
            identity,
            "CLOSED",
            "CANCELLATION_ACCEPTED",
            True,
          ))
        _, _ ->
          Error("Cannot accept cancellation request for the fulfillment order.")
      }
    "fulfillmentOrderRejectCancellationRequest" ->
      case
        captured_string_field(fulfillment_order, "requestStatus"),
        has_pending_cancellation_request(fulfillment_order)
      {
        Some("ACCEPTED"), True ->
          Ok(update_fulfillment_order_request_status(
            order,
            fulfillment_order,
            identity,
            "IN_PROGRESS",
            "CANCELLATION_REJECTED",
            False,
          ))
        _, _ ->
          Error("Cannot reject cancellation request for the fulfillment order.")
      }
    _ -> Error("Unsupported fulfillment order request mutation.")
  }
}

@internal
pub fn update_fulfillment_order_request_status(
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
  status: String,
  request_status: String,
  zero_line_items: Bool,
) -> #(CapturedJsonValue, OrderRecord, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let with_line_items = case zero_line_items {
    True ->
      replace_fulfillment_order_line_items(
        fulfillment_order,
        zero_fulfillment_order_line_items(fulfillment_order),
      )
    False -> fulfillment_order
  }
  let updated =
    replace_captured_object_fields(with_line_items, [
      #("status", CapturedString(status)),
      #("requestStatus", CapturedString(request_status)),
      #("updatedAt", CapturedString(updated_at)),
    ])
  #(
    updated,
    replace_order_fulfillment_order(
      order,
      captured_string_field(fulfillment_order, "id") |> option.unwrap(""),
      updated,
    ),
    next_identity,
  )
}

@internal
pub fn append_fulfillment_order_cancellation_request(
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
  message: Option(String),
) -> #(CapturedJsonValue, OrderRecord, SyntheticIdentityRegistry) {
  let #(request, identity_after_request) =
    make_fulfillment_order_merchant_request(
      identity,
      "CANCELLATION_REQUEST",
      message,
      CapturedObject([]),
    )
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_request)
  let updated =
    replace_captured_object_fields(fulfillment_order, [
      #("updatedAt", CapturedString(updated_at)),
      #(
        "merchantRequests",
        CapturedArray(
          list.append(fulfillment_order_merchant_requests(fulfillment_order), [
            request,
          ]),
        ),
      ),
    ])
  #(
    updated,
    replace_order_fulfillment_order(
      order,
      captured_string_field(fulfillment_order, "id") |> option.unwrap(""),
      updated,
    ),
    next_identity,
  )
}

@internal
pub fn handle_fulfillment_order_lifecycle_mutation(
  root_name: String,
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
      root_name,
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      case read_string_argument(field, "id", variables) {
        None -> #(key, json.null(), store, identity, [], [], [])
        Some(id) -> {
          case root_name {
            "fulfillmentOrderReschedule" -> {
              let payload =
                serialize_fulfillment_order_mutation_payload(
                  field,
                  [],
                  [
                    inferred_nullable_user_error(
                      None,
                      "Fulfillment order must be scheduled.",
                    ),
                  ],
                  fragments,
                )
              let draft = fulfillment_order_log_draft(root_name, [id])
              #(key, payload, store, identity, [], [], [draft])
            }
            "fulfillmentOrderClose" -> {
              let payload =
                serialize_fulfillment_order_mutation_payload(
                  field,
                  [],
                  [
                    inferred_nullable_user_error(
                      None,
                      "The fulfillment order's assigned fulfillment service must be of api type",
                    ),
                  ],
                  fragments,
                )
              let draft = fulfillment_order_log_draft(root_name, [id])
              #(key, payload, store, identity, [], [], [draft])
            }
            _ ->
              case find_order_with_fulfillment_order(store, id) {
                None -> #(
                  key,
                  json.null(),
                  store,
                  identity,
                  [],
                  [fulfillment_order_invalid_id_error(root_name, key, id)],
                  [],
                )
                Some(match) -> {
                  let #(order, fulfillment_order) = match
                  case
                    root_name == "fulfillmentOrderCancel",
                    fulfillment_order_cancel_block_message(fulfillment_order)
                  {
                    True, Some(message) -> {
                      let payload =
                        serialize_fulfillment_order_mutation_payload(
                          field,
                          [],
                          [
                            nullable_user_error(
                              fulfillment_order_cancel_user_error_field(message),
                              message,
                              None,
                            ),
                          ],
                          fragments,
                        )
                      #(key, payload, store, identity, [], [], [])
                    }
                    _, _ -> {
                      case
                        root_name,
                        fulfillment_order_hold_validation_errors(
                          fulfillment_order,
                          variables,
                        )
                      {
                        "fulfillmentOrderHold", [_, ..] as user_errors -> {
                          let payload =
                            serialize_fulfillment_order_mutation_payload(
                              field,
                              [
                                #("fulfillmentHold", Some(CapturedNull)),
                                #("fulfillmentOrder", Some(CapturedNull)),
                                #(
                                  "remainingFulfillmentOrder",
                                  Some(CapturedNull),
                                ),
                              ],
                              user_errors,
                              fragments,
                            )
                          #(key, payload, store, identity, [], [], [])
                        }
                        _, _ -> {
                          let #(values, next_order, next_identity) =
                            apply_fulfillment_order_lifecycle(
                              root_name,
                              order,
                              fulfillment_order,
                              identity,
                              field,
                              variables,
                            )
                          let next_store = store.stage_order(store, next_order)
                          let payload =
                            serialize_fulfillment_order_mutation_payload(
                              field,
                              values,
                              [],
                              fragments,
                            )
                          let draft =
                            fulfillment_order_log_draft(root_name, [id])
                          #(
                            key,
                            payload,
                            next_store,
                            next_identity,
                            [next_order.id],
                            [],
                            [draft],
                          )
                        }
                      }
                    }
                  }
                }
              }
          }
        }
      }
    }
  }
}

@internal
pub fn fulfillment_order_cancel_block_message(
  fulfillment_order: CapturedJsonValue,
) -> Option(String) {
  case fulfillment_order_has_manually_reported_progress(fulfillment_order) {
    True ->
      Some(
        "Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first.",
      )
    False ->
      case fulfillment_order_cancel_allowed(fulfillment_order) {
        True -> None
        False ->
          Some(
            "Fulfillment order is not in cancelable request state and can't be canceled.",
          )
      }
  }
}

@internal
pub fn fulfillment_order_cancel_allowed(
  fulfillment_order: CapturedJsonValue,
) -> Bool {
  let status =
    captured_string_field(fulfillment_order, "status")
    |> option.unwrap("OPEN")
  let request_status =
    captured_string_field(fulfillment_order, "requestStatus")
    |> option.unwrap("UNSUBMITTED")
  list.contains(["SUBMITTED", "CANCELLATION_REQUESTED"], request_status)
  || list.contains(["OPEN", "IN_PROGRESS"], status)
}

@internal
pub fn fulfillment_order_has_manually_reported_progress(
  fulfillment_order: CapturedJsonValue,
) -> Bool {
  captured_bool_field(fulfillment_order, "__draftProxyManuallyReportedProgress")
  |> option.unwrap(False)
}

@internal
pub fn apply_fulfillment_order_lifecycle(
  root_name: String,
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  List(#(String, Option(CapturedJsonValue))),
  OrderRecord,
  SyntheticIdentityRegistry,
) {
  case root_name {
    "fulfillmentOrderHold" ->
      apply_fulfillment_order_hold(
        order,
        fulfillment_order,
        identity,
        variables,
      )
    "fulfillmentOrderReleaseHold" ->
      apply_fulfillment_order_release_hold(order, fulfillment_order, identity)
    "fulfillmentOrderMove" ->
      apply_fulfillment_order_move(
        order,
        fulfillment_order,
        identity,
        variables,
      )
    "fulfillmentOrderReportProgress" ->
      apply_fulfillment_order_status(
        "fulfillmentOrder",
        order,
        fulfillment_order,
        identity,
        "IN_PROGRESS",
      )
    "fulfillmentOrderOpen" ->
      apply_fulfillment_order_status(
        "fulfillmentOrder",
        order,
        fulfillment_order,
        identity,
        "OPEN",
      )
    "fulfillmentOrderCancel" ->
      apply_fulfillment_order_cancel(order, fulfillment_order, identity)
    _ -> {
      let id = read_string_argument(field, "id", variables) |> option.unwrap("")
      #(
        [#("fulfillmentOrder", Some(fulfillment_order))],
        replace_order_fulfillment_order(order, id, fulfillment_order),
        identity,
      )
    }
  }
}

@internal
pub fn apply_fulfillment_order_hold(
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  List(#(String, Option(CapturedJsonValue))),
  OrderRecord,
  SyntheticIdentityRegistry,
) {
  let fulfillment_hold_input =
    fulfillment_order_hold_input_from_variables(variables)
  let requested =
    read_fulfillment_order_line_item_inputs(fulfillment_hold_input)
  let #(selected_line_items, remaining_line_items, identity_after_split) =
    split_fulfillment_order_line_items(identity, fulfillment_order, requested)
  let #(hold_id, identity_after_hold) =
    synthetic_identity.make_synthetic_gid(
      identity_after_split,
      "FulfillmentHold",
    )
  let #(updated_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity_after_hold)
  let hold =
    CapturedObject([
      #("id", CapturedString(hold_id)),
      #(
        "handle",
        CapturedString(fulfillment_order_hold_handle(fulfillment_hold_input)),
      ),
      #(
        "reason",
        CapturedString(
          read_string(fulfillment_hold_input, "reason")
          |> option.unwrap("OTHER"),
        ),
      ),
      #(
        "reasonNotes",
        optional_captured_string(read_string(
          fulfillment_hold_input,
          "reasonNotes",
        )),
      ),
      #("displayReason", CapturedString("Other")),
      #("heldByRequestingApp", CapturedBool(True)),
    ])
  let fulfillment_holds =
    list.append(order_fulfillment_holds(fulfillment_order), [hold])
  let held_fulfillment_order =
    fulfillment_order
    |> replace_fulfillment_order_line_items(selected_line_items)
    |> replace_captured_object_fields([
      #("status", CapturedString("ON_HOLD")),
      #("updatedAt", CapturedString(updated_at)),
      #(
        "supportedActions",
        captured_supported_actions(fulfillment_order_hold_supported_actions(
          fulfillment_holds,
        )),
      ),
      #("fulfillmentHolds", CapturedArray(fulfillment_holds)),
    ])
  let #(remaining_fulfillment_order, next_identity) = case requested {
    [] -> #(None, identity_after_timestamp)
    [_, ..] -> {
      case remaining_line_items {
        [] -> #(None, identity_after_timestamp)
        [_, ..] -> {
          let #(replacement, identity_after_replacement) =
            build_replacement_fulfillment_order(
              identity_after_timestamp,
              fulfillment_order,
              remaining_line_items,
              [],
            )
          #(Some(replacement), identity_after_replacement)
        }
      }
    }
  }
  let next_order =
    replace_order_fulfillment_order_with_extras(
      order,
      captured_string_field(fulfillment_order, "id") |> option.unwrap(""),
      held_fulfillment_order,
      case remaining_fulfillment_order {
        Some(remaining_fulfillment_order) -> [remaining_fulfillment_order]
        None -> []
      },
    )
  #(
    [
      #("fulfillmentHold", Some(hold)),
      #("fulfillmentOrder", Some(held_fulfillment_order)),
      #("remainingFulfillmentOrder", remaining_fulfillment_order),
    ],
    next_order,
    next_identity,
  )
}

@internal
pub fn fulfillment_order_hold_supported_actions(
  holds: List(CapturedJsonValue),
) -> List(String) {
  case
    fulfillment_order_active_requesting_app_holds(holds)
    >= max_fulfillment_holds_per_api_client
  {
    True -> ["RELEASE_HOLD", "MOVE"]
    False -> ["RELEASE_HOLD", "HOLD", "MOVE"]
  }
}

@internal
pub fn apply_fulfillment_order_release_hold(
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
) -> #(
  List(#(String, Option(CapturedJsonValue))),
  OrderRecord,
  SyntheticIdentityRegistry,
) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let released_line_items = fulfillment_order_line_items(fulfillment_order)
  let target_id = captured_string_field(fulfillment_order, "id")
  let release_siblings =
    order_fulfillment_orders(order.data)
    |> list.filter(fn(candidate) {
      captured_string_field(candidate, "id") != target_id
      && captured_string_field(candidate, "status") != Some("CLOSED")
      && fulfillment_order_has_matching_line_item(
        candidate,
        released_line_items,
      )
    })
  let merged_line_items =
    merge_released_fulfillment_order_line_items(
      released_line_items,
      release_siblings,
    )
  let released =
    fulfillment_order
    |> replace_fulfillment_order_line_items(merged_line_items)
    |> replace_captured_object_fields([
      #("status", CapturedString("OPEN")),
      #("updatedAt", CapturedString(updated_at)),
      #(
        "supportedActions",
        captured_supported_actions(computed_fulfillment_order_supported_actions(
          Some("OPEN"),
          merged_line_items,
        )),
      ),
      #("fulfillmentHolds", CapturedArray([])),
    ])
  let updated_fulfillment_orders =
    order_fulfillment_orders(order.data)
    |> list.map(fn(candidate) {
      case captured_string_field(candidate, "id") == target_id {
        True -> released
        False ->
          case
            fulfillment_order_has_matching_line_item(
              candidate,
              released_line_items,
            )
          {
            True ->
              candidate
              |> replace_fulfillment_order_line_items(
                zero_fulfillment_order_line_items(candidate),
              )
              |> replace_captured_object_fields([
                #("status", CapturedString("CLOSED")),
                #("updatedAt", CapturedString(updated_at)),
                #("fulfillmentHolds", CapturedArray([])),
              ])
            False -> candidate
          }
      }
    })
  let next_order =
    OrderRecord(
      ..order,
      data: replace_captured_object_fields(order.data, [
        #("fulfillmentOrders", CapturedArray(updated_fulfillment_orders)),
      ]),
    )
  #([#("fulfillmentOrder", Some(released))], next_order, next_identity)
}

@internal
pub fn fulfillment_order_has_matching_line_item(
  fulfillment_order: CapturedJsonValue,
  line_items: List(CapturedJsonValue),
) -> Bool {
  fulfillment_order_line_items(fulfillment_order)
  |> list.any(fn(candidate) {
    let candidate_line_item_id = fulfillment_source_line_item_id(candidate)
    line_items
    |> list.any(fn(line_item) {
      candidate_line_item_id != ""
      && candidate_line_item_id == fulfillment_source_line_item_id(line_item)
    })
  })
}

@internal
pub fn merge_released_fulfillment_order_line_items(
  line_items: List(CapturedJsonValue),
  siblings: List(CapturedJsonValue),
) -> List(CapturedJsonValue) {
  line_items
  |> list.map(fn(line_item) {
    let source_line_item_id = fulfillment_source_line_item_id(line_item)
    let sibling_line_items =
      siblings
      |> list.flat_map(fulfillment_order_line_items)
      |> list.filter(fn(candidate) {
        source_line_item_id != ""
        && source_line_item_id == fulfillment_source_line_item_id(candidate)
      })
    let total_quantity =
      captured_int_field(line_item, "totalQuantity") |> option.unwrap(0)
    let remaining_quantity =
      captured_int_field(line_item, "remainingQuantity") |> option.unwrap(0)
    replace_captured_object_fields(line_item, [
      #(
        "totalQuantity",
        CapturedInt(
          total_quantity
          + sum_fulfillment_order_line_item_field(
            sibling_line_items,
            "totalQuantity",
          ),
        ),
      ),
      #(
        "remainingQuantity",
        CapturedInt(
          remaining_quantity
          + sum_fulfillment_order_line_item_field(
            sibling_line_items,
            "remainingQuantity",
          ),
        ),
      ),
      #(
        "lineItemFulfillableQuantity",
        CapturedInt(
          remaining_quantity
          + sum_fulfillment_order_line_item_field(
            sibling_line_items,
            "remainingQuantity",
          ),
        ),
      ),
    ])
  })
}

@internal
pub fn sum_fulfillment_order_line_item_field(
  line_items: List(CapturedJsonValue),
  field_name: String,
) -> Int {
  line_items
  |> list.fold(0, fn(sum, line_item) {
    let fallback =
      captured_int_field(line_item, "remainingQuantity") |> option.unwrap(0)
    sum
    + { captured_int_field(line_item, field_name) |> option.unwrap(fallback) }
  })
}

@internal
pub fn zero_fulfillment_order_line_items(
  fulfillment_order: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  fulfillment_order_line_items(fulfillment_order)
  |> list.map(fn(line_item) {
    replace_captured_object_fields(line_item, [
      #("totalQuantity", CapturedInt(0)),
      #("remainingQuantity", CapturedInt(0)),
      #("lineItemFulfillableQuantity", CapturedInt(0)),
    ])
  })
}

@internal
pub fn apply_fulfillment_order_move(
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  List(#(String, Option(CapturedJsonValue))),
  OrderRecord,
  SyntheticIdentityRegistry,
) {
  let requested = read_fulfillment_order_line_item_inputs(variables)
  let #(selected_line_items, remaining_line_items, identity_after_split) =
    split_fulfillment_order_line_items(identity, fulfillment_order, requested)
  let new_location_id = case dict.get(variables, "newLocationId") {
    Ok(root_field.StringVal(id)) -> Some(id)
    _ -> None
  }
  let assigned_location =
    CapturedObject([
      #("name", CapturedString("Shop location")),
      #("locationId", optional_captured_string(new_location_id)),
    ])
  let #(moved, identity_after_moved) =
    build_replacement_fulfillment_order(
      identity_after_split,
      fulfillment_order,
      selected_line_items,
      [#("assignedLocation", assigned_location)],
    )
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_moved)
  let original =
    fulfillment_order
    |> replace_fulfillment_order_line_items(remaining_line_items)
    |> replace_captured_object_fields([
      #("updatedAt", CapturedString(updated_at)),
    ])
  let remaining = case remaining_line_items {
    [] -> None
    [_, ..] -> Some(original)
  }
  let next_order =
    replace_order_fulfillment_order_with_extras(
      order,
      captured_string_field(fulfillment_order, "id") |> option.unwrap(""),
      case remaining {
        Some(original) -> original
        None -> moved
      },
      case remaining {
        Some(_) -> [moved]
        None -> []
      },
    )
  #(
    [
      #("movedFulfillmentOrder", Some(moved)),
      #("originalFulfillmentOrder", Some(original)),
      #("remainingFulfillmentOrder", remaining),
    ],
    next_order,
    next_identity,
  )
}

@internal
pub fn apply_fulfillment_order_status(
  payload_key: String,
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
  status: String,
) -> #(
  List(#(String, Option(CapturedJsonValue))),
  OrderRecord,
  SyntheticIdentityRegistry,
) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let updated =
    replace_captured_object_fields(fulfillment_order, case status {
      "IN_PROGRESS" -> [
        #("status", CapturedString(status)),
        #("updatedAt", CapturedString(updated_at)),
        #(
          "supportedActions",
          captured_supported_actions(
            computed_fulfillment_order_supported_actions(
              Some(status),
              fulfillment_order_line_items(fulfillment_order),
            ),
          ),
        ),
        #("__draftProxyManuallyReportedProgress", CapturedBool(True)),
      ]
      "OPEN" -> [
        #("status", CapturedString(status)),
        #("updatedAt", CapturedString(updated_at)),
        #(
          "supportedActions",
          captured_supported_actions(
            computed_fulfillment_order_supported_actions(
              Some(status),
              fulfillment_order_line_items(fulfillment_order),
            ),
          ),
        ),
        #("__draftProxyManuallyReportedProgress", CapturedBool(False)),
      ]
      _ -> [
        #("status", CapturedString(status)),
        #("updatedAt", CapturedString(updated_at)),
        #(
          "supportedActions",
          captured_supported_actions(
            computed_fulfillment_order_supported_actions(
              Some(status),
              fulfillment_order_line_items(fulfillment_order),
            ),
          ),
        ),
      ]
    })
  let replacements = case status {
    "IN_PROGRESS" -> [
      #("displayFulfillmentStatus", CapturedString("IN_PROGRESS")),
    ]
    "OPEN" -> [#("displayFulfillmentStatus", CapturedString("UNFULFILLED"))]
    _ -> []
  }
  let next_order =
    replace_order_fulfillment_order(
      OrderRecord(
        ..order,
        data: replace_captured_object_fields(order.data, replacements),
      ),
      captured_string_field(fulfillment_order, "id") |> option.unwrap(""),
      updated,
    )
  #([#(payload_key, Some(updated))], next_order, next_identity)
}

@internal
pub fn apply_fulfillment_order_cancel(
  order: OrderRecord,
  fulfillment_order: CapturedJsonValue,
  identity: SyntheticIdentityRegistry,
) -> #(
  List(#(String, Option(CapturedJsonValue))),
  OrderRecord,
  SyntheticIdentityRegistry,
) {
  let #(updated_at, identity_after_closed) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let closed =
    fulfillment_order
    |> replace_fulfillment_order_line_items([])
    |> replace_captured_object_fields([
      #("status", CapturedString("CLOSED")),
      #("updatedAt", CapturedString(updated_at)),
    ])
  let #(replacement, next_identity) =
    build_replacement_fulfillment_order(
      identity_after_closed,
      fulfillment_order,
      fulfillment_order_line_items(fulfillment_order),
      [],
    )
  let next_order =
    replace_order_fulfillment_order_with_extras(
      order,
      captured_string_field(fulfillment_order, "id") |> option.unwrap(""),
      closed,
      [replacement],
    )
  #(
    [
      #("fulfillmentOrder", Some(closed)),
      #("replacementFulfillmentOrder", Some(replacement)),
    ],
    next_order,
    next_identity,
  )
}

@internal
pub fn serialize_fulfillment_order_mutation_payload(
  field: Selection,
  values: List(#(String, Option(CapturedJsonValue))),
  user_errors: List(#(Option(List(String)), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "fulfillmentOrder"
            | "remainingFulfillmentOrder"
            | "movedFulfillmentOrder"
            | "originalFulfillmentOrder"
            | "replacementFulfillmentOrder" -> #(
              key,
              case find_named_captured_value(values, name.value) {
                Some(CapturedNull) -> json.null()
                Some(value) ->
                  serialize_order_fulfillment_order(child, value, fragments)
                None -> json.null()
              },
            )
            "fulfillmentHold" -> #(
              key,
              case find_named_captured_value(values, "fulfillmentHold") {
                Some(CapturedNull) -> json.null()
                Some(hold) ->
                  serialize_captured_selection(child, Some(hold), fragments)
                None ->
                  case find_named_captured_value(values, "fulfillmentOrder") {
                    Some(fulfillment_order) ->
                      case order_fulfillment_holds(fulfillment_order) {
                        [hold, ..] ->
                          serialize_captured_selection(
                            child,
                            Some(hold),
                            fragments,
                          )
                        [] -> json.null()
                      }
                    None -> json.null()
                  }
              },
            )
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_field_user_error(child, error)
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
pub fn serialize_submit_fulfillment_request_payload(
  field: Selection,
  original_fulfillment_order: Option(CapturedJsonValue),
  submitted_fulfillment_order: Option(CapturedJsonValue),
  unsubmitted_fulfillment_order: Option(CapturedJsonValue),
  user_errors: List(#(Option(List(String)), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "originalFulfillmentOrder" -> #(
              key,
              serialize_captured_fulfillment_order_option(
                child,
                original_fulfillment_order,
                fragments,
              ),
            )
            "submittedFulfillmentOrder" -> #(
              key,
              serialize_captured_fulfillment_order_option(
                child,
                submitted_fulfillment_order,
                fragments,
              ),
            )
            "unsubmittedFulfillmentOrder" -> #(
              key,
              serialize_captured_fulfillment_order_option(
                child,
                unsubmitted_fulfillment_order,
                fragments,
              ),
            )
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_field_user_error(child, error)
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
pub fn serialize_captured_fulfillment_order_option(
  field: Selection,
  fulfillment_order: Option(CapturedJsonValue),
  fragments: FragmentMap,
) -> Json {
  case fulfillment_order {
    Some(fulfillment_order) ->
      serialize_order_fulfillment_order(field, fulfillment_order, fragments)
    None -> json.null()
  }
}

@internal
pub fn serialize_nullable_field_user_error(
  field: Selection,
  error: #(Option(List(String)), String, Option(String)),
) -> Json {
  let #(field_path, message, code) = error
  let code_source = case code {
    Some(code) -> SrcString(code)
    None -> fulfillment_order_user_error_code(message)
  }
  project_graphql_value(
    src_object([
      #("field", user_error_field_source(field_path)),
      #("message", SrcString(message)),
      #("code", code_source),
    ]),
    selection_children(field),
    dict.new(),
  )
}

@internal
pub fn fulfillment_order_cancel_user_error_field(
  message: String,
) -> Option(List(String)) {
  case message {
    "Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first." ->
      Some(["id"])
    _ -> None
  }
}

@internal
pub fn fulfillment_order_user_error_code(message: String) -> SourceValue {
  case message {
    "Cannot cancel fulfillment order that has had progress reported. Mark as unfulfilled first." ->
      SrcString("fulfillment_order_has_manually_reported_progress")
    "Fulfillment order is not in cancelable request state and can't be canceled." ->
      SrcString("fulfillment_order_cannot_be_cancelled")
    "The handle provided for the fulfillment hold is already in use by this app for another hold on this fulfillment order." ->
      SrcString("DUPLICATE_FULFILLMENT_HOLD_HANDLE")
    "The maximum number of fulfillment holds for this fulfillment order has been reached for this app. An app can only have up to 10 holds on a single fulfillment order at any one time." ->
      SrcString("FULFILLMENT_ORDER_HOLD_LIMIT_REACHED")
    "The fulfillment order is not in a splittable state." ->
      SrcString("FULFILLMENT_ORDER_NOT_SPLITTABLE")
    "You must select at least one item to place on partial hold." ->
      SrcString("GREATER_THAN_ZERO")
    "The line item quantity is invalid." ->
      SrcString("INVALID_LINE_ITEM_QUANTITY")
    "must contain unique line item ids" ->
      SrcString("DUPLICATED_FULFILLMENT_ORDER_LINE_ITEMS")
    "Handle is too long (maximum is 64 characters)" -> SrcString("TOO_LONG")
    _ -> SrcNull
  }
}

@internal
pub fn find_named_captured_value(
  values: List(#(String, Option(CapturedJsonValue))),
  name: String,
) -> Option(CapturedJsonValue) {
  values
  |> list.find_map(fn(pair) {
    let #(key, value) = pair
    case key == name, value {
      True, Some(value) -> Ok(value)
      _, _ -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn fulfillment_order_invalid_id_error(
  root_name: String,
  response_key: String,
  id: String,
) -> Json {
  json.object([
    #("message", json.string("Invalid id: " <> id)),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
    #(
      "path",
      json.array(
        [
          case response_key == "" {
            True -> root_name
            False -> response_key
          },
        ],
        json.string,
      ),
    ),
  ])
}

@internal
pub fn fulfillment_order_log_draft(
  root_name: String,
  staged_ids: List(String),
) -> LogDraft {
  single_root_log_draft(
    root_name,
    staged_ids,
    store_types.Staged,
    "orders",
    "stage-locally",
    Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
  )
}
