//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_field_response_key, project_graphql_value,
}

import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, RequiredArgument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  captured_json_from_commit, captured_json_source, captured_string_field,
  field_arguments, find_order_with_fulfillment,
  find_order_with_fulfillment_order, inferred_user_error, json_get,
  json_get_bool, json_get_string, non_null_json, optional_captured_string,
  order_fulfillments, read_object, read_object_list, read_string,
  read_string_arg, replace_captured_object_fields, selection_children,
  serialize_user_error, user_error,
}
import shopify_draft_proxy/proxy/user_error_codes

import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CustomerRecord, type DraftOrderRecord,
  type DraftOrderVariantCatalogRecord, type OrderRecord,
  type ProductVariantRecord, CapturedArray, CapturedObject, CapturedString,
  CustomerRecord, DraftOrderRecord, DraftOrderVariantCatalogRecord, OrderRecord,
  ProductVariantRecord,
}

@internal
pub fn handle_fulfillment_mutation(
  root_name: String,
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
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let fulfillment_id = case root_name {
        "fulfillmentTrackingInfoUpdate" ->
          read_string_arg(args, "fulfillmentId")
        _ -> read_string_arg(args, "id")
      }
      case fulfillment_id {
        None -> #(key, json.null(), store, identity, [], [], [])
        Some(id) -> {
          // Pattern 2: fulfillment mutations identify only the fulfillment.
          // Hydrate the containing order first so the local mutation can stage
          // the same read-after-write order payload without forwarding the
          // supported mutation to Shopify.
          let hydrated_store =
            maybe_hydrate_order_for_fulfillment(store, id, upstream)
          case find_order_with_fulfillment(hydrated_store, id) {
            None -> {
              let payload =
                serialize_fulfillment_mutation_payload(
                  field,
                  None,
                  [
                    inferred_user_error(
                      case root_name {
                        "fulfillmentTrackingInfoUpdate" -> ["fulfillmentId"]
                        _ -> ["id"]
                      },
                      case root_name {
                        "fulfillmentTrackingInfoUpdate" ->
                          "Fulfillment does not exist."
                        _ -> "Fulfillment not found."
                      },
                    ),
                  ],
                  fragments,
                )
              #(key, payload, store, identity, [], [], [])
            }
            Some(match) -> {
              let #(order, fulfillment) = match
              case
                fulfillment_mutation_state_precondition_errors(
                  root_name,
                  fulfillment,
                )
              {
                [] -> {
                  let #(updated_fulfillment, next_identity) =
                    update_fulfillment_for_root(
                      root_name,
                      fulfillment,
                      args,
                      identity,
                    )
                  let updated_order =
                    update_order_fulfillment(order, id, updated_fulfillment)
                  let next_store = store.stage_order(store, updated_order)
                  let payload =
                    serialize_fulfillment_mutation_payload(
                      field,
                      Some(updated_fulfillment),
                      [],
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      root_name,
                      [id],
                      store_types.Staged,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally staged "
                        <> root_name
                        <> " in shopify-draft-proxy.",
                      ),
                    )
                  #(key, payload, next_store, next_identity, [order.id], [], [
                    draft,
                  ])
                }
                errors -> {
                  let payload =
                    serialize_fulfillment_mutation_payload(
                      field,
                      None,
                      errors,
                      fragments,
                    )
                  #(key, payload, store, identity, [], [], [])
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
pub fn fulfillment_mutation_state_precondition_errors(
  root_name: String,
  fulfillment: CapturedJsonValue,
) -> List(#(List(String), String, Option(String))) {
  case root_name {
    "fulfillmentTrackingInfoUpdate" ->
      case captured_string_field(fulfillment, "status") {
        Some("CANCELLED") -> [
          user_error(
            ["fulfillmentId"],
            "fulfillment_is_cancelled",
            Some(user_error_codes.invalid),
          ),
        ]
        _ -> []
      }

    "fulfillmentCancel" ->
      case
        captured_string_field(fulfillment, "status"),
        captured_string_field(fulfillment, "displayStatus")
      {
        Some("CANCELLED"), _ -> [
          user_error(
            ["id"],
            "fulfillment_cannot_be_cancelled",
            Some(user_error_codes.invalid),
          ),
        ]
        _, Some("DELIVERED") -> [
          user_error(
            ["id"],
            "fulfillment_already_delivered",
            Some(user_error_codes.invalid),
          ),
        ]
        _, _ -> []
      }

    _ -> []
  }
}

@internal
pub fn maybe_hydrate_order_for_fulfillment(
  store_in: Store,
  fulfillment_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(fulfillment_id)
    || option.is_some(find_order_with_fulfillment(store_in, fulfillment_id))
  {
    True -> store_in
    False -> {
      let query =
        "query OrdersFulfillmentHydrate($id: ID!) {
  fulfillment(id: $id) {
    id
    order { id name email phone createdAt updatedAt closed closedAt cancelledAt cancelReason displayFinancialStatus displayFulfillmentStatus note tags fulfillments { id status displayStatus createdAt updatedAt trackingInfo { number url company } } }
  }
}
"
      let variables = json.object([#("id", json.string(fulfillment_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "OrdersFulfillmentHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_order_for_fulfillment_response(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn maybe_hydrate_order_for_fulfillment_order(
  store_in: Store,
  fulfillment_order_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(fulfillment_order_id)
    || option.is_some(find_order_with_fulfillment_order(
      store_in,
      fulfillment_order_id,
    ))
  {
    True -> store_in
    False -> {
      let query =
        "query OrdersFulfillmentOrderHydrate($id: ID!) {
  fulfillmentOrder(id: $id) {
    id
    order {
      id
      name
      email
      phone
      createdAt
      updatedAt
      closed
      closedAt
      cancelledAt
      cancelReason
      displayFinancialStatus
      displayFulfillmentStatus
      note
      tags
      fulfillments(first: 5) {
        id
        status
        displayStatus
        createdAt
        updatedAt
        trackingInfo { number url company }
      }
      fulfillmentOrders(first: 10) {
        nodes {
          id
          status
          requestStatus
          lineItems(first: 10) {
            nodes {
              id
              totalQuantity
              remainingQuantity
              lineItem {
                id
                title
                quantity
                fulfillableQuantity
              }
            }
          }
        }
      }
    }
  }
}
"
      let variables = json.object([#("id", json.string(fulfillment_order_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "OrdersFulfillmentOrderHydrate",
          query,
          variables,
        )
      {
        Ok(value) ->
          hydrate_order_for_fulfillment_order_response(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn hydrate_order_for_fulfillment_order_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "fulfillmentOrder") |> option.then(non_null_json) {
        Some(fulfillment_order) ->
          case
            json_get(fulfillment_order, "order") |> option.then(non_null_json)
          {
            Some(order) ->
              case order_record_from_json(order) {
                Ok(record) -> store.upsert_base_orders(store_in, [record])
                Error(_) -> store_in
              }
            None -> store_in
          }
        None -> store_in
      }
    None -> store_in
  }
}

@internal
pub fn hydrate_order_for_fulfillment_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "fulfillment") |> option.then(non_null_json) {
        Some(fulfillment) ->
          case json_get(fulfillment, "order") |> option.then(non_null_json) {
            Some(order) ->
              case order_record_from_json(order) {
                Ok(record) -> store.upsert_base_orders(store_in, [record])
                Error(_) -> store_in
              }
            None -> store_in
          }
        None -> store_in
      }
    None -> store_in
  }
}

@internal
pub fn order_record_from_json(
  value: commit.JsonValue,
) -> Result(OrderRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(OrderRecord(id: id, cursor: None, data: captured_json_from_commit(value)))
}

@internal
pub fn draft_order_record_from_json(
  value: commit.JsonValue,
) -> Result(DraftOrderRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(DraftOrderRecord(
    id: id,
    cursor: None,
    data: captured_json_from_commit(value),
  ))
}

@internal
pub fn maybe_hydrate_draft_order_by_id(
  store_in: Store,
  draft_order_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(draft_order_id)
    || option.is_some(store.get_draft_order_by_id(store_in, draft_order_id))
  {
    True -> store_in
    False -> {
      let query =
        "query OrdersDraftOrderHydrate($id: ID!) {
  draftOrder(id: $id) { id name status ready email taxExempt taxesIncluded reserveInventoryUntil paymentTerms invoiceUrl note tags customAttributes { key value } customer { id email displayName } billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip phone } shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip phone } shippingLine { title code custom originalPriceSet { shopMoney { amount currencyCode } } discountedPriceSet { shopMoney { amount currencyCode } } } appliedDiscount { title description value valueType amountSet { shopMoney { amount currencyCode } } } subtotalPriceSet { shopMoney { amount currencyCode } } totalDiscountsSet { shopMoney { amount currencyCode } } totalShippingPriceSet { shopMoney { amount currencyCode } } totalPriceSet { shopMoney { amount currencyCode } } totalQuantityOfLineItems lineItems { nodes { id title name quantity sku variantTitle custom requiresShipping taxable customAttributes { key value } appliedDiscount { title description value valueType amountSet { shopMoney { amount currencyCode } } } originalUnitPriceSet { shopMoney { amount currencyCode } } originalTotalSet { shopMoney { amount currencyCode } } discountedTotalSet { shopMoney { amount currencyCode } } totalDiscountSet { shopMoney { amount currencyCode } } variant { id title sku } } } order { id email customer { id email displayName } currentTotalPriceSet { shopMoney { amount currencyCode } } totalPriceSet { shopMoney { amount currencyCode } } lineItems { nodes { id title name quantity sku variantTitle originalUnitPriceSet { shopMoney { amount currencyCode } } variant { id title sku } } } } }
}
"
      let variables = json.object([#("id", json.string(draft_order_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "OrdersDraftOrderHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_draft_order_by_id_response(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn hydrate_draft_order_by_id_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "draftOrder") |> option.then(non_null_json) {
        Some(draft_order) ->
          case draft_order_record_from_json(draft_order) {
            Ok(record) -> store.upsert_base_draft_orders(store_in, [record])
            Error(_) -> store_in
          }
        None -> store_in
      }
    None -> store_in
  }
}

@internal
pub fn maybe_hydrate_draft_order_variant_catalog_from_input(
  store_in: Store,
  input: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  read_object_list(input, "lineItems")
  |> list.fold(store_in, fn(current_store, line_item) {
    case read_string(line_item, "variantId") {
      Some(variant_id) ->
        maybe_hydrate_draft_order_variant_catalog(
          current_store,
          variant_id,
          upstream,
        )
      None -> current_store
    }
  })
}

@internal
pub fn maybe_hydrate_draft_order_customer_from_input(
  store_in: Store,
  input: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  let customer_id =
    read_object(input, "purchasingEntity")
    |> option.then(fn(entity) { read_string(entity, "customerId") })
  case customer_id {
    Some(id) -> maybe_hydrate_customer_by_id(store_in, id, upstream)
    None -> store_in
  }
}

@internal
pub fn maybe_hydrate_customer_by_id(
  store_in: Store,
  customer_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(customer_id)
    || option.is_some(store.get_effective_customer_by_id(store_in, customer_id))
  {
    True -> store_in
    False -> {
      let query =
        "query OrdersDraftOrderCustomerHydrate($id: ID!) {
  customer(id: $id) { id email displayName firstName lastName }
}
"
      let variables = json.object([#("id", json.string(customer_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "OrdersDraftOrderCustomerHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_customer_by_id_response(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn hydrate_customer_by_id_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  let customer =
    json_get(value, "data")
    |> option.then(fn(data) {
      json_get(data, "customer")
      |> option.or(json_get(data, "node"))
      |> option.then(non_null_json)
    })
  case customer {
    Some(customer) ->
      case customer_record_from_json(customer) {
        Ok(record) -> store.upsert_base_customers(store_in, [record])
        Error(_) -> store_in
      }
    None -> store_in
  }
}

@internal
pub fn customer_record_from_json(
  value: commit.JsonValue,
) -> Result(CustomerRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  Ok(CustomerRecord(
    id: id,
    first_name: json_get_string(value, "firstName"),
    last_name: json_get_string(value, "lastName"),
    display_name: json_get_string(value, "displayName"),
    email: json_get_string(value, "email"),
    legacy_resource_id: None,
    locale: None,
    note: None,
    can_delete: None,
    verified_email: None,
    data_sale_opt_out: False,
    tax_exempt: None,
    tax_exemptions: [],
    state: None,
    tags: [],
    number_of_orders: None,
    amount_spent: None,
    default_email_address: None,
    default_phone_number: None,
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    account_activation_token: None,
    created_at: None,
    updated_at: None,
  ))
}

@internal
pub fn maybe_hydrate_product_variant_by_id(
  store_in: Store,
  variant_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(variant_id)
    || option.is_some(store.get_effective_variant_by_id(store_in, variant_id))
  {
    True -> store_in
    False -> {
      let query =
        "query OrdersProductVariantHydrate($id: ID!) {
  productVariant(id: $id) { id title sku price product { id title } }
}
"
      let variables = json.object([#("id", json.string(variant_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "OrdersProductVariantHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_product_variant_by_id_response(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn hydrate_product_variant_by_id_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  let variant =
    json_get(value, "data")
    |> option.then(fn(data) {
      json_get(data, "productVariant")
      |> option.or(json_get(data, "node"))
      |> option.then(non_null_json)
    })
  case variant {
    Some(variant) ->
      case product_variant_record_from_json(variant) {
        Ok(record) -> store.upsert_base_product_variants(store_in, [record])
        Error(_) -> store_in
      }
    None -> store_in
  }
}

@internal
pub fn product_variant_record_from_json(
  value: commit.JsonValue,
) -> Result(ProductVariantRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  let product_id =
    json_get(value, "product")
    |> option.then(fn(product) { json_get_string(product, "id") })
    |> option.unwrap("")
  let product_title =
    json_get(value, "product")
    |> option.then(fn(product) { json_get_string(product, "title") })
  let title =
    product_title
    |> option.or(json_get_string(value, "title"))
    |> option.unwrap("Variant")
  Ok(ProductVariantRecord(
    id: id,
    product_id: product_id,
    title: title,
    sku: json_get_string(value, "sku"),
    barcode: None,
    price: json_get_string(value, "price"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: None,
    selected_options: [],
    media_ids: [],
    inventory_item: None,
    contextual_pricing: None,
    cursor: None,
  ))
}

@internal
pub fn maybe_hydrate_draft_order_variant_catalog(
  store_in: Store,
  variant_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(variant_id)
    || option.is_some(store.get_draft_order_variant_catalog_by_id(
      store_in,
      variant_id,
    ))
    || option.is_some(store.get_effective_variant_by_id(store_in, variant_id))
  {
    True -> store_in
    False -> {
      let query =
        "query OrdersDraftOrderVariantHydrate($id: ID!) {
  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }
}
"
      let variables = json.object([#("id", json.string(variant_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "OrdersDraftOrderVariantHydrate",
          query,
          variables,
        )
      {
        Ok(value) ->
          hydrate_draft_order_variant_catalog_response(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn hydrate_draft_order_variant_catalog_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  let variant =
    json_get(value, "data")
    |> option.then(fn(data) {
      json_get(data, "productVariant")
      |> option.or(json_get(data, "node"))
      |> option.then(non_null_json)
    })
  case variant {
    Some(variant) ->
      case draft_order_variant_catalog_from_json(variant) {
        Ok(record) ->
          store.upsert_base_draft_order_variant_catalog(store_in, [record])
        Error(_) -> store_in
      }
    None -> store_in
  }
}

@internal
pub fn draft_order_variant_catalog_from_json(
  value: commit.JsonValue,
) -> Result(DraftOrderVariantCatalogRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option.to_result(Nil))
  let product_title =
    json_get(value, "product")
    |> option.then(fn(product) { json_get_string(product, "title") })
  let variant_title = json_get_string(value, "title")
  let title =
    product_title
    |> option.or(variant_title)
    |> option.unwrap("Variant")
  let sku = json_get_string(value, "sku")
  let requires_shipping =
    json_get(value, "inventoryItem")
    |> option.then(fn(item) { json_get_bool(item, "requiresShipping") })
    |> option.unwrap(True)
  let taxable = json_get_bool(value, "taxable") |> option.unwrap(True)
  let price = json_get_string(value, "price") |> option.unwrap("0.0")
  Ok(DraftOrderVariantCatalogRecord(
    variant_id: id,
    title: title,
    name: title,
    variant_title: variant_title,
    sku: sku,
    requires_shipping: requires_shipping,
    taxable: taxable,
    unit_price: price,
    currency_code: "CAD",
  ))
}

@internal
pub fn maybe_hydrate_order_by_id(
  store_in: Store,
  order_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(order_id)
    || option.is_some(store.get_order_by_id(store_in, order_id))
  {
    True -> store_in
    False -> {
      let query =
        "query OrdersOrderHydrate($id: ID!) {
  order(id: $id) { id name email phone poNumber createdAt updatedAt closed closedAt cancelledAt cancelReason displayFinancialStatus displayFulfillmentStatus presentmentCurrencyCode paymentGatewayNames note tags customAttributes { key value } customer { id email displayName } totalOutstandingSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } totalReceivedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } currentTotalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } totalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } transactions { id kind status gateway amountSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } } refunds { id note totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } refundLineItems(first: 10) { nodes { id quantity restockType lineItem { id title } subtotalSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } } } transactions(first: 10) { nodes { id kind status gateway amountSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } } } } fulfillments { id status displayStatus createdAt updatedAt trackingInfo { number url company } } shippingLines { nodes { id title code source originalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } discountedPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } } } lineItems { nodes { id title name quantity currentQuantity sku variantTitle originalUnitPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } originalTotalSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } } variant { id title sku } } } }
}
"
      let variables = json.object([#("id", json.string(order_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "OrdersOrderHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_order_by_id_response(store_in, value)
        Error(_) -> store_in
      }
    }
  }
}

@internal
pub fn hydrate_order_by_id_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "order") |> option.then(non_null_json) {
        Some(order) ->
          case order_record_from_json(order) {
            Ok(record) -> store.upsert_base_orders(store_in, [record])
            Error(_) -> store_in
          }
        None -> store_in
      }
    None -> store_in
  }
}

