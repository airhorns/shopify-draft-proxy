//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_field_response_key,
}

import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, RequiredArgument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  captured_field_or_null, captured_int_field, captured_money_amount,
  captured_money_value, captured_object_field, captured_string_field,
  draft_order_gid_tail, field_arguments, inferred_nullable_user_error,
  inferred_user_error, max_float, money_set, nonzero_float,
  optional_captured_string, parse_amount, prepend_captured_replacement, read_int,
  read_object, read_object_list, read_string, read_string_arg, read_string_list,
  replace_captured_object_fields, replace_if_present, selection_children,
  serialize_nullable_user_error, serialize_user_error, upsert_captured_fields,
}
import shopify_draft_proxy/proxy/orders/draft_order_builders.{
  build_draft_order_address, build_draft_order_from_input,
  build_draft_order_line_items, build_draft_order_purchasing_entity,
  build_draft_order_shipping_line, captured_attributes, discount_amount,
  total_quantity,
}
import shopify_draft_proxy/proxy/orders/hydration.{
  maybe_hydrate_draft_order_customer_from_input,
  maybe_hydrate_draft_order_variant_catalog_from_input,
  maybe_hydrate_order_by_id,
}

import shopify_draft_proxy/proxy/orders/serializers.{serialize_draft_order_node}

import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}

import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DraftOrderRecord, CapturedArray, CapturedBool,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, DraftOrderRecord,
}

