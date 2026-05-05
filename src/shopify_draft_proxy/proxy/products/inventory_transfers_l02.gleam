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
import shopify_draft_proxy/proxy/products/inventory_transfers_l01.{
  make_inventory_transfer_line_item,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared_l01.{user_errors_source}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryTransferLineItemInput, type ProductUserError,
  InventoryTransferLineItemInput, ProductUserError,
} as product_types
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
pub fn validate_inventory_transfer_line_items(
  store: Store,
  inputs: List(InventoryTransferLineItemInput),
) -> List(ProductUserError) {
  inputs
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(input, index) = pair
    let path = ["input", "lineItems", int.to_string(index)]
    let item_errors = case input.inventory_item_id {
      Some(inventory_item_id) ->
        case
          store.find_effective_variant_by_inventory_item_id(
            store,
            inventory_item_id,
          )
        {
          Some(variant) -> {
            let tracked = case variant.inventory_item {
              Some(item) -> item.tracked == Some(True)
              None -> False
            }
            case tracked {
              True -> []
              False -> [
                ProductUserError(
                  list.append(path, ["inventoryItemId"]),
                  "The inventory item does not track inventory.",
                  Some("UNTRACKED_ITEM"),
                ),
              ]
            }
          }
          None -> [
            ProductUserError(
              list.append(path, ["inventoryItemId"]),
              "The inventory item can't be found.",
              Some("ITEM_NOT_FOUND"),
            ),
          ]
        }
      None -> [
        ProductUserError(
          list.append(path, ["inventoryItemId"]),
          "The inventory item can't be found.",
          Some("ITEM_NOT_FOUND"),
        ),
      ]
    }
    let quantity_errors = case input.quantity {
      Some(quantity) if quantity > 0 -> []
      _ -> [
        ProductUserError(
          list.append(path, ["quantity"]),
          "Quantity must be greater than 0.",
          Some("INVALID_QUANTITY"),
        ),
      ]
    }
    list.append(item_errors, quantity_errors)
  })
}

@internal
pub fn make_inventory_transfer_line_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(InventoryTransferLineItemInput),
) -> #(List(InventoryTransferLineItemRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(records, current_identity) = acc
      case make_inventory_transfer_line_item(store, current_identity, input) {
        #(Some(record), next_identity) -> #([record, ..records], next_identity)
        #(None, next_identity) -> #(records, next_identity)
      }
    })
  #(list.reverse(reversed), final_identity)
}

@internal
pub fn inventory_transfer_delete_payload(
  deleted_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryTransferDeletePayload")),
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}
