//// Internal products-domain implementation split from proxy/products.gleam.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Location, type ObjectField, type Selection,
  type VariableDefinition, Argument, Directive, Field, InlineFragment, NullValue,
  ObjectField, ObjectValue, OperationDefinition, SelectionSet, StringValue,
  VariableDefinition, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, type RootFieldError, BoolVal, FloatVal, IntVal, ListVal,
  NullVal, ObjectVal, StringVal, get_field_arguments, get_root_fields,
}
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_field_value, project_graphql_value,
  serialize_connection, serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, RequiredArgument,
  build_null_argument_error, find_argument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/products/inventory_l02.{
  validate_bulk_create_inventory_quantities,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type BulkVariantUserError, type ProductUserError, BulkVariantUserError,
  ProductUserError, max_product_variants,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  has_variant_id, has_variant_option_input,
}
import shopify_draft_proxy/proxy/products/variants_l05.{
  validate_bulk_variant_option_input,
}
import shopify_draft_proxy/proxy/products/variants_l06.{
  validate_bulk_variant_scalar_input, validate_product_variant_scalar_input,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type ChannelRecord, type CollectionImageRecord,
  type CollectionRecord, type CollectionRuleRecord, type CollectionRuleSetRecord,
  type InventoryItemRecord, type InventoryLevelRecord,
  type InventoryLocationRecord, type InventoryMeasurementRecord,
  type InventoryQuantityRecord, type InventoryShipmentLineItemRecord,
  type InventoryShipmentRecord, type InventoryShipmentTrackingRecord,
  type InventoryTransferLineItemRecord,
  type InventoryTransferLocationSnapshotRecord, type InventoryTransferRecord,
  type InventoryWeightRecord, type InventoryWeightValue, type LocationRecord,
  type ProductCategoryRecord, type ProductCollectionRecord,
  type ProductFeedRecord, type ProductMediaRecord, type ProductMetafieldRecord,
  type ProductOperationRecord, type ProductOperationUserErrorRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductResourceFeedbackRecord, type ProductSeoRecord,
  type ProductVariantRecord, type ProductVariantSelectedOptionRecord,
  type PublicationRecord, type SellingPlanGroupRecord, type SellingPlanRecord,
  type ShopResourceFeedbackRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, CollectionRecord,
  CollectionRuleRecord, CollectionRuleSetRecord, InventoryItemRecord,
  InventoryLevelRecord, InventoryLocationRecord, InventoryMeasurementRecord,
  InventoryQuantityRecord, InventoryShipmentLineItemRecord,
  InventoryShipmentRecord, InventoryShipmentTrackingRecord,
  InventoryTransferLineItemRecord, InventoryTransferLocationSnapshotRecord,
  InventoryTransferRecord, InventoryWeightFloat, InventoryWeightInt,
  InventoryWeightRecord, LocationRecord, ProductCollectionRecord,
  ProductFeedRecord, ProductMediaRecord, ProductMetafieldRecord,
  ProductOperationRecord, ProductOperationUserErrorRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductResourceFeedbackRecord,
  ProductSeoRecord, ProductVariantRecord, ProductVariantSelectedOptionRecord,
  PublicationRecord, SellingPlanGroupRecord, SellingPlanRecord,
  ShopResourceFeedbackRecord,
}

@internal
pub fn product_create_variant_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  read_object_list_field(input, "variants")
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(variant_input, index) = pair
    validate_product_variant_scalar_input(variant_input, [
      "variants",
      int.to_string(index),
    ])
  })
}

@internal
pub fn validate_bulk_create_variant_batch(
  store: Store,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  retained_variant_count: Int,
) -> List(BulkVariantUserError) {
  case retained_variant_count + list.length(inputs) > max_product_variants {
    True -> [
      BulkVariantUserError(
        None,
        "You can only have a maximum of "
          <> int.to_string(max_product_variants)
          <> " variants per product",
        Some("LIMIT_EXCEEDED"),
      ),
    ]
    False ->
      inputs
      |> enumerate_items()
      |> list.flat_map(fn(pair) {
        let #(input, index) = pair
        let scalar_errors = validate_bulk_variant_scalar_input(input, index)
        let #(selected_options, option_errors) =
          validate_bulk_variant_option_input(
            store,
            product_id,
            input,
            index,
            "create",
          )
        let inventory_errors =
          validate_bulk_create_inventory_quantities(
            store,
            input,
            index,
            selected_options,
          )
        list.append(scalar_errors, list.append(option_errors, inventory_errors))
      })
  }
}

@internal
pub fn validate_bulk_update_variant_batch(
  store: Store,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  variants: List(ProductVariantRecord),
) -> List(BulkVariantUserError) {
  case inputs {
    [] -> [
      BulkVariantUserError(
        None,
        "Something went wrong, please try again.",
        None,
      ),
    ]
    _ ->
      inputs
      |> enumerate_items()
      |> list.flat_map(fn(pair) {
        let #(input, index) = pair
        case read_string_field(input, "id") {
          None -> [
            BulkVariantUserError(
              Some(["variants", int.to_string(index), "id"]),
              "Product variant is missing ID attribute",
              Some("PRODUCT_VARIANT_ID_MISSING"),
            ),
          ]
          Some(variant_id) ->
            case has_variant_id(variants, variant_id) {
              False -> [
                BulkVariantUserError(
                  Some(["variants", int.to_string(index), "id"]),
                  "Product variant does not exist",
                  Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ),
              ]
              True ->
                case dict.has_key(input, "inventoryQuantities") {
                  True -> [
                    BulkVariantUserError(
                      Some([
                        "variants",
                        int.to_string(index),
                        "inventoryQuantities",
                      ]),
                      "Inventory quantities can only be provided during create. To update inventory for existing variants, use inventoryAdjustQuantities.",
                      Some("NO_INVENTORY_QUANTITIES_ON_VARIANTS_UPDATE"),
                    ),
                  ]
                  False ->
                    list.append(
                      validate_bulk_variant_scalar_input(input, index),
                      case has_variant_option_input(input) {
                        True -> {
                          let #(_, errors) =
                            validate_bulk_variant_option_input(
                              store,
                              product_id,
                              input,
                              index,
                              "update",
                            )
                          errors
                        }
                        False -> []
                      },
                    )
                }
            }
        }
      })
  }
}