@internal
pub fn handle_draft_order_create(
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
          // Pattern 2: draftOrderCreate stays local, but real variant IDs in
          // captured inputs need a narrow upstream variant/catalog hydration.
          let hydrated_store =
            maybe_hydrate_draft_order_variant_catalog_from_input(
              store,
              input,
              upstream,
            )
            |> maybe_hydrate_draft_order_customer_from_input(input, upstream)
          let user_errors =
            validate_draft_order_create_input(hydrated_store, input)
          case user_errors {
            [] -> {
              let #(draft_order, next_identity) =
                build_draft_order_from_input(hydrated_store, identity, input)
              let next_store =
                store.stage_draft_order(hydrated_store, draft_order)
              let payload =
                serialize_draft_order_mutation_payload(
                  field,
                  Some(draft_order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "draftOrderCreate",
                  [draft_order.id],
                  store_types.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged draftOrderCreate in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, next_identity, [draft_order.id], [], [
                draft,
              ])
            }
            _ -> {
              let payload =
                serialize_draft_order_nullable_error_payload(
                  field,
                  None,
                  user_errors,
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "draftOrderCreate",
                  [],
                  store_types.Failed,
                  "orders",
                  "stage-locally",
                  Some("Locally rejected draftOrderCreate validation branch."),
                )
              #(key, payload, store, identity, [], [], [draft])
            }
          }
        }
        _ -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

@internal
pub fn handle_draft_order_create_from_order(
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
      "draftOrderCreateFromOrder",
      [RequiredArgument(name: "orderId", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case read_string_arg(args, "orderId") {
        Some(order_id) -> {
          // Pattern 2: createFromOrder needs the source order read from the
          // cassette/upstream, then stages the new draft locally.
          let hydrated_store =
            maybe_hydrate_order_by_id(store, order_id, upstream)
          case find_order_source_by_id(hydrated_store, order_id) {
            Some(source) -> {
              let #(order, source_draft_order) = source
              let #(draft_order, next_identity) =
                build_draft_order_from_order(
                  hydrated_store,
                  identity,
                  order,
                  source_draft_order,
                )
              let next_store =
                store.stage_draft_order(hydrated_store, draft_order)
              let payload =
                serialize_draft_order_mutation_payload(
                  field,
                  Some(draft_order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "draftOrderCreateFromOrder",
                  [draft_order.id],
                  store_types.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged draftOrderCreateFromOrder in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, next_identity, [draft_order.id], [], [
                draft,
              ])
            }
            None -> {
              case store.get_order_by_id(hydrated_store, order_id) {
                Some(order) -> {
                  let empty_source =
                    DraftOrderRecord(
                      id: "",
                      cursor: None,
                      data: CapturedObject([]),
                    )
                  let #(draft_order, next_identity) =
                    build_draft_order_from_order(
                      hydrated_store,
                      identity,
                      order.data,
                      empty_source,
                    )
                  let next_store =
                    store.stage_draft_order(hydrated_store, draft_order)
                  let payload =
                    serialize_draft_order_mutation_payload(
                      field,
                      Some(draft_order),
                      [],
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      "draftOrderCreateFromOrder",
                      [draft_order.id],
                      store_types.Staged,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally staged draftOrderCreateFromOrder in shopify-draft-proxy.",
                      ),
                    )
                  #(
                    key,
                    payload,
                    next_store,
                    next_identity,
                    [draft_order.id],
                    [],
                    [draft],
                  )
                }
                None -> {
                  let payload =
                    serialize_draft_order_mutation_payload(
                      field,
                      None,
                      [inferred_user_error(["orderId"], "Order does not exist")],
                      fragments,
                    )
                  #(key, payload, store, identity, [], [], [])
                }
              }
            }
          }
        }
        None -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

@internal
pub fn find_order_source_by_id(
  store: Store,
  order_id: String,
) -> Option(#(CapturedJsonValue, DraftOrderRecord)) {
  store.list_effective_draft_orders(store)
  |> list.find_map(fn(draft_order) {
    case captured_object_field(draft_order.data, "order") {
      Some(order) ->
        case captured_string_field(order, "id") {
          Some(id) if id == order_id -> Ok(#(order, draft_order))
          _ -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn build_draft_order_from_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  order: CapturedJsonValue,
  source_draft_order: DraftOrderRecord,
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(draft_order_id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "DraftOrder")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let currency_code = captured_source_order_currency(order)
  let #(line_items, next_identity) =
    build_draft_order_line_items_from_order(
      identity_after_time,
      draft_order_line_items(order),
      currency_code,
    )
  let subtotal =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum
      +. captured_money_amount(item, "originalUnitPriceSet")
      *. int.to_float(captured_int_field(item, "quantity") |> option.unwrap(0))
    })
    |> nonzero_float(captured_money_amount(order, "currentTotalPriceSet"))
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
      #(
        "invoiceUrl",
        CapturedString(
          "https://shopify-draft-proxy.local/draft_orders/"
          <> draft_order_id
          <> "/invoice",
        ),
      ),
      #("status", CapturedString("OPEN")),
      #("ready", CapturedBool(True)),
      #("email", source_order_email(order, source_draft_order.data)),
      #("note", captured_field_or_null(order, "note")),
      #("tags", captured_field_or_empty_array(order, "tags")),
      #("customer", source_order_customer(order, source_draft_order.data)),
      #("taxExempt", CapturedBool(False)),
      #("taxesIncluded", CapturedBool(False)),
      #("reserveInventoryUntil", CapturedNull),
      #("paymentTerms", CapturedNull),
      #("appliedDiscount", CapturedNull),
      #(
        "customAttributes",
        captured_field_or_empty_array(order, "customAttributes"),
      ),
      #("billingAddress", captured_field_or_null(order, "billingAddress")),
      #("shippingAddress", captured_field_or_null(order, "shippingAddress")),
      #("shippingLine", CapturedNull),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("subtotalPriceSet", money_set(subtotal, currency_code)),
      #("totalDiscountsSet", money_set(0.0, currency_code)),
      #("totalShippingPriceSet", money_set(0.0, currency_code)),
      #("totalPriceSet", money_set(subtotal, currency_code)),
      #("totalQuantityOfLineItems", CapturedInt(total_quantity(line_items))),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
  #(
    DraftOrderRecord(id: draft_order_id, cursor: None, data: data),
    next_identity,
  )
}

@internal
pub fn build_draft_order_line_items_from_order(
  identity: SyntheticIdentityRegistry,
  line_items: List(CapturedJsonValue),
  currency_code: String,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  line_items
  |> list.fold(initial, fn(acc, item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "DraftOrderLineItem",
      )
    #(
      list.append(items, [
        build_draft_order_line_item_from_order(id, item, currency_code),
      ]),
      next_identity,
    )
  })
}

@internal
pub fn build_draft_order_line_item_from_order(
  id: String,
  item: CapturedJsonValue,
  currency_code: String,
) -> CapturedJsonValue {
  let quantity = captured_int_field(item, "quantity") |> option.unwrap(0)
  let original_unit_price =
    captured_field_or_money(item, "originalUnitPriceSet", currency_code)
  let original_total =
    captured_money_value(original_unit_price) *. int.to_float(quantity)
  CapturedObject([
    #("id", CapturedString(id)),
    #("title", captured_field_or_null(item, "title")),
    #("name", captured_field_or_null(item, "title")),
    #("quantity", CapturedInt(quantity)),
    #("sku", nullable_empty_captured_string(item, "sku")),
    #("variantTitle", nullable_default_title(item)),
    #(
      "variantId",
      optional_captured_string(source_order_line_item_variant_id(item)),
    ),
    #("productId", CapturedNull),
    #("custom", CapturedBool(source_order_line_item_custom(item))),
    #("requiresShipping", CapturedBool(True)),
    #("taxable", CapturedBool(True)),
    #("customAttributes", CapturedArray([])),
    #("appliedDiscount", CapturedNull),
    #("originalUnitPriceSet", original_unit_price),
    #("originalTotalSet", money_set(original_total, currency_code)),
    #("discountedTotalSet", money_set(original_total, currency_code)),
    #("totalDiscountSet", money_set(0.0, currency_code)),
    #("variant", source_order_line_item_variant(item)),
  ])
}

@internal
pub fn captured_source_order_currency(order: CapturedJsonValue) -> String {
  captured_money_currency(order, "currentTotalPriceSet")
  |> option.or(captured_money_currency(order, "totalPriceSet"))
  |> option.or(captured_money_currency(order, "subtotalPriceSet"))
  |> option.or(first_order_line_item_currency(order))
  |> option.unwrap("CAD")
}

@internal
pub fn first_order_line_item_currency(
  order: CapturedJsonValue,
) -> Option(String) {
  order
  |> draft_order_line_items
  |> list.find_map(fn(item) {
    case captured_money_currency(item, "originalUnitPriceSet") {
      Some(currency) -> Ok(currency)
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn source_order_email(
  order: CapturedJsonValue,
  source_draft_order: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_string_field(order, "email") {
    Some(email) -> CapturedString(email)
    None ->
      case captured_object_field(order, "customer") {
        Some(customer) ->
          case captured_string_field(customer, "email") {
            Some(email) -> CapturedString(email)
            None -> captured_field_or_null(source_draft_order, "email")
          }
        None -> captured_field_or_null(source_draft_order, "email")
      }
  }
}

@internal
pub fn source_order_customer(
  order: CapturedJsonValue,
  source_draft_order: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_object_field(order, "customer") {
    Some(customer) -> customer
    None -> captured_field_or_null(source_draft_order, "customer")
  }
}

@internal
pub fn source_order_line_item_variant_id(
  item: CapturedJsonValue,
) -> Option(String) {
  case captured_object_field(item, "variant") {
    Some(variant) -> captured_string_field(variant, "id")
    None -> captured_string_field(item, "variantId")
  }
}

@internal
pub fn source_order_line_item_custom(item: CapturedJsonValue) -> Bool {
  case source_order_line_item_variant_id(item) {
    Some(_) -> False
    None -> True
  }
}

@internal
pub fn source_order_line_item_variant(
  item: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_object_field(item, "variant") {
    Some(variant) -> variant
    None ->
      case captured_string_field(item, "variantId") {
        Some(id) ->
          CapturedObject([
            #("id", CapturedString(id)),
            #("title", captured_field_or_null(item, "variantTitle")),
            #("sku", nullable_empty_captured_string(item, "sku")),
          ])
        None -> CapturedNull
      }
  }
}

@internal
pub fn validate_draft_order_create_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String, Option(String))) {
  let line_items = read_object_list(input, "lineItems")
  case line_items {
    [] -> [inferred_nullable_user_error(None, "Add at least 1 product")]
    _ -> {
      let line_item_errors =
        line_items
        |> list.index_map(fn(line_item, index) {
          validate_draft_order_create_line_item(store, line_item, index)
        })
        |> list.flatten
      list.flatten([
        validate_draft_order_create_email(input),
        validate_draft_order_create_reserve(input),
        validate_draft_order_create_payment_terms(input),
        line_item_errors,
      ])
    }
  }
}

@internal
pub fn validate_draft_order_calculate_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String, Option(String))) {
  let line_items = read_object_list(input, "lineItems")
  case line_items {
    [] -> [inferred_nullable_user_error(None, "Add at least 1 product")]
    _ -> {
      let line_item_errors =
        line_items
        |> list.index_map(fn(line_item, index) {
          validate_draft_order_create_line_item(store, line_item, index)
        })
        |> list.flatten
      list.flatten([
        validate_draft_order_create_email(input),
        validate_draft_order_create_reserve(input),
        line_item_errors,
      ])
    }
  }
}

@internal
pub fn validate_draft_order_create_email(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String, Option(String))) {
  case read_string(input, "email") {
    Some(email) ->
      case valid_email_address(email) {
        True -> []
        False -> [
          inferred_nullable_user_error(Some(["email"]), "Email is invalid"),
        ]
      }
    _ -> []
  }
}

