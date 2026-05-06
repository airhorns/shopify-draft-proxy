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
import shopify_draft_proxy/proxy/products/inventory_transfers_l00.{
  find_inventory_transfer_line_item_by_item_id,
  get_inventory_transfer_by_optional_id,
  make_inventory_transfer_location_snapshot,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l01.{
  inventory_transfer_not_found_error, read_inventory_transfer_line_item_inputs,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l02.{
  inventory_transfer_delete_payload, make_inventory_transfer_line_items,
  validate_inventory_transfer_line_items,
}
import shopify_draft_proxy/proxy/products/products_l00.{pad_start_zero}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_string_field, read_string_list_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryTransferLineItemInput, type MutationFieldResult,
  type ProductUserError, InventoryTransferLineItemInput, MutationFieldResult,
  ProductUserError,
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
pub fn handle_inventory_transfer_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let transfer_id =
    graphql_helpers.read_arg_string(
      graphql_helpers.field_args(field, variables),
      "id",
    )
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_delete_payload(
          None,
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) ->
      case transfer.status == "DRAFT" {
        False ->
          mutation_result(
            key,
            inventory_transfer_delete_payload(
              None,
              [
                ProductUserError(
                  ["id"],
                  "Can't delete the transfer if it's not in the draft status.",
                  None,
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        True -> {
          let next_store =
            store.delete_staged_inventory_transfer(store, transfer.id)
          mutation_result(
            key,
            inventory_transfer_delete_payload(
              Some(transfer.id),
              [],
              field,
              fragments,
            ),
            next_store,
            identity,
            [transfer.id],
          )
        }
      }
  }
}

@internal
pub fn make_inventory_transfer_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  status: String,
) -> #(
  Option(InventoryTransferRecord),
  List(ProductUserError),
  SyntheticIdentityRegistry,
) {
  let line_item_inputs = read_inventory_transfer_line_item_inputs(input)
  let user_errors =
    validate_inventory_transfer_line_items(store, line_item_inputs)
  case user_errors {
    [] -> {
      let #(id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryTransfer")
      let #(line_items, identity_after_items) =
        make_inventory_transfer_line_items(
          store,
          identity_after_id,
          line_item_inputs,
        )
      let #(date_created, next_identity) = case
        read_string_field(input, "dateCreated")
      {
        Some(value) -> #(value, identity_after_items)
        None ->
          synthetic_identity.make_synthetic_timestamp(identity_after_items)
      }
      let transfer_index =
        list.length(store.list_effective_inventory_transfers(store)) + 1
      let transfer =
        InventoryTransferRecord(
          id: id,
          name: "#T" <> pad_start_zero(int.to_string(transfer_index), 4),
          reference_name: read_string_field(input, "referenceName"),
          status: status,
          note: read_string_field(input, "note"),
          tags: read_string_list_field(input, "tags") |> option.unwrap([]),
          date_created: date_created,
          origin: make_inventory_transfer_location_snapshot(
            store,
            read_string_field(input, "originLocationId"),
            next_identity,
          ),
          destination: make_inventory_transfer_location_snapshot(
            store,
            read_string_field(input, "destinationLocationId"),
            next_identity,
          ),
          line_items: line_items,
        )
      #(Some(transfer), [], next_identity)
    }
    errors -> #(None, errors, identity)
  }
}

@internal
pub fn make_inventory_transfer_line_items_reusing_ids(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(InventoryTransferLineItemInput),
  prior_items: List(InventoryTransferLineItemRecord),
) -> #(List(InventoryTransferLineItemRecord), SyntheticIdentityRegistry) {
  let #(items, next_identity) =
    make_inventory_transfer_line_items(store, identity, inputs)
  let items =
    list.map(items, fn(item) {
      case
        find_inventory_transfer_line_item_by_item_id(
          prior_items,
          item.inventory_item_id,
        )
      {
        Some(prior) -> InventoryTransferLineItemRecord(..item, id: prior.id)
        None -> item
      }
    })
  #(items, next_identity)
}