@internal
pub fn update_fulfillment_for_root(
  root_name: String,
  fulfillment: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let replacements = case root_name {
    "fulfillmentTrackingInfoUpdate" -> [
      #("updatedAt", CapturedString(updated_at)),
      #("trackingInfo", tracking_info_from_args(args)),
    ]
    _ -> [
      #("updatedAt", CapturedString(updated_at)),
      #("status", CapturedString("CANCELLED")),
      #("displayStatus", CapturedString("CANCELED")),
    ]
  }
  #(replace_captured_object_fields(fulfillment, replacements), next_identity)
}

@internal
pub fn tracking_info_from_args(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case dict.get(args, "trackingInfoInput") {
    Ok(root_field.ObjectVal(input)) ->
      CapturedArray([
        CapturedObject([
          #("number", optional_captured_string(read_string(input, "number"))),
          #("url", optional_captured_string(read_string(input, "url"))),
          #("company", optional_captured_string(read_string(input, "company"))),
        ]),
      ])
    _ -> CapturedArray([])
  }
}

@internal
pub fn update_order_fulfillment(
  order: OrderRecord,
  fulfillment_id: String,
  updated_fulfillment: CapturedJsonValue,
) -> OrderRecord {
  let updated_fulfillments =
    order_fulfillments(order.data)
    |> list.map(fn(fulfillment) {
      case captured_string_field(fulfillment, "id") == Some(fulfillment_id) {
        True -> updated_fulfillment
        False -> fulfillment
      }
    })
  let display_status = case
    captured_string_field(updated_fulfillment, "status")
  {
    Some("CANCELLED") -> [
      #("displayFulfillmentStatus", CapturedString("UNFULFILLED")),
    ]
    _ -> []
  }
  let updated_data =
    order.data
    |> replace_captured_object_fields(list.append(
      [#("fulfillments", CapturedArray(updated_fulfillments))],
      display_status,
    ))
  OrderRecord(..order, data: updated_data)
}

@internal
pub fn serialize_fulfillment_mutation_payload(
  field: Selection,
  fulfillment: Option(CapturedJsonValue),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "fulfillment" -> #(key, case fulfillment {
              Some(value) ->
                project_graphql_value(
                  captured_json_source(value),
                  selection_children(child),
                  fragments,
                )
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