@internal
pub fn valid_email_address(email: String) -> Bool {
  case string.contains(email, " ") {
    True -> False
    False ->
      case string.split(email, "@") {
        [local, domain] ->
          string.trim(local) != "" && string.contains(domain, ".")
        _ -> False
      }
  }
}

@internal
pub fn validate_draft_order_create_reserve(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String, Option(String))) {
  case read_string(input, "reserveInventoryUntil") {
    Some(value) ->
      case
        iso_timestamp.parse_iso(value),
        iso_timestamp.parse_iso(iso_timestamp.now_iso())
      {
        Ok(reserve_until), Ok(now) ->
          case reserve_until < now {
            True -> [
              inferred_nullable_user_error(
                None,
                "Reserve until can't be in the past",
              ),
            ]
            False -> []
          }
        _, _ -> []
      }
    _ -> []
  }
}

@internal
pub fn validate_draft_order_create_payment_terms(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(Option(List(String)), String, Option(String))) {
  case read_object(input, "paymentTerms") {
    Some(payment_terms) ->
      case read_string(payment_terms, "paymentTermsTemplateId") {
        Some(_) -> [
          inferred_nullable_user_error(
            None,
            "The user must have access to set payment terms.",
          ),
        ]
        None -> [
          inferred_nullable_user_error(
            None,
            "Payment terms template id can not be empty.",
          ),
        ]
      }
    None -> []
  }
}

