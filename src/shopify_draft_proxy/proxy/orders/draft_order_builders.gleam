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
  captured_money_amount, captured_number, captured_number_field,
  captured_object_field, captured_string_field, max_float, money_set,
  optional_captured_string, parse_amount, read_bool, read_int, read_number,
  read_object, read_object_list, read_string, read_string_list,
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
pub fn build_draft_order_from_input(
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
      #(
        "purchasingEntity",
        build_draft_order_purchasing_entity(read_object(
          input,
          "purchasingEntity",
        )),
      ),
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

@internal
pub fn build_draft_order_line_items(
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

@internal
pub fn build_draft_order_line_item(
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

@internal
pub fn build_variant_draft_order_line_item(
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

@internal
pub fn build_custom_draft_order_line_item(
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

@internal
pub fn catalog_requires_shipping(
  catalog: Option(DraftOrderVariantCatalogRecord),
) -> Bool {
  case catalog {
    Some(record) -> record.requires_shipping
    None -> True
  }
}

@internal
pub fn catalog_taxable(
  catalog: Option(DraftOrderVariantCatalogRecord),
) -> Bool {
  case catalog {
    Some(record) -> record.taxable
    None -> True
  }
}

@internal
pub fn build_draft_order_customer(
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

@internal
pub fn build_draft_order_purchasing_entity(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  case input {
    Some(entity) ->
      case read_object(entity, "purchasingCompany") {
        Some(purchasing_company) ->
          CapturedObject([
            #("__typename", CapturedString("PurchasingCompany")),
            #(
              "company",
              captured_id_object(read_string(purchasing_company, "companyId")),
            ),
            #(
              "contact",
              captured_id_object(read_string(
                purchasing_company,
                "companyContactId",
              )),
            ),
            #(
              "location",
              captured_id_object(read_string(
                purchasing_company,
                "companyLocationId",
              )),
            ),
          ])
        None -> CapturedNull
      }
    None -> CapturedNull
  }
}

@internal
pub fn captured_id_object(id: Option(String)) -> CapturedJsonValue {
  case id {
    Some(id) -> CapturedObject([#("id", CapturedString(id))])
    None -> CapturedNull
  }
}

@internal
pub fn build_draft_order_address(
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

@internal
pub fn build_order_update_address(
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
        #("address2", optional_captured_string(read_string(input, "address2"))),
        #("company", optional_captured_string(read_string(input, "company"))),
        #("city", optional_captured_string(read_string(input, "city"))),
        #("province", optional_captured_string(read_string(input, "province"))),
        #(
          "provinceCode",
          optional_captured_string(read_string(input, "provinceCode")),
        ),
        #("country", optional_captured_string(read_string(input, "country"))),
        #(
          "countryCodeV2",
          optional_captured_string(
            read_string(input, "countryCodeV2")
            |> option.or(read_string(input, "countryCode")),
          ),
        ),
        #("zip", optional_captured_string(read_string(input, "zip"))),
        #("phone", optional_captured_string(read_string(input, "phone"))),
      ])
  }
}

@internal
pub fn build_draft_order_shipping_line(
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

@internal
pub fn build_draft_order_applied_discount(
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

@internal
pub fn captured_attributes(
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

@internal
pub fn discount_amount(discount: CapturedJsonValue, base: Float) -> Float {
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

@internal
pub fn draft_order_currency(
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

@internal
pub fn line_item_currency(line_items: List(CapturedJsonValue)) -> String {
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

@internal
pub fn total_quantity(line_items: List(CapturedJsonValue)) -> Int {
  line_items
  |> list.fold(0, fn(sum, item) {
    sum
    + case captured_object_field(item, "quantity") {
      Some(CapturedInt(quantity)) -> quantity
      _ -> 0
    }
  })
}