@internal
pub fn validate_draft_order_create_line_item(
  store: Store,
  line_item: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(#(Option(List(String)), String, Option(String))) {
  case read_string(line_item, "variantId") {
    Some(variant_id) ->
      case store.get_draft_order_variant_catalog_by_id(store, variant_id) {
        Some(_) -> []
        None ->
          case store.get_effective_variant_by_id(store, variant_id) {
            Some(_) -> []
            None -> [
              inferred_nullable_user_error(
                None,
                "Product with ID "
                  <> draft_order_gid_tail(variant_id)
                  <> " is no longer available.",
              ),
            ]
          }
      }
    None -> validate_custom_draft_order_line_item(line_item, index)
  }
}

@internal
pub fn validate_custom_draft_order_line_item(
  line_item: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(#(Option(List(String)), String, Option(String))) {
  case read_string(line_item, "title") {
    Some(title) ->
      case string.trim(title) != "" {
        True -> validate_custom_draft_order_line_item_values(line_item, index)
        False -> [
          inferred_nullable_user_error(None, "Merchandise title is empty."),
        ]
      }
    _ -> [inferred_nullable_user_error(None, "Merchandise title is empty.")]
  }
}

@internal
pub fn validate_custom_draft_order_line_item_values(
  line_item: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(#(Option(List(String)), String, Option(String))) {
  let quantity = read_int(line_item, "quantity", 1)
  case quantity < 1 {
    True -> [
      inferred_nullable_user_error(
        Some(["lineItems", int.to_string(index), "quantity"]),
        "Quantity must be greater than or equal to 1",
      ),
    ]
    False -> {
      let amount =
        read_string(line_item, "originalUnitPrice")
        |> option.unwrap("0")
        |> parse_amount
      case amount <. 0.0 {
        True -> [
          inferred_nullable_user_error(
            None,
            "Cannot send negative price for line_item",
          ),
        ]
        False -> []
      }
    }
  }
}

@internal
pub fn serialize_draft_order_nullable_error_payload(
  field: Selection,
  draft_order: Option(DraftOrderRecord),
  user_errors: List(#(Option(List(String)), String, Option(String))),
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
                serialize_nullable_user_error(child, error)
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
pub fn serialize_draft_order_mutation_payload(
  field: Selection,
  draft_order: Option(DraftOrderRecord),
  user_errors: List(#(List(String), String, Option(String))),
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

@internal
pub fn build_updated_draft_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  draft_order: DraftOrderRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(updated_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let currency_code = captured_order_currency(draft_order.data)
  let #(line_items, next_identity) = case dict.has_key(input, "lineItems") {
    True ->
      build_draft_order_line_items(
        store,
        identity_after_time,
        read_object_list(input, "lineItems"),
      )
    False -> #(draft_order_line_items(draft_order.data), identity_after_time)
  }
  let replacements =
    []
    |> replace_if_present(
      input,
      "email",
      optional_captured_string(read_string(input, "email")),
    )
    |> replace_if_present(
      input,
      "note",
      optional_captured_string(read_string(input, "note")),
    )
    |> replace_if_present(
      input,
      "tags",
      CapturedArray(
        read_string_list(input, "tags")
        |> list.sort(by: string.compare)
        |> list.map(CapturedString),
      ),
    )
    |> replace_if_present(
      input,
      "customAttributes",
      captured_attributes(read_object_list(input, "customAttributes")),
    )
    |> replace_if_present(
      input,
      "billingAddress",
      build_draft_order_address(read_object(input, "billingAddress")),
    )
    |> replace_if_present(
      input,
      "shippingAddress",
      build_draft_order_address(read_object(input, "shippingAddress")),
    )
    |> replace_if_present(
      input,
      "shippingLine",
      build_draft_order_shipping_line(read_object(input, "shippingLine")),
    )
    |> replace_if_present(
      input,
      "purchasingEntity",
      build_draft_order_purchasing_entity(read_object(input, "purchasingEntity")),
    )
    |> prepend_captured_replacement("updatedAt", CapturedString(updated_at))
    |> prepend_captured_replacement(
      "lineItems",
      CapturedObject([#("nodes", CapturedArray(line_items))]),
    )
  let updated_data =
    draft_order.data
    |> replace_captured_object_fields(replacements)
    |> recalculate_draft_order_totals(currency_code)
  #(DraftOrderRecord(..draft_order, data: updated_data), next_identity)
}

@internal
pub fn duplicate_draft_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  draft_order: DraftOrderRecord,
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(draft_order_id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "DraftOrder")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let currency_code = captured_order_currency(draft_order.data)
  let #(line_items, next_identity) =
    duplicate_draft_order_line_items(
      identity_after_time,
      draft_order_line_items(draft_order.data),
      currency_code,
    )
  let data =
    draft_order.data
    |> replace_captured_object_fields([
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
      #(
        "invoiceUrl",
        CapturedString(
          "https://shopify-draft-proxy.local/draft_orders/"
          <> draft_order_id
          <> "/invoice",
        ),
      ),
      #("orderId", CapturedNull),
      #("completedAt", CapturedNull),
      #("status", CapturedString("OPEN")),
      #("ready", CapturedBool(True)),
      #("taxExempt", CapturedBool(False)),
      #("reserveInventoryUntil", CapturedNull),
      #("paymentTerms", CapturedNull),
      #("appliedDiscount", CapturedNull),
      #("shippingLine", CapturedNull),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
    ])
    |> recalculate_draft_order_totals(currency_code)
  #(
    DraftOrderRecord(id: draft_order_id, cursor: None, data: data),
    next_identity,
  )
}

@internal
pub fn duplicate_draft_order_line_items(
  identity: SyntheticIdentityRegistry,
  line_items: List(CapturedJsonValue),
  currency_code: String,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  line_items
  |> list.fold(initial, fn(acc, item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "DraftOrderLineItem",
      )
    #(
      list.append(items, [
        duplicate_draft_order_line_item(id, item, currency_code),
      ]),
      next_identity,
    )
  })
}

@internal
pub fn duplicate_draft_order_line_item(
  id: String,
  item: CapturedJsonValue,
  currency_code: String,
) -> CapturedJsonValue {
  let quantity = captured_int_field(item, "quantity") |> option.unwrap(0)
  let original_total = case captured_object_field(item, "originalTotalSet") {
    Some(total) -> total
    None ->
      money_set(
        captured_money_amount(item, "originalUnitPriceSet")
          *. int.to_float(quantity),
        currency_code,
      )
  }
  item
  |> replace_captured_object_fields([
    #("id", CapturedString(id)),
    #("appliedDiscount", CapturedNull),
    #("discountedTotalSet", original_total),
    #("totalDiscountSet", money_set(0.0, currency_code)),
  ])
}

@internal
pub fn complete_draft_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  draft_order: DraftOrderRecord,
  source_name: Option(String),
  payment_pending: Bool,
) -> #(DraftOrderRecord, SyntheticIdentityRegistry) {
  let #(completed_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(order, next_identity) =
    build_order_from_completed_draft_order(
      store,
      identity_after_time,
      draft_order,
      completed_at,
      source_name,
      payment_pending,
    )
  let order_id = captured_string_field(order, "id")
  let data =
    draft_order.data
    |> replace_captured_object_fields([
      #("status", CapturedString("COMPLETED")),
      #("ready", CapturedBool(True)),
      #("completedAt", CapturedString(completed_at)),
      #("updatedAt", CapturedString(completed_at)),
      #("orderId", optional_captured_string(order_id)),
      #("order", order),
    ])
  #(DraftOrderRecord(..draft_order, data: data), next_identity)
}

@internal
pub fn build_order_from_completed_draft_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  draft_order: DraftOrderRecord,
  completed_at: String,
  source_name: Option(String),
  payment_pending: Bool,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(order_id, identity_after_order) =
    synthetic_identity.make_synthetic_gid(identity, "Order")
  let #(line_items, next_identity) =
    build_order_line_items_from_draft_order(
      identity_after_order,
      draft_order_line_items(draft_order.data),
    )
  let currency_code = captured_order_currency(draft_order.data)
  let payment_gateway_names = case payment_pending {
    True -> []
    False -> [CapturedString("manual")]
  }
  let financial_status = case payment_pending {
    True -> "PENDING"
    False -> "PAID"
  }
  #(
    CapturedObject([
      #("id", CapturedString(order_id)),
      #(
        "name",
        CapturedString("#" <> int.to_string(completed_order_count(store) + 1)),
      ),
      #("createdAt", CapturedString(completed_at)),
      #("updatedAt", CapturedString(completed_at)),
      #("email", captured_field_or_null(draft_order.data, "email")),
      #("phone", CapturedNull),
      #("poNumber", CapturedNull),
      #("closed", CapturedBool(False)),
      #("closedAt", CapturedNull),
      #("cancelledAt", CapturedNull),
      #("cancelReason", CapturedNull),
      #("sourceName", normalized_completed_order_source_name(source_name)),
      #("paymentGatewayNames", CapturedArray(payment_gateway_names)),
      #("displayFinancialStatus", CapturedString(financial_status)),
      #("displayFulfillmentStatus", CapturedString("UNFULFILLED")),
      #("note", captured_field_or_null(draft_order.data, "note")),
      #("tags", captured_field_or_empty_array(draft_order.data, "tags")),
      #(
        "customAttributes",
        captured_field_or_empty_array(draft_order.data, "customAttributes"),
      ),
      #("metafields", CapturedArray([])),
      #(
        "billingAddress",
        captured_field_or_null(draft_order.data, "billingAddress"),
      ),
      #(
        "shippingAddress",
        captured_field_or_null(draft_order.data, "shippingAddress"),
      ),
      #(
        "subtotalPriceSet",
        captured_field_or_money(
          draft_order.data,
          "subtotalPriceSet",
          currency_code,
        ),
      ),
      #(
        "currentTotalPriceSet",
        captured_field_or_money(
          draft_order.data,
          "totalPriceSet",
          currency_code,
        ),
      ),
      #(
        "totalPriceSet",
        captured_field_or_money(
          draft_order.data,
          "totalPriceSet",
          currency_code,
        ),
      ),
      #(
        "totalOutstandingSet",
        money_set(
          case payment_pending {
            True -> captured_money_amount(draft_order.data, "totalPriceSet")
            False -> 0.0
          },
          currency_code,
        ),
      ),
      #("totalRefundedSet", money_set(0.0, currency_code)),
      #("totalTaxSet", money_set(0.0, currency_code)),
      #("totalDiscountsSet", money_set(0.0, currency_code)),
      #("discountCodes", CapturedArray([])),
      #("discountApplications", CapturedArray([])),
      #("taxLines", CapturedArray([])),
      #("taxesIncluded", CapturedBool(False)),
      #("customer", captured_field_or_null(draft_order.data, "customer")),
      #(
        "purchasingEntity",
        captured_field_or_null(draft_order.data, "purchasingEntity"),
      ),
      #("shippingLines", completed_order_shipping_lines(draft_order.data)),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
      #(
        "paymentTerms",
        captured_field_or_null(draft_order.data, "paymentTerms"),
      ),
      #("transactions", CapturedArray([])),
      #("refunds", CapturedArray([])),
      #("returns", CapturedArray([])),
    ]),
    next_identity,
  )
}

@internal
pub fn build_order_line_items_from_draft_order(
  identity: SyntheticIdentityRegistry,
  line_items: List(CapturedJsonValue),
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  line_items
  |> list.fold(initial, fn(acc, item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(current_identity, "LineItem")
    #(
      list.append(items, [build_order_line_item_from_draft_order(id, item)]),
      next_identity,
    )
  })
}

@internal
pub fn build_order_line_item_from_draft_order(
  id: String,
  item: CapturedJsonValue,
) -> CapturedJsonValue {
  CapturedObject([
    #("id", CapturedString(id)),
    #("title", captured_field_or_null(item, "title")),
    #("quantity", captured_field_or_int(item, "quantity", 0)),
    #("sku", nullable_empty_captured_string(item, "sku")),
    #("variantId", CapturedNull),
    #("variantTitle", nullable_default_title(item)),
    #(
      "originalUnitPriceSet",
      captured_field_or_money(
        item,
        "originalUnitPriceSet",
        captured_order_currency(item),
      ),
    ),
    #("taxLines", CapturedArray([])),
  ])
}

@internal
pub fn completed_order_shipping_lines(
  data: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_object_field(data, "shippingLine") {
    Some(CapturedObject(fields)) ->
      CapturedArray([
        CapturedObject(
          upsert_captured_fields(fields, [
            #("source", CapturedNull),
            #("taxLines", CapturedArray([])),
          ]),
        ),
      ])
    _ -> CapturedArray([])
  }
}

@internal
pub fn completed_order_count(store: Store) -> Int {
  store.list_effective_draft_orders(store)
  |> list.fold(0, fn(count, record) {
    case captured_object_field(record.data, "order") {
      Some(CapturedObject(_)) -> count + 1
      _ -> count
    }
  })
}

@internal
pub fn normalized_completed_order_source_name(
  source_name: Option(String),
) -> CapturedJsonValue {
  case source_name {
    Some(_) -> CapturedString("347082227713")
    None -> CapturedNull
  }
}

@internal
pub fn captured_field_or_empty_array(
  value: CapturedJsonValue,
  name: String,
) -> CapturedJsonValue {
  captured_object_field(value, name) |> option.unwrap(CapturedArray([]))
}

@internal
pub fn captured_field_or_money(
  value: CapturedJsonValue,
  name: String,
  currency_code: String,
) -> CapturedJsonValue {
  captured_object_field(value, name)
  |> option.unwrap(money_set(0.0, currency_code))
}

@internal
pub fn captured_field_or_int(
  value: CapturedJsonValue,
  name: String,
  fallback: Int,
) -> CapturedJsonValue {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> CapturedInt(value)
    _ -> CapturedInt(fallback)
  }
}

@internal
pub fn nullable_empty_captured_string(
  value: CapturedJsonValue,
  name: String,
) -> CapturedJsonValue {
  case captured_string_field(value, name) {
    Some("") -> CapturedNull
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

@internal
pub fn nullable_default_title(item: CapturedJsonValue) -> CapturedJsonValue {
  case captured_string_field(item, "variantTitle") {
    Some("Default Title") -> CapturedNull
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

@internal
pub fn recalculate_draft_order_totals(
  data: CapturedJsonValue,
  currency_code: String,
) -> CapturedJsonValue {
  let line_items = draft_order_line_items(data)
  let applied_discount =
    captured_object_field(data, "appliedDiscount")
    |> option.unwrap(CapturedNull)
  let shipping_line =
    captured_object_field(data, "shippingLine") |> option.unwrap(CapturedNull)
  let line_discount_total =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum +. captured_money_amount(item, "totalDiscountSet")
    })
  let discounted_line_subtotal =
    line_items
    |> list.fold(0.0, fn(sum, item) {
      sum +. draft_order_line_item_discounted_total(item)
    })
  let order_discount_total =
    discount_amount(applied_discount, discounted_line_subtotal)
  let subtotal =
    max_float(0.0, discounted_line_subtotal -. order_discount_total)
  let shipping_total = captured_money_amount(shipping_line, "originalPriceSet")
  let total_discount = line_discount_total +. order_discount_total
  let total = subtotal +. shipping_total
  data
  |> replace_captured_object_fields([
    #("subtotalPriceSet", money_set(subtotal, currency_code)),
    #("totalDiscountsSet", money_set(total_discount, currency_code)),
    #("totalShippingPriceSet", money_set(shipping_total, currency_code)),
    #("totalPriceSet", money_set(total, currency_code)),
    #("totalQuantityOfLineItems", CapturedInt(total_quantity(line_items))),
  ])
}

@internal
pub fn draft_order_line_item_discounted_total(
  item: CapturedJsonValue,
) -> Float {
  case captured_object_field(item, "discountedTotalSet") {
    Some(discounted_total) -> captured_money_value(discounted_total)
    None ->
      captured_money_amount(item, "originalUnitPriceSet")
      *. int.to_float(captured_int_field(item, "quantity") |> option.unwrap(0))
  }
}

@internal
pub fn captured_order_currency(data: CapturedJsonValue) -> String {
  captured_money_currency(data, "totalPriceSet")
  |> option.or(captured_money_currency(data, "subtotalPriceSet"))
  |> option.or(captured_money_currency(data, "totalShippingPriceSet"))
  |> option.unwrap("CAD")
}

@internal
pub fn captured_money_currency(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(money_set) ->
      case captured_object_field(money_set, "shopMoney") {
        Some(shop_money) -> captured_string_field(shop_money, "currencyCode")
        None -> None
      }
    None -> None
  }
}

@internal
pub fn draft_order_line_items(
  data: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(data, "lineItems") {
    Some(line_items) ->
      case captured_object_field(line_items, "nodes") {
        Some(CapturedArray(items)) -> items
        _ -> []
      }
    None -> []
  }
}
