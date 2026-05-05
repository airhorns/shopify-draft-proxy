//// Read-only Products foundation for the Gleam port.
////
//// The module currently covers Shopify-like no-data behavior for
//// product-adjacent query roots plus the first seeded `product(id:)` detail
//// read. Stateful product lifecycle, variants, inventory, collections,
//// publications, selling plans, and metafields land in later passes before the
//// TS product runtime can be removed.

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
  type LogDraft, RequiredArgument, build_null_argument_error, find_argument,
  single_root_log_draft, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
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
  InventoryItemRecord, InventoryLevelRecord, InventoryLocationRecord,
  InventoryMeasurementRecord, InventoryQuantityRecord,
  InventoryShipmentLineItemRecord, InventoryShipmentRecord,
  InventoryShipmentTrackingRecord, InventoryTransferLineItemRecord,
  InventoryTransferLocationSnapshotRecord, InventoryTransferRecord,
  InventoryWeightFloat, InventoryWeightInt, InventoryWeightRecord,
  LocationRecord, ProductCollectionRecord, ProductFeedRecord, ProductMediaRecord,
  ProductMetafieldRecord, ProductOperationRecord,
  ProductOperationUserErrorRecord, ProductOptionRecord, ProductOptionValueRecord,
  ProductRecord, ProductResourceFeedbackRecord, ProductSeoRecord,
  ProductVariantRecord, ProductVariantSelectedOptionRecord, PublicationRecord,
  SellingPlanGroupRecord, SellingPlanRecord, ShopResourceFeedbackRecord,
}

pub type ProductsError {
  ParseFailed(RootFieldError)
}

type ProductSetInventoryQuantityInput {
  ProductSetInventoryQuantityInput(
    location_id: Option(String),
    name: String,
    quantity: Int,
  )
}

const product_set_variant_limit = 2048

const product_set_option_limit = 3

const product_set_option_value_limit = 100

const product_set_file_limit = 250

const product_set_inventory_quantities_limit = 250

pub fn is_products_query_root(name: String) -> Bool {
  case name {
    "product"
    | "productByIdentifier"
    | "products"
    | "productsCount"
    | "collection"
    | "collectionByIdentifier"
    | "collectionByHandle"
    | "collections"
    | "locations"
    | "channel"
    | "channels"
    | "publication"
    | "publications"
    | "publicationsCount"
    | "publishedProductsCount"
    | "productVariant"
    | "productVariantByIdentifier"
    | "productVariants"
    | "productVariantsCount"
    | "inventoryTransfer"
    | "inventoryTransfers"
    | "inventoryShipment"
    | "inventoryItem"
    | "inventoryItems"
    | "inventoryLevel"
    | "inventoryProperties"
    | "productFeed"
    | "productFeeds"
    | "productTags"
    | "productTypes"
    | "productVendors"
    | "productSavedSearches"
    | "productResourceFeedback"
    | "productOperation"
    | "productDuplicateJob"
    | "sellingPlanGroup"
    | "sellingPlanGroups" -> True
    _ -> False
  }
}

pub fn is_products_mutation_root(name: String) -> Bool {
  case name {
    "productOptionsCreate"
    | "productCreate"
    | "productOptionUpdate"
    | "productOptionsDelete"
    | "productOptionsReorder"
    | "productChangeStatus"
    | "productDelete"
    | "productUpdate"
    | "productDuplicate"
    | "productSet"
    | "productVariantCreate"
    | "productVariantUpdate"
    | "productVariantDelete"
    | "productVariantsBulkCreate"
    | "productVariantsBulkUpdate"
    | "productVariantsBulkDelete"
    | "productVariantsBulkReorder"
    | "inventoryAdjustQuantities"
    | "inventoryActivate"
    | "inventoryDeactivate"
    | "inventoryBulkToggleActivation"
    | "inventoryItemUpdate"
    | "inventorySetQuantities"
    | "inventoryMoveQuantities"
    | "collectionAddProducts"
    | "collectionAddProductsV2"
    | "collectionRemoveProducts"
    | "collectionReorderProducts"
    | "collectionUpdate"
    | "collectionDelete"
    | "collectionCreate"
    | "productPublish"
    | "productUnpublish"
    | "publicationCreate"
    | "publicationUpdate"
    | "publicationDelete"
    | "publishablePublish"
    | "publishableUnpublish"
    | "productFeedCreate"
    | "productFeedDelete"
    | "productFullSync"
    | "productBundleCreate"
    | "productBundleUpdate"
    | "combinedListingUpdate"
    | "productVariantRelationshipBulkUpdate"
    | "productCreateMedia"
    | "productUpdateMedia"
    | "productDeleteMedia"
    | "productReorderMedia"
    | "productVariantAppendMedia"
    | "productVariantDetachMedia"
    | "bulkProductResourceFeedbackCreate"
    | "inventoryShipmentCreate"
    | "inventoryShipmentCreateInTransit"
    | "inventoryShipmentAddItems"
    | "inventoryShipmentRemoveItems"
    | "inventoryShipmentReceive"
    | "inventoryShipmentUpdateItemQuantities"
    | "inventoryShipmentSetTracking"
    | "inventoryShipmentMarkInTransit"
    | "inventoryShipmentDelete"
    | "inventoryTransferCreate"
    | "inventoryTransferCreateAsReadyToShip"
    | "inventoryTransferEdit"
    | "inventoryTransferSetItems"
    | "inventoryTransferRemoveItems"
    | "inventoryTransferMarkAsReadyToShip"
    | "inventoryTransferDuplicate"
    | "inventoryTransferCancel"
    | "inventoryTransferDelete"
    | "shopResourceFeedbackCreate"
    | "sellingPlanGroupCreate"
    | "sellingPlanGroupUpdate"
    | "sellingPlanGroupDelete"
    | "sellingPlanGroupAddProducts"
    | "sellingPlanGroupRemoveProducts"
    | "sellingPlanGroupAddProductVariants"
    | "sellingPlanGroupRemoveProductVariants"
    | "productJoinSellingPlanGroups"
    | "productLeaveSellingPlanGroups"
    | "productVariantJoinSellingPlanGroups"
    | "productVariantLeaveSellingPlanGroups"
    | "tagsAdd"
    | "tagsRemove" -> True
    _ -> False
  }
}

/// True iff any string in the request's resolved root-field arguments
/// or in the variables dict points at a product already present in
/// local state, or at a proxy-synthetic gid. This gates LiveHybrid
/// passthrough for cold upstream `product` / `productByIdentifier`
/// reads while keeping staged read-after-write flows fully local.
///
/// We must scan resolved arguments — not just variable values —
/// because callers frequently embed proxy-synthetic gids as inline
/// string literals (`product(id: "gid://shopify/Product/N?shopify-
/// draft-proxy=synthetic")`). Inline literals never appear in the
/// variables dict, so a variables-only check sends synthetic gids
/// upstream where they 404.
pub fn local_has_product_id(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  let inline_arg_values = case
    root_field.get_root_field_arguments(document, variables)
  {
    Ok(args) -> dict.values(args)
    Error(_) -> []
  }
  list.append(inline_arg_values, dict.values(variables))
  |> list.any(fn(value) { resolved_value_matches_local_product(proxy, value) })
}

fn resolved_value_matches_local_product(
  proxy: DraftProxy,
  value: ResolvedValue,
) -> Bool {
  case value {
    StringVal(id) ->
      is_proxy_synthetic_gid(id)
      || case store.get_effective_product_by_id(proxy.store, id) {
        Some(_) -> True
        None ->
          case has_effective_product_metafield_owner(proxy.store, id) {
            True -> True
            False ->
              case store.get_effective_product_by_handle(proxy.store, id) {
                Some(_) -> True
                None -> False
              }
          }
      }
    ObjectVal(fields) ->
      dict.values(fields)
      |> list.any(fn(field) {
        resolved_value_matches_local_product(proxy, field)
      })
    ListVal(items) ->
      list.any(items, fn(item) {
        resolved_value_matches_local_product(proxy, item)
      })
    _ -> False
  }
}

fn local_has_product_state(proxy: DraftProxy) -> Bool {
  !list.is_empty(store.list_effective_products(proxy.store))
  || dict.size(proxy.store.staged_state.deleted_product_ids) > 0
}

/// Pattern 1: cold product detail and catalog reads in LiveHybrid can
/// forward the captured Shopify document verbatim. Once local product
/// state exists, stay in the local serializer so staged lifecycle
/// effects and staged deletes are observable without runtime writes.
fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "product" ->
      !local_has_product_id(proxy, document, variables)
    parse_operation.QueryOperation, "productByIdentifier" ->
      !local_has_product_id(proxy, document, variables)
    parse_operation.QueryOperation, "products" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productsCount" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "collection" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "collectionByIdentifier" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "collectionByHandle" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "collections" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "locations" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "publication" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "publications" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "publicationsCount" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "publishedProductsCount" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productVariant" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productVariantByIdentifier" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productVariants" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productVariantsCount" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "inventoryTransfer" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "inventoryTransfers" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "inventoryShipment" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "inventoryItem" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "inventoryItems" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "inventoryLevel" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "inventoryProperties" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productTags" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productTypes" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productVendors" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productSavedSearches" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productOperation" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productDuplicateJob" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "productResourceFeedback" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "channel" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "channels" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "sellingPlanGroup" ->
      !local_has_product_domain_state(proxy)
    parse_operation.QueryOperation, "sellingPlanGroups" ->
      !local_has_product_domain_state(proxy)
    _, _ -> False
  }
}

fn local_has_product_domain_state(proxy: DraftProxy) -> Bool {
  local_has_product_state(proxy)
  || !list.is_empty(store.list_effective_collections(proxy.store))
  || !list.is_empty(store.list_effective_product_variants(proxy.store))
  || !list.is_empty(store.list_effective_locations(proxy.store))
  || !list.is_empty(store.list_effective_publications(proxy.store))
  || dict.size(proxy.store.staged_state.deleted_publication_ids) > 0
  || dict.size(proxy.store.base_state.deleted_publication_ids) > 0
  || !list.is_empty(store.list_effective_channels(proxy.store))
  || !list.is_empty(store.list_effective_product_feeds(proxy.store))
  || !list.is_empty(store.list_effective_inventory_transfers(proxy.store))
  || !list.is_empty(store.list_effective_inventory_shipments(proxy.store))
  || !list.is_empty(store.list_effective_selling_plan_groups(proxy.store))
  || dict.size(proxy.store.base_state.product_operations) > 0
  || dict.size(proxy.store.staged_state.product_operations) > 0
  || dict.size(proxy.store.base_state.product_resource_feedback) > 0
  || dict.size(proxy.store.staged_state.product_resource_feedback) > 0
}

pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        document,
        variables,
      )
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case process(proxy.store, document, variables) {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #(
                        "message",
                        json.string("Failed to handle products query"),
                      ),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

pub fn handle_products_query(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, ProductsError) {
  case get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) ->
      Ok(serialize_root_fields(
        store,
        fields,
        variables,
        get_document_fragments(document),
      ))
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "product" ->
              serialize_product_root(store, field, variables, fragments)
            "productByIdentifier" ->
              serialize_product_by_identifier_root(
                store,
                field,
                variables,
                fragments,
              )
            "collection" ->
              serialize_collection_root(store, field, variables, fragments)
            "collectionByIdentifier" ->
              serialize_collection_by_identifier_root(
                store,
                field,
                variables,
                fragments,
              )
            "collectionByHandle" ->
              serialize_collection_by_handle_root(
                store,
                field,
                variables,
                fragments,
              )
            "productFeed" ->
              serialize_product_feed_root(store, field, variables, fragments)
            "productResourceFeedback" ->
              serialize_product_resource_feedback_root(
                store,
                field,
                variables,
                fragments,
              )
            "productOperation" ->
              serialize_product_operation_root(
                store,
                field,
                variables,
                fragments,
              )
            "inventoryTransfer" ->
              serialize_inventory_transfer_root(
                store,
                field,
                variables,
                fragments,
              )
            "inventoryTransfers" ->
              serialize_inventory_transfers_connection(
                store,
                field,
                variables,
                fragments,
              )
            "inventoryShipment" ->
              serialize_inventory_shipment_root(
                store,
                field,
                variables,
                fragments,
              )
            "inventoryItem" ->
              serialize_inventory_item_root(store, field, variables, fragments)
            "inventoryLevel" ->
              serialize_inventory_level_root(store, field, variables, fragments)
            "productVariant" ->
              serialize_product_variant_root(store, field, variables, fragments)
            "productVariantByIdentifier" ->
              serialize_product_variant_by_identifier_root(
                store,
                field,
                variables,
                fragments,
              )
            "products" ->
              serialize_products_connection(store, field, variables, fragments)
            "productVariants" ->
              serialize_product_variants_connection(
                store,
                field,
                variables,
                fragments,
              )
            "inventoryItems" ->
              serialize_inventory_items_connection(
                store,
                field,
                variables,
                fragments,
              )
            "inventoryProperties" ->
              serialize_inventory_properties(field, fragments)
            "productTags" ->
              serialize_string_connection(product_tags(store), field, variables)
            "productTypes" ->
              serialize_string_connection(
                product_types(store),
                field,
                variables,
              )
            "productVendors" ->
              serialize_string_connection(
                product_vendors(store),
                field,
                variables,
              )
            "collections" ->
              serialize_collections_connection(
                store,
                field,
                variables,
                fragments,
              )
            "locations" ->
              serialize_locations_connection(store, field, variables, fragments)
            "channel" ->
              serialize_channel_root(store, field, variables, fragments)
            "channels" ->
              serialize_channels_connection(store, field, variables, fragments)
            "publication" ->
              serialize_publication_root(store, field, variables, fragments)
            "publications" ->
              serialize_publications_connection(
                store,
                field,
                variables,
                fragments,
              )
            "publicationsCount" ->
              serialize_exact_count(
                field,
                list.length(store.list_effective_publications(store)),
              )
            "publishedProductsCount" ->
              serialize_exact_count(
                field,
                published_products_count_for_field(store, field, variables),
              )
            "productFeeds" ->
              serialize_product_feeds_connection(
                store,
                field,
                variables,
                fragments,
              )
            "productSavedSearches" ->
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
            "productsCount" | "productVariantsCount" ->
              serialize_exact_count(field, case name.value {
                "productsCount" ->
                  product_count_for_field(store, field, variables)
                _ -> store.get_effective_product_variant_count(store)
              })
            "productDuplicateJob" ->
              serialize_product_duplicate_job(field, variables)
            "sellingPlanGroup" ->
              serialize_selling_plan_group_root(
                store,
                field,
                variables,
                fragments,
              )
            "sellingPlanGroups" ->
              serialize_selling_plan_groups_connection(
                store,
                field,
                variables,
                fragments,
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

fn serialize_product_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_by_id(store, id) {
        Some(product) ->
          serialize_product_selection(
            store,
            product,
            field,
            variables,
            fragments,
          )
        None ->
          case has_effective_product_metafield_owner(store, id) {
            True ->
              serialize_product_metafield_owner_selection(
                store,
                id,
                field,
                variables,
              )
            False -> json.null()
          }
      }
    None -> json.null()
  }
}

pub fn serialize_product_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_product_by_id(store, id) {
    Some(product) ->
      project_graphql_value(
        product_source_with_store(store, product),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

fn serialize_product_by_identifier_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_identifier_argument(field, variables) {
    Some(identifier) ->
      case product_by_identifier(store, identifier) {
        Some(product) ->
          serialize_product_selection(
            store,
            product,
            field,
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn product_by_identifier(
  store: Store,
  identifier: Dict(String, ResolvedValue),
) -> Option(ProductRecord) {
  case read_string_field(identifier, "id") {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None ->
      case read_string_field(identifier, "handle") {
        Some(handle) -> store.get_effective_product_by_handle(store, handle)
        None -> None
      }
  }
}

fn serialize_product_selection(
  store: Store,
  product: ProductRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  let source =
    product_source_with_store_and_publication(
      store,
      product,
      selected_publication_id(selections, variables),
    )
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "metafield" -> #(
              key,
              serialize_product_metafield(
                store,
                product.id,
                selection,
                variables,
              ),
            )
            "metafields" -> #(
              key,
              serialize_product_metafields_connection(
                store,
                product.id,
                selection,
                variables,
              ),
            )
            "variants" -> #(
              key,
              serialize_product_variants_for_product_connection(
                store,
                product,
                selection,
                variables,
                fragments,
              ),
            )
            _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_product_metafield_owner_selection(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "__typename" -> #(key, json.string("Product"))
            "id" -> #(key, json.string(owner_id))
            "metafield" -> #(
              key,
              serialize_product_metafield(store, owner_id, selection, variables),
            )
            "metafields" -> #(
              key,
              serialize_product_metafields_connection(
                store,
                owner_id,
                selection,
                variables,
              ),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn has_effective_product_metafield_owner(
  store: Store,
  owner_id: String,
) -> Bool {
  case store.get_effective_metafields_by_owner_id(store, owner_id) {
    [] -> False
    _ -> True
  }
}

fn serialize_product_metafield(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let namespace = read_string_argument(field, variables, "namespace")
  let key = read_string_argument(field, variables, "key")
  let found =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.find(fn(metafield) {
      metafield.namespace == option.unwrap(namespace, "")
      && metafield.key == option.unwrap(key, "")
    })
    |> option.from_result
  case found {
    Some(metafield) ->
      metafields.serialize_metafield_selection(
        product_metafield_to_core(metafield),
        field,
        default_selected_field_options(),
      )
    None -> json.null()
  }
}

fn serialize_product_metafields_connection(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let namespace = read_string_argument(field, variables, "namespace")
  let records =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.filter(fn(metafield) {
      case namespace {
        Some(ns) -> metafield.namespace == ns
        None -> True
      }
    })
    |> list.map(product_metafield_to_core)
  metafields.serialize_metafields_connection(
    records,
    field,
    variables,
    default_selected_field_options(),
  )
}

fn product_metafield_to_core(
  record: ProductMetafieldRecord,
) -> metafields.MetafieldRecordCore {
  metafields.MetafieldRecordCore(
    id: record.id,
    namespace: record.namespace,
    key: record.key,
    type_: record.type_,
    value: record.value,
    compare_digest: record.compare_digest,
    json_value: record.json_value,
    created_at: record.created_at,
    updated_at: record.updated_at,
    owner_type: record.owner_type,
  )
}

fn serialize_collection_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_collection_by_id(store, id) {
        Some(collection) ->
          serialize_collection_object(
            store,
            collection,
            get_selected_child_fields(field, default_selected_field_options()),
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_collection_by_identifier_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_identifier_argument(field, variables) {
    Some(identifier) ->
      case collection_by_identifier(store, identifier) {
        Some(collection) ->
          serialize_collection_object(
            store,
            collection,
            get_selected_child_fields(field, default_selected_field_options()),
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_collection_by_handle_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "handle") {
    Some(handle) ->
      case store.get_effective_collection_by_handle(store, handle) {
        Some(collection) ->
          serialize_collection_object(
            store,
            collection,
            get_selected_child_fields(field, default_selected_field_options()),
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn collection_by_identifier(
  store: Store,
  identifier: Dict(String, ResolvedValue),
) -> Option(CollectionRecord) {
  case read_string_field(identifier, "id") {
    Some(id) -> store.get_effective_collection_by_id(store, id)
    None ->
      case read_string_field(identifier, "handle") {
        Some(handle) -> store.get_effective_collection_by_handle(store, handle)
        None -> None
      }
  }
}

fn serialize_collection_object(
  store: Store,
  collection: CollectionRecord,
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  serialize_collection_object_with_options(
    store,
    collection,
    selections,
    variables,
    fragments,
    None,
  )
}

pub fn serialize_collection_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_collection_by_id(store, id) {
    Some(collection) ->
      project_graphql_value(
        collection_source_with_store(store, collection),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

fn serialize_collection_object_with_options(
  store: Store,
  collection: CollectionRecord,
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  products_count_override: Option(Int),
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          serialize_collection_field(
            store,
            collection,
            selection,
            name.value,
            variables,
            fragments,
            products_count_override,
          )
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn serialize_collection_field(
  store: Store,
  collection: CollectionRecord,
  field: Selection,
  field_name: String,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  products_count_override: Option(Int),
) -> Json {
  case field_name {
    "__typename" -> json.string("Collection")
    "id" -> json.string(collection.id)
    "legacyResourceId" ->
      json.string(
        collection.legacy_resource_id
        |> option.unwrap(legacy_resource_id_from_gid(collection.id)),
      )
    "title" -> json.string(collection.title)
    "handle" -> json.string(collection.handle)
    "publishedOnCurrentPublication" | "publishedOnCurrentChannel" ->
      json.bool(collection.publication_ids != [])
    "publishedOnPublication" ->
      json.bool(case read_string_argument(field, variables, "publicationId") {
        Some(id) -> list.contains(collection.publication_ids, id)
        None -> False
      })
    "availablePublicationsCount"
    | "resourcePublicationsCount"
    | "publicationCount" ->
      serialize_exact_count(field, list.length(collection.publication_ids))
    "updatedAt" -> optional_string_json(collection.updated_at)
    "description" -> optional_string_json(collection.description)
    "descriptionHtml" -> optional_string_json(collection.description_html)
    "image" ->
      serialize_collection_image(
        collection.image,
        get_selected_child_fields(field, default_selected_field_options()),
      )
    "productsCount" ->
      serialize_exact_count(
        field,
        products_count_override
          |> option.unwrap(
            collection.products_count
            |> option.unwrap(
              list.length(store.list_effective_products_for_collection(
                store,
                collection.id,
              )),
            ),
          ),
      )
    "hasProduct" ->
      json.bool(collection_has_product(store, collection.id, field, variables))
    "sortOrder" -> optional_string_json(collection.sort_order)
    "templateSuffix" -> optional_string_json(collection.template_suffix)
    "seo" ->
      project_graphql_value(
        product_seo_source(collection.seo),
        get_selected_child_fields(field, default_selected_field_options()),
        fragments,
      )
    "ruleSet" ->
      serialize_collection_rule_set(
        collection.rule_set,
        get_selected_child_fields(field, default_selected_field_options()),
      )
    "products" ->
      serialize_collection_products_connection(
        store,
        collection,
        field,
        variables,
        fragments,
      )
    "metafield" ->
      serialize_product_metafield(store, collection.id, field, variables)
    "metafields" ->
      serialize_product_metafields_connection(
        store,
        collection.id,
        field,
        variables,
      )
    _ -> json.null()
  }
}

fn serialize_collection_image(
  image: Option(CollectionImageRecord),
  selections: List(Selection),
) -> Json {
  case image {
    None -> json.null()
    Some(image) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "id" -> optional_string_json(image.id)
                "altText" -> optional_string_json(image.alt_text)
                "url" | "src" | "originalSrc" | "transformedSrc" ->
                  optional_string_json(image.url)
                "width" -> optional_int_json(image.width)
                "height" -> optional_int_json(image.height)
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        }),
      )
  }
}

fn serialize_collection_rule_set(
  rule_set: Option(CollectionRuleSetRecord),
  selections: List(Selection),
) -> Json {
  case rule_set {
    None -> json.null()
    Some(rule_set) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "appliedDisjunctively" ->
                  json.bool(rule_set.applied_disjunctively)
                "rules" ->
                  json.array(rule_set.rules, fn(rule) {
                    serialize_collection_rule(
                      rule,
                      get_selected_child_fields(
                        selection,
                        default_selected_field_options(),
                      ),
                    )
                  })
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        }),
      )
  }
}

fn serialize_collection_rule(
  rule: CollectionRuleRecord,
  selections: List(Selection),
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "column" -> json.string(rule.column)
            "relation" -> json.string(rule.relation)
            "condition" -> json.string(rule.condition)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn serialize_collection_products_connection(
  store: Store,
  collection: CollectionRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let entries =
    store.list_effective_products_for_collection(store, collection.id)
  let window =
    paginate_connection_items(
      entries,
      field,
      variables,
      collection_product_cursor,
      default_connection_window_options(),
    )
  let has_next_page = case collection.products_count {
    Some(count) -> window.has_next_page || count > list.length(window.items)
    None -> window.has_next_page
  }
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: collection_product_cursor,
      serialize_node: fn(entry, node_field, _index) {
        let #(product, _) = entry
        project_graphql_value(
          product_source_with_store(store, product),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: False,
        prefix_cursors: False,
        include_cursors: True,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn serialize_collections_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let collections =
    filtered_collections(store, field, variables)
    |> sort_collections(field, variables)
  case collections {
    [] -> serialize_empty_connection(field, default_selected_field_options())
    _ -> {
      let get_cursor = fn(collection, _index) {
        collection_cursor_for_field(collection, field, variables)
      }
      let window =
        paginate_connection_items(
          collections,
          field,
          variables,
          get_cursor,
          default_connection_window_options(),
        )
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: window.items,
          has_next_page: window.has_next_page,
          has_previous_page: window.has_previous_page,
          get_cursor_value: get_cursor,
          serialize_node: fn(collection, node_field, _index) {
            serialize_collection_object(
              store,
              collection,
              get_selected_child_fields(
                node_field,
                default_selected_field_options(),
              ),
              variables,
              fragments,
            )
          },
          selected_field_options: default_selected_field_options(),
          page_info_options: ConnectionPageInfoOptions(
            include_inline_fragments: False,
            prefix_cursors: False,
            include_cursors: True,
            fallback_start_cursor: None,
            fallback_end_cursor: None,
          ),
        ),
      )
    }
  }
}

fn serialize_locations_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let locations = store.list_effective_locations(store)
  let window =
    paginate_connection_items(
      locations,
      field,
      variables,
      location_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: location_cursor,
      serialize_node: fn(location, node_field, _index) {
        project_graphql_value(
          location_source(location),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: False,
        prefix_cursors: False,
        include_cursors: True,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn location_cursor(location: LocationRecord, _index: Int) -> String {
  location.cursor |> option.unwrap(location.id)
}

fn location_source(location: LocationRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Location")),
    #("id", SrcString(location.id)),
    #("name", SrcString(location.name)),
  ])
}

fn serialize_publications_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let publications = store.list_effective_publications(store)
  let window =
    paginate_connection_items(
      publications,
      field,
      variables,
      publication_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: publication_cursor,
      serialize_node: fn(publication, node_field, _index) {
        project_graphql_value(
          publication_source(store, publication),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: False,
        prefix_cursors: False,
        include_cursors: True,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn publication_cursor(publication: PublicationRecord, _index: Int) -> String {
  publication.cursor |> option.unwrap("cursor:" <> publication.id)
}

fn serialize_publication_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_publication_by_id(store, id) {
        Some(publication) ->
          project_graphql_value(
            publication_source(store, publication),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_channel_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_channel_by_id(store, id) {
        Some(channel) ->
          project_graphql_value(
            channel_source(store, channel),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_channels_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let channels = store.list_effective_channels(store)
  let window =
    paginate_connection_items(
      channels,
      field,
      variables,
      channel_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: channel_cursor,
      serialize_node: fn(channel, node_field, _index) {
        project_graphql_value(
          channel_source(store, channel),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: False,
        prefix_cursors: False,
        include_cursors: True,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn channel_cursor(channel: ChannelRecord, _index: Int) -> String {
  channel.cursor |> option.unwrap("cursor:" <> channel.id)
}

fn publication_source(
  store: Store,
  publication: PublicationRecord,
) -> SourceValue {
  let channel =
    store.list_effective_channels(store)
    |> list.find(fn(channel) { channel.publication_id == Some(publication.id) })
    |> option.from_result
  let published_products =
    products_published_to_publication(store, publication.id)
  let published_collections =
    collections_published_to_publication(store, publication.id)
  src_object([
    #("__typename", SrcString("Publication")),
    #("id", SrcString(publication.id)),
    #("name", graphql_helpers.option_string_source(publication.name)),
    #("autoPublish", SrcBool(publication.auto_publish |> option.unwrap(False))),
    #(
      "supportsFuturePublishing",
      SrcBool(publication.supports_future_publishing |> option.unwrap(False)),
    ),
    #("catalog", publication_catalog_source(publication.catalog_id)),
    #("channel", optional_channel_source(store, channel)),
    #("products", publication_products_connection_source(published_products)),
    #("productsCount", count_source(list.length(published_products))),
    #("publishedProductsCount", count_source(list.length(published_products))),
    #("collectionsCount", count_source(list.length(published_collections))),
  ])
}

fn publication_catalog_source(catalog_id: Option(String)) -> SourceValue {
  case catalog_id {
    Some(id) ->
      src_object([
        #("__typename", SrcString("MarketCatalog")),
        #("id", SrcString(id)),
      ])
    None -> SrcNull
  }
}

fn optional_channel_source(
  store: Store,
  channel: Option(ChannelRecord),
) -> SourceValue {
  case channel {
    Some(channel) -> channel_source(store, channel)
    None -> SrcNull
  }
}

fn channel_source(store: Store, channel: ChannelRecord) -> SourceValue {
  let publication = case channel.publication_id {
    Some(id) -> store.get_effective_publication_by_id(store, id)
    None -> None
  }
  let product_count = case channel.publication_id {
    Some(id) -> list.length(products_published_to_publication(store, id))
    None -> 0
  }
  src_object([
    #("__typename", SrcString("Channel")),
    #("id", SrcString(channel.id)),
    #("name", graphql_helpers.option_string_source(channel.name)),
    #("handle", graphql_helpers.option_string_source(channel.handle)),
    #("publication", optional_publication_source(publication)),
    #("productsCount", count_source(product_count)),
  ])
}

fn optional_publication_source(
  publication: Option(PublicationRecord),
) -> SourceValue {
  case publication {
    Some(publication) ->
      src_object([
        #("__typename", SrcString("Publication")),
        #("id", SrcString(publication.id)),
        #("name", graphql_helpers.option_string_source(publication.name)),
      ])
    None -> SrcNull
  }
}

fn publication_products_connection_source(
  products: List(ProductRecord),
) -> SourceValue {
  let edges =
    products
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(product, index) = pair
      src_object([
        #("cursor", SrcString(product_cursor(product, index))),
        #("node", product_source(product)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #("nodes", SrcList(list.map(products, product_source))),
    #("pageInfo", connection_page_info_source(products, product_cursor)),
  ])
}

fn products_published_to_publication(
  store: Store,
  publication_id: String,
) -> List(ProductRecord) {
  store.list_effective_products(store)
  |> list.filter(fn(product) {
    product.status == "ACTIVE"
    && list.contains(product.publication_ids, publication_id)
  })
}

fn collections_published_to_publication(
  store: Store,
  publication_id: String,
) -> List(CollectionRecord) {
  store.list_effective_collections(store)
  |> list.filter(fn(collection) {
    list.contains(collection.publication_ids, publication_id)
  })
}

fn published_products_count_for_field(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "publicationId") {
    Some(publication_id) ->
      products_published_to_publication(store, publication_id) |> list.length
    None ->
      store.list_effective_products(store)
      |> list.filter(fn(product) {
        product.status == "ACTIVE" && !list.is_empty(product.publication_ids)
      })
      |> list.length
  }
}

fn serialize_product_feed_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_feed_by_id(store, id) {
        Some(feed) ->
          project_graphql_value(
            product_feed_source(feed),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_product_feeds_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let feeds = store.list_effective_product_feeds(store)
  let window =
    paginate_connection_items(
      feeds,
      field,
      variables,
      product_feed_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_feed_cursor,
      serialize_node: fn(feed, node_field, _index) {
        project_graphql_value(
          product_feed_source(feed),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn product_feed_cursor(feed: ProductFeedRecord, _index: Int) -> String {
  feed.id
}

fn product_feed_source(feed: ProductFeedRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductFeed")),
    #("id", SrcString(feed.id)),
    #("country", graphql_helpers.option_string_source(feed.country)),
    #("language", graphql_helpers.option_string_source(feed.language)),
    #("status", SrcString(feed.status)),
  ])
}

fn serialize_product_resource_feedback_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_resource_feedback(store, id) {
        Some(feedback) ->
          project_graphql_value(
            product_resource_feedback_source(feedback),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn product_resource_feedback_source(
  feedback: ProductResourceFeedbackRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductResourceFeedback")),
    #("productId", SrcString(feedback.product_id)),
    #("state", SrcString(feedback.state)),
    #("messages", SrcList(list.map(feedback.messages, SrcString))),
    #("feedbackGeneratedAt", SrcString(feedback.feedback_generated_at)),
    #("productUpdatedAt", SrcString(feedback.product_updated_at)),
  ])
}

fn shop_resource_feedback_source(
  feedback: ShopResourceFeedbackRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("AppFeedback")),
    #("state", SrcString(feedback.state)),
    #("feedbackGeneratedAt", SrcString(feedback.feedback_generated_at)),
    #(
      "messages",
      SrcList(
        list.map(feedback.messages, fn(message) {
          src_object([#("message", SrcString(message))])
        }),
      ),
    ),
    #("app", SrcNull),
    #("link", SrcNull),
  ])
}

fn serialize_inventory_transfer_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_inventory_transfer_by_id(store, id) {
        Some(transfer) ->
          project_graphql_value(
            inventory_transfer_source(store, transfer),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_inventory_transfers_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let transfers = store.list_effective_inventory_transfers(store)
  let window =
    paginate_connection_items(
      transfers,
      field,
      variables,
      inventory_transfer_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: inventory_transfer_cursor,
      serialize_node: fn(transfer, node_field, _index) {
        project_graphql_value(
          inventory_transfer_source(store, transfer),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: False,
        prefix_cursors: False,
        include_cursors: True,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

fn inventory_transfer_cursor(
  transfer: InventoryTransferRecord,
  _index: Int,
) -> String {
  "cursor:" <> transfer.id
}

fn inventory_transfer_source(
  store: Store,
  transfer: InventoryTransferRecord,
) -> SourceValue {
  let total_quantity = inventory_transfer_total_quantity(transfer)
  src_object([
    #("__typename", SrcString("InventoryTransfer")),
    #("id", SrcString(transfer.id)),
    #("name", SrcString(transfer.name)),
    #(
      "referenceName",
      graphql_helpers.option_string_source(transfer.reference_name),
    ),
    #("status", SrcString(transfer.status)),
    #("note", graphql_helpers.option_string_source(transfer.note)),
    #("tags", SrcList(list.map(transfer.tags, SrcString))),
    #("dateCreated", SrcString(transfer.date_created)),
    #("totalQuantity", SrcInt(total_quantity)),
    #("receivedQuantity", SrcInt(0)),
    #("origin", inventory_transfer_location_source(store, transfer.origin)),
    #(
      "destination",
      inventory_transfer_location_source(store, transfer.destination),
    ),
    #("lineItems", inventory_transfer_line_items_source(store, transfer)),
    #("lineItemsCount", count_source(total_quantity)),
    #("events", empty_connection_source()),
    #("shipments", empty_connection_source()),
    #("metafields", empty_connection_source()),
    #("metafield", SrcNull),
    #("hasTimelineComment", SrcBool(False)),
  ])
}

fn inventory_transfer_location_source(
  store: Store,
  snapshot: Option(InventoryTransferLocationSnapshotRecord),
) -> SourceValue {
  case snapshot {
    Some(snapshot) -> {
      let location = case snapshot.id {
        Some(id) -> {
          case store.get_effective_location_by_id(store, id) {
            Some(location) -> location_source(location)
            None -> SrcNull
          }
        }
        None -> SrcNull
      }
      src_object([
        #("__typename", SrcString("InventoryTransferLocationSnapshot")),
        #("name", SrcString(snapshot.name)),
        #("snapshottedAt", SrcString(snapshot.snapshotted_at)),
        #("location", location),
        #("address", src_object([])),
      ])
    }
    None -> SrcNull
  }
}

fn inventory_transfer_line_items_source(
  store: Store,
  transfer: InventoryTransferRecord,
) -> SourceValue {
  let edges =
    transfer.line_items
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(line_item, _) = pair
      src_object([
        #("cursor", SrcString("cursor:" <> line_item.id)),
        #(
          "node",
          inventory_transfer_line_item_source(store, transfer, line_item),
        ),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(transfer.line_items, fn(line_item) {
          inventory_transfer_line_item_source(store, transfer, line_item)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(transfer.line_items, fn(line_item, _index) {
        "cursor:" <> line_item.id
      }),
    ),
  ])
}

fn inventory_transfer_line_item_source(
  store: Store,
  transfer: InventoryTransferRecord,
  line_item: InventoryTransferLineItemRecord,
) -> SourceValue {
  let is_ready =
    transfer.status == "READY_TO_SHIP" || transfer.status == "IN_PROGRESS"
  let inventory_item = case
    store.find_effective_variant_by_inventory_item_id(
      store,
      line_item.inventory_item_id,
    )
  {
    Some(variant) -> shipment_inventory_item_source(store, variant)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("InventoryTransferLineItem")),
    #("id", SrcString(line_item.id)),
    #("title", graphql_helpers.option_string_source(line_item.title)),
    #("totalQuantity", SrcInt(line_item.total_quantity)),
    #("shippedQuantity", SrcInt(line_item.shipped_quantity)),
    #(
      "pickedForShipmentQuantity",
      SrcInt(line_item.picked_for_shipment_quantity),
    ),
    #(
      "processableQuantity",
      SrcInt(line_item.total_quantity - line_item.shipped_quantity),
    ),
    #(
      "shippableQuantity",
      SrcInt(case is_ready {
        True -> line_item.total_quantity - line_item.shipped_quantity
        False -> 0
      }),
    ),
    #("inventoryItem", inventory_item),
  ])
}

fn inventory_transfer_total_quantity(transfer: InventoryTransferRecord) -> Int {
  list.fold(transfer.line_items, 0, fn(total, line_item) {
    total + line_item.total_quantity
  })
}

fn serialize_inventory_shipment_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_inventory_shipment_by_id(store, id) {
        Some(shipment) ->
          project_graphql_value(
            inventory_shipment_source(store, shipment),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn inventory_shipment_source(
  store: Store,
  shipment: InventoryShipmentRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryShipment")),
    #("id", SrcString(shipment.id)),
    #("movementId", SrcString(shipment.movement_id)),
    #("name", SrcString(shipment.name)),
    #("status", SrcString(shipment.status)),
    #("createdAt", SrcString(shipment.created_at)),
    #("updatedAt", SrcString(shipment.updated_at)),
    #("lineItemTotalQuantity", SrcInt(shipment_line_item_total(shipment))),
    #("totalAcceptedQuantity", SrcInt(shipment_total_accepted(shipment))),
    #("totalReceivedQuantity", SrcInt(shipment_total_received(shipment))),
    #("totalRejectedQuantity", SrcInt(shipment_total_rejected(shipment))),
    #("tracking", inventory_shipment_tracking_source(shipment.tracking)),
    #("lineItems", inventory_shipment_line_items_source(store, shipment)),
    #("lineItemsCount", count_source(list.length(shipment.line_items))),
  ])
}

fn inventory_shipment_tracking_source(
  tracking: Option(InventoryShipmentTrackingRecord),
) -> SourceValue {
  case tracking {
    Some(tracking) ->
      src_object([
        #(
          "trackingNumber",
          graphql_helpers.option_string_source(tracking.tracking_number),
        ),
        #("company", graphql_helpers.option_string_source(tracking.company)),
        #(
          "trackingUrl",
          graphql_helpers.option_string_source(tracking.tracking_url),
        ),
        #(
          "arrivesAt",
          graphql_helpers.option_string_source(tracking.arrives_at),
        ),
      ])
    None -> SrcNull
  }
}

fn inventory_shipment_line_items_source(
  store: Store,
  shipment: InventoryShipmentRecord,
) -> SourceValue {
  let edges =
    shipment.line_items
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(line_item, _) = pair
      src_object([
        #("cursor", SrcString(line_item.id)),
        #("node", inventory_shipment_line_item_source(store, line_item)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(shipment.line_items, fn(line_item) {
          inventory_shipment_line_item_source(store, line_item)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(shipment.line_items, fn(line_item, _index) {
        line_item.id
      }),
    ),
  ])
}

fn inventory_shipment_line_item_source(
  store: Store,
  line_item: InventoryShipmentLineItemRecord,
) -> SourceValue {
  let inventory_item = case
    store.find_effective_variant_by_inventory_item_id(
      store,
      line_item.inventory_item_id,
    )
  {
    Some(variant) -> shipment_inventory_item_source(store, variant)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("InventoryShipmentLineItem")),
    #("id", SrcString(line_item.id)),
    #("quantity", SrcInt(line_item.quantity)),
    #("acceptedQuantity", SrcInt(line_item.accepted_quantity)),
    #("rejectedQuantity", SrcInt(line_item.rejected_quantity)),
    #("unreceivedQuantity", SrcInt(shipment_line_item_unreceived(line_item))),
    #("inventoryItem", inventory_item),
  ])
}

fn shipment_inventory_item_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  case variant.inventory_item {
    Some(item) ->
      src_object([
        #("__typename", SrcString("InventoryItem")),
        #("id", SrcString(item.id)),
        #("sku", graphql_helpers.option_string_source(variant.sku)),
        #("tracked", graphql_helpers.option_bool_source(item.tracked)),
        #(
          "requiresShipping",
          graphql_helpers.option_bool_source(item.requires_shipping),
        ),
        #("measurement", optional_measurement_source(item.measurement)),
        #(
          "countryCodeOfOrigin",
          graphql_helpers.option_string_source(item.country_code_of_origin),
        ),
        #(
          "provinceCodeOfOrigin",
          graphql_helpers.option_string_source(item.province_code_of_origin),
        ),
        #(
          "harmonizedSystemCode",
          graphql_helpers.option_string_source(item.harmonized_system_code),
        ),
        #(
          "inventoryLevels",
          inventory_levels_connection_source(active_inventory_levels(
            item.inventory_levels,
          )),
        ),
        #("variant", product_variant_source_without_inventory(store, variant)),
      ])
    None -> SrcNull
  }
}

fn shipment_line_item_unreceived(
  line_item: InventoryShipmentLineItemRecord,
) -> Int {
  int.max(
    0,
    line_item.quantity
      - line_item.accepted_quantity
      - line_item.rejected_quantity,
  )
}

fn shipment_line_item_total(shipment: InventoryShipmentRecord) -> Int {
  list.fold(shipment.line_items, 0, fn(total, line_item) {
    total + line_item.quantity
  })
}

fn shipment_total_accepted(shipment: InventoryShipmentRecord) -> Int {
  list.fold(shipment.line_items, 0, fn(total, line_item) {
    total + line_item.accepted_quantity
  })
}

fn shipment_total_rejected(shipment: InventoryShipmentRecord) -> Int {
  list.fold(shipment.line_items, 0, fn(total, line_item) {
    total + line_item.rejected_quantity
  })
}

fn shipment_total_received(shipment: InventoryShipmentRecord) -> Int {
  shipment_total_accepted(shipment) + shipment_total_rejected(shipment)
}

fn filtered_collections(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(CollectionRecord) {
  search_query_parser.apply_search_query(
    store.list_effective_collections(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    fn(collection, term) {
      collection_matches_positive_query_term(store, collection, term)
    },
  )
}

fn collection_matches_positive_query_term(
  store: Store,
  collection: CollectionRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let value = search_query_parser.search_query_term_value(term)
  case option.map(term.field, string.lowercase) {
    None ->
      search_query_parser.matches_search_query_string(
        Some(collection.title),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
      || search_query_parser.matches_search_query_string(
        Some(collection.handle),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("title") ->
      search_query_parser.matches_search_query_string(
        Some(collection.title),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("handle") ->
      search_query_parser.matches_search_query_string(
        Some(collection.handle),
        value,
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("collection_type") -> {
      let normalized =
        search_query_parser.strip_search_query_value_quotes(value)
        |> string.trim
        |> string.lowercase
      case normalized {
        "smart" -> collection.is_smart
        "custom" -> !collection.is_smart
        _ -> True
      }
    }
    Some("id") ->
      resource_id_matches(collection.id, collection.legacy_resource_id, value)
    Some("product_id") -> collection_has_product_id(store, collection.id, value)
    Some("updated_at") -> True
    Some("product_publication_status")
    | Some("publishable_status")
    | Some("published_at")
    | Some("published_status") -> True
    _ -> True
  }
}

fn collection_has_product_id(
  store: Store,
  collection_id: String,
  raw_value: String,
) -> Bool {
  let normalized =
    search_query_parser.strip_search_query_value_quotes(raw_value)
    |> string.trim
  store.list_effective_products_for_collection(store, collection_id)
  |> list.any(fn(entry) {
    let #(product, _) = entry
    product.id == normalized
    || product.legacy_resource_id == Some(normalized)
    || resource_tail(product.id) == normalized
    || resource_tail(normalized) == resource_tail(product.id)
  })
}

fn sort_collections(
  collections: List(CollectionRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(CollectionRecord) {
  let sort_key = read_string_argument(field, variables, "sortKey")
  let sorted =
    list.sort(collections, fn(left, right) {
      compare_collections_by_sort_key(left, right, sort_key)
    })
  case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(sorted)
    _ -> sorted
  }
}

fn compare_collections_by_sort_key(
  left: CollectionRecord,
  right: CollectionRecord,
  sort_key: Option(String),
) -> order.Order {
  case sort_key {
    Some("TITLE") ->
      case string.compare(left.title, right.title) {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    Some("UPDATED_AT") ->
      case
        resource_ids.compare_nullable_strings(left.updated_at, right.updated_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
  }
}

fn collection_cursor_for_field(
  collection: CollectionRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> String {
  case read_string_argument(field, variables, "sortKey") {
    Some("TITLE") ->
      collection.title_cursor
      |> option.unwrap(option.unwrap(collection.cursor, collection.id))
    Some("UPDATED_AT") ->
      collection.updated_at_cursor
      |> option.unwrap(option.unwrap(collection.cursor, collection.id))
    _ -> option.unwrap(collection.cursor, collection.id)
  }
}

fn collection_product_cursor(
  entry: #(ProductRecord, ProductCollectionRecord),
  _index: Int,
) -> String {
  let #(_, membership) = entry
  membership.cursor |> option.unwrap(membership.product_id)
}

fn product_collection_cursor(
  entry: #(CollectionRecord, ProductCollectionRecord),
  _index: Int,
) -> String {
  let #(collection, membership) = entry
  membership.cursor |> option.unwrap(collection.id)
}

fn collection_has_product(
  store: Store,
  collection_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_string_argument(field, variables, "id") {
    Some(product_id) ->
      store.list_effective_products_for_collection(store, collection_id)
      |> list.any(fn(entry) {
        let #(product, _) = entry
        product.id == product_id
      })
    None -> False
  }
}

fn serialize_product_variant_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_variant_by_id(store, id) {
        Some(variant) ->
          serialize_product_variant_object(
            store,
            variant,
            get_selected_child_fields(field, default_selected_field_options()),
            field,
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_product_variant_by_identifier_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_identifier_argument(field, variables) {
    Some(identifier) ->
      case read_string_field(identifier, "id") {
        Some(id) ->
          case store.get_effective_variant_by_id(store, id) {
            Some(variant) ->
              serialize_product_variant_object(
                store,
                variant,
                get_selected_child_fields(
                  field,
                  default_selected_field_options(),
                ),
                field,
                variables,
                fragments,
              )
            None -> json.null()
          }
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_inventory_item_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.find_effective_variant_by_inventory_item_id(store, id) {
        Some(variant) ->
          serialize_inventory_item_object(
            store,
            variant,
            get_selected_child_fields(field, default_selected_field_options()),
            field,
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_inventory_level_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case find_inventory_level_target(store, id) {
        Some(#(variant, level)) ->
          project_graphql_value(
            inventory_level_source_with_item(store, variant, level),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_products_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let products =
    filtered_products(store, field, variables)
    |> sort_products(field, variables)
  case products {
    [] -> serialize_empty_connection(field, default_selected_field_options())
    _ -> {
      let window =
        paginate_connection_items(
          products,
          field,
          variables,
          product_cursor,
          default_connection_window_options(),
        )
      let count = product_count_for_field(store, field, variables)
      let has_next_page =
        window.has_next_page || count > list.length(window.items)
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: window.items,
          has_next_page: has_next_page,
          has_previous_page: window.has_previous_page,
          get_cursor_value: fn(product, index) {
            product_cursor_for_field(product, index, field, variables)
          },
          serialize_node: fn(product, node_field, _index) {
            project_graphql_value(
              product_source_with_store(store, product),
              get_selected_child_fields(
                node_field,
                default_selected_field_options(),
              ),
              fragments,
            )
          },
          selected_field_options: default_selected_field_options(),
          page_info_options: ConnectionPageInfoOptions(
            include_inline_fragments: False,
            prefix_cursors: False,
            include_cursors: True,
            fallback_start_cursor: None,
            fallback_end_cursor: None,
          ),
        ),
      )
    }
  }
}

fn serialize_selling_plan_group_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_selling_plan_group_by_id(store, id) {
        Some(group) ->
          serialize_selling_plan_group_object(
            store,
            group,
            get_selected_child_fields(field, default_selected_field_options()),
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_selling_plan_groups_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let groups = store.list_effective_selling_plan_groups(store)
  case groups {
    [] -> serialize_empty_connection(field, default_selected_field_options())
    _ -> {
      let window =
        paginate_connection_items(
          groups,
          field,
          variables,
          selling_plan_group_cursor,
          default_connection_window_options(),
        )
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: window.items,
          has_next_page: window.has_next_page,
          has_previous_page: window.has_previous_page,
          get_cursor_value: selling_plan_group_cursor,
          serialize_node: fn(group, node_field, _index) {
            serialize_selling_plan_group_object(
              store,
              group,
              get_selected_child_fields(
                node_field,
                default_selected_field_options(),
              ),
              variables,
              fragments,
            )
          },
          selected_field_options: default_selected_field_options(),
          page_info_options: default_connection_page_info_options(),
        ),
      )
    }
  }
}

fn serialize_selling_plan_group_object(
  store: Store,
  group: SellingPlanGroupRecord,
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "__typename" -> json.string("SellingPlanGroup")
            "id" -> json.string(group.id)
            "appId" -> optional_string(group.app_id)
            "name" -> json.string(group.name)
            "merchantCode" -> json.string(group.merchant_code)
            "description" -> optional_string(group.description)
            "options" -> json.array(group.options, json.string)
            "position" -> optional_int_json(group.position)
            "summary" -> optional_string(group.summary)
            "createdAt" -> optional_string(group.created_at)
            "productsCount" ->
              serialize_exact_count(selection, list.length(group.product_ids))
            "productVariantsCount" ->
              serialize_exact_count(
                selection,
                product_variant_count_for_selling_plan_group(
                  store,
                  group,
                  selection,
                  variables,
                ),
              )
            "appliesToProduct" ->
              json.bool(
                case read_string_argument(selection, variables, "productId") {
                  Some(product_id) ->
                    list.contains(group.product_ids, product_id)
                  None -> False
                },
              )
            "appliesToProductVariant" ->
              json.bool(
                case
                  read_string_argument(selection, variables, "productVariantId")
                {
                  Some(variant_id) ->
                    list.contains(group.product_variant_ids, variant_id)
                  None -> False
                },
              )
            "appliesToProductVariants" ->
              json.bool(
                case read_string_argument(selection, variables, "productId") {
                  Some(product_id) ->
                    list.any(group.product_variant_ids, fn(variant_id) {
                      case
                        store.get_effective_variant_by_id(store, variant_id)
                      {
                        Some(variant) -> variant.product_id == product_id
                        None -> False
                      }
                    })
                  None -> False
                },
              )
            "products" ->
              serialize_selling_plan_group_products_connection(
                store,
                group,
                selection,
                variables,
                fragments,
              )
            "productVariants" ->
              serialize_selling_plan_group_variants_connection(
                store,
                group,
                selection,
                variables,
                fragments,
              )
            "sellingPlans" ->
              serialize_selling_plans_connection(
                group.selling_plans,
                selection,
                variables,
                fragments,
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn serialize_selling_plan_group_products_connection(
  store: Store,
  group: SellingPlanGroupRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let products =
    group.product_ids
    |> list.filter_map(fn(product_id) {
      store.get_effective_product_by_id(store, product_id) |> option_to_result
    })
  serialize_product_list_connection(
    products,
    store,
    field,
    variables,
    fragments,
  )
}

fn serialize_selling_plan_group_variants_connection(
  store: Store,
  group: SellingPlanGroupRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let product_id = read_string_argument(field, variables, "productId")
  let variants =
    group.product_variant_ids
    |> list.filter_map(fn(variant_id) {
      store.get_effective_variant_by_id(store, variant_id) |> option_to_result
    })
    |> list.filter(fn(variant) {
      case product_id {
        Some(id) -> variant.product_id == id
        None -> True
      }
    })
  let window =
    paginate_connection_items(
      variants,
      field,
      variables,
      product_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        serialize_product_variant_object(
          store,
          variant,
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          node_field,
          variables,
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_product_list_connection(
  products: List(ProductRecord),
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let window =
    paginate_connection_items(
      products,
      field,
      variables,
      product_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_cursor,
      serialize_node: fn(product, node_field, _index) {
        project_graphql_value(
          product_source_with_store(store, product),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_selling_plans_connection(
  plans: List(SellingPlanRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let window =
    paginate_connection_items(
      plans,
      field,
      variables,
      selling_plan_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: selling_plan_cursor,
      serialize_node: fn(plan, node_field, _index) {
        project_graphql_value(
          captured_json_source(plan.data),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn product_variant_count_for_selling_plan_group(
  store: Store,
  group: SellingPlanGroupRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "productId") {
    Some(product_id) ->
      group.product_variant_ids
      |> list.filter(fn(variant_id) {
        case store.get_effective_variant_by_id(store, variant_id) {
          Some(variant) -> variant.product_id == product_id
          None -> False
        }
      })
      |> list.length
    None -> list.length(group.product_variant_ids)
  }
}

fn filtered_products(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductRecord) {
  search_query_parser.apply_search_query(
    store.list_effective_products(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    fn(product, term) {
      product_matches_positive_query_term(store, product, term)
    },
  )
}

fn sort_products(
  products: List(ProductRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductRecord) {
  case read_string_argument(field, variables, "sortKey") {
    None -> products
    Some(sort_key) -> {
      let sorted =
        list.sort(products, fn(left, right) {
          compare_products_by_sort_key(left, right, sort_key)
        })
      case read_bool_argument(field, variables, "reverse") {
        Some(True) -> list.reverse(sorted)
        _ -> sorted
      }
    }
  }
}

fn compare_products_by_sort_key(
  left: ProductRecord,
  right: ProductRecord,
  sort_key: String,
) -> order.Order {
  case sort_key {
    "TITLE" ->
      case string.compare(left.title, right.title) {
        order.Eq -> string.compare(left.id, right.id)
        other -> other
      }
    "VENDOR" ->
      case compare_optional_strings_as_empty(left.vendor, right.vendor) {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "PRODUCT_TYPE" ->
      case
        compare_optional_strings_as_empty(left.product_type, right.product_type)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "PUBLISHED_AT" ->
      case
        compare_optional_strings_as_empty(left.published_at, right.published_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "UPDATED_AT" ->
      case
        compare_optional_strings_as_empty(left.updated_at, right.updated_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "ID" -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
    _ -> order.Eq
  }
}

fn compare_optional_strings_as_empty(
  left: Option(String),
  right: Option(String),
) -> order.Order {
  string.compare(option.unwrap(left, ""), option.unwrap(right, ""))
}

fn product_search_parse_options() -> search_query_parser.SearchQueryParseOptions {
  search_query_parser.SearchQueryParseOptions(
    ..search_query_parser.default_parse_options(),
    recognize_not_keyword: True,
  )
}

fn product_string_match_options() -> search_query_parser.SearchQueryStringMatchOptions {
  search_query_parser.SearchQueryStringMatchOptions(word_prefix: True)
}

fn product_count_for_field(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "query") {
    Some(_) -> list.length(filtered_products(store, field, variables))
    None -> store.get_effective_product_count(store)
  }
}

fn product_matches_positive_query_term(
  store: Store,
  product: ProductRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case option.map(term.field, string.lowercase) {
    None -> product_matches_search_text(product, term.value)
    Some("id") -> product_id_matches(product, term.value)
    Some("title") ->
      search_query_parser.matches_search_query_string(
        Some(product.title),
        search_query_parser.search_query_term_value(term),
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("handle") ->
      search_query_parser.matches_search_query_string(
        Some(product.handle),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("tag") ->
      list.any(product_searchable_tags(store, product), fn(tag) {
        search_query_parser.matches_search_query_string(
          Some(tag),
          search_query_parser.search_query_term_value(term),
          search_query_parser.ExactMatch,
          product_string_match_options(),
        )
      })
    Some("product_type") ->
      search_query_parser.matches_search_query_string(
        product.product_type,
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("vendor") ->
      search_query_parser.matches_search_query_string(
        product.vendor,
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("status") ->
      search_query_parser.matches_search_query_string(
        Some(product_searchable_status(store, product)),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("sku") ->
      product_searchable_variants(store, product.id)
      |> list.any(fn(variant) {
        search_query_parser.matches_search_query_string(
          variant.sku,
          search_query_parser.search_query_term_value(term),
          search_query_parser.ExactMatch,
          product_string_match_options(),
        )
      })
    Some("inventory_total") ->
      search_query_parser.matches_search_query_number(
        option.map(product.total_inventory, int.to_float),
        term,
      )
    Some("tag_not") ->
      !list.any(product_searchable_tags(store, product), fn(tag) {
        search_query_parser.matches_search_query_string(
          Some(tag),
          search_query_parser.search_query_term_value(term),
          search_query_parser.ExactMatch,
          product_string_match_options(),
        )
      })
    Some("published_at") ->
      matches_nullable_product_timestamp(product.published_at, term)
    Some("updated_at") ->
      matches_nullable_product_timestamp(product.updated_at, term)
    Some("created_at") ->
      matches_nullable_product_timestamp(product.created_at, term)
    _ -> True
  }
}

fn matches_nullable_product_timestamp(
  value: Option(String),
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case
    search_query_parser.strip_search_query_value_quotes(
      search_query_parser.search_query_term_value(term),
    )
  {
    "*" -> option.is_some(value)
    _ -> search_query_parser.matches_search_query_date(value, term, 0)
  }
}

fn product_searchable_variants(
  store: Store,
  product_id: String,
) -> List(ProductVariantRecord) {
  let base_variants = store.get_base_variants_by_product_id(store, product_id)
  let effective_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  case has_staged_variants_for_product(store, product_id), base_variants {
    False, _ -> effective_variants
    True, [] -> effective_variants
    True, _ ->
      case list.length(effective_variants) > list.length(base_variants) {
        True -> base_variants
        False -> []
      }
  }
}

fn has_staged_variants_for_product(store: Store, product_id: String) -> Bool {
  store.staged_state.product_variants
  |> dict.values
  |> list.any(fn(variant) { variant.product_id == product_id })
}

fn product_searchable_status(store: Store, product: ProductRecord) -> String {
  case dict.get(store.base_state.products, product.id) {
    Ok(base_product) ->
      case base_product.status == product.status {
        True -> product.status
        False -> base_product.status
      }
    Error(_) -> product.status
  }
}

fn product_searchable_tags(
  store: Store,
  product: ProductRecord,
) -> List(String) {
  case dict.get(store.base_state.products, product.id) {
    Ok(base_product) ->
      case base_product.tags == product.tags {
        True -> product.tags
        False -> base_product.tags
      }
    Error(_) -> product.tags
  }
}

fn product_matches_search_text(
  product: ProductRecord,
  raw_value: String,
) -> Bool {
  let searchable_values = [
    product.title,
    product.handle,
    option.unwrap(product.vendor, ""),
    option.unwrap(product.product_type, ""),
  ]
  list.any(list.append(searchable_values, product.tags), fn(candidate) {
    search_query_parser.matches_search_query_string(
      Some(candidate),
      raw_value,
      search_query_parser.IncludesMatch,
      product_string_match_options(),
    )
  })
}

fn product_id_matches(product: ProductRecord, raw_value: String) -> Bool {
  resource_id_matches(product.id, product.legacy_resource_id, raw_value)
}

fn resource_id_matches(
  resource_id: String,
  legacy_resource_id: Option(String),
  raw_value: String,
) -> Bool {
  let normalized =
    search_query_parser.strip_search_query_value_quotes(raw_value)
    |> string.trim
  case normalized {
    "" -> True
    _ -> {
      resource_id == normalized
      || option.unwrap(legacy_resource_id, "") == normalized
      || resource_tail(resource_id) == normalized
      || resource_tail(normalized) == resource_tail(resource_id)
    }
  }
}

fn resource_tail(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(tail) -> tail
    Error(_) -> id
  }
}

fn serialize_inventory_items_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let variants =
    filtered_inventory_item_variants(store, field, variables)
    |> reverse_inventory_item_variants(field, variables)
  let window =
    paginate_connection_items(
      variants,
      field,
      variables,
      inventory_item_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: inventory_item_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        project_graphql_value(
          inventory_item_source(store, variant),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn filtered_inventory_item_variants(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductVariantRecord) {
  search_query_parser.apply_search_query(
    inventory_item_variants(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    inventory_item_variant_matches_positive_query_term,
  )
}

fn inventory_item_variants(store: Store) -> List(ProductVariantRecord) {
  store.list_effective_product_variants(store)
  |> list.filter(fn(variant) {
    case variant.inventory_item {
      Some(_) -> True
      None -> False
    }
  })
  |> list.sort(fn(left, right) {
    string.compare(
      inventory_item_variant_cursor(left, 0),
      inventory_item_variant_cursor(right, 0),
    )
  })
}

fn reverse_inventory_item_variants(
  variants: List(ProductVariantRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductVariantRecord) {
  case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(variants)
    _ -> variants
  }
}

fn inventory_item_variant_matches_positive_query_term(
  variant: ProductVariantRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case variant.inventory_item {
    Some(item) -> {
      let value = search_query_parser.search_query_term_value(term)
      case option.map(term.field, string.lowercase) {
        None ->
          list.any(
            [item.id, option.unwrap(variant.sku, ""), variant.id],
            fn(candidate) {
              search_query_parser.matches_search_query_string(
                Some(candidate),
                value,
                search_query_parser.IncludesMatch,
                product_string_match_options(),
              )
            },
          )
        Some("id") -> resource_id_matches(item.id, None, value)
        Some("sku") ->
          search_query_parser.matches_search_query_string(
            variant.sku,
            value,
            search_query_parser.ExactMatch,
            product_string_match_options(),
          )
        Some("tracked") ->
          bool_string(option.unwrap(item.tracked, False))
          == string.lowercase(value)
        _ -> True
      }
    }
    None -> False
  }
}

fn bool_string(value: Bool) -> String {
  case value {
    True -> "true"
    False -> "false"
  }
}

fn inventory_item_variant_cursor(
  variant: ProductVariantRecord,
  _index: Int,
) -> String {
  case variant.inventory_item {
    Some(item) -> item.id
    None -> variant.id
  }
}

fn serialize_product_variants_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let variants =
    store.list_effective_product_variants(store)
    |> list.sort(fn(left, right) {
      resource_ids.compare_shopify_resource_ids(left.id, right.id)
    })
  let ordered_variants = case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(variants)
    _ -> variants
  }
  let window =
    paginate_connection_items(
      ordered_variants,
      field,
      variables,
      product_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        serialize_product_variant_object(
          store,
          variant,
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          node_field,
          variables,
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_inventory_properties(
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    inventory_properties_source(),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_properties_source() -> SourceValue {
  src_object([
    #(
      "quantityNames",
      SrcList(list.map(
        inventory_quantity_name_definitions(),
        inventory_quantity_name_source,
      )),
    ),
  ])
}

fn inventory_quantity_name_definitions() -> List(
  #(String, String, Bool, List(String), List(String)),
) {
  [
    #("available", "Available", True, ["on_hand"], []),
    #("committed", "Committed", True, ["on_hand"], []),
    #("damaged", "Damaged", False, ["on_hand"], []),
    #("incoming", "Incoming", False, [], []),
    #("on_hand", "On hand", True, [], [
      "available",
      "committed",
      "damaged",
      "quality_control",
      "reserved",
      "safety_stock",
    ]),
    #("quality_control", "Quality control", False, ["on_hand"], []),
    #("reserved", "Reserved", True, ["on_hand"], []),
    #("safety_stock", "Safety stock", False, ["on_hand"], []),
  ]
}

fn inventory_quantity_name_source(
  definition: #(String, String, Bool, List(String), List(String)),
) -> SourceValue {
  let #(name, display_name, is_in_use, belongs_to, comprises) = definition
  src_object([
    #("name", SrcString(name)),
    #("displayName", SrcString(display_name)),
    #("isInUse", SrcBool(is_in_use)),
    #("belongsTo", SrcList(list.map(belongs_to, SrcString))),
    #("comprises", SrcList(list.map(comprises, SrcString))),
  ])
}

fn serialize_string_connection(
  values: List(String),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let sorted_values = normalize_string_catalog(values)
  let ordered_values = case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(sorted_values)
    _ -> sorted_values
  }
  let window =
    paginate_connection_items(
      ordered_values,
      field,
      variables,
      string_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: string_cursor,
      serialize_node: fn(value, _node_field, _index) { json.string(value) },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn product_tags(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.flat_map(fn(product) { product.tags })
}

fn product_types(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.filter_map(fn(product) {
    case product.product_type {
      Some(value) -> Ok(value)
      None -> Error(Nil)
    }
  })
}

fn product_vendors(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.filter_map(fn(product) {
    case product.vendor {
      Some(value) -> Ok(value)
      None -> Error(Nil)
    }
  })
}

fn normalize_string_catalog(values: List(String)) -> List(String) {
  values
  |> list.filter(fn(value) { string.length(string.trim(value)) > 0 })
  |> list.fold(dict.new(), fn(seen, value) { dict.insert(seen, value, True) })
  |> dict.keys()
  |> list.sort(string.compare)
}

fn string_cursor(value: String, _index: Int) -> String {
  value
}

fn product_variant_cursor(
  variant: ProductVariantRecord,
  _index: Int,
) -> String {
  case variant.cursor {
    Some(cursor) -> cursor
    None -> variant.id
  }
}

fn product_cursor(product: ProductRecord, _index: Int) -> String {
  case product.cursor {
    Some(cursor) -> cursor
    None -> product.id
  }
}

fn selling_plan_group_cursor(
  group: SellingPlanGroupRecord,
  _index: Int,
) -> String {
  case group.cursor {
    Some(cursor) -> cursor
    None -> group.id
  }
}

fn selling_plan_cursor(plan: SellingPlanRecord, _index: Int) -> String {
  plan.id
}

fn product_cursor_for_field(
  product: ProductRecord,
  index: Int,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> String {
  case product.cursor {
    Some(_) -> product_cursor(product, index)
    None ->
      case read_string_argument(field, variables, "sortKey") {
        Some("TITLE") ->
          product_sort_cursor_string(product, string.lowercase(product.title))
        Some("VENDOR") ->
          product_sort_cursor_string(
            product,
            product.vendor |> option.unwrap("") |> string.lowercase,
          )
        Some("PRODUCT_TYPE") ->
          product_sort_cursor_string(
            product,
            product.product_type |> option.unwrap("") |> string.lowercase,
          )
        Some("ID") ->
          product_sort_cursor_int(product, product_numeric_id(product))
        Some("PUBLISHED_AT") ->
          product_sort_cursor_timestamp(product, product.published_at)
        Some("UPDATED_AT") ->
          product_sort_cursor_timestamp(product, product.updated_at)
        _ -> product_cursor(product, index)
      }
  }
}

fn product_sort_cursor_string(product: ProductRecord, value: String) -> String {
  product_sort_cursor_payload(product, json.to_string(json.string(value)))
}

fn product_sort_cursor_int(product: ProductRecord, value: Int) -> String {
  product_sort_cursor_payload(product, int.to_string(value))
}

fn product_sort_cursor_timestamp(
  product: ProductRecord,
  value: Option(String),
) -> String {
  let timestamp = case value {
    Some(raw) -> iso_timestamp.parse_iso(raw) |> result.unwrap(0)
    None -> 0
  }
  product_sort_cursor_int(product, timestamp)
}

fn product_sort_cursor_payload(
  product: ProductRecord,
  encoded_value: String,
) -> String {
  let payload =
    "{\"last_id\":"
    <> int.to_string(product_numeric_id(product))
    <> ",\"last_value\":"
    <> encoded_value
    <> "}"
  payload
  |> bit_array.from_string
  |> bit_array.base64_encode(True)
}

fn product_numeric_id(product: ProductRecord) -> Int {
  case product.legacy_resource_id {
    Some(value) ->
      case int.parse(value) {
        Ok(parsed) -> parsed
        Error(_) -> product_numeric_id_from_gid(product.id)
      }
    None -> product_numeric_id_from_gid(product.id)
  }
}

fn product_numeric_id_from_gid(id: String) -> Int {
  case list.last(string.split(id, "/")) {
    Ok(tail) ->
      case int.parse(tail) {
        Ok(parsed) -> parsed
        Error(_) -> 0
      }
    Error(_) -> 0
  }
}

fn serialize_exact_count(field: Selection, count: Int) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "count" -> #(key, json.int(count))
              "precision" -> #(key, json.string("EXACT"))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn serialize_product_duplicate_job(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let id = read_string_argument(field, variables, "id")
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "id" -> #(key, optional_string(id))
              "done" -> #(key, json.bool(True))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn serialize_product_operation_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_operation_by_id(store, id) {
        Some(operation) ->
          project_graphql_value(
            product_operation_source(store, operation),
            graphql_helpers.field_raw_selections(field),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn product_operation_source(
  store: Store,
  operation: ProductOperationRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString(operation.type_name)),
    #("id", SrcString(operation.id)),
    #("status", SrcString(operation.status)),
    #("product", case operation.product_id {
      Some(product_id) ->
        case store.get_effective_product_by_id(store, product_id) {
          Some(product) -> product_source_with_store(store, product)
          None -> SrcNull
        }
      None -> SrcNull
    }),
    #("newProduct", case operation.new_product_id {
      Some(product_id) ->
        case store.get_effective_product_by_id(store, product_id) {
          Some(product) -> product_source_with_store(store, product)
          None -> SrcNull
        }
      None -> SrcNull
    }),
    #(
      "userErrors",
      SrcList(list.map(
        operation.user_errors,
        product_operation_user_error_source,
      )),
    ),
  ])
}

fn product_operation_user_error_source(
  error: ProductOperationUserErrorRecord,
) -> SourceValue {
  let field_value = case error.field {
    Some(field) -> SrcList(list.map(field, SrcString))
    None -> SrcNull
  }
  src_object([
    #("field", field_value),
    #("message", SrcString(error.message)),
    #("code", graphql_helpers.option_string_source(error.code)),
  ])
}

fn product_duplicate_payload(
  store: Store,
  new_product: Option(ProductRecord),
  operation: Option(ProductOperationRecord),
  user_errors: List(ProductOperationUserErrorRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let operation_value = case operation {
    Some(operation) -> product_operation_source(store, operation)
    None -> SrcNull
  }
  let source =
    src_object([
      #("__typename", SrcString("ProductDuplicatePayload")),
      #("productDuplicateOperation", operation_value),
      #(
        "userErrors",
        SrcList(list.map(user_errors, product_operation_user_error_source)),
      ),
    ])
  let entries =
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "newProduct" ->
              case new_product {
                Some(product) -> #(
                  key,
                  serialize_product_selection(
                    store,
                    product,
                    selection,
                    variables,
                    fragments,
                  ),
                )
                None -> #(key, json.null())
              }
            _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn product_set_payload(
  store: Store,
  product: Option(ProductRecord),
  operation: Option(ProductOperationRecord),
  user_errors: List(ProductOperationUserErrorRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let operation_value = case operation {
    Some(operation) -> product_operation_source(store, operation)
    None -> SrcNull
  }
  let source =
    src_object([
      #("__typename", SrcString("ProductSetPayload")),
      #("productSetOperation", operation_value),
      #(
        "userErrors",
        SrcList(list.map(user_errors, product_operation_user_error_source)),
      ),
    ])
  let entries =
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "product" ->
              case product {
                Some(product) -> #(
                  key,
                  serialize_product_selection(
                    store,
                    product,
                    selection,
                    variables,
                    fragments,
                  ),
                )
                None -> #(key, json.null())
              }
            _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn read_identifier_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Dict(String, ResolvedValue)) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, "identifier") {
        Ok(ObjectVal(identifier)) -> Some(identifier)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_string_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(StringVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_bool_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
  name: String,
) -> Option(Bool) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(BoolVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_include_inactive_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_bool_argument(field, variables, "includeInactive") {
    Some(True) -> True
    _ -> False
  }
}

fn read_string_field(
  fields: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(fields, name) {
    Ok(StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn selected_publication_id(
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
) -> Option(String) {
  selections
  |> list.find_map(fn(selection) {
    case selection {
      Field(name: name, ..) if name.value == "publishedOnPublication" ->
        read_string_argument(selection, variables, "publicationId")
        |> option_to_result
      Field(selection_set: Some(SelectionSet(selections: inner, ..)), ..)
      | InlineFragment(selection_set: SelectionSet(selections: inner, ..), ..) ->
        selected_publication_id(inner, variables) |> option_to_result
      _ -> Error(Nil)
    }
  })
  |> option.from_result
}

fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(value) -> Ok(value)
    None -> Error(Nil)
  }
}

fn optional_string(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

pub fn product_source(product: ProductRecord) -> SourceValue {
  product_source_with_relationships(
    product,
    empty_connection_source(),
    empty_connection_source(),
    empty_connection_source(),
    SrcList([]),
    empty_connection_source(),
    count_source(0),
    None,
  )
}

fn product_source_with_store(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  product_source_with_store_and_publication(store, product, None)
}

fn product_source_with_store_and_publication(
  store: Store,
  product: ProductRecord,
  publication_id: Option(String),
) -> SourceValue {
  product_source_with_relationships(
    product,
    product_collections_connection_source(store, product),
    product_variants_connection_source(store, product),
    product_media_connection_source(store, product),
    product_options_source(store.get_effective_options_by_product_id(
      store,
      product.id,
    )),
    selling_plan_group_connection_source(
      store.list_effective_selling_plan_groups_visible_for_product(
        store,
        product.id,
      ),
    ),
    count_source(
      list.length(store.list_effective_selling_plan_groups_for_product(
        store,
        product.id,
      )),
    ),
    publication_id,
  )
}

fn product_source_with_relationships(
  product: ProductRecord,
  collections: SourceValue,
  variants: SourceValue,
  media: SourceValue,
  options: SourceValue,
  selling_plan_groups: SourceValue,
  selling_plan_groups_count: SourceValue,
  publication_id: Option(String),
) -> SourceValue {
  let visible_publication_count = case product.status == "ACTIVE" {
    True -> list.length(product.publication_ids)
    False -> 0
  }
  let published_on_publication = case publication_id, product.status {
    Some(id), "ACTIVE" -> list.contains(product.publication_ids, id)
    _, _ -> False
  }
  src_object([
    #("__typename", SrcString("Product")),
    #("id", SrcString(product.id)),
    #(
      "legacyResourceId",
      graphql_helpers.option_string_source(product.legacy_resource_id),
    ),
    #("title", SrcString(product.title)),
    #("handle", SrcString(product.handle)),
    #("status", SrcString(product.status)),
    #("vendor", graphql_helpers.option_string_source(product.vendor)),
    #("productType", graphql_helpers.option_string_source(product.product_type)),
    #("tags", SrcList(list.map(product.tags, SrcString))),
    #(
      "totalInventory",
      graphql_helpers.option_int_source(product.total_inventory),
    ),
    #(
      "tracksInventory",
      graphql_helpers.option_bool_source(product.tracks_inventory),
    ),
    #("createdAt", graphql_helpers.option_string_source(product.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(product.updated_at)),
    #("publishedAt", graphql_helpers.option_string_source(product.published_at)),
    #("descriptionHtml", SrcString(product.description_html)),
    #(
      "onlineStorePreviewUrl",
      graphql_helpers.option_string_source(product.online_store_preview_url),
    ),
    #(
      "templateSuffix",
      graphql_helpers.option_string_source(product.template_suffix),
    ),
    #("seo", product_seo_source(product.seo)),
    #("category", optional_product_category_source(product.category)),
    #(
      "contextualPricing",
      optional_captured_json_source(product.contextual_pricing),
    ),
    #("publishedOnCurrentPublication", SrcBool(visible_publication_count > 0)),
    #("publishedOnCurrentChannel", SrcBool(visible_publication_count > 0)),
    #("publishedOnPublication", SrcBool(published_on_publication)),
    #("availablePublicationsCount", count_source(visible_publication_count)),
    #("resourcePublicationsCount", count_source(visible_publication_count)),
    #("collections", collections),
    #("media", media),
    #("images", empty_connection_source()),
    #("options", options),
    #("variants", variants),
    #("requiresSellingPlan", SrcBool(False)),
    #("sellingPlanGroups", selling_plan_groups),
    #("sellingPlanGroupsCount", selling_plan_groups_count),
  ])
}

fn product_media_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let media = store.get_effective_media_by_product_id(store, product.id)
  src_object([
    #(
      "edges",
      SrcList(
        list.map(enumerate_items(media), fn(entry) {
          let #(record, index) = entry
          src_object([
            #("cursor", SrcString(product_media_cursor(record, index))),
            #("node", product_media_source(record)),
          ])
        }),
      ),
    ),
    #("nodes", SrcList(list.map(media, product_media_source))),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", connection_start_cursor(media, product_media_cursor)),
        #("endCursor", connection_end_cursor(media, product_media_cursor)),
      ]),
    ),
  ])
}

fn product_media_cursor(media: ProductMediaRecord, _index: Int) -> String {
  "cursor:" <> media.key
}

fn product_media_source(media: ProductMediaRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString(product_media_typename(media))),
    #("id", graphql_helpers.option_string_source(media.id)),
    #("alt", graphql_helpers.option_string_source(media.alt)),
    #(
      "mediaContentType",
      graphql_helpers.option_string_source(media.media_content_type),
    ),
    #("status", graphql_helpers.option_string_source(media.status)),
    #("preview", product_media_preview_source(media)),
    #(
      "image",
      product_media_image_source(
        media.image_url |> option.or(media.preview_image_url),
      ),
    ),
  ])
}

fn product_media_typename(media: ProductMediaRecord) -> String {
  case media.media_content_type {
    Some("IMAGE") -> "MediaImage"
    Some("VIDEO") -> "Video"
    Some("EXTERNAL_VIDEO") -> "ExternalVideo"
    Some("MODEL_3D") -> "Model3d"
    _ -> "Media"
  }
}

fn product_media_preview_source(media: ProductMediaRecord) -> SourceValue {
  src_object([
    #("image", product_media_image_source(media.preview_image_url)),
  ])
}

fn product_media_image_source(url: Option(String)) -> SourceValue {
  case url {
    Some(url) -> src_object([#("url", SrcString(url))])
    None -> SrcNull
  }
}

fn count_source(count: Int) -> SourceValue {
  src_object([
    #("__typename", SrcString("Count")),
    #("count", SrcInt(count)),
    #("precision", SrcString("EXACT")),
  ])
}

fn product_collections_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let collections =
    store.list_effective_collections_for_product(store, product.id)
  let edges =
    collections
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(entry, index) = pair
      let #(collection, _) = entry
      src_object([
        #("cursor", SrcString(product_collection_cursor(entry, index))),
        #("node", collection_source_with_store(store, collection)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(collections, fn(entry) {
          let #(collection, _) = entry
          collection_source_with_store(store, collection)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(collections, product_collection_cursor),
    ),
  ])
}

fn collection_source_with_store(
  store: Store,
  collection: CollectionRecord,
) -> SourceValue {
  collection_source_with_store_and_publication(store, collection, None)
}

fn collection_source_with_store_and_publication(
  store: Store,
  collection: CollectionRecord,
  publication_id: Option(String),
) -> SourceValue {
  let publication_count = list.length(collection.publication_ids)
  let published_on_publication = case publication_id {
    Some(id) -> list.contains(collection.publication_ids, id)
    None -> False
  }
  src_object([
    #("__typename", SrcString("Collection")),
    #("id", SrcString(collection.id)),
    #(
      "legacyResourceId",
      graphql_helpers.option_string_source(collection.legacy_resource_id),
    ),
    #("title", SrcString(collection.title)),
    #("handle", SrcString(collection.handle)),
    #("updatedAt", graphql_helpers.option_string_source(collection.updated_at)),
    #(
      "description",
      graphql_helpers.option_string_source(collection.description),
    ),
    #(
      "descriptionHtml",
      graphql_helpers.option_string_source(collection.description_html),
    ),
    #("publishedOnPublication", SrcBool(published_on_publication)),
    #("availablePublicationsCount", count_source(publication_count)),
    #("resourcePublicationsCount", count_source(publication_count)),
    #("publicationCount", count_source(publication_count)),
    #("sortOrder", graphql_helpers.option_string_source(collection.sort_order)),
    #(
      "templateSuffix",
      graphql_helpers.option_string_source(collection.template_suffix),
    ),
    #("products", collection_products_connection_source(store, collection)),
  ])
}

fn collection_products_connection_source(
  store: Store,
  collection: CollectionRecord,
) -> SourceValue {
  let products =
    store.list_effective_products_for_collection(store, collection.id)
  let edges =
    products
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(entry, index) = pair
      let #(product, _) = entry
      src_object([
        #("cursor", SrcString(collection_product_cursor(entry, index))),
        #("node", product_source(product)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(products, fn(entry) {
          let #(product, _) = entry
          product_source(product)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(products, collection_product_cursor),
    ),
  ])
}

fn product_options_source(options: List(ProductOptionRecord)) -> SourceValue {
  SrcList(list.map(options, product_option_source))
}

fn product_option_source(option: ProductOptionRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductOption")),
    #("id", SrcString(option.id)),
    #("name", SrcString(option.name)),
    #("position", SrcInt(option.position)),
    #(
      "values",
      SrcList(
        option.option_values
        |> list.filter(fn(value) { value.has_variants })
        |> list.map(fn(value) { SrcString(value.name) }),
      ),
    ),
    #(
      "optionValues",
      SrcList(list.map(option.option_values, product_option_value_source)),
    ),
  ])
}

fn product_option_value_source(
  option_value: ProductOptionValueRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductOptionValue")),
    #("id", SrcString(option_value.id)),
    #("name", SrcString(option_value.name)),
    #("hasVariants", SrcBool(option_value.has_variants)),
  ])
}

pub fn serialize_product_option_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_product_option_by_id(store, id) {
    Some(option) ->
      project_graphql_value(
        product_option_source(option),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_product_option_value_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_product_option_value_by_id(store, id) {
    Some(option_value) ->
      project_graphql_value(
        product_option_value_source(option_value),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_selling_plan_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let plan =
    store.list_effective_selling_plan_groups(store)
    |> list.flat_map(fn(group) { group.selling_plans })
    |> list.find(fn(plan) { plan.id == id })
  case plan {
    Ok(plan) ->
      project_graphql_value(
        captured_json_source(plan.data),
        selections,
        fragments,
      )
    Error(_) -> json.null()
  }
}

fn product_variants_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let variants = store.get_effective_variants_by_product_id(store, product.id)
  let edges =
    variants
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(variant, index) = pair
      src_object([
        #("cursor", SrcString(product_variant_cursor(variant, index))),
        #("node", product_variant_source(store, variant)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(variants, fn(variant) {
          product_variant_source(store, variant)
        }),
      ),
    ),
    #("pageInfo", connection_page_info_source(variants, product_variant_cursor)),
  ])
}

fn serialize_product_variants_for_product_connection(
  store: Store,
  product: ProductRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let variants = store.get_effective_variants_by_product_id(store, product.id)
  let window =
    paginate_connection_items(
      variants,
      field,
      variables,
      product_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        serialize_product_variant_object(
          store,
          variant,
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          node_field,
          variables,
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_product_variant_object(
  store: Store,
  variant: ProductVariantRecord,
  selections: List(Selection),
  owner_field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let source = product_variant_source(store, variant)
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "metafield" ->
              serialize_product_metafield(
                store,
                variant.id,
                selection,
                variables,
              )
            "metafields" ->
              serialize_product_metafields_connection(
                store,
                variant.id,
                selection,
                variables,
              )
            _ -> project_graphql_field_value(source, selection, fragments)
          }
        _ -> project_graphql_field_value(source, owner_field, fragments)
      }
      #(key, value)
    }),
  )
}

pub fn product_variant_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  product_variant_source_with_inventory(
    store,
    variant,
    variant_inventory_item_source(variant),
  )
}

fn product_variant_source_without_inventory(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  product_variant_source_with_inventory(store, variant, SrcNull)
}

fn product_variant_source_with_inventory(
  store: Store,
  variant: ProductVariantRecord,
  inventory_item: SourceValue,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductVariant")),
    #("id", SrcString(variant.id)),
    #("title", SrcString(variant.title)),
    #("sku", graphql_helpers.option_string_source(variant.sku)),
    #("barcode", graphql_helpers.option_string_source(variant.barcode)),
    #("price", graphql_helpers.option_string_source(variant.price)),
    #(
      "compareAtPrice",
      graphql_helpers.option_string_source(variant.compare_at_price),
    ),
    #("taxable", graphql_helpers.option_bool_source(variant.taxable)),
    #(
      "inventoryPolicy",
      graphql_helpers.option_string_source(variant.inventory_policy),
    ),
    #(
      "inventoryQuantity",
      graphql_helpers.option_int_source(variant.inventory_quantity),
    ),
    #(
      "selectedOptions",
      SrcList(list.map(variant.selected_options, selected_option_source)),
    ),
    #("inventoryItem", inventory_item),
    #("product", variant_product_source(store, variant.product_id)),
    #("media", variant_media_connection_source(store, variant)),
    #(
      "sellingPlanGroups",
      selling_plan_group_connection_source(
        store.list_effective_selling_plan_groups_visible_for_product_variant(
          store,
          variant.id,
        ),
      ),
    ),
    #(
      "sellingPlanGroupsCount",
      count_source(
        list.length(
          store.list_effective_selling_plan_groups_for_product_variant(
            store,
            variant.id,
          ),
        ),
      ),
    ),
    #(
      "contextualPricing",
      optional_captured_json_source(variant.contextual_pricing),
    ),
  ])
}

fn selling_plan_group_connection_source(
  groups: List(SellingPlanGroupRecord),
) -> SourceValue {
  src_object([
    #("nodes", SrcList(list.map(groups, selling_plan_group_summary_source))),
  ])
}

fn selling_plan_group_summary_source(
  group: SellingPlanGroupRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("SellingPlanGroup")),
    #("id", SrcString(group.id)),
    #("name", SrcString(group.name)),
    #("merchantCode", SrcString(group.merchant_code)),
  ])
}

fn variant_media_connection_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  let product_media =
    store.get_effective_media_by_product_id(store, variant.product_id)
  let media =
    variant.media_ids
    |> list.filter_map(fn(media_id) {
      find_media_by_id(product_media, media_id) |> option_to_result
    })
  src_object([
    #(
      "edges",
      SrcList(
        list.map(enumerate_items(media), fn(entry) {
          let #(record, index) = entry
          src_object([
            #("cursor", SrcString(product_media_cursor(record, index))),
            #("node", product_media_source(record)),
          ])
        }),
      ),
    ),
    #("nodes", SrcList(list.map(media, product_media_source))),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", connection_start_cursor(media, product_media_cursor)),
        #("endCursor", connection_end_cursor(media, product_media_cursor)),
      ]),
    ),
  ])
}

fn variant_inventory_item_source(variant: ProductVariantRecord) -> SourceValue {
  case variant.inventory_item {
    Some(item) -> inventory_item_source_without_variant(item)
    None -> SrcNull
  }
}

fn inventory_item_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  case variant.inventory_item {
    Some(item) ->
      inventory_item_source_with_variant(
        item,
        product_variant_source_without_inventory(store, variant),
      )
    None -> SrcNull
  }
}

fn inventory_item_source_without_variant(
  item: InventoryItemRecord,
) -> SourceValue {
  inventory_item_source_with_variant(item, SrcNull)
}

fn inventory_item_source_with_variant(
  item: InventoryItemRecord,
  variant: SourceValue,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryItem")),
    #("id", SrcString(item.id)),
    #("tracked", graphql_helpers.option_bool_source(item.tracked)),
    #(
      "requiresShipping",
      graphql_helpers.option_bool_source(item.requires_shipping),
    ),
    #("measurement", optional_measurement_source(item.measurement)),
    #(
      "countryCodeOfOrigin",
      graphql_helpers.option_string_source(item.country_code_of_origin),
    ),
    #(
      "provinceCodeOfOrigin",
      graphql_helpers.option_string_source(item.province_code_of_origin),
    ),
    #(
      "harmonizedSystemCode",
      graphql_helpers.option_string_source(item.harmonized_system_code),
    ),
    #(
      "inventoryLevels",
      inventory_levels_connection_source(active_inventory_levels(
        item.inventory_levels,
      )),
    ),
    #("variant", variant),
  ])
}

fn serialize_inventory_item_object(
  store: Store,
  variant: ProductVariantRecord,
  selections: List(Selection),
  owner_field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case variant.inventory_item {
    Some(item) -> {
      let source = inventory_item_source(store, variant)
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "inventoryLevels" ->
                  serialize_inventory_item_levels_field(
                    item,
                    selection,
                    variables,
                    fragments,
                  )
                "inventoryLevel" ->
                  serialize_inventory_item_level_field(
                    item,
                    selection,
                    variables,
                    fragments,
                  )
                _ -> project_graphql_field_value(source, selection, fragments)
              }
            _ -> project_graphql_field_value(source, owner_field, fragments)
          }
          #(key, value)
        }),
      )
    }
    None -> json.null()
  }
}

fn serialize_inventory_item_levels_field(
  item: InventoryItemRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let levels =
    filter_inventory_levels_by_include_inactive(
      item.inventory_levels,
      read_include_inactive_argument(field, variables),
    )
  project_graphql_field_value(
    src_object([
      #("inventoryLevels", inventory_levels_connection_source(levels)),
    ]),
    field,
    fragments,
  )
}

fn serialize_inventory_item_level_field(
  item: InventoryItemRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let include_inactive = read_include_inactive_argument(field, variables)
  case read_string_argument(field, variables, "locationId") {
    Some(location_id) ->
      case find_inventory_level(item.inventory_levels, location_id) {
        Some(level) ->
          case include_inactive || inventory_level_is_active(level) {
            True ->
              project_graphql_value(
                inventory_level_source(level),
                get_selected_child_fields(
                  field,
                  default_selected_field_options(),
                ),
                fragments,
              )
            False -> json.null()
          }
        None -> json.null()
      }
    None -> json.null()
  }
}

fn inventory_levels_connection_source(
  levels: List(InventoryLevelRecord),
) -> SourceValue {
  let edges =
    levels
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(level, index) = pair
      src_object([
        #("cursor", SrcString(inventory_level_cursor(level, index))),
        #("node", inventory_level_source(level)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #("nodes", SrcList(list.map(levels, inventory_level_source))),
    #("pageInfo", connection_page_info_source(levels, inventory_level_cursor)),
  ])
}

fn inventory_level_source(level: InventoryLevelRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryLevel")),
    #("id", SrcString(level.id)),
    #("isActive", graphql_helpers.option_bool_source(level.is_active)),
    #(
      "location",
      src_object([
        #("__typename", SrcString("Location")),
        #("id", SrcString(level.location.id)),
        #("name", SrcString(level.location.name)),
      ]),
    ),
    #("quantities", SrcList(list.map(level.quantities, quantity_source))),
  ])
}

fn inventory_level_source_with_item(
  store: Store,
  variant: ProductVariantRecord,
  level: InventoryLevelRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryLevel")),
    #("id", SrcString(level.id)),
    #("isActive", graphql_helpers.option_bool_source(level.is_active)),
    #(
      "location",
      src_object([
        #("__typename", SrcString("Location")),
        #("id", SrcString(level.location.id)),
        #("name", SrcString(level.location.name)),
      ]),
    ),
    #("quantities", SrcList(list.map(level.quantities, quantity_source))),
    #("item", inventory_item_source(store, variant)),
  ])
}

fn quantity_source(quantity: InventoryQuantityRecord) -> SourceValue {
  src_object([
    #("name", SrcString(quantity.name)),
    #("quantity", SrcInt(quantity.quantity)),
    #("updatedAt", graphql_helpers.option_string_source(quantity.updated_at)),
  ])
}

fn optional_measurement_source(
  measurement: Option(InventoryMeasurementRecord),
) -> SourceValue {
  case measurement {
    Some(value) ->
      src_object([#("weight", optional_weight_source(value.weight))])
    None -> SrcNull
  }
}

fn optional_weight_source(
  weight: Option(InventoryWeightRecord),
) -> SourceValue {
  case weight {
    Some(value) ->
      src_object([
        #("unit", SrcString(value.unit)),
        #("value", inventory_weight_value_source(value.value)),
      ])
    None -> SrcNull
  }
}

fn inventory_weight_value_source(value) -> SourceValue {
  case value {
    InventoryWeightInt(value) -> SrcInt(value)
    InventoryWeightFloat(value) -> SrcFloat(value)
  }
}

fn connection_page_info_source(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  src_object([
    #("hasNextPage", SrcBool(False)),
    #("hasPreviousPage", SrcBool(False)),
    #("startCursor", connection_start_cursor(items, get_cursor)),
    #("endCursor", connection_end_cursor(items, get_cursor)),
  ])
}

fn connection_start_cursor(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  case items {
    [first, ..] -> SrcString(get_cursor(first, 0))
    [] -> SrcNull
  }
}

fn connection_end_cursor(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  case list.last(items) {
    Ok(last) -> SrcString(get_cursor(last, list.length(items) - 1))
    Error(_) -> SrcNull
  }
}

fn inventory_level_cursor(level: InventoryLevelRecord, _index: Int) -> String {
  case level.cursor {
    Some(cursor) -> cursor
    None -> level.id
  }
}

fn enumerate_items(items: List(a)) -> List(#(a, Int)) {
  enumerate_items_loop(items, 0, [])
}

fn enumerate_items_loop(
  items: List(a),
  index: Int,
  acc: List(#(a, Int)),
) -> List(#(a, Int)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      enumerate_items_loop(rest, index + 1, [#(first, index), ..acc])
  }
}

fn selected_option_source(
  selected_option: ProductVariantSelectedOptionRecord,
) -> SourceValue {
  src_object([
    #("name", SrcString(selected_option.name)),
    #("value", SrcString(selected_option.value)),
  ])
}

fn variant_product_source(store: Store, product_id: String) -> SourceValue {
  case store.get_effective_product_by_id(store, product_id) {
    Some(product) -> product_source(product)
    None -> SrcNull
  }
}

fn product_seo_source(seo: ProductSeoRecord) -> SourceValue {
  src_object([
    #("title", graphql_helpers.option_string_source(seo.title)),
    #("description", graphql_helpers.option_string_source(seo.description)),
  ])
}

fn optional_product_category_source(
  category: Option(ProductCategoryRecord),
) -> SourceValue {
  case category {
    Some(category) ->
      src_object([
        #("__typename", SrcString("TaxonomyCategory")),
        #("id", SrcString(category.id)),
        #("fullName", SrcString(category.full_name)),
      ])
    None -> SrcNull
  }
}

fn empty_connection_source() -> SourceValue {
  src_object([
    #("edges", SrcList([])),
    #("nodes", SrcList([])),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", SrcNull),
        #("endCursor", SrcNull),
      ]),
    ),
  ])
}

fn optional_captured_json_source(
  value: Option(CapturedJsonValue),
) -> SourceValue {
  case value {
    Some(value) -> captured_json_source(value)
    None -> SrcNull
  }
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
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, captured_json_source(item))
        })
        |> dict.from_list,
      )
  }
}

fn optional_string_json(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

fn optional_int_json(value: Option(Int)) -> Json {
  case value {
    Some(value) -> json.int(value)
    None -> json.null()
  }
}

fn legacy_resource_id_from_gid(id: String) -> String {
  case string.split(id, "/") |> list.last {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, "?") {
        [tail, ..] -> tail
        [] -> id
      }
    Error(_) -> id
  }
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

type ProductUserError {
  ProductUserError(field: List(String), message: String, code: Option(String))
}

type BulkVariantUserError {
  BulkVariantUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

type VariantValidationProblem {
  VariantValidationProblem(
    kind: String,
    suffix: List(String),
    bulk_suffix: List(String),
    message: String,
    bulk_code: Option(String),
    product_code: Option(String),
  )
}

type NumericRead {
  NumericValue(Float)
  NumericNotANumber
  NumericMissing
  NumericNull
}

type QuantityRead {
  QuantityInt(Int)
  QuantityFloat(Float)
  QuantityNotANumber
  QuantityMissing
  QuantityNull
}

const max_product_variants = 2048

const max_variant_price = 1.0e18

const max_variant_weight = 2.0e9

const min_inventory_quantity = -1_000_000_000

const max_inventory_quantity = 1_000_000_000

const max_variant_text_length = 255

type InventoryTransferLineItemInput {
  InventoryTransferLineItemInput(
    inventory_item_id: Option(String),
    quantity: Option(Int),
  )
}

type InventoryTransferLineItemUpdate {
  InventoryTransferLineItemUpdate(
    inventory_item_id: String,
    new_quantity: Int,
    delta_quantity: Int,
  )
}

type CollectionProductMove {
  CollectionProductMove(id: String, new_position: Int)
}

type CollectionProductPlacement {
  AppendProducts
  PrependReverseProducts
}

type VariantMediaInput {
  VariantMediaInput(variant_id: String, media_ids: List(String))
}

type NullableFieldUserError {
  NullableFieldUserError(field: Option(List(String)), message: String)
}

type InventoryAdjustmentChange {
  InventoryAdjustmentChange(
    inventory_item_id: String,
    location_id: String,
    name: String,
    delta: Int,
    quantity_after_change: Option(Int),
    ledger_document_uri: Option(String),
  )
}

type InventoryAdjustmentChangeInput {
  InventoryAdjustmentChangeInput(
    inventory_item_id: Option(String),
    location_id: Option(String),
    ledger_document_uri: Option(String),
    delta: Option(Int),
    change_from_quantity: Option(Int),
  )
}

type InventoryAdjustmentGroup {
  InventoryAdjustmentGroup(
    id: String,
    created_at: String,
    reason: String,
    reference_document_uri: Option(String),
    changes: List(InventoryAdjustmentChange),
  )
}

type InventorySetQuantityInput {
  InventorySetQuantityInput(
    inventory_item_id: Option(String),
    location_id: Option(String),
    quantity: Option(Int),
    compare_quantity: Option(Int),
    change_from_quantity: Option(Int),
  )
}

type InventoryMoveTerminalInput {
  InventoryMoveTerminalInput(
    location_id: Option(String),
    name: Option(String),
    ledger_document_uri: Option(String),
  )
}

type InventoryMoveQuantityInput {
  InventoryMoveQuantityInput(
    inventory_item_id: Option(String),
    quantity: Option(Int),
    from: InventoryMoveTerminalInput,
    to: InventoryMoveTerminalInput,
  )
}

type ProductVariantPositionInput {
  ProductVariantPositionInput(id: String, position: Int)
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    top_level_error_data_entries: List(#(String, Json)),
    /// True when local validation rejected the input before staging
    /// any state. The dispatch site records the mutation log entry as
    /// Failed (rather than Staged) so __meta/commit replay does not
    /// re-send a payload Shopify will also reject.
    staging_failed: Bool,
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(MutationOutcome, ProductsError) {
  process_mutation_with_upstream(
    store,
    identity,
    request_path,
    document,
    variables,
    empty_upstream_context(),
  )
}

pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> Result(MutationOutcome, ProductsError) {
  case get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let operation_path = get_operation_path_label(document)
      let hydrated_store =
        hydrate_products_for_live_hybrid_mutation(store, variables, upstream)
      Ok(handle_mutation_fields(
        hydrated_store,
        identity,
        document,
        operation_path,
        request_path,
        fields,
        fragments,
        variables,
      ))
    }
  }
}

const product_hydrate_nodes_query: String = "
query ProductsHydrateNodes($ids: [ID!]!) {
  nodes(ids: $ids) {
    __typename
    id
    ... on Product {
      legacyResourceId
      title
      handle
      status
      vendor
      productType
      tags
      totalInventory
      tracksInventory
      createdAt
      updatedAt
      publishedAt
      descriptionHtml
      onlineStorePreviewUrl
      templateSuffix
      seo { title description }
      options {
        id
        name
        position
        optionValues { id name hasVariants }
      }
      metafields(first: 250) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          jsonValue
          createdAt
          updatedAt
          ownerType
        }
      }
      media(first: 250) {
        nodes {
          id
          alt
          mediaContentType
          status
          preview { image { url width height } }
          image { id url altText width height }
        }
      }
      collections(first: 250) {
        nodes {
          id
          legacyResourceId
          title
          handle
          updatedAt
          description
          descriptionHtml
          sortOrder
          templateSuffix
          seo { title description }
          productsCount { count }
        }
      }
      variants(first: 250) {
        nodes {
          id
          title
          sku
          barcode
          price
          compareAtPrice
          taxable
          inventoryPolicy
          inventoryQuantity
          selectedOptions { name value }
          metafields(first: 250) {
            nodes {
              id
              namespace
              key
              type
              value
              compareDigest
              jsonValue
              createdAt
              updatedAt
              ownerType
            }
          }
          inventoryItem {
            id
            tracked
            requiresShipping
            measurement { weight { unit value } }
            inventoryLevels(first: 50) {
              nodes {
                id
                isActive
                location { id name }
                quantities(names: [\"available\", \"on_hand\", \"committed\", \"incoming\", \"reserved\", \"damaged\", \"quality_control\", \"safety_stock\"]) {
                  name
                  quantity
                  updatedAt
                }
              }
            }
          }
          sellingPlanGroups(first: 50) {
            nodes { id name merchantCode }
          }
        }
      }
      sellingPlanGroups(first: 50) {
        nodes { id name merchantCode }
      }
    }
    ... on Collection {
      legacyResourceId
      title
      handle
      updatedAt
      description
      descriptionHtml
      sortOrder
      templateSuffix
      seo { title description }
      metafields(first: 250) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          jsonValue
          createdAt
          updatedAt
          ownerType
        }
      }
      productsCount { count }
      products(first: 250) {
        edges {
          cursor
          node { id title handle status vendor productType tags totalInventory tracksInventory }
        }
      }
    }
    ... on ProductVariant {
      title
      sku
      barcode
      price
      compareAtPrice
      taxable
      inventoryPolicy
      inventoryQuantity
      selectedOptions { name value }
      product { id title handle status totalInventory tracksInventory }
      product {
        variants(first: 250) {
          nodes {
            id
            title
            sku
            barcode
            price
            compareAtPrice
            taxable
            inventoryPolicy
            inventoryQuantity
            selectedOptions { name value }
            inventoryItem {
              id
              tracked
              requiresShipping
              measurement { weight { unit value } }
            }
          }
        }
      }
      metafields(first: 250) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          jsonValue
          createdAt
          updatedAt
          ownerType
        }
      }
      inventoryItem {
        id
        tracked
        requiresShipping
        measurement { weight { unit value } }
      }
      sellingPlanGroups(first: 50) {
        nodes { id name merchantCode }
      }
    }
    ... on InventoryItem {
      tracked
      requiresShipping
      measurement { weight { unit value } }
      variant {
        id
        title
        inventoryQuantity
        selectedOptions { name value }
        product {
          id
          title
          handle
          status
          totalInventory
          tracksInventory
        }
      }
      inventoryLevels(first: 50) {
        nodes {
          id
          isActive
          location { id name }
          quantities(names: [\"available\", \"on_hand\", \"committed\", \"incoming\", \"reserved\", \"damaged\", \"quality_control\", \"safety_stock\"]) {
            name
            quantity
            updatedAt
          }
        }
      }
    }
    ... on InventoryLevel {
      id
      isActive
      location { id name }
      quantities(names: [\"available\", \"on_hand\", \"committed\", \"incoming\", \"reserved\", \"damaged\", \"quality_control\", \"safety_stock\"]) {
        name
        quantity
        updatedAt
      }
      item {
        id
        tracked
        requiresShipping
        variant {
          id
          title
          inventoryQuantity
          selectedOptions { name value }
          product {
            id
            title
            handle
            status
            totalInventory
            tracksInventory
          }
        }
      }
    }
    ... on Location {
      id
      name
      isActive
    }
  }
}
"

pub fn hydrate_products_for_live_hybrid_mutation(
  store: Store,
  variables: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  let product_ids =
    variables
    |> dict.values
    |> list.flat_map(collect_gid_strings)
    |> dedupe_hydration_ids
    |> list.filter(fn(id) { product_domain_hydratable_gid(id) })
    |> list.filter(fn(id) { !product_domain_has_effective_id(store, id) })
    |> list.sort(by: resource_ids.compare_shopify_resource_ids)
  let location_ids =
    collect_product_set_location_ids(variables)
    |> dedupe_hydration_ids
    |> list.filter(fn(id) { !location_has_effective_id(store, id) })
    |> list.sort(by: resource_ids.compare_shopify_resource_ids)
  let ids = list.append(product_ids, location_ids)
  case ids {
    [] -> store
    _ ->
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "ProductsHydrateNodes",
          product_hydrate_nodes_query,
          json.object([#("ids", json.array(ids, json.string))]),
        )
      {
        Ok(body) -> upsert_hydrated_nodes(store, body)
        Error(_) -> store
      }
  }
}

fn collect_gid_strings(value: ResolvedValue) -> List(String) {
  case value {
    StringVal(value) ->
      case string.starts_with(value, "gid://shopify/") {
        True -> [value]
        False -> []
      }
    ListVal(values) -> list.flat_map(values, collect_gid_strings)
    ObjectVal(fields) ->
      fields |> dict.values |> list.flat_map(collect_gid_strings)
    _ -> []
  }
}

fn product_domain_hydratable_gid(id: String) -> Bool {
  string.starts_with(id, "gid://shopify/Product/")
  || string.starts_with(id, "gid://shopify/Collection/")
  || string.starts_with(id, "gid://shopify/ProductVariant/")
  || string.starts_with(id, "gid://shopify/InventoryItem/")
  || string.starts_with(id, "gid://shopify/InventoryLevel/")
}

fn collect_product_set_location_ids(
  variables: Dict(String, ResolvedValue),
) -> List(String) {
  case dict.get(variables, "input") {
    Ok(ObjectVal(input)) ->
      read_object_list_field(input, "variants")
      |> list.flat_map(fn(variant) {
        read_object_list_field(variant, "inventoryQuantities")
        |> list.filter_map(fn(quantity) {
          case read_string_field(quantity, "locationId") {
            Some(id) ->
              case string.starts_with(id, "gid://shopify/Location/") {
                True -> Ok(id)
                False -> Error(Nil)
              }
            _ -> Error(Nil)
          }
        })
      })
    _ -> []
  }
}

fn location_has_effective_id(store: Store, id: String) -> Bool {
  case store.get_effective_location_by_id(store, id) {
    Some(_) -> True
    None -> False
  }
}

fn dedupe_hydration_ids(ids: List(String)) -> List(String) {
  list.fold(ids, [], fn(acc, id) {
    case list.contains(acc, id) {
      True -> acc
      False -> list.append(acc, [id])
    }
  })
}

fn product_domain_has_effective_id(draft_store: Store, id: String) -> Bool {
  case store.get_effective_product_by_id(draft_store, id) {
    Some(_) -> True
    None ->
      case store.get_effective_collection_by_id(draft_store, id) {
        Some(_) -> True
        None ->
          case store.get_effective_variant_by_id(draft_store, id) {
            Some(_) -> True
            None ->
              case
                store.find_effective_variant_by_inventory_item_id(
                  draft_store,
                  id,
                )
              {
                Some(_) -> True
                None ->
                  case store.get_effective_location_by_id(draft_store, id) {
                    Some(_) -> True
                    None -> False
                  }
              }
          }
      }
  }
}

fn upsert_hydrated_nodes(store: Store, body: commit.JsonValue) -> Store {
  case json_field(body, ["data", "nodes"]) {
    Some(commit.JsonArray(nodes)) ->
      list.fold(nodes, store, upsert_hydrated_node)
    _ -> store
  }
}

fn upsert_hydrated_node(store: Store, node: commit.JsonValue) -> Store {
  case json_string_field(node, "id") {
    Some(id) -> {
      case string.starts_with(id, "gid://shopify/Product/") {
        True -> upsert_hydrated_product(store, node)
        False ->
          case string.starts_with(id, "gid://shopify/Collection/") {
            True -> upsert_hydrated_collection(store, node)
            False ->
              case string.starts_with(id, "gid://shopify/ProductVariant/") {
                True -> upsert_hydrated_variant_without_product(store, node)
                False ->
                  case string.starts_with(id, "gid://shopify/InventoryItem/") {
                    True ->
                      upsert_hydrated_inventory_item_without_variant(
                        store,
                        id,
                        node,
                      )
                    False ->
                      case
                        string.starts_with(id, "gid://shopify/InventoryLevel/")
                      {
                        True -> upsert_hydrated_inventory_level(store, node)
                        False ->
                          case
                            string.starts_with(id, "gid://shopify/Location/")
                          {
                            True -> upsert_hydrated_location(store, node)
                            False -> store
                          }
                      }
                  }
              }
          }
      }
    }
    _ -> store
  }
}

fn upsert_hydrated_product(store: Store, node: commit.JsonValue) -> Store {
  case product_record_from_json(node) {
    None -> store
    Some(product) -> {
      let variants =
        json_array_field(node, ["variants", "nodes"])
        |> list.filter_map(fn(variant) {
          case product_variant_from_json(product.id, variant) {
            Some(record) -> Ok(record)
            None -> Error(Nil)
          }
        })
      let options =
        json_array_field(node, ["options"])
        |> list.index_map(fn(option_node, index) {
          product_option_from_json(product.id, option_node, index + 1)
        })
        |> list.filter_map(option_to_result)
      let media =
        json_array_field(node, ["media", "nodes"])
        |> list.index_map(fn(media_node, index) {
          product_media_from_json(product.id, media_node, index + 1)
        })
        |> list.filter_map(option_to_result)
      let product_metafields =
        owner_metafields_from_json(product.id, "PRODUCT", node)
      let product_collection_nodes =
        json_array_field(node, ["collections", "nodes"])
      let product_collections =
        product_collection_nodes
        |> list.filter_map(fn(collection_node) {
          case collection_record_from_json(collection_node) {
            Some(collection) -> Ok(collection)
            None -> Error(Nil)
          }
        })
      let product_collection_memberships =
        product_collections
        |> list.index_map(fn(collection, index) {
          ProductCollectionRecord(
            collection_id: collection.id,
            product_id: product.id,
            position: index + 1,
            cursor: None,
          )
        })
      let variant_metafields =
        variants
        |> list.flat_map(fn(variant) {
          json_array_field(node, ["variants", "nodes"])
          |> list.find_map(fn(variant_node) {
            case json_string_field(variant_node, "id") == Some(variant.id) {
              True ->
                Ok(owner_metafields_from_json(
                  variant.id,
                  "PRODUCTVARIANT",
                  variant_node,
                ))
              False -> Error(Nil)
            }
          })
          |> result.unwrap([])
        })
      let selling_plan_groups =
        selling_plan_groups_from_json(
          store,
          product.id,
          [],
          json_array_field(node, ["sellingPlanGroups", "nodes"]),
        )
      let variant_selling_plan_groups =
        json_array_field(node, ["variants", "nodes"])
        |> list.flat_map(fn(variant_node) {
          case json_string_field(variant_node, "id") {
            Some(variant_id) ->
              selling_plan_groups_from_json(
                store,
                product.id,
                [variant_id],
                json_array_field(variant_node, ["sellingPlanGroups", "nodes"]),
              )
            None -> []
          }
        })
      store
      |> store.upsert_base_products([product])
      |> store.upsert_base_product_variants(variants)
      |> store.upsert_base_collections(product_collections)
      |> store.upsert_base_product_collections(product_collection_memberships)
      |> store.replace_base_options_for_product(product.id, options)
      |> store.replace_base_media_for_product(product.id, media)
      |> store.replace_base_metafields_for_owner(product.id, product_metafields)
      |> replace_base_metafields_for_owners(variant_metafields)
      |> store.upsert_base_selling_plan_groups(list.append(
        selling_plan_groups,
        variant_selling_plan_groups,
      ))
    }
  }
}

fn upsert_hydrated_collection(store: Store, node: commit.JsonValue) -> Store {
  case collection_record_from_json(node) {
    None -> store
    Some(collection) -> {
      let product_entries = collection_product_entries_from_json(node)
      let product_hydrated_store =
        list.fold(product_entries, store, fn(current_store, entry) {
          let #(value, _, _) = entry
          upsert_hydrated_product(current_store, value)
        })
      let memberships =
        product_entries
        |> list.filter_map(fn(entry) {
          let #(value, position, cursor) = entry
          case json_string_field(value, "id") {
            Some(product_id) ->
              Ok(ProductCollectionRecord(
                collection_id: collection.id,
                product_id: product_id,
                position: position,
                cursor: cursor,
              ))
            None -> Error(Nil)
          }
        })
      product_hydrated_store
      |> store.upsert_base_collections([collection])
      |> store.upsert_base_product_collections(memberships)
      |> store.replace_base_metafields_for_owner(
        collection.id,
        owner_metafields_from_json(collection.id, "COLLECTION", node),
      )
    }
  }
}

fn collection_product_entries_from_json(
  node: commit.JsonValue,
) -> List(#(commit.JsonValue, Int, Option(String))) {
  let edge_entries =
    json_array_field(node, ["products", "edges"])
    |> list.index_map(fn(edge, index) {
      case json_field(edge, ["node"]) {
        Some(value) ->
          Some(#(value, index + 1, json_string_field(edge, "cursor")))
        None -> None
      }
    })
    |> list.filter_map(option_to_result)
  case edge_entries {
    [] ->
      json_array_field(node, ["products", "nodes"])
      |> list.index_map(fn(value, index) { #(value, index + 1, None) })
    _ -> edge_entries
  }
}

fn upsert_hydrated_variant_without_product(
  store: Store,
  node: commit.JsonValue,
) -> Store {
  case json_string_field_at(node, ["product", "id"]) {
    Some(product_id) ->
      case product_variant_from_json(product_id, node) {
        Some(variant) -> {
          let product_node = json_field(node, ["product"])
          let hydrated_store = case product_node {
            Some(value) ->
              case product_node_has_hydratable_fields(value) {
                True -> upsert_hydrated_product(store, value)
                False -> store
              }
            None -> store
          }
          let fallback_store = case
            store.get_effective_product_by_id(hydrated_store, product_id)
          {
            Some(_) -> hydrated_store
            None -> {
              let product =
                ProductRecord(
                  id: product_id,
                  legacy_resource_id: None,
                  title: json_string_field_at(node, ["product", "title"])
                    |> option.unwrap(product_id),
                  handle: json_string_field_at(node, ["product", "handle"])
                    |> option.unwrap(product_id),
                  status: json_string_field_at(node, ["product", "status"])
                    |> option.unwrap("ACTIVE"),
                  vendor: None,
                  product_type: None,
                  tags: [],
                  total_inventory: json_int_field_at(node, [
                    "product",
                    "totalInventory",
                  ]),
                  tracks_inventory: json_bool_field_at(node, [
                    "product",
                    "tracksInventory",
                  ]),
                  created_at: None,
                  updated_at: None,
                  published_at: None,
                  description_html: "",
                  online_store_preview_url: None,
                  template_suffix: None,
                  seo: ProductSeoRecord(title: None, description: None),
                  category: None,
                  publication_ids: [],
                  contextual_pricing: None,
                  cursor: None,
                )
              store.upsert_base_products(hydrated_store, [product])
            }
          }
          let selling_plan_groups =
            selling_plan_groups_from_json(
              fallback_store,
              product_id,
              [variant.id],
              json_array_field(node, ["sellingPlanGroups", "nodes"]),
            )
          fallback_store
          |> store.upsert_base_product_variants([variant])
          |> store.replace_base_metafields_for_owner(
            variant.id,
            owner_metafields_from_json(variant.id, "PRODUCTVARIANT", node),
          )
          |> store.upsert_base_selling_plan_groups(selling_plan_groups)
        }
        None -> store
      }
    None -> store
  }
}

fn upsert_hydrated_location(store: Store, node: commit.JsonValue) -> Store {
  case location_record_from_json(node) {
    Some(location) -> store.upsert_base_locations(store, [location])
    None -> store
  }
}

fn product_node_has_hydratable_fields(node: commit.JsonValue) -> Bool {
  option.is_some(json_string_field(node, "title"))
  || option.is_some(json_string_field(node, "handle"))
  || option.is_some(json_string_field(node, "status"))
  || option.is_some(json_field(node, ["variants"]))
}

fn replace_base_metafields_for_owners(
  store: Store,
  metafields: List(ProductMetafieldRecord),
) -> Store {
  metafields
  |> owner_ids_for_metafields([])
  |> list.fold(store, fn(current_store, owner_id) {
    current_store
    |> store.replace_base_metafields_for_owner(
      owner_id,
      list.filter(metafields, fn(metafield) { metafield.owner_id == owner_id }),
    )
  })
}

fn owner_ids_for_metafields(
  metafields: List(ProductMetafieldRecord),
  ids: List(String),
) -> List(String) {
  case metafields {
    [] -> list.reverse(ids)
    [first, ..rest] ->
      case list.contains(ids, first.owner_id) {
        True -> owner_ids_for_metafields(rest, ids)
        False -> owner_ids_for_metafields(rest, [first.owner_id, ..ids])
      }
  }
}

fn upsert_hydrated_inventory_level(
  store: Store,
  node: commit.JsonValue,
) -> Store {
  let level = inventory_level_from_json(node)
  let item_id = json_string_field_at(node, ["item", "id"])
  let product_id =
    json_string_field_at(node, ["item", "variant", "product", "id"])
  let variant_id = json_string_field_at(node, ["item", "variant", "id"])
  case level, item_id, product_id, variant_id {
    Some(level), Some(item_id), Some(product_id), Some(variant_id) -> {
      let base_item =
        inventory_item_from_json(
          item_id,
          json_field(node, ["item"]) |> option.unwrap(node),
        )
        |> option.unwrap(
          InventoryItemRecord(
            id: item_id,
            tracked: None,
            requires_shipping: None,
            measurement: None,
            country_code_of_origin: None,
            province_code_of_origin: None,
            harmonized_system_code: None,
            inventory_levels: [],
          ),
        )
      let item = InventoryItemRecord(..base_item, inventory_levels: [level])
      let product =
        ProductRecord(
          id: product_id,
          legacy_resource_id: None,
          title: json_string_field_at(node, [
            "item",
            "variant",
            "product",
            "title",
          ])
            |> option.unwrap(product_id),
          handle: json_string_field_at(node, [
            "item",
            "variant",
            "product",
            "handle",
          ])
            |> option.unwrap(product_id),
          status: json_string_field_at(node, [
            "item",
            "variant",
            "product",
            "status",
          ])
            |> option.unwrap("ACTIVE"),
          vendor: None,
          product_type: None,
          tags: [],
          total_inventory: json_int_field_at(node, [
            "item",
            "variant",
            "product",
            "totalInventory",
          ]),
          tracks_inventory: json_bool_field_at(node, [
            "item",
            "variant",
            "product",
            "tracksInventory",
          ]),
          created_at: None,
          updated_at: None,
          published_at: None,
          description_html: "",
          online_store_preview_url: None,
          template_suffix: None,
          seo: ProductSeoRecord(title: None, description: None),
          category: None,
          publication_ids: [],
          contextual_pricing: None,
          cursor: None,
        )
      let variant =
        ProductVariantRecord(
          id: variant_id,
          product_id: product_id,
          title: json_string_field_at(node, ["item", "variant", "title"])
            |> option.unwrap(""),
          sku: json_string_field_at(node, ["item", "variant", "sku"]),
          barcode: None,
          price: None,
          compare_at_price: None,
          taxable: None,
          inventory_policy: None,
          inventory_quantity: json_int_field_at(node, [
            "item",
            "variant",
            "inventoryQuantity",
          ]),
          selected_options: json_array_field(node, [
            "item",
            "variant",
            "selectedOptions",
          ])
            |> list.map(selected_option_from_json),
          media_ids: [],
          inventory_item: Some(item),
          contextual_pricing: None,
          cursor: None,
        )
      store
      |> store.upsert_base_products([product])
      |> store.upsert_base_product_variants([variant])
    }
    _, _, _, _ -> store
  }
}

fn upsert_hydrated_inventory_item_without_variant(
  store: Store,
  id: String,
  node: commit.JsonValue,
) -> Store {
  let item = inventory_item_from_json(id, node)
  let product_id = json_string_field_at(node, ["variant", "product", "id"])
  let variant_id = json_string_field_at(node, ["variant", "id"])
  case item, product_id, variant_id {
    Some(item), Some(product_id), Some(variant_id) -> {
      let product =
        ProductRecord(
          id: product_id,
          legacy_resource_id: None,
          title: json_string_field_at(node, ["variant", "product", "title"])
            |> option.unwrap(product_id),
          handle: json_string_field_at(node, ["variant", "product", "handle"])
            |> option.unwrap(product_id),
          status: json_string_field_at(node, ["variant", "product", "status"])
            |> option.unwrap("ACTIVE"),
          vendor: None,
          product_type: None,
          tags: [],
          total_inventory: json_int_field_at(node, [
            "variant",
            "product",
            "totalInventory",
          ]),
          tracks_inventory: json_bool_field_at(node, [
            "variant",
            "product",
            "tracksInventory",
          ]),
          created_at: None,
          updated_at: None,
          published_at: None,
          description_html: "",
          online_store_preview_url: None,
          template_suffix: None,
          seo: ProductSeoRecord(title: None, description: None),
          category: None,
          publication_ids: [],
          contextual_pricing: None,
          cursor: None,
        )
      let variant =
        ProductVariantRecord(
          id: variant_id,
          product_id: product_id,
          title: json_string_field_at(node, ["variant", "title"])
            |> option.unwrap(""),
          sku: json_string_field_at(node, ["variant", "sku"]),
          barcode: None,
          price: None,
          compare_at_price: None,
          taxable: None,
          inventory_policy: None,
          inventory_quantity: json_int_field_at(node, [
            "variant",
            "inventoryQuantity",
          ]),
          selected_options: json_array_field(node, [
            "variant",
            "selectedOptions",
          ])
            |> list.map(selected_option_from_json),
          media_ids: [],
          inventory_item: Some(item),
          contextual_pricing: None,
          cursor: None,
        )
      store
      |> store.upsert_base_products([product])
      |> store.upsert_base_product_variants([variant])
    }
    _, _, _ -> store
  }
}

fn product_record_from_json(node: commit.JsonValue) -> Option(ProductRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(ProductRecord(
        id: id,
        legacy_resource_id: json_string_field(node, "legacyResourceId"),
        title: json_string_field(node, "title") |> option.unwrap(""),
        handle: json_string_field(node, "handle") |> option.unwrap(id),
        status: json_string_field(node, "status") |> option.unwrap("ACTIVE"),
        vendor: json_string_field(node, "vendor"),
        product_type: json_string_field(node, "productType"),
        tags: json_string_array_field(node, ["tags"]),
        total_inventory: json_int_field(node, "totalInventory"),
        tracks_inventory: json_bool_field(node, "tracksInventory"),
        created_at: json_string_field(node, "createdAt"),
        updated_at: json_string_field(node, "updatedAt"),
        published_at: json_string_field(node, "publishedAt"),
        description_html: json_string_field(node, "descriptionHtml")
          |> option.unwrap(""),
        online_store_preview_url: json_string_field(
          node,
          "onlineStorePreviewUrl",
        ),
        template_suffix: json_string_field(node, "templateSuffix"),
        seo: ProductSeoRecord(
          title: json_string_field_at(node, ["seo", "title"]),
          description: json_string_field_at(node, ["seo", "description"]),
        ),
        category: None,
        publication_ids: json_string_array_field(node, ["publicationIds"]),
        contextual_pricing: json_field(node, ["contextualPricing"])
          |> option.map(captured_json_from_commit),
        cursor: None,
      ))
  }
}

fn collection_record_from_json(
  node: commit.JsonValue,
) -> Option(CollectionRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(CollectionRecord(
        id: id,
        legacy_resource_id: json_string_field(node, "legacyResourceId"),
        title: json_string_field(node, "title") |> option.unwrap(""),
        handle: json_string_field(node, "handle") |> option.unwrap(id),
        publication_ids: json_string_array_field(node, ["publicationIds"]),
        updated_at: json_string_field(node, "updatedAt"),
        description: json_string_field(node, "description"),
        description_html: json_string_field(node, "descriptionHtml"),
        image: None,
        sort_order: json_string_field(node, "sortOrder"),
        template_suffix: json_string_field(node, "templateSuffix"),
        seo: ProductSeoRecord(
          title: json_string_field_at(node, ["seo", "title"]),
          description: json_string_field_at(node, ["seo", "description"]),
        ),
        rule_set: None,
        products_count: json_int_field_at(node, ["productsCount", "count"]),
        is_smart: False,
        cursor: None,
        title_cursor: None,
        updated_at_cursor: None,
      ))
  }
}

fn product_option_from_json(
  product_id: String,
  node: commit.JsonValue,
  fallback_position: Int,
) -> Option(ProductOptionRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(ProductOptionRecord(
        id: id,
        product_id: product_id,
        name: json_string_field(node, "name") |> option.unwrap(""),
        position: json_int_field(node, "position")
          |> option.unwrap(fallback_position),
        option_values: json_array_field(node, ["optionValues"])
          |> list.map(product_option_value_from_json),
      ))
  }
}

fn product_option_value_from_json(
  node: commit.JsonValue,
) -> ProductOptionValueRecord {
  ProductOptionValueRecord(
    id: json_string_field(node, "id")
      |> option.unwrap(json_string_field(node, "name") |> option.unwrap("")),
    name: json_string_field(node, "name") |> option.unwrap(""),
    has_variants: json_bool_field(node, "hasVariants") |> option.unwrap(False),
  )
}

fn product_variant_from_json(
  product_id: String,
  node: commit.JsonValue,
) -> Option(ProductVariantRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(ProductVariantRecord(
        id: id,
        product_id: product_id,
        title: json_string_field(node, "title") |> option.unwrap(""),
        sku: json_string_field(node, "sku"),
        barcode: json_string_field(node, "barcode"),
        price: json_string_or_number_field(node, "price"),
        compare_at_price: json_string_or_number_field(node, "compareAtPrice"),
        taxable: json_bool_field(node, "taxable"),
        inventory_policy: json_string_field(node, "inventoryPolicy"),
        inventory_quantity: json_int_field(node, "inventoryQuantity"),
        selected_options: json_array_field(node, ["selectedOptions"])
          |> list.map(selected_option_from_json),
        media_ids: [],
        inventory_item: json_field(node, ["inventoryItem"])
          |> option.then(fn(item) {
            case json_string_field(item, "id") {
              Some(item_id) -> inventory_item_from_json(item_id, item)
              None -> None
            }
          }),
        contextual_pricing: json_field(node, ["contextualPricing"])
          |> option.map(captured_json_from_commit),
        cursor: None,
      ))
  }
}

fn product_media_from_json(
  product_id: String,
  node: commit.JsonValue,
  position: Int,
) -> Option(ProductMediaRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(ProductMediaRecord(
        key: id,
        product_id: product_id,
        position: position,
        id: Some(id),
        media_content_type: json_string_field(node, "mediaContentType"),
        alt: json_string_field(node, "alt")
          |> option.or(json_string_field_at(node, ["image", "altText"])),
        status: json_string_field(node, "status"),
        product_image_id: json_string_field_at(node, ["image", "id"]),
        image_url: json_string_field_at(node, ["image", "url"]),
        image_width: json_int_field_at(node, ["image", "width"]),
        image_height: json_int_field_at(node, ["image", "height"]),
        preview_image_url: json_string_field_at(node, [
          "preview",
          "image",
          "url",
        ]),
        source_url: json_string_field(node, "originalSource"),
      ))
  }
}

fn owner_metafields_from_json(
  owner_id: String,
  owner_type: String,
  node: commit.JsonValue,
) -> List(ProductMetafieldRecord) {
  let connection_nodes =
    list.append(
      json_array_field(node, ["metafields", "nodes"]),
      json_array_field(node, ["metafields", "edges"])
        |> list.filter_map(fn(edge) {
          json_field(edge, ["node"]) |> option_to_result
        }),
    )
  list.append(connection_nodes, direct_metafield_nodes(node))
  |> dedupe_metafield_nodes([])
  |> list.filter_map(fn(metafield_node) {
    product_metafield_from_json(owner_id, owner_type, metafield_node)
    |> option_to_result
  })
}

fn direct_metafield_nodes(node: commit.JsonValue) -> List(commit.JsonValue) {
  case node {
    commit.JsonObject(fields) ->
      fields
      |> list.filter_map(fn(pair) {
        let #(key, value) = pair
        case key {
          "metafield" | "metafields" -> Error(Nil)
          _ ->
            case json_string_field(value, "id") {
              Some(id) ->
                case string.starts_with(id, "gid://shopify/Metafield/") {
                  True -> Ok(value)
                  False -> Error(Nil)
                }
              _ -> Error(Nil)
            }
        }
      })
    _ -> []
  }
}

fn dedupe_metafield_nodes(
  nodes: List(commit.JsonValue),
  seen_ids: List(String),
) -> List(commit.JsonValue) {
  case nodes {
    [] -> []
    [first, ..rest] ->
      case json_string_field(first, "id") {
        Some(id) ->
          case list.contains(seen_ids, id) {
            True -> dedupe_metafield_nodes(rest, seen_ids)
            False -> [first, ..dedupe_metafield_nodes(rest, [id, ..seen_ids])]
          }
        None -> dedupe_metafield_nodes(rest, seen_ids)
      }
  }
}

fn product_metafield_from_json(
  owner_id: String,
  owner_type: String,
  node: commit.JsonValue,
) -> Option(ProductMetafieldRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(ProductMetafieldRecord(
        id: id,
        owner_id: owner_id,
        namespace: json_string_field(node, "namespace") |> option.unwrap(""),
        key: json_string_field(node, "key") |> option.unwrap(""),
        type_: json_string_field(node, "type"),
        value: json_string_field(node, "value"),
        compare_digest: json_string_field(node, "compareDigest"),
        json_value: json_field(node, ["jsonValue"])
          |> option.map(commit.json_value_to_json),
        created_at: json_string_field(node, "createdAt"),
        updated_at: json_string_field(node, "updatedAt"),
        owner_type: json_string_field(node, "ownerType")
          |> option.or(Some(owner_type)),
      ))
  }
}

fn selected_option_from_json(
  node: commit.JsonValue,
) -> ProductVariantSelectedOptionRecord {
  ProductVariantSelectedOptionRecord(
    name: json_string_field(node, "name") |> option.unwrap(""),
    value: json_string_field(node, "value") |> option.unwrap(""),
  )
}

fn inventory_item_from_json(
  id: String,
  node: commit.JsonValue,
) -> Option(InventoryItemRecord) {
  Some(InventoryItemRecord(
    id: id,
    tracked: json_bool_field(node, "tracked"),
    requires_shipping: json_bool_field(node, "requiresShipping"),
    measurement: inventory_measurement_from_json(
      json_field(node, ["measurement"]),
    ),
    country_code_of_origin: json_string_field(node, "countryCodeOfOrigin"),
    province_code_of_origin: json_string_field(node, "provinceCodeOfOrigin"),
    harmonized_system_code: json_string_field(node, "harmonizedSystemCode"),
    inventory_levels: json_array_field(node, ["inventoryLevels", "nodes"])
      |> list.filter_map(fn(level) {
        case inventory_level_from_json(level) {
          Some(record) -> Ok(record)
          None -> Error(Nil)
        }
      }),
  ))
}

fn inventory_measurement_from_json(
  value: Option(commit.JsonValue),
) -> Option(InventoryMeasurementRecord) {
  value
  |> option.map(fn(measurement) {
    InventoryMeasurementRecord(
      weight: inventory_weight_from_json(json_field(measurement, ["weight"])),
    )
  })
}

fn inventory_weight_from_json(
  value: Option(commit.JsonValue),
) -> Option(InventoryWeightRecord) {
  case value {
    Some(weight) ->
      case
        json_string_field(weight, "unit"),
        json_inventory_weight_value(weight, "value")
      {
        Some(unit), Some(value) ->
          Some(InventoryWeightRecord(unit: unit, value: value))
        _, _ -> None
      }
    None -> None
  }
}

fn json_inventory_weight_value(
  value: commit.JsonValue,
  key: String,
) -> Option(InventoryWeightValue) {
  case json_field(value, [key]) {
    Some(commit.JsonInt(value)) -> Some(InventoryWeightInt(value))
    Some(commit.JsonFloat(value)) -> Some(InventoryWeightFloat(value))
    _ -> None
  }
}

fn location_record_from_json(node: commit.JsonValue) -> Option(LocationRecord) {
  case json_string_field(node, "id") {
    Some(id) ->
      Some(LocationRecord(
        id: id,
        name: json_string_field(node, "name") |> option.unwrap(""),
        cursor: None,
      ))
    None -> None
  }
}

fn selling_plan_groups_from_json(
  store: Store,
  product_id: String,
  variant_ids: List(String),
  nodes: List(commit.JsonValue),
) -> List(SellingPlanGroupRecord) {
  nodes
  |> list.filter_map(fn(node) {
    selling_plan_group_from_json(store, product_id, variant_ids, node)
    |> option_to_result
  })
}

fn selling_plan_group_from_json(
  store: Store,
  _product_id: String,
  _variant_ids: List(String),
  node: commit.JsonValue,
) -> Option(SellingPlanGroupRecord) {
  case
    json_string_field(node, "id"),
    json_field(node, ["productIds"]),
    json_field(node, ["productVariantIds"])
  {
    Some(id), Some(_), _ | Some(id), _, Some(_) -> {
      let existing = store.get_effective_selling_plan_group_by_id(store, id)
      let product_ids = json_string_array_field(node, ["productIds"])
      let product_variant_ids =
        json_string_array_field(node, ["productVariantIds"])
      Some(SellingPlanGroupRecord(
        id: id,
        app_id: None,
        name: json_string_field(node, "name")
          |> option.unwrap(existing_group_name(existing)),
        merchant_code: json_string_field(node, "merchantCode")
          |> option.unwrap(existing_group_merchant_code(existing)),
        description: existing_group_description(existing),
        options: existing_group_options(existing),
        position: existing_group_position(existing),
        summary: option.then(existing, fn(group) { group.summary }),
        created_at: option.then(existing, fn(group) { group.created_at }),
        product_ids: dedupe_preserving_order(list.append(
          option.map(existing, fn(group) { group.product_ids })
            |> option.unwrap([]),
          product_ids,
        )),
        product_variant_ids: dedupe_preserving_order(list.append(
          option.map(existing, fn(group) { group.product_variant_ids })
            |> option.unwrap([]),
          product_variant_ids,
        )),
        selling_plans: option.map(existing, fn(group) { group.selling_plans })
          |> option.unwrap([]),
        cursor: option.then(existing, fn(group) { group.cursor }),
      ))
    }
    _, _, _ -> None
  }
}

fn inventory_level_from_json(
  node: commit.JsonValue,
) -> Option(InventoryLevelRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(InventoryLevelRecord(
        id: id,
        location: InventoryLocationRecord(
          id: json_string_field_at(node, ["location", "id"])
            |> option.unwrap(""),
          name: json_string_field_at(node, ["location", "name"])
            |> option.unwrap(""),
        ),
        quantities: json_array_field(node, ["quantities"])
          |> list.map(inventory_quantity_from_json),
        is_active: json_bool_field(node, "isActive"),
        cursor: None,
      ))
  }
}

fn inventory_quantity_from_json(
  node: commit.JsonValue,
) -> InventoryQuantityRecord {
  InventoryQuantityRecord(
    name: json_string_field(node, "name") |> option.unwrap("available"),
    quantity: json_int_field(node, "quantity") |> option.unwrap(0),
    updated_at: json_string_field(node, "updatedAt"),
  )
}

fn json_field(
  value: commit.JsonValue,
  path: List(String),
) -> Option(commit.JsonValue) {
  case path {
    [] -> Some(value)
    [key, ..rest] ->
      case value {
        commit.JsonObject(fields) ->
          case list.find(fields, fn(pair) { pair.0 == key }) {
            Ok(pair) -> json_field(pair.1, rest)
            Error(_) -> None
          }
        _ -> None
      }
  }
}

fn json_array_field(
  value: commit.JsonValue,
  path: List(String),
) -> List(commit.JsonValue) {
  case json_field(value, path) {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

fn json_string_field(value: commit.JsonValue, key: String) -> Option(String) {
  json_string_field_at(value, [key])
}

fn json_string_field_at(
  value: commit.JsonValue,
  path: List(String),
) -> Option(String) {
  case json_field(value, path) {
    Some(commit.JsonString(value)) -> Some(value)
    _ -> None
  }
}

fn json_string_or_number_field(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  case json_field(value, [key]) {
    Some(commit.JsonString(value)) -> Some(value)
    Some(commit.JsonInt(value)) -> Some(int.to_string(value))
    Some(commit.JsonFloat(value)) -> Some(float.to_string(value))
    _ -> None
  }
}

fn json_int_field(value: commit.JsonValue, key: String) -> Option(Int) {
  json_int_field_at(value, [key])
}

fn json_int_field_at(
  value: commit.JsonValue,
  path: List(String),
) -> Option(Int) {
  case json_field(value, path) {
    Some(commit.JsonInt(value)) -> Some(value)
    _ -> None
  }
}

fn json_bool_field(value: commit.JsonValue, key: String) -> Option(Bool) {
  json_bool_field_at(value, [key])
}

fn json_bool_field_at(
  value: commit.JsonValue,
  path: List(String),
) -> Option(Bool) {
  case json_field(value, path) {
    Some(commit.JsonBool(value)) -> Some(value)
    _ -> None
  }
}

fn json_string_array_field(
  value: commit.JsonValue,
  path: List(String),
) -> List(String) {
  json_array_field(value, path)
  |> list.filter_map(fn(item) {
    case item {
      commit.JsonString(value) -> Ok(value)
      _ -> Error(Nil)
    }
  })
}

fn captured_json_from_commit(value: commit.JsonValue) -> CapturedJsonValue {
  case value {
    commit.JsonNull -> CapturedNull
    commit.JsonBool(value) -> CapturedBool(value)
    commit.JsonInt(value) -> CapturedInt(value)
    commit.JsonFloat(value) -> CapturedFloat(value)
    commit.JsonString(value) -> CapturedString(value)
    commit.JsonArray(items) ->
      CapturedArray(list.map(items, captured_json_from_commit))
    commit.JsonObject(fields) ->
      CapturedObject(
        list.map(fields, fn(pair) {
          #(pair.0, captured_json_from_commit(pair.1))
        }),
      )
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

fn admin_api_version_at_least(
  request_path: String,
  minimum_version: String,
) -> Bool {
  case
    admin_api_version_from_path(request_path),
    parse_admin_api_version(minimum_version)
  {
    Some(version), Some(minimum) -> compare_admin_api_versions(version, minimum)
    _, _ -> False
  }
}

fn admin_api_version_from_path(path: String) -> Option(#(Int, Int)) {
  case string.split(path, "/") {
    ["", "admin", "api", version, "graphql.json"] ->
      parse_admin_api_version(version)
    _ -> None
  }
}

fn parse_admin_api_version(version: String) -> Option(#(Int, Int)) {
  case string.split(version, "-") {
    [year, month] ->
      case int.parse(year), int.parse(month) {
        Ok(parsed_year), Ok(parsed_month) -> Some(#(parsed_year, parsed_month))
        _, _ -> None
      }
    _ -> None
  }
}

fn compare_admin_api_versions(version: #(Int, Int), minimum: #(Int, Int)) {
  let #(year, month) = version
  let #(minimum_year, minimum_month) = minimum
  year > minimum_year || { year == minimum_year && month >= minimum_month }
}

fn inventory_adjust_202604_contract_error(
  enabled: Bool,
  input: Dict(String, ResolvedValue),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  case enabled {
    False -> None
    True ->
      case
        input_list_has_object_missing_field(
          input,
          "changes",
          "changeFromQuantity",
        )
      {
        True ->
          Some(inventory_missing_change_from_error(
            field,
            "InventoryChangeInput",
          ))
        False ->
          case has_idempotency_key(field, variables) {
            True -> None
            False -> Some(missing_idempotency_key_error(field))
          }
      }
  }
}

fn inventory_set_202604_contract_error(
  enabled: Bool,
  input: Dict(String, ResolvedValue),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  case enabled {
    False -> None
    True ->
      case
        input_list_has_object_missing_field(
          input,
          "quantities",
          "changeFromQuantity",
        )
      {
        True ->
          Some(inventory_missing_change_from_error(
            field,
            "InventoryQuantityInput",
          ))
        False ->
          case has_idempotency_key(field, variables) {
            True -> None
            False -> Some(missing_idempotency_key_error(field))
          }
      }
  }
}

fn input_list_has_object_missing_field(
  input: Dict(String, ResolvedValue),
  list_field: String,
  required_field: String,
) -> Bool {
  case dict.get(input, list_field) {
    Ok(ListVal(values)) ->
      list.any(values, fn(value) {
        case value {
          ObjectVal(fields) -> !dict.has_key(fields, required_field)
          _ -> False
        }
      })
    _ -> False
  }
}

fn inventory_missing_change_from_error(
  field: Selection,
  input_type: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        input_type
        <> " must include the following argument: changeFromQuantity.",
      ),
    ),
    #(
      "extensions",
      json.object([#("code", json.string("INVALID_FIELD_ARGUMENTS"))]),
    ),
    #("path", json.array([get_field_response_key(field)], json.string)),
  ])
}

fn missing_idempotency_key_error(field: Selection) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The @idempotent directive is required for this mutation but was not provided.",
      ),
    ),
    #("extensions", json.object([#("code", json.string("BAD_REQUEST"))])),
    #("path", json.array([get_field_response_key(field)], json.string)),
  ])
}

fn max_input_size_exceeded_error(
  root_name: String,
  argument_name: String,
  actual_size: Int,
  field: Selection,
  document: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The input array size of "
        <> int.to_string(actual_size)
        <> " is greater than the maximum allowed of "
        <> int.to_string(max_product_variants)
        <> ".",
      ),
    ),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #("path", json.array([root_name, argument_name], json.string)),
    #(
      "extensions",
      json.object([#("code", json.string("MAX_INPUT_SIZE_EXCEEDED"))]),
    ),
  ])
}

fn has_idempotency_key(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_idempotency_key(field, variables) {
    Some(_) -> True
    None -> False
  }
}

fn read_idempotency_key(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(String) {
  let directive_arguments = case field {
    Field(directives: directives, ..) ->
      directives
      |> list.filter_map(fn(directive) {
        case directive {
          Directive(name: name, arguments: arguments, ..)
            if name.value == "idempotent"
          -> Ok(arguments)
          _ -> Error(Nil)
        }
      })
      |> list.first
      |> option.from_result
    _ -> None
  }
  case directive_arguments {
    None -> None
    Some(arguments) -> {
      let argument = case find_argument(arguments, "key") {
        Some(argument) -> Some(argument)
        None -> find_argument(arguments, "idempotencyKey")
      }
      case argument {
        Some(Argument(value: StringValue(value: value, ..), ..)) ->
          non_empty_string(value)
        Some(Argument(value: VariableValue(variable: variable), ..)) ->
          case dict.get(variables, variable.name.value) {
            Ok(StringVal(value)) -> non_empty_string(value)
            _ -> None
          }
        _ -> None
      }
    }
  }
}

fn non_empty_string(value: String) -> Option(String) {
  let trimmed = string.trim(value)
  case string.length(trimmed) > 0 {
    True -> Some(trimmed)
    False -> None
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  request_path: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationOutcome {
  let uses_inventory_quantity_202604_contract =
    admin_api_version_at_least(request_path, "2026-04")
  let initial = #([], [], store, identity, [], [])
  let #(
    data_entries,
    all_errors,
    final_store,
    final_identity,
    all_staged,
    all_drafts,
  ) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        entries,
        errors,
        current_store,
        current_identity,
        staged_ids,
        drafts,
      ) = acc
      case field {
        Field(name: name, ..) ->
          case name.value {
            "productOptionsCreate" -> {
              let result =
                handle_product_options_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productOptionsCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productCreate" -> {
              let result =
                handle_product_create(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productCreate locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productOptionUpdate" -> {
              let result =
                handle_product_option_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productOptionUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productOptionsDelete" -> {
              let result =
                handle_product_options_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productOptionsDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productOptionsReorder" -> {
              let result =
                handle_product_options_reorder(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productOptionsReorder locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productChangeStatus" -> {
              let result =
                handle_product_change_status(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productChangeStatus locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productDelete" -> {
              let result =
                handle_product_delete(
                  current_store,
                  current_identity,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productDelete locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productUpdate" -> {
              let result =
                handle_product_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productDuplicate" -> {
              let result =
                handle_product_duplicate(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productDuplicate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productSet" -> {
              let result =
                handle_product_set(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(store.Staged, "Gleam staged productSet locally.")
                True -> #(
                  store.Failed,
                  "Gleam rejected productSet locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productVariantCreate" -> {
              let result =
                handle_product_variant_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store.Staged,
                  "Gleam staged productVariantCreate locally.",
                )
                True -> #(
                  store.Failed,
                  "Gleam rejected productVariantCreate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantUpdate" -> {
              let result =
                handle_product_variant_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store.Staged,
                  "Gleam staged productVariantUpdate locally.",
                )
                True -> #(
                  store.Failed,
                  "Gleam rejected productVariantUpdate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantDelete" -> {
              let result =
                handle_product_variant_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productVariantDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkCreate" -> {
              let result =
                handle_product_variants_bulk_create(
                  current_store,
                  current_identity,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store.Staged,
                  "Gleam staged productVariantsBulkCreate locally.",
                )
                True -> #(
                  store.Failed,
                  "Gleam rejected productVariantsBulkCreate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                case result.top_level_errors {
                  [] -> list.append(entries, [#(result.key, result.payload)])
                  _ -> list.append(entries, result.top_level_error_data_entries)
                },
                list.append(errors, result.top_level_errors),
                result.store,
                result.identity,
                case result.top_level_errors {
                  [] -> list.append(staged_ids, result.staged_resource_ids)
                  _ -> staged_ids
                },
                case result.top_level_errors {
                  [] -> list.append(drafts, [draft])
                  _ -> drafts
                },
              )
            }
            "productVariantsBulkUpdate" -> {
              let result =
                handle_product_variants_bulk_update(
                  current_store,
                  current_identity,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store.Staged,
                  "Gleam staged productVariantsBulkUpdate locally.",
                )
                True -> #(
                  store.Failed,
                  "Gleam rejected productVariantsBulkUpdate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkDelete" -> {
              let result =
                handle_product_variants_bulk_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productVariantsBulkDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkReorder" -> {
              let result =
                handle_product_variants_bulk_reorder(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productVariantsBulkReorder locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryAdjustQuantities" -> {
              let result =
                handle_inventory_adjust_quantities(
                  current_store,
                  current_identity,
                  uses_inventory_quantity_202604_contract,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryAdjustQuantities locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "inventoryActivate" -> {
              let result =
                handle_inventory_activate(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryActivate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryDeactivate" -> {
              let result =
                handle_inventory_deactivate(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryDeactivate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryBulkToggleActivation" -> {
              let result =
                handle_inventory_bulk_toggle_activation(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryBulkToggleActivation locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryItemUpdate" -> {
              let result =
                handle_inventory_item_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryItemUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventorySetQuantities" -> {
              let result =
                handle_inventory_set_quantities(
                  current_store,
                  current_identity,
                  uses_inventory_quantity_202604_contract,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventorySetQuantities locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "inventoryMoveQuantities" -> {
              let result =
                handle_inventory_move_quantities(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryMoveQuantities locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionAddProducts" -> {
              let result =
                handle_collection_add_products(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged collectionAddProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionAddProductsV2" -> {
              let result =
                handle_collection_add_products_v2(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged collectionAddProductsV2 locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionCreate" -> {
              let result =
                handle_collection_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged collectionCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionRemoveProducts" -> {
              let result =
                handle_collection_remove_products(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged collectionRemoveProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionReorderProducts" -> {
              let result =
                handle_collection_reorder_products(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged collectionReorderProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionUpdate" -> {
              let result =
                handle_collection_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged collectionUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionDelete" -> {
              let result =
                handle_collection_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged collectionDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productPublish" -> {
              let result =
                handle_product_publication_mutation(
                  current_store,
                  current_identity,
                  "ProductPublishPayload",
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productPublish locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productUnpublish" -> {
              let result =
                handle_product_publication_mutation(
                  current_store,
                  current_identity,
                  "ProductUnpublishPayload",
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productUnpublish locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "publicationCreate" | "publicationUpdate" | "publicationDelete" -> {
              let result =
                handle_publication_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "publishablePublish" | "publishableUnpublish" -> {
              let result =
                handle_publishable_publication_mutation(
                  current_store,
                  current_identity,
                  name.value == "publishablePublish",
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productFeedCreate" -> {
              let result =
                handle_product_feed_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productFeedCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productFeedDelete" -> {
              let result =
                handle_product_feed_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productFeedDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productFullSync" -> {
              let result =
                handle_product_full_sync(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productFullSync locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productBundleCreate" | "productBundleUpdate" -> {
              let result =
                handle_product_bundle_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some(
                    "Gleam staged captured Product bundle guardrails locally.",
                  ),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "combinedListingUpdate" -> {
              let result =
                handle_combined_listing_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some(
                    "Gleam staged captured combinedListingUpdate guardrails locally.",
                  ),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantRelationshipBulkUpdate" -> {
              let result =
                handle_product_variant_relationship_bulk_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some(
                    "Gleam staged captured ProductVariant relationship guardrails locally.",
                  ),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productCreateMedia"
            | "productUpdateMedia"
            | "productDeleteMedia"
            | "productReorderMedia" -> {
              let result =
                handle_product_media_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged " <> name.value <> " locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productVariantAppendMedia" | "productVariantDetachMedia" -> {
              let result =
                handle_product_variant_media_mutation(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged ProductVariant media membership locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "bulkProductResourceFeedbackCreate" -> {
              let result =
                handle_bulk_product_resource_feedback_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some(
                    "Gleam staged bulkProductResourceFeedbackCreate locally.",
                  ),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentCreate" | "inventoryShipmentCreateInTransit" -> {
              let result =
                handle_inventory_shipment_create(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentSetTracking" -> {
              let result =
                handle_inventory_shipment_set_tracking(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryShipmentSetTracking locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentMarkInTransit" -> {
              let result =
                handle_inventory_shipment_mark_in_transit(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryShipmentMarkInTransit locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentAddItems" -> {
              let result =
                handle_inventory_shipment_add_items(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryShipmentAddItems locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentRemoveItems" -> {
              let result =
                handle_inventory_shipment_remove_items(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryShipmentRemoveItems locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentReceive" -> {
              let result =
                handle_inventory_shipment_receive(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryShipmentReceive locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentUpdateItemQuantities" -> {
              let result =
                handle_inventory_shipment_update_item_quantities(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some(
                    "Gleam staged inventoryShipmentUpdateItemQuantities locally.",
                  ),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentDelete" -> {
              let result =
                handle_inventory_shipment_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged inventoryShipmentDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryTransferCreate"
            | "inventoryTransferCreateAsReadyToShip"
            | "inventoryTransferEdit"
            | "inventoryTransferSetItems"
            | "inventoryTransferRemoveItems"
            | "inventoryTransferMarkAsReadyToShip"
            | "inventoryTransferDuplicate"
            | "inventoryTransferCancel"
            | "inventoryTransferDelete" -> {
              let result =
                handle_inventory_transfer_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "shopResourceFeedbackCreate" -> {
              let result =
                handle_shop_resource_feedback_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged shopResourceFeedbackCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "sellingPlanGroupCreate"
            | "sellingPlanGroupUpdate"
            | "sellingPlanGroupDelete"
            | "sellingPlanGroupAddProducts"
            | "sellingPlanGroupRemoveProducts"
            | "sellingPlanGroupAddProductVariants"
            | "sellingPlanGroupRemoveProductVariants" -> {
              let result =
                handle_selling_plan_group_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productJoinSellingPlanGroups"
            | "productLeaveSellingPlanGroups"
            | "productVariantJoinSellingPlanGroups"
            | "productVariantLeaveSellingPlanGroups" -> {
              let result =
                handle_product_selling_plan_group_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "tagsAdd" -> {
              let result =
                handle_tags_update(
                  current_store,
                  current_identity,
                  True,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged tagsAdd locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "tagsRemove" -> {
              let result =
                handle_tags_update(
                  current_store,
                  current_identity,
                  False,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged tagsRemove locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            _ -> acc
          }
        _ -> acc
      }
    })
  let envelope = case all_errors, data_entries {
    [], _ -> json.object([#("data", json.object(data_entries))])
    _, [] -> json.object([#("errors", json.preprocessed_array(all_errors))])
    _, _ ->
      json.object([
        #("errors", json.preprocessed_array(all_errors)),
        #("data", json.object(data_entries)),
      ])
  }
  let final_staged_ids = case all_errors {
    [] -> all_staged
    _ -> []
  }
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: final_staged_ids,
    log_drafts: all_drafts,
  )
}

fn handle_product_options_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_options_create_payload(
          store,
          None,
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_options_create_payload(
              store,
              None,
              [
                ProductUserError(["productId"], "Product does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) ->
          stage_product_options_create(
            store,
            identity,
            key,
            product_id,
            read_option_create_inputs(args),
            graphql_helpers.read_arg_string(args, "variantStrategy")
              == Some("CREATE"),
            field,
            fragments,
          )
      }
  }
}

fn handle_product_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  // Real Shopify accepts both `productCreate(product: ProductCreateInput!)`
  // (current schema) and `productCreate(input: ProductInput!)` (older API
  // versions / pre-2024). Reading only `product` made the proxy fabricate
  // a misleading `["title"], "Title can't be blank"` userError when the
  // legacy shape was used; emit a structurally honest top-level error
  // instead when neither shows up at all.
  let input = case graphql_helpers.read_arg_object(args, "product") {
    Some(d) -> Some(d)
    None -> graphql_helpers.read_arg_object(args, "input")
  }
  case input {
    None -> {
      let errors =
        validate_required_field_arguments(
          field,
          variables,
          "productCreate",
          [RequiredArgument("product", "ProductCreateInput!")],
          operation_path,
          document,
        )
      mutation_error_result(key, store, identity, errors)
    }
    Some(input) -> {
      let user_errors = product_create_validation_errors(input)
      case user_errors {
        [_, ..] ->
          mutation_result(
            key,
            product_create_payload(store, None, user_errors, field, fragments),
            store,
            identity,
            [],
          )
        [] -> {
          let #(product, identity_after_product) =
            created_product_record(store, identity, input)
          let #(options, default_variant, final_identity, graph_ids) =
            make_product_create_option_graph(
              identity_after_product,
              product,
              read_object_list_field(input, "productOptions"),
            )
          let #(_, next_store) = store.upsert_staged_product(store, product)
          let next_store =
            next_store
            |> store.replace_staged_options_for_product(product.id, options)
            |> store.replace_staged_variants_for_product(product.id, [
              default_variant,
            ])
          let synced_product =
            ProductRecord(
              ..product,
              total_inventory: Some(0),
              tracks_inventory: Some(False),
            )
          let #(_, next_store) =
            store.upsert_staged_product(next_store, synced_product)
          mutation_result(
            key,
            product_create_payload(
              next_store,
              Some(synced_product),
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            [synced_product.id, ..graph_ids],
          )
        }
      }
    }
  }
}

fn handle_product_options_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_options_delete_payload(
          store,
          [],
          None,
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_options_delete_payload(
              store,
              [],
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          stage_product_options_delete(
            store,
            identity,
            key,
            product,
            read_arg_string_list(args, "options"),
            field,
            fragments,
          )
      }
  }
}

fn handle_product_options_reorder(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_options_reorder_payload(
          store,
          None,
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_options_reorder_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          stage_product_options_reorder(
            store,
            identity,
            key,
            product,
            read_arg_object_list(args, "options"),
            field,
            fragments,
          )
      }
  }
}

fn handle_product_change_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  case
    product_change_status_null_product_id_error(document, operation_path, field)
  {
    Some(error) -> mutation_error_result(key, store, identity, [error])
    None -> {
      let args = graphql_helpers.field_args(field, variables)
      case graphql_helpers.read_arg_string(args, "productId") {
        None ->
          mutation_result(
            key,
            product_change_status_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product_id) -> {
          let status = graphql_helpers.read_arg_string(args, "status")
          case is_valid_product_status(status) {
            False ->
              mutation_result(
                key,
                product_change_status_payload(
                  store,
                  None,
                  [
                    ProductUserError(
                      ["status"],
                      "Product status is required",
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
            True ->
              case store.get_effective_product_by_id(store, product_id) {
                None ->
                  mutation_result(
                    key,
                    product_change_status_payload(
                      store,
                      None,
                      [
                        ProductUserError(
                          ["productId"],
                          "Product does not exist",
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
                Some(product) -> {
                  let assert Some(next_status) = status
                  let #(updated_at, next_identity) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  let next_product =
                    ProductRecord(
                      ..product,
                      status: next_status,
                      updated_at: Some(updated_at),
                    )
                  let #(_, next_store) =
                    store.upsert_staged_product(store, next_product)
                  mutation_result(
                    key,
                    product_change_status_payload(
                      next_store,
                      Some(next_product),
                      [],
                      field,
                      fragments,
                    ),
                    next_store,
                    next_identity,
                    [next_product.id],
                  )
                }
              }
          }
        }
      }
    }
  }
}

fn handle_product_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  case product_delete_input_error(document, field, variables) {
    Some(error) -> mutation_error_result(key, store, identity, [error])
    None -> {
      let args = graphql_helpers.field_args(field, variables)
      let input = graphql_helpers.read_arg_object(args, "input")
      let id = case input {
        Some(input) -> graphql_helpers.read_arg_string(input, "id")
        None -> graphql_helpers.read_arg_string(args, "id")
      }
      case id {
        None ->
          mutation_error_result(key, store, identity, [
            build_product_delete_invalid_variable_error(
              "input",
              json.object([]),
              None,
              document,
            ),
          ])
        Some(product_id) ->
          case store.get_effective_product_by_id(store, product_id) {
            None ->
              mutation_result(
                key,
                product_delete_payload(
                  None,
                  [ProductUserError(["id"], "Product does not exist", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(_) -> {
              let next_store = store.delete_staged_product(store, product_id)
              mutation_result(
                key,
                product_delete_payload(Some(product_id), [], field, fragments),
                next_store,
                identity,
                [product_id],
              )
            }
          }
      }
    }
  }
}

fn handle_product_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "product")
  let id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case id {
    None ->
      mutation_result(
        key,
        product_update_payload(
          store,
          None,
          [ProductUserError(["id"], "Product does not exist", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id), input {
        None, _ ->
          mutation_result(
            key,
            product_update_payload(
              store,
              None,
              [ProductUserError(["id"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product), Some(input) ->
          case product_update_validation_error(input) {
            Some(error) ->
              mutation_result(
                key,
                product_update_payload(
                  store,
                  Some(product),
                  [error],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            None -> {
              let #(next_product, next_identity) =
                updated_product_record(identity, product, input)
              let #(_, next_store) =
                store.upsert_staged_product(store, next_product)
              mutation_result(
                key,
                product_update_payload(
                  next_store,
                  Some(next_product),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [next_product.id],
              )
            }
          }
        Some(_), None ->
          mutation_result(
            key,
            product_update_payload(
              store,
              None,
              [ProductUserError(["id"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
  }
}

fn handle_product_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_id = graphql_helpers.read_arg_string(args, "productId")
  let synchronous = case dict.get(args, "synchronous") {
    Ok(BoolVal(False)) -> False
    _ -> True
  }
  case product_id {
    None ->
      mutation_result(
        key,
        product_duplicate_payload(
          store,
          None,
          None,
          [
            ProductOperationUserErrorRecord(
              Some(["productId"]),
              "Product id is required",
              None,
            ),
          ],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          case synchronous {
            False ->
              stage_missing_async_product_duplicate(
                store,
                identity,
                key,
                field,
                variables,
                fragments,
              )
            True ->
              mutation_result(
                key,
                product_duplicate_payload(
                  store,
                  None,
                  None,
                  [
                    ProductOperationUserErrorRecord(
                      Some(["productId"]),
                      "Product not found",
                      None,
                    ),
                  ],
                  field,
                  variables,
                  fragments,
                ),
                store,
                identity,
                [],
              )
          }
        Some(source_product) ->
          stage_product_duplicate(
            store,
            identity,
            key,
            product_id,
            source_product,
            graphql_helpers.read_arg_string(args, "newTitle"),
            synchronous,
            field,
            variables,
            fragments,
          )
      }
  }
}

fn handle_product_set(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_object(args, "input") {
    None ->
      mutation_result(
        key,
        product_set_payload(
          store,
          None,
          None,
          [
            ProductOperationUserErrorRecord(
              Some(["input"]),
              "Product input is required",
              None,
            ),
          ],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(input) -> {
      case product_set_max_input_size_errors(input) {
        [_, ..] as errors -> mutation_error_result(key, store, identity, errors)
        [] ->
          case product_set_shape_validation_errors(input) {
            [] ->
              case
                resolve_product_set_existing_product(
                  store,
                  graphql_helpers.read_arg_object(args, "identifier"),
                  input,
                )
              {
                Ok(existing) ->
                  case product_set_validation_errors(input, existing) {
                    [] ->
                      stage_product_set(
                        store,
                        identity,
                        key,
                        existing,
                        input,
                        read_arg_bool_default_true(args, "synchronous"),
                        field,
                        variables,
                        fragments,
                      )
                    errors ->
                      mutation_rejected_result(
                        key,
                        product_set_payload(
                          store,
                          None,
                          None,
                          errors,
                          field,
                          variables,
                          fragments,
                        ),
                        store,
                        identity,
                      )
                  }
                Error(error) ->
                  mutation_rejected_result(
                    key,
                    product_set_payload(
                      store,
                      None,
                      None,
                      [error],
                      field,
                      variables,
                      fragments,
                    ),
                    store,
                    identity,
                  )
              }
            errors ->
              mutation_rejected_result(
                key,
                product_set_payload(
                  store,
                  None,
                  None,
                  errors,
                  field,
                  variables,
                  fragments,
                ),
                store,
                identity,
              )
          }
      }
    }
  }
}

fn product_set_validation_errors(
  input: Dict(String, ResolvedValue),
  existing: Option(ProductRecord),
) -> List(ProductOperationUserErrorRecord) {
  list.append(
    product_set_product_field_errors(input, existing),
    list.append(
      product_set_requires_variants_for_options_errors(input),
      list.append(
        product_set_duplicate_variant_errors(input),
        product_set_scalar_variant_errors(input),
      ),
    ),
  )
}

fn product_set_shape_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  list.append(
    product_set_variant_limit_errors(input),
    list.append(
      product_set_option_limit_errors(input),
      list.append(
        product_set_file_limit_errors(input),
        product_set_inventory_quantities_limit_errors(input),
      ),
    ),
  )
}

fn product_set_variant_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let variants = read_object_list_field(input, "variants")
  case list.length(variants) > product_set_variant_limit {
    True -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "variants"]),
        message: "Product can have a maximum of 2048 variants.",
        code: Some("EXCEEDED_VARIANT_LIMIT"),
      ),
    ]
    False -> []
  }
}

fn product_set_max_input_size_errors(
  input: Dict(String, ResolvedValue),
) -> List(Json) {
  list.append(
    product_set_variant_max_input_size_errors(input),
    product_set_inventory_quantities_max_input_size_errors(input),
  )
}

fn product_set_variant_max_input_size_errors(
  input: Dict(String, ResolvedValue),
) -> List(Json) {
  let variants = read_object_list_field(input, "variants")
  case list.length(variants) > product_set_variant_limit {
    True -> [
      max_input_size_error(list.length(variants), product_set_variant_limit, [
        "productSet",
        "input",
        "variants",
      ]),
    ]
    False -> []
  }
}

fn product_set_inventory_quantities_max_input_size_errors(
  input: Dict(String, ResolvedValue),
) -> List(Json) {
  read_object_list_field(input, "variants")
  |> list.filter_map(fn(variant_input) {
    let quantities =
      read_object_list_field(variant_input, "inventoryQuantities")
    case list.length(quantities) > product_set_inventory_quantities_limit {
      True ->
        Ok(
          max_input_size_error(
            list.length(quantities),
            product_set_inventory_quantities_limit,
            ["productSet", "input", "variants", "inventoryQuantities"],
          ),
        )
      False -> Error(Nil)
    }
  })
}

fn max_input_size_error(length: Int, maximum: Int, path: List(String)) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The input array size of "
        <> int.to_string(length)
        <> " is greater than the maximum allowed of "
        <> int.to_string(maximum)
        <> ".",
      ),
    ),
    #("path", json.array(path, json.string)),
    #(
      "extensions",
      json.object([#("code", json.string("MAX_INPUT_SIZE_EXCEEDED"))]),
    ),
  ])
}

fn product_set_option_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let options = read_object_list_field(input, "productOptions")
  let option_count_errors = case
    list.length(options) > product_set_option_limit
  {
    True -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "productOptions"]),
        message: "Options count is over the allowed limit.",
        code: Some("INVALID_INPUT"),
      ),
    ]
    False -> []
  }
  let value_count_errors =
    options
    |> enumerate_items()
    |> list.filter_map(fn(pair) {
      let #(option_input, index) = pair
      let values = read_object_list_field(option_input, "values")
      case list.length(values) > product_set_option_value_limit {
        True ->
          Ok(ProductOperationUserErrorRecord(
            field: Some([
              "input",
              "productOptions",
              int.to_string(index),
              "values",
            ]),
            message: "Option values count is over the allowed limit.",
            code: Some("INVALID_INPUT"),
          ))
        False -> Error(Nil)
      }
    })
  list.append(option_count_errors, value_count_errors)
}

fn product_set_file_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let files = read_object_list_field(input, "files")
  case list.length(files) > product_set_file_limit {
    True -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "files"]),
        message: "Files count is over the allowed limit.",
        code: Some("INVALID_INPUT"),
      ),
    ]
    False -> []
  }
}

fn product_set_inventory_quantities_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  read_object_list_field(input, "variants")
  |> enumerate_items()
  |> list.filter_map(fn(pair) {
    let #(variant_input, index) = pair
    let quantities =
      read_object_list_field(variant_input, "inventoryQuantities")
    case list.length(quantities) > product_set_inventory_quantities_limit {
      True ->
        Ok(ProductOperationUserErrorRecord(
          field: Some([
            "input",
            "variants",
            int.to_string(index),
            "inventoryQuantities",
          ]),
          message: "Inventory quantities count is over the allowed limit.",
          code: Some("INVENTORY_QUANTITIES_LIMIT_EXCEEDED"),
        ))
      False -> Error(Nil)
    }
  })
}

fn product_set_product_field_errors(
  input: Dict(String, ResolvedValue),
  existing: Option(ProductRecord),
) -> List(ProductOperationUserErrorRecord) {
  let maybe_error = case existing {
    Some(_) -> product_update_validation_error(input)
    None -> product_create_validation_error(input)
  }
  case maybe_error {
    Some(ProductUserError(field: path, message: message, code: code)) -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", ..path]),
        message: message,
        code: code,
      ),
    ]
    None -> []
  }
}

fn product_set_requires_variants_for_options_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  case
    read_object_list_field(input, "productOptions"),
    read_object_list_field(input, "variants")
  {
    [_, ..], [] -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "variants"]),
        message: "Variants input is required when updating product options",
        code: None,
      ),
    ]
    _, _ -> []
  }
}

fn product_set_scalar_variant_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  read_object_list_field(input, "variants")
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(variant_input, index) = pair
    variant_validation_problems(variant_input)
    |> list.map(fn(problem) {
      let VariantValidationProblem(suffix: suffix, message: message, ..) =
        problem
      ProductOperationUserErrorRecord(
        field: Some(["input", "variants", int.to_string(index), ..suffix]),
        message: message,
        code: Some("INVALID_VARIANT"),
      )
    })
  })
}

/// Detect input variants whose option-value tuples collide with an
/// earlier variant in the same `productSet` input. Shopify rejects these
/// at the API layer with one userError per offending later occurrence;
/// without local detection the proxy stages the duplicates and the
/// failure only surfaces at __meta/commit replay (see QA evidence in
/// `config/parity-specs/products/productSet-duplicate-variants.json`).
fn product_set_duplicate_variant_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let variant_inputs = read_object_list_field(input, "variants")
  case variant_inputs {
    [] -> []
    _ -> {
      let positions = product_set_option_positions(input)
      let signatures =
        list.map(variant_inputs, fn(variant_input) {
          product_set_variant_signature(variant_input, positions)
        })
      list.index_map(signatures, fn(signature, index) { #(index, signature) })
      |> list.filter_map(fn(pair) {
        let #(index, signature) = pair
        let earlier = list.take(signatures, index)
        case list.contains(earlier, signature) {
          False -> Error(Nil)
          True ->
            Ok(ProductOperationUserErrorRecord(
              field: Some(["input", "variants", int.to_string(index)]),
              message: "The variant '"
                <> product_set_variant_signature_title(signature)
                <> "' already exists. Please change at least one option value.",
              code: None,
            ))
        }
      })
    }
  }
}

fn product_set_option_positions(
  input: Dict(String, ResolvedValue),
) -> Dict(String, Int) {
  read_object_list_field(input, "productOptions")
  |> list.index_map(fn(option_input, index) { #(option_input, index) })
  |> list.fold(dict.new(), fn(acc, pair) {
    let #(option_input, index) = pair
    case read_string_field(option_input, "name") {
      None -> acc
      Some(name) -> {
        let position =
          read_int_field(option_input, "position")
          |> option.unwrap(index + 1)
        dict.insert(acc, name, position)
      }
    }
  })
}

fn product_set_variant_signature(
  variant_input: Dict(String, ResolvedValue),
  positions: Dict(String, Int),
) -> List(#(Int, String, String)) {
  read_object_list_field(variant_input, "optionValues")
  |> list.filter_map(fn(option_value) {
    case
      read_string_field(option_value, "optionName"),
      read_string_field(option_value, "name")
    {
      Some(option_name), Some(value) -> {
        let position = dict.get(positions, option_name) |> result.unwrap(9999)
        Ok(#(position, option_name, value))
      }
      _, _ -> Error(Nil)
    }
  })
  |> list.sort(fn(a, b) {
    let #(pos_a, _, _) = a
    let #(pos_b, _, _) = b
    int.compare(pos_a, pos_b)
  })
}

fn product_set_variant_signature_title(
  signature: List(#(Int, String, String)),
) -> String {
  signature
  |> list.map(fn(entry) {
    let #(_, _, value) = entry
    value
  })
  |> string.join(" / ")
}

fn resolve_product_set_existing_product(
  store: Store,
  identifier: Option(Dict(String, ResolvedValue)),
  input: Dict(String, ResolvedValue),
) -> Result(Option(ProductRecord), ProductOperationUserErrorRecord) {
  case identifier {
    Some(identifier) ->
      case product_set_identifier_has_reference(identifier) {
        True ->
          case product_by_identifier(store, identifier) {
            Some(product) -> validate_product_set_resolved_product(product)
            None ->
              Error(
                product_set_product_does_not_exist_error(
                  product_set_identifier_reference_field(identifier),
                ),
              )
          }
        False -> resolve_product_set_input_product(store, input)
      }
    None -> resolve_product_set_input_product(store, input)
  }
}

fn product_set_identifier_has_reference(
  identifier: Dict(String, ResolvedValue),
) -> Bool {
  dict.has_key(identifier, "id") || dict.has_key(identifier, "handle")
}

fn resolve_product_set_input_product(
  store: Store,
  input: Dict(String, ResolvedValue),
) -> Result(Option(ProductRecord), ProductOperationUserErrorRecord) {
  case product_set_input_product_reference(input) {
    Some(#(id, field)) ->
      case store.get_effective_product_by_id(store, id) {
        Some(product) -> validate_product_set_resolved_product(product)
        None -> Error(product_set_product_does_not_exist_error(field))
      }
    None -> Ok(None)
  }
}

fn product_set_input_product_reference(
  input: Dict(String, ResolvedValue),
) -> Option(#(String, List(String))) {
  case read_string_field(input, "id") {
    Some(id) -> Some(#(id, ["input", "id"]))
    None ->
      case read_string_field(input, "productId") {
        Some(product_id) -> Some(#(product_id, ["input", "productId"]))
        None -> None
      }
  }
}

fn product_set_identifier_reference_field(
  identifier: Dict(String, ResolvedValue),
) -> List(String) {
  case read_string_field(identifier, "id") {
    Some(_) -> ["identifier", "id"]
    None ->
      case read_string_field(identifier, "handle") {
        Some(_) -> ["identifier", "handle"]
        None -> ["identifier"]
      }
  }
}

fn validate_product_set_resolved_product(
  product: ProductRecord,
) -> Result(Option(ProductRecord), ProductOperationUserErrorRecord) {
  case product.status == "SUSPENDED" {
    True -> Error(product_set_product_suspended_error())
    False -> Ok(Some(product))
  }
}

fn product_set_product_does_not_exist_error(
  field: List(String),
) -> ProductOperationUserErrorRecord {
  ProductOperationUserErrorRecord(
    field: Some(field),
    message: "Product does not exist",
    code: Some("PRODUCT_DOES_NOT_EXIST"),
  )
}

fn product_set_product_suspended_error() -> ProductOperationUserErrorRecord {
  ProductOperationUserErrorRecord(
    field: Some(["input"]),
    message: "Product is suspended",
    code: Some("INVALID_PRODUCT"),
  )
}

fn read_arg_bool_default_true(
  args: Dict(String, ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(args, name) {
    Ok(BoolVal(False)) -> False
    _ -> True
  }
}

fn stage_product_set(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  existing: Option(ProductRecord),
  input: Dict(String, ResolvedValue),
  synchronous: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(product, identity_after_product) = case existing {
    Some(product) -> updated_product_record(identity, product, input)
    None -> {
      let #(created, next_identity) =
        created_product_record(store, identity, input)
      #(
        ProductRecord(
          ..created,
          online_store_preview_url: Some(make_product_preview_url(created)),
        ),
        next_identity,
      )
    }
  }
  let #(_, store_after_product) = store.upsert_staged_product(store, product)
  let #(store_after_graph, identity_after_graph, staged_ids) =
    apply_product_set_graph(
      store_after_product,
      identity_after_product,
      existing,
      product.id,
      input,
    )
  let #(synced_product, next_store, identity_after_summary) =
    sync_product_set_inventory_summary(
      store_after_graph,
      identity_after_graph,
      product.id,
      existing,
    )
  let product = synced_product |> option.unwrap(product)
  case synchronous {
    True ->
      mutation_result(
        key,
        product_set_payload(
          next_store,
          Some(product),
          None,
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity_after_summary,
        [product.id, ..staged_ids],
      )
    False -> {
      let #(operation_id, identity_after_operation) =
        synthetic_identity.make_synthetic_gid(
          identity_after_summary,
          "ProductSetOperation",
        )
      let operation =
        ProductOperationRecord(
          id: operation_id,
          type_name: "ProductSetOperation",
          product_id: Some(product.id),
          new_product_id: None,
          status: "COMPLETE",
          user_errors: [],
        )
      let #(staged_operation, store_after_operation) =
        store.stage_product_operation(next_store, operation)
      let initial_operation =
        ProductOperationRecord(
          ..staged_operation,
          product_id: None,
          status: "CREATED",
        )
      mutation_result(
        key,
        product_set_payload(
          store_after_operation,
          None,
          Some(initial_operation),
          [],
          field,
          variables,
          fragments,
        ),
        store_after_operation,
        identity_after_operation,
        [product.id, operation_id, ..staged_ids],
      )
    }
  }
}

fn make_product_preview_url(product: ProductRecord) -> String {
  "https://shopify-draft-proxy.local/products_preview?product_id="
  <> product.id
  <> "&handle="
  <> product.handle
}

fn sync_product_set_inventory_summary(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  previous_product: Option(ProductRecord),
) -> #(Option(ProductRecord), Store, SyntheticIdentityRegistry) {
  case store.get_effective_product_by_id(store, product_id) {
    None -> #(None, store, identity)
    Some(product) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let variants =
        store.get_effective_variants_by_product_id(store, product_id)
      let total_inventory = case previous_product {
        Some(previous) -> previous.total_inventory
        None -> sum_product_set_create_inventory(variants)
      }
      let next_product =
        ProductRecord(
          ..product,
          total_inventory: total_inventory,
          tracks_inventory: derive_tracks_inventory(variants),
          updated_at: Some(updated_at),
        )
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      #(Some(next_product), next_store, next_identity)
    }
  }
}

fn sum_product_set_create_inventory(
  variants: List(ProductVariantRecord),
) -> Option(Int) {
  let quantities =
    variants
    |> list.filter(fn(variant) {
      case variant.inventory_item {
        Some(item) -> item.tracked != Some(False)
        None -> True
      }
    })
    |> list.filter_map(fn(variant) {
      case variant.inventory_quantity {
        Some(quantity) -> Ok(quantity)
        None -> Error(Nil)
      }
    })
  case quantities {
    [] -> None
    _ ->
      Some(list.fold(quantities, 0, fn(total, quantity) { total + quantity }))
  }
}

fn stage_missing_async_product_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(operation_id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "ProductDuplicateOperation")
  let operation =
    ProductOperationRecord(
      id: operation_id,
      type_name: "ProductDuplicateOperation",
      product_id: None,
      new_product_id: None,
      status: "COMPLETE",
      user_errors: [
        ProductOperationUserErrorRecord(
          Some(["productId"]),
          "Product does not exist",
          None,
        ),
      ],
    )
  let #(staged_operation, next_store) =
    store.stage_product_operation(store, operation)
  let initial_operation =
    ProductOperationRecord(
      ..staged_operation,
      status: "CREATED",
      user_errors: [],
    )
  mutation_result(
    key,
    product_duplicate_payload(
      next_store,
      None,
      Some(initial_operation),
      [],
      field,
      variables,
      fragments,
    ),
    next_store,
    next_identity,
    [operation_id],
  )
}

fn stage_product_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  source_product: ProductRecord,
  new_title: Option(String),
  synchronous: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(duplicate_product, identity_after_product) =
    duplicated_product_record(store, identity, source_product, new_title)
  let #(_, store_after_product) =
    store.upsert_staged_product(store, duplicate_product)
  let #(store_after_relationships, identity_after_relationships, staged_ids) =
    duplicate_product_relationships(
      store_after_product,
      identity_after_product,
      product_id,
      duplicate_product.id,
    )
  case synchronous {
    True ->
      mutation_result(
        key,
        product_duplicate_payload(
          store_after_relationships,
          Some(duplicate_product),
          None,
          [],
          field,
          variables,
          fragments,
        ),
        store_after_relationships,
        identity_after_relationships,
        [duplicate_product.id, ..staged_ids],
      )
    False -> {
      let #(operation_id, identity_after_operation) =
        synthetic_identity.make_synthetic_gid(
          identity_after_relationships,
          "ProductDuplicateOperation",
        )
      let operation =
        ProductOperationRecord(
          id: operation_id,
          type_name: "ProductDuplicateOperation",
          product_id: Some(product_id),
          new_product_id: Some(duplicate_product.id),
          status: "COMPLETE",
          user_errors: [],
        )
      let #(staged_operation, next_store) =
        store.stage_product_operation(store_after_relationships, operation)
      let initial_operation =
        ProductOperationRecord(
          ..staged_operation,
          new_product_id: None,
          status: "CREATED",
        )
      mutation_result(
        key,
        product_duplicate_payload(
          next_store,
          None,
          Some(initial_operation),
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity_after_operation,
        [duplicate_product.id, operation_id, ..staged_ids],
      )
    }
  }
}

fn handle_product_publication_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  typename: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case id {
    None ->
      mutation_result(
        key,
        product_publication_payload(
          typename,
          store,
          None,
          [
            ProductUserError(["input", "id"], "Product id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_publication_payload(
              typename,
              store,
              None,
              [
                ProductUserError(["input", "id"], "Product not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) -> {
          let #(_, next_store) = store.upsert_staged_product(store, product)
          mutation_result(
            key,
            product_publication_payload(
              typename,
              next_store,
              Some(product),
              [],
              field,
              fragments,
            ),
            next_store,
            identity,
            [product.id],
          )
        }
      }
  }
}

fn handle_publication_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "publicationCreate" -> {
      let store = ensure_default_publication_baseline(store)
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let #(publication_id, next_identity) =
        make_unique_publication_gid(store, identity)
      let name = read_string_field(input, "name")
      let publication =
        PublicationRecord(
          id: publication_id,
          name: name,
          auto_publish: read_bool_field(input, "autoPublish"),
          supports_future_publishing: Some(False),
          catalog_id: read_string_field(input, "catalogId"),
          channel_id: read_string_field(input, "channelId"),
          cursor: None,
        )
      let #(staged, next_store) =
        store.upsert_staged_publication(store, publication)
      mutation_result(
        key,
        publication_mutation_payload(
          next_store,
          "PublicationCreatePayload",
          Some(staged),
          None,
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        [staged.id],
      )
    }
    "publicationUpdate" -> {
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let publication_id =
        graphql_helpers.read_arg_string(args, "id")
        |> option.or(read_string_field(input, "id"))
      case publication_id {
        None ->
          mutation_result(
            key,
            publication_mutation_payload(
              store,
              "PublicationUpdatePayload",
              None,
              None,
              [ProductUserError(["id"], "Publication id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(id) ->
          case store.get_effective_publication_by_id(store, id) {
            None ->
              mutation_result(
                key,
                publication_mutation_payload(
                  store,
                  "PublicationUpdatePayload",
                  None,
                  None,
                  [ProductUserError(["id"], "Publication not found", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(existing) -> {
              let publication =
                PublicationRecord(
                  ..existing,
                  name: read_string_field(input, "name")
                    |> option.or(existing.name),
                  auto_publish: read_bool_field(input, "autoPublish")
                    |> option.or(existing.auto_publish),
                  supports_future_publishing: read_bool_field(
                      input,
                      "supportsFuturePublishing",
                    )
                    |> option.or(existing.supports_future_publishing),
                  catalog_id: read_string_field(input, "catalogId")
                    |> option.or(existing.catalog_id),
                  channel_id: read_string_field(input, "channelId")
                    |> option.or(existing.channel_id),
                )
              let #(staged, next_store) =
                store.upsert_staged_publication(store, publication)
              mutation_result(
                key,
                publication_mutation_payload(
                  next_store,
                  "PublicationUpdatePayload",
                  Some(staged),
                  None,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                identity,
                [staged.id],
              )
            }
          }
      }
    }
    "publicationDelete" -> {
      case graphql_helpers.read_arg_string(args, "id") {
        None ->
          mutation_result(
            key,
            publication_mutation_payload(
              store,
              "PublicationDeletePayload",
              None,
              None,
              [ProductUserError(["id"], "Publication id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(id) ->
          case store.get_effective_publication_by_id(store, id) {
            None ->
              mutation_result(
                key,
                publication_mutation_payload(
                  store,
                  "PublicationDeletePayload",
                  None,
                  None,
                  [ProductUserError(["id"], "Publication not found", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(existing) -> {
              let next_store =
                store
                |> remove_publication_from_publishables(id)
                |> store.delete_staged_publication(id)
              mutation_result(
                key,
                publication_mutation_payload(
                  next_store,
                  "PublicationDeletePayload",
                  Some(existing),
                  Some(id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                identity,
                [id],
              )
            }
          }
      }
    }
    _ -> mutation_result(key, json.null(), store, identity, [])
  }
}

fn ensure_default_publication_baseline(store: Store) -> Store {
  case
    store.get_effective_publication_by_id(store, "gid://shopify/Publication/1")
  {
    Some(_) -> store
    None -> {
      let publication =
        PublicationRecord(
          id: "gid://shopify/Publication/1",
          name: Some("Online Store"),
          auto_publish: Some(True),
          supports_future_publishing: Some(False),
          catalog_id: None,
          channel_id: None,
          cursor: Some("cursor:gid://shopify/Publication/1"),
        )
      store.upsert_base_publications(store, [publication])
    }
  }
}

fn handle_publishable_publication_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  is_publish: Bool,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let publishable_id = graphql_helpers.read_arg_string(args, "id")
  let publication_targets = read_publication_targets(args)
  case publishable_id {
    None ->
      mutation_result(
        key,
        publishable_mutation_payload(
          store,
          None,
          [ProductUserError(["id"], "Publishable id is required", None)],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(id) ->
      case store.get_effective_product_by_id(store, id) {
        Some(product) ->
          publishable_product_result(
            store,
            identity,
            key,
            product,
            publication_targets,
            is_publish,
            field,
            variables,
            fragments,
          )
        None ->
          case store.get_effective_collection_by_id(store, id) {
            Some(collection) ->
              publishable_collection_result(
                store,
                identity,
                key,
                collection,
                publication_targets,
                is_publish,
                field,
                variables,
                fragments,
              )
            None ->
              mutation_result(
                key,
                publishable_mutation_payload(
                  store,
                  None,
                  [
                    ProductUserError(
                      ["id"],
                      "Only Product and Collection publishable IDs are supported locally",
                      None,
                    ),
                  ],
                  field,
                  variables,
                  fragments,
                ),
                store,
                identity,
                [],
              )
          }
      }
  }
}

fn publishable_product_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  publication_targets: List(String),
  is_publish: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  case publication_targets {
    [] ->
      mutation_result(
        key,
        publishable_mutation_payload(
          store,
          Some(product_source_with_store(store, product)),
          [ProductUserError(["input"], "Publication target is required", None)],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    _ -> {
      let next_publication_ids = case is_publish {
        True ->
          merge_publication_targets(
            product.publication_ids,
            publication_targets,
          )
        False ->
          remove_publication_targets(
            product.publication_ids,
            publication_targets,
          )
      }
      let next_product =
        ProductRecord(..product, publication_ids: next_publication_ids)
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      mutation_result(
        key,
        publishable_mutation_payload(
          next_store,
          Some(product_source_with_store_and_publication(
            next_store,
            next_product,
            selected_publication_id(
              get_selected_child_fields(field, default_selected_field_options()),
              variables,
            ),
          )),
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity,
        [next_product.id],
      )
    }
  }
}

fn publishable_collection_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  collection: CollectionRecord,
  publication_targets: List(String),
  is_publish: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  case publication_targets {
    [] ->
      mutation_result(
        key,
        publishable_mutation_payload(
          store,
          Some(collection_source_with_store(store, collection)),
          [ProductUserError(["input"], "Publication target is required", None)],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    _ -> {
      let next_publication_ids = case is_publish {
        True ->
          merge_publication_targets(
            collection.publication_ids,
            publication_targets,
          )
        False ->
          remove_publication_targets(
            collection.publication_ids,
            publication_targets,
          )
      }
      let next_collection =
        CollectionRecord(..collection, publication_ids: next_publication_ids)
      let next_store = store.upsert_staged_collections(store, [next_collection])
      mutation_result(
        key,
        publishable_mutation_payload(
          next_store,
          Some(collection_source_with_store_and_publication(
            next_store,
            next_collection,
            selected_publication_id(
              get_selected_child_fields(field, default_selected_field_options()),
              variables,
            ),
          )),
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity,
        [next_collection.id],
      )
    }
  }
}

fn make_unique_publication_gid(
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(String, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "Publication")
  case store.get_effective_publication_by_id(store, id) {
    Some(_) -> make_unique_publication_gid(store, next_identity)
    None -> #(id, next_identity)
  }
}

fn read_publication_targets(args: Dict(String, ResolvedValue)) -> List(String) {
  read_arg_object_list(args, "input")
  |> list.filter_map(fn(input) {
    read_string_field(input, "publicationId") |> option_to_result
  })
}

fn merge_publication_targets(
  current: List(String),
  targets: List(String),
) -> List(String) {
  list.append(current, targets) |> dedupe_preserving_order
}

fn remove_publication_targets(
  current: List(String),
  targets: List(String),
) -> List(String) {
  current
  |> list.filter(fn(id) { !list.contains(targets, id) })
}

fn remove_publication_from_publishables(
  store: Store,
  publication_id: String,
) -> Store {
  let next_store =
    store.list_effective_products(store)
    |> list.filter(fn(product) {
      list.contains(product.publication_ids, publication_id)
    })
    |> list.fold(store, fn(acc, product) {
      let next_product =
        ProductRecord(
          ..product,
          publication_ids: remove_publication_targets(product.publication_ids, [
            publication_id,
          ]),
        )
      let #(_, staged_store) = store.upsert_staged_product(acc, next_product)
      staged_store
    })
  store.list_effective_collections(next_store)
  |> list.filter(fn(collection) {
    list.contains(collection.publication_ids, publication_id)
  })
  |> list.fold(next_store, fn(acc, collection) {
    let next_collection =
      CollectionRecord(
        ..collection,
        publication_ids: remove_publication_targets(collection.publication_ids, [
          publication_id,
        ]),
      )
    store.upsert_staged_collections(acc, [next_collection])
  })
}

fn handle_product_feed_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  // The captured local-runtime fixture comes from the TS path where the
  // mutation-log entry consumes the first synthetic id before the feed is
  // minted, so preserve that observable id sequence for this staged root.
  let #(_, identity_after_log_slot) =
    synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
  let #(feed_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_log_slot,
      "ProductFeed",
    )
  let feed =
    ProductFeedRecord(
      id: feed_id,
      country: read_string_field(input, "country"),
      language: read_string_field(input, "language"),
      status: "ACTIVE",
    )
  let #(staged_feed, next_store) = store.upsert_staged_product_feed(store, feed)
  mutation_result(
    key,
    product_feed_create_payload(staged_feed, [], field, fragments),
    next_store,
    next_identity,
    [staged_feed.id],
  )
}

fn handle_product_feed_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  case id {
    Some(feed_id) ->
      case store.get_effective_product_feed_by_id(store, feed_id) {
        Some(_) -> {
          let next_store = store.delete_staged_product_feed(store, feed_id)
          mutation_result(
            key,
            product_feed_delete_payload(Some(feed_id), [], field, fragments),
            next_store,
            identity,
            [feed_id],
          )
        }
        None ->
          mutation_result(
            key,
            product_feed_delete_payload(
              None,
              [
                ProductUserError(["id"], "ProductFeed does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      mutation_result(
        key,
        product_feed_delete_payload(
          None,
          [ProductUserError(["id"], "ProductFeed does not exist", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

fn handle_product_full_sync(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  case id {
    Some(feed_id) ->
      case store.get_effective_product_feed_by_id(store, feed_id) {
        Some(_) ->
          mutation_result(
            key,
            product_full_sync_payload(Some(feed_id), [], field, fragments),
            store,
            identity,
            [feed_id],
          )
        None ->
          mutation_result(
            key,
            product_full_sync_payload(
              None,
              [
                ProductUserError(["id"], "ProductFeed does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      mutation_result(
        key,
        product_full_sync_payload(
          None,
          [ProductUserError(["id"], "ProductFeed does not exist", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

fn handle_product_bundle_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let product_id = read_string_field(input, "productId")
  let existing_product = case product_id {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None -> None
  }
  let user_errors = case root_name, product_id, existing_product {
    "productBundleUpdate", _, None -> [
      NullableFieldUserError(None, "Product does not exist"),
    ]
    _, _, _ -> {
      case read_object_list_field(input, "components") {
        [] -> [
          NullableFieldUserError(None, "At least one component is required."),
        ]
        _ -> []
      }
    }
  }
  mutation_result(
    key,
    product_bundle_mutation_payload(root_name, user_errors, field, fragments),
    store,
    identity,
    [],
  )
}

fn handle_combined_listing_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let parent_product_id =
    graphql_helpers.read_arg_string(args, "parentProductId")
  let parent_product = case parent_product_id {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None -> None
  }
  case parent_product {
    None ->
      mutation_result(
        key,
        combined_listing_update_payload(
          SrcNull,
          [
            ProductUserError(
              ["parentProductId"],
              "Product does not exist",
              Some("PARENT_PRODUCT_NOT_FOUND"),
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product) ->
      mutation_result(
        key,
        combined_listing_update_payload(
          product_source_with_store(store, product),
          [],
          field,
          fragments,
        ),
        store,
        identity,
        [product.id],
      )
  }
}

fn handle_product_variant_relationship_bulk_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inputs = read_arg_object_list(args, "input")
  let missing_ids =
    inputs
    |> list.flat_map(missing_variant_relationship_ids(store))
  let user_errors = case missing_ids {
    [] -> []
    _ -> [
      ProductUserError(
        ["input"],
        "The product variants with ID(s) "
          <> json_string_array_literal(missing_ids)
          <> " could not be found.",
        Some("PRODUCT_VARIANTS_NOT_FOUND"),
      ),
    ]
  }
  mutation_result(
    key,
    product_variant_relationship_bulk_update_payload(
      user_errors,
      field,
      fragments,
    ),
    store,
    identity,
    [],
  )
}

fn missing_variant_relationship_ids(
  store: Store,
) -> fn(Dict(String, ResolvedValue)) -> List(String) {
  fn(input) {
    let parent_variant_id = case
      read_string_field(input, "parentProductVariantId")
    {
      Some(id) -> Some(id)
      None ->
        case read_string_field(input, "parentProductId") {
          Some(product_id) ->
            store.get_effective_variants_by_product_id(store, product_id)
            |> list.first
            |> option.from_result
            |> option.map(fn(variant) { variant.id })
          None -> None
        }
    }
    let parent_missing = case parent_variant_id {
      Some(id) ->
        case store.get_effective_variant_by_id(store, id) {
          Some(_) -> []
          None -> [id]
        }
      None -> []
    }
    let relationship_ids =
      list.append(
        read_object_list_field(input, "productVariantRelationshipsToCreate"),
        read_object_list_field(input, "productVariantRelationshipsToUpdate"),
      )
      |> list.filter_map(fn(relationship) {
        read_string_field(relationship, "id") |> option_to_result
      })
    let relationship_missing =
      relationship_ids
      |> list.filter(fn(id) {
        case store.get_effective_variant_by_id(store, id) {
          Some(_) -> False
          None -> True
        }
      })
    list.append(parent_missing, relationship_missing)
  }
}

fn json_string_array_literal(values: List(String)) -> String {
  let content =
    values
    |> list.map(fn(value) { "\"" <> value <> "\"" })
    |> string.join(",")
  "[" <> content <> "]"
}

fn handle_product_media_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  case root_name {
    "productCreateMedia" ->
      handle_product_create_media(
        store,
        identity,
        document,
        field,
        fragments,
        variables,
      )
    "productUpdateMedia" ->
      handle_product_update_media(
        store,
        identity,
        document,
        field,
        fragments,
        variables,
      )
    "productDeleteMedia" ->
      handle_product_delete_media(
        store,
        identity,
        document,
        field,
        fragments,
        variables,
      )
    "productReorderMedia" ->
      handle_product_reorder_media(store, identity, field, fragments, variables)
    _ ->
      mutation_result(
        get_field_response_key(field),
        json.null(),
        store,
        identity,
        [],
      )
  }
}

fn handle_product_create_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_id = graphql_helpers.read_arg_string(args, "productId")
  case product_id {
    Some("") ->
      mutation_error_result(key, store, identity, [
        invalid_product_media_product_id_variable_error("", document),
      ])
    _ ->
      case invalid_create_media_content_type(args, document) {
        Some(error) -> mutation_error_result(key, store, identity, [error])
        None ->
          case product_id {
            None ->
              mutation_result(
                key,
                product_create_media_payload(
                  store,
                  [],
                  [
                    ProductUserError(
                      ["productId"],
                      "Product id is required",
                      None,
                    ),
                  ],
                  None,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(product_id) ->
              case store.get_effective_product_by_id(store, product_id) {
                None ->
                  mutation_result(
                    key,
                    product_media_not_found_payload(
                      store,
                      "create",
                      field,
                      fragments,
                    ),
                    store,
                    identity,
                    [],
                  )
                Some(_) ->
                  stage_product_create_media(
                    store,
                    identity,
                    key,
                    product_id,
                    read_arg_object_list(args, "media"),
                    field,
                    fragments,
                  )
              }
          }
      }
  }
}

fn stage_product_create_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let existing_media =
    store.get_effective_media_by_product_id(store, product_id)
  let initial = #(identity, [], [])
  let #(next_identity, created_reversed, user_errors_reversed) =
    inputs
    |> enumerate_items()
    |> list.fold(initial, fn(acc, entry) {
      let #(current_identity, created, errors) = acc
      let #(input, index) = entry
      let media_content_type =
        read_string_field(input, "mediaContentType") |> option.unwrap("IMAGE")
      case
        media_content_type == "IMAGE"
        && !is_valid_media_source(read_string_field(input, "originalSource"))
      {
        True -> #(current_identity, created, [
          ProductUserError(
            ["media", int.to_string(index), "originalSource"],
            "Image URL is invalid",
            None,
          ),
          ..errors
        ])
        False -> {
          let position = list.length(existing_media) + list.length(created)
          let #(record, identity_after_record) =
            make_created_media_record(
              current_identity,
              product_id,
              input,
              position,
            )
          #(identity_after_record, [record, ..created], errors)
        }
      }
    })
  let created_media = list.reverse(created_reversed)
  let user_errors = list.reverse(user_errors_reversed)
  let response_store = case created_media {
    [] -> store
    _ ->
      store.replace_staged_media_for_product(
        store,
        product_id,
        list.append(existing_media, created_media),
      )
  }
  let final_store = case created_media {
    [] -> store
    _ ->
      store.replace_staged_media_for_product(
        store,
        product_id,
        list.append(
          existing_media,
          list.map(created_media, transition_created_media_to_processing),
        ),
      )
  }
  let product = store.get_effective_product_by_id(response_store, product_id)
  let staged_ids = created_media |> list.filter_map(media_record_id_result)
  mutation_result(
    key,
    product_create_media_payload(
      response_store,
      created_media,
      user_errors,
      product,
      field,
      fragments,
    ),
    final_store,
    next_identity,
    staged_ids,
  )
}

fn handle_product_update_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_id = graphql_helpers.read_arg_string(args, "productId")
  case product_id {
    Some("") ->
      mutation_error_result(key, store, identity, [
        invalid_product_media_product_id_variable_error("", document),
      ])
    None ->
      mutation_result(
        key,
        product_update_media_payload(
          [],
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_media_not_found_payload(store, "update", field, fragments),
            store,
            identity,
            [],
          )
        Some(_) ->
          stage_product_update_media(
            store,
            identity,
            key,
            product_id,
            read_arg_object_list(args, "media"),
            field,
            fragments,
          )
      }
  }
}

fn stage_product_update_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  updates: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  case first_missing_media_update(updates, effective_media) {
    Some(update) -> {
      let media_id = read_string_field(update, "id")
      let media_value = case media_id {
        Some(_) -> SrcNull
        None -> SrcList([])
      }
      let error = case media_id {
        Some(id) ->
          ProductUserError(
            ["media"],
            "Media id " <> id <> " does not exist",
            None,
          )
        None -> ProductUserError(["media", "id"], "Media id is required", None)
      }
      mutation_result(
        key,
        product_update_media_payload_with_media_value(
          media_value,
          [error],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    }
    None ->
      case first_non_ready_media_update(updates, effective_media) {
        Some(index) ->
          mutation_result(
            key,
            product_update_media_payload(
              [],
              [
                ProductUserError(
                  ["media", int.to_string(index), "id"],
                  "Non-ready media cannot be updated.",
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
        None -> {
          let updated_media =
            effective_media
            |> list.map(fn(media) {
              case find_media_update(updates, media.id) {
                Some(update) -> update_media_record(media, update)
                None -> settle_media_to_ready(media)
              }
            })
          let changed_media =
            updated_media
            |> list.filter(fn(media) {
              case find_media_update(updates, media.id) {
                Some(_) -> True
                None -> False
              }
            })
          let next_store =
            store.replace_staged_media_for_product(
              store,
              product_id,
              updated_media,
            )
          mutation_result(
            key,
            product_update_media_payload(changed_media, [], field, fragments),
            next_store,
            identity,
            changed_media |> list.filter_map(media_record_id_result),
          )
        }
      }
  }
}

fn handle_product_delete_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_id = graphql_helpers.read_arg_string(args, "productId")
  case product_id {
    Some("") ->
      mutation_error_result(key, store, identity, [
        invalid_product_media_product_id_variable_error("", document),
      ])
    None ->
      mutation_result(
        key,
        product_delete_media_payload(
          store,
          None,
          SrcList([]),
          SrcList([]),
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_media_not_found_payload(store, "delete", field, fragments),
            store,
            identity,
            [],
          )
        Some(_) ->
          stage_product_delete_media(
            store,
            identity,
            key,
            product_id,
            read_arg_string_list(args, "mediaIds"),
            field,
            fragments,
          )
      }
  }
}

fn stage_product_delete_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  media_ids: List(String),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  case first_unknown_media_id(media_ids, effective_media) {
    Some(media_id) ->
      mutation_result(
        key,
        product_delete_media_payload(
          store,
          None,
          SrcNull,
          SrcNull,
          [
            ProductUserError(
              ["mediaIds"],
              "Media id " <> media_id <> " does not exist",
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
    None -> {
      let deleted_media =
        effective_media
        |> list.filter(fn(media) {
          case media.id {
            Some(id) -> list.contains(media_ids, id)
            None -> False
          }
        })
      let next_media =
        effective_media
        |> list.filter(fn(media) {
          case media.id {
            Some(id) -> !list.contains(media_ids, id)
            None -> True
          }
        })
      let next_store =
        store.replace_staged_media_for_product(store, product_id, next_media)
      let deleted_media_ids =
        deleted_media |> list.filter_map(media_record_id_result)
      let deleted_product_image_ids =
        deleted_media |> list.filter_map(product_media_product_image_id_result)
      let product = store.get_effective_product_by_id(next_store, product_id)
      mutation_result(
        key,
        product_delete_media_payload(
          next_store,
          product,
          SrcList(list.map(deleted_media_ids, SrcString)),
          SrcList(list.map(deleted_product_image_ids, SrcString)),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        deleted_media_ids,
      )
    }
  }
}

fn handle_product_reorder_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        product_reorder_media_payload(
          None,
          [ProductUserError(["id"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_reorder_media_payload(
              None,
              [ProductUserError(["id"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let #(next_store, user_errors) =
            reorder_product_media(
              store,
              product_id,
              read_arg_object_list(args, "moves"),
            )
          case user_errors {
            [] -> {
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              mutation_result(
                key,
                product_reorder_media_payload(
                  Some(job_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [product_id],
              )
            }
            _ ->
              mutation_result(
                key,
                product_reorder_media_payload(
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
          }
        }
      }
  }
}

fn reorder_product_media(
  store: Store,
  product_id: String,
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(Store, List(ProductUserError)) {
  let #(moves, user_errors) = read_collection_product_moves(raw_moves)
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  let media_ids =
    effective_media
    |> list.filter_map(media_record_id_result)
  let user_errors =
    list.fold(enumerate_items(moves), user_errors, fn(errors, entry) {
      let #(move, index) = entry
      let CollectionProductMove(id: media_id, new_position: _) = move
      case list.contains(media_ids, media_id) {
        True -> errors
        False ->
          list.append(errors, [
            ProductUserError(
              ["moves", int.to_string(index), "id"],
              "Media does not exist",
              None,
            ),
          ])
      }
    })
  case user_errors {
    [] -> {
      let reordered_media =
        apply_product_media_moves(effective_media, moves)
        |> enumerate_items()
        |> list.map(fn(entry) {
          let #(media, position) = entry
          ProductMediaRecord(..media, position: position)
        })
      #(
        store.replace_staged_media_for_product(
          store,
          product_id,
          reordered_media,
        ),
        [],
      )
    }
    _ -> #(store, user_errors)
  }
}

fn apply_product_media_moves(
  media: List(ProductMediaRecord),
  moves: List(CollectionProductMove),
) -> List(ProductMediaRecord) {
  list.fold(moves, media, fn(current_media, move) {
    let CollectionProductMove(id: media_id, new_position: new_position) = move
    case find_media_by_id(current_media, media_id) {
      None -> current_media
      Some(record) -> {
        let without_record =
          current_media
          |> list.filter(fn(candidate) { candidate.id != Some(media_id) })
        insert_product_media_at_position(without_record, record, new_position)
      }
    }
  })
}

fn insert_product_media_at_position(
  media: List(ProductMediaRecord),
  record: ProductMediaRecord,
  position: Int,
) -> List(ProductMediaRecord) {
  let insertion_index = int.min(position, list.length(media))
  let before = list.take(media, insertion_index)
  let after = list.drop(media, insertion_index)
  list.append(before, [record, ..after])
}

fn handle_product_variant_media_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variant_media_payload(
          store,
          None,
          [],
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variant_media_payload(
              store,
              None,
              [],
              [ProductUserError(["productId"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) -> {
          let inputs =
            read_variant_media_inputs(read_arg_object_list(args, "variantMedia"))
          let is_append = case field {
            Field(name: name, ..) -> name.value == "productVariantAppendMedia"
            _ -> False
          }
          let #(next_store, updated_variant_ids, user_errors) =
            stage_variant_media_memberships(
              store,
              product_id,
              inputs,
              is_append,
            )
          let response_store = case user_errors {
            [] -> next_store
            _ -> store
          }
          let variants =
            updated_variant_ids
            |> dedupe_preserving_order
            |> list.filter_map(fn(variant_id) {
              store.get_effective_variant_by_id(response_store, variant_id)
              |> option_to_result
            })
          let staged_ids = case user_errors {
            [] -> [product_id, ..dedupe_preserving_order(updated_variant_ids)]
            _ -> []
          }
          mutation_result(
            key,
            product_variant_media_payload(
              response_store,
              Some(product),
              variants,
              user_errors,
              field,
              fragments,
            ),
            response_store,
            identity,
            staged_ids,
          )
        }
      }
  }
}

fn read_variant_media_inputs(
  raw_inputs: List(Dict(String, ResolvedValue)),
) -> List(VariantMediaInput) {
  raw_inputs
  |> list.filter_map(fn(input) {
    case read_string_field(input, "variantId") {
      Some(variant_id) ->
        Ok(VariantMediaInput(
          variant_id: variant_id,
          media_ids: read_arg_string_list(input, "mediaIds"),
        ))
      None -> Error(Nil)
    }
  })
}

fn stage_variant_media_memberships(
  store: Store,
  product_id: String,
  inputs: List(VariantMediaInput),
  is_append: Bool,
) -> #(Store, List(String), List(ProductUserError)) {
  let effective_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let product_media_ids =
    store.get_effective_media_by_product_id(store, product_id)
    |> list.filter_map(media_record_id_result)
  let #(next_variants, updated_variant_ids, user_errors) =
    list.fold(enumerate_items(inputs), #([], [], []), fn(acc, item) {
      let #(updated_variants, updated_ids, errors) = acc
      let #(entry, index) = item
      let VariantMediaInput(variant_id: variant_id, media_ids: media_ids) =
        entry
      case find_variant_by_id(effective_variants, variant_id) {
        None -> #(
          updated_variants,
          updated_ids,
          list.append(errors, [
            ProductUserError(
              ["variantMedia", int.to_string(index), "variantId"],
              "Variant does not exist",
              None,
            ),
          ]),
        )
        Some(variant) ->
          case first_unknown_media_index(media_ids, product_media_ids) {
            Some(media_index) -> #(
              updated_variants,
              updated_ids,
              list.append(errors, [
                ProductUserError(
                  [
                    "variantMedia",
                    int.to_string(index),
                    "mediaIds",
                    int.to_string(media_index),
                  ],
                  "Media does not exist",
                  None,
                ),
              ]),
            )
            None -> {
              let next_media_ids = case is_append {
                True ->
                  dedupe_preserving_order(list.append(
                    variant.media_ids,
                    media_ids,
                  ))
                False ->
                  list.filter(variant.media_ids, fn(media_id) {
                    !list.contains(media_ids, media_id)
                  })
              }
              #(
                [
                  ProductVariantRecord(..variant, media_ids: next_media_ids),
                  ..updated_variants
                ],
                [variant.id, ..updated_ids],
                errors,
              )
            }
          }
      }
    })
  case user_errors {
    [] -> {
      let staged_variants =
        effective_variants
        |> list.map(fn(variant) {
          find_variant_by_id(next_variants, variant.id)
          |> option.unwrap(variant)
        })
      #(
        store.replace_staged_variants_for_product(
          store,
          product_id,
          staged_variants,
        ),
        list.reverse(updated_variant_ids),
        [],
      )
    }
    _ -> #(store, list.reverse(updated_variant_ids), user_errors)
  }
}

fn find_variant_by_id(
  variants: List(ProductVariantRecord),
  variant_id: String,
) -> Option(ProductVariantRecord) {
  variants
  |> list.find(fn(variant) { variant.id == variant_id })
  |> option.from_result
}

fn first_unknown_media_index(
  media_ids: List(String),
  product_media_ids: List(String),
) -> Option(Int) {
  media_ids
  |> enumerate_strings()
  |> list.find(fn(entry) {
    let #(media_id, _) = entry
    !list.contains(product_media_ids, media_id)
  })
  |> result.map(fn(entry) {
    let #(_, index) = entry
    index
  })
  |> option.from_result
}

fn invalid_create_media_content_type(
  args: Dict(String, ResolvedValue),
  document: String,
) -> Option(Json) {
  case dict.get(args, "media") {
    Ok(ListVal(values)) ->
      values
      |> enumerate_items()
      |> list.find_map(fn(entry) {
        let #(value, index) = entry
        case value {
          ObjectVal(fields) ->
            case read_string_field(fields, "mediaContentType") {
              Some(media_content_type) ->
                case is_create_media_content_type(media_content_type) {
                  True -> Error(Nil)
                  False ->
                    Ok(invalid_product_media_content_type_variable_error(
                      values,
                      index,
                      media_content_type,
                      document,
                    ))
                }
              None -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn is_create_media_content_type(value: String) -> Bool {
  case value {
    "VIDEO" | "EXTERNAL_VIDEO" | "MODEL_3D" | "IMAGE" -> True
    _ -> False
  }
}

fn is_valid_media_source(value: Option(String)) -> Bool {
  case value {
    Some(value) -> {
      let trimmed = string.trim(value)
      string.length(trimmed) > 0
      && {
        string.starts_with(trimmed, "http://")
        || string.starts_with(trimmed, "https://")
      }
    }
    None -> False
  }
}

fn make_created_media_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
  position: Int,
) -> #(ProductMediaRecord, SyntheticIdentityRegistry) {
  let media_content_type =
    read_string_field(input, "mediaContentType") |> option.unwrap("IMAGE")
  let #(media_id, identity_after_media) =
    make_synthetic_media_id(identity, media_content_type)
  let #(product_image_id, next_identity) =
    make_synthetic_product_image_id(identity_after_media, media_content_type)
  let source_url =
    option.then(read_string_field(input, "originalSource"), non_empty_string)
  #(
    ProductMediaRecord(
      key: product_id <> ":media:" <> int.to_string(position),
      product_id: product_id,
      position: position,
      id: Some(media_id),
      media_content_type: Some(media_content_type),
      alt: read_string_field(input, "alt"),
      status: Some("UPLOADED"),
      product_image_id: product_image_id,
      image_url: None,
      image_width: None,
      image_height: None,
      preview_image_url: None,
      source_url: source_url,
    ),
    next_identity,
  )
}

fn make_synthetic_media_id(
  identity: SyntheticIdentityRegistry,
  media_content_type: String,
) -> #(String, SyntheticIdentityRegistry) {
  case media_content_type {
    "IMAGE" -> synthetic_identity.make_synthetic_gid(identity, "MediaImage")
    _ -> synthetic_identity.make_synthetic_gid(identity, "Media")
  }
}

fn make_synthetic_product_image_id(
  identity: SyntheticIdentityRegistry,
  media_content_type: String,
) -> #(Option(String), SyntheticIdentityRegistry) {
  case media_content_type {
    "IMAGE" -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "ProductImage")
      #(Some(id), next_identity)
    }
    _ -> #(None, identity)
  }
}

fn transition_created_media_to_processing(
  media: ProductMediaRecord,
) -> ProductMediaRecord {
  ProductMediaRecord(
    ..media,
    status: Some("PROCESSING"),
    image_url: None,
    preview_image_url: None,
  )
}

fn settle_media_to_ready(media: ProductMediaRecord) -> ProductMediaRecord {
  case media.status {
    Some("PROCESSING") -> transition_media_to_ready(media)
    _ -> media
  }
}

fn transition_media_to_ready(media: ProductMediaRecord) -> ProductMediaRecord {
  let ready_url =
    media.source_url
    |> option.or(media.image_url)
    |> option.or(media.preview_image_url)
  ProductMediaRecord(
    ..media,
    status: Some("READY"),
    image_url: ready_url,
    preview_image_url: ready_url,
  )
}

fn update_media_record(
  media: ProductMediaRecord,
  input: Dict(String, ResolvedValue),
) -> ProductMediaRecord {
  let next_image_url =
    option.then(
      read_string_field(input, "previewImageSource"),
      non_empty_string,
    )
    |> option.or(option.then(
      read_string_field(input, "originalSource"),
      non_empty_string,
    ))
    |> option.or(media.image_url)
    |> option.or(media.preview_image_url)
    |> option.or(media.source_url)
  ProductMediaRecord(
    ..media,
    alt: read_string_field(input, "alt") |> option.or(media.alt),
    status: Some("READY"),
    image_url: next_image_url,
    preview_image_url: next_image_url,
    source_url: media.source_url |> option.or(next_image_url),
  )
}

fn first_missing_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media: List(ProductMediaRecord),
) -> Option(Dict(String, ResolvedValue)) {
  updates
  |> list.find(fn(update) {
    case read_string_field(update, "id") {
      Some(id) -> !has_media_id(media, id)
      None -> True
    }
  })
  |> option.from_result
}

fn first_non_ready_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media: List(ProductMediaRecord),
) -> Option(Int) {
  updates
  |> enumerate_items()
  |> list.find_map(fn(entry) {
    let #(update, index) = entry
    case read_string_field(update, "id") {
      Some(id) ->
        case find_media_by_id(media, id) {
          Some(record) ->
            case record.status {
              Some("READY") -> Error(Nil)
              _ -> Ok(index)
            }
          None -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

fn find_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media_id: Option(String),
) -> Option(Dict(String, ResolvedValue)) {
  case media_id {
    None -> None
    Some(id) ->
      updates
      |> list.find(fn(update) { read_string_field(update, "id") == Some(id) })
      |> option.from_result
  }
}

fn first_unknown_media_id(
  media_ids: List(String),
  media: List(ProductMediaRecord),
) -> Option(String) {
  media_ids
  |> list.find(fn(id) { !has_media_id(media, id) })
  |> option.from_result
}

fn has_media_id(media: List(ProductMediaRecord), id: String) -> Bool {
  case find_media_by_id(media, id) {
    Some(_) -> True
    None -> False
  }
}

fn find_media_by_id(
  media: List(ProductMediaRecord),
  id: String,
) -> Option(ProductMediaRecord) {
  media
  |> list.find(fn(record) { record.id == Some(id) })
  |> option.from_result
}

fn media_record_id_result(media: ProductMediaRecord) -> Result(String, Nil) {
  case media.id {
    Some(id) -> Ok(id)
    None -> Error(Nil)
  }
}

fn product_media_product_image_id_result(
  media: ProductMediaRecord,
) -> Result(String, Nil) {
  case media.product_image_id {
    Some(id) -> Ok(id)
    None -> Error(Nil)
  }
}

fn product_create_media_payload(
  store: Store,
  media: List(ProductMediaRecord),
  user_errors: List(ProductUserError),
  product: Option(ProductRecord),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(product) -> product_source_with_store(store, product)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductCreateMediaPayload")),
      #("media", SrcList(list.map(media, product_media_source))),
      #("mediaUserErrors", user_errors_source(user_errors)),
      #("product", product_value),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_update_media_payload(
  media: List(ProductMediaRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  product_update_media_payload_with_media_value(
    SrcList(list.map(media, product_media_source)),
    user_errors,
    field,
    fragments,
  )
}

fn product_update_media_payload_with_media_value(
  media: SourceValue,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductUpdateMediaPayload")),
      #("media", media),
      #("mediaUserErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_delete_media_payload(
  store: Store,
  product: Option(ProductRecord),
  deleted_media_ids: SourceValue,
  deleted_product_image_ids: SourceValue,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(product) -> product_source_with_store(store, product)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductDeleteMediaPayload")),
      #("deletedMediaIds", deleted_media_ids),
      #("deletedProductImageIds", deleted_product_image_ids),
      #("mediaUserErrors", user_errors_source(user_errors)),
      #("product", product_value),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_reorder_media_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductReorderMediaPayload")),
      #("job", job_value),
      #("mediaUserErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variant_media_payload(
  store: Store,
  product: Option(ProductRecord),
  variants: List(ProductVariantRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantMediaPayload")),
      #("product", product_value),
      #(
        "productVariants",
        SrcList(
          list.map(variants, fn(variant) {
            product_variant_source(store, variant)
          }),
        ),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_media_not_found_payload(
  store: Store,
  shape: String,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case shape {
    "delete" ->
      product_delete_media_payload(
        store,
        None,
        SrcNull,
        SrcNull,
        [ProductUserError(["productId"], "Product does not exist", None)],
        field,
        fragments,
      )
    "create" ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("ProductCreateMediaPayload")),
          #("media", SrcNull),
          #(
            "mediaUserErrors",
            user_errors_source([
              ProductUserError(["productId"], "Product does not exist", None),
            ]),
          ),
          #("product", SrcNull),
        ]),
        get_selected_child_fields(field, default_selected_field_options()),
        fragments,
      )
    _ ->
      product_update_media_payload_with_media_value(
        SrcNull,
        [ProductUserError(["productId"], "Product does not exist", None)],
        field,
        fragments,
      )
  }
}

fn invalid_product_media_product_id_variable_error(
  product_id: String,
  document: String,
) -> Json {
  let message = "Invalid global id '" <> product_id <> "'"
  let base = [
    #(
      "message",
      json.string("Variable $productId of type ID! was provided invalid value"),
    ),
  ]
  let with_locations = case
    find_variable_definition_location(document, "productId")
  {
    Some(loc) ->
      list.append(base, [
        #("locations", graphql_helpers.locations_json(loc, document)),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", json.string(product_id)),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #("path", json.preprocessed_array([])),
                #("explanation", json.string(message)),
                #("message", json.string(message)),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

fn invalid_product_media_content_type_variable_error(
  media_values: List(ResolvedValue),
  media_index: Int,
  media_content_type: String,
  document: String,
) -> Json {
  let explanation =
    "Expected \""
    <> media_content_type
    <> "\" to be one of: VIDEO, EXTERNAL_VIDEO, MODEL_3D, IMAGE"
  let base = [
    #(
      "message",
      json.string(
        "Variable $media of type [CreateMediaInput!]! was provided invalid value for "
        <> int.to_string(media_index)
        <> ".mediaContentType ("
        <> explanation
        <> ")",
      ),
    ),
  ]
  let with_locations = case
    find_variable_definition_location(document, "media")
  {
    Some(loc) ->
      list.append(base, [
        #("locations", graphql_helpers.locations_json(loc, document)),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", json.array(media_values, resolved_value_to_json)),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #(
                  "path",
                  json.preprocessed_array([
                    json.int(media_index),
                    json.string("mediaContentType"),
                  ]),
                ),
                #("explanation", json.string(explanation)),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

fn handle_bulk_product_resource_feedback_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let initial = #(store, identity, [], [], [])
  let #(next_store, next_identity, feedback, user_errors, staged_ids) =
    read_arg_object_list(args, "feedbackInput")
    |> enumerate_items()
    |> list.fold(initial, fn(acc, entry) {
      let #(current_store, current_identity, records, errors, ids) = acc
      let #(input, index) = entry
      let #(record, identity_after_record) =
        make_product_resource_feedback_record(current_identity, input)
      case record {
        Some(feedback_record) ->
          case
            store.get_effective_product_by_id(
              current_store,
              feedback_record.product_id,
            )
          {
            Some(_) -> {
              let #(staged, staged_store) =
                store.upsert_staged_product_resource_feedback(
                  current_store,
                  feedback_record,
                )
              #(
                staged_store,
                identity_after_record,
                list.append(records, [staged]),
                errors,
                list.append(ids, [staged.product_id]),
              )
            }
            None -> #(
              current_store,
              identity_after_record,
              records,
              list.append(errors, [
                ProductUserError(
                  ["feedbackInput", int.to_string(index), "productId"],
                  "Product does not exist",
                  None,
                ),
              ]),
              ids,
            )
          }
        None -> #(
          current_store,
          identity_after_record,
          records,
          list.append(errors, [
            ProductUserError(
              ["feedbackInput", int.to_string(index), "productId"],
              "Product does not exist",
              None,
            ),
          ]),
          ids,
        )
      }
    })
  mutation_result(
    key,
    bulk_product_resource_feedback_create_payload(
      feedback,
      user_errors,
      field,
      fragments,
    ),
    next_store,
    next_identity,
    staged_ids,
  )
}

fn handle_shop_resource_feedback_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let #(record, next_identity) =
    make_shop_resource_feedback_record(identity, input)
  case record {
    Some(feedback) -> {
      let #(staged, next_store) =
        store.upsert_staged_shop_resource_feedback(store, feedback)
      mutation_result(
        key,
        shop_resource_feedback_create_payload(
          Some(staged),
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        [staged.id],
      )
    }
    None ->
      mutation_result(
        key,
        shop_resource_feedback_create_payload(
          None,
          [ProductUserError(["input", "state"], "State is invalid", None)],
          field,
          fragments,
        ),
        store,
        next_identity,
        [],
      )
  }
}

fn make_product_resource_feedback_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(Option(ProductResourceFeedbackRecord), SyntheticIdentityRegistry) {
  let product_id = read_string_field(input, "productId")
  let state = read_string_field(input, "state")
  let #(feedback_generated_at, next_identity) =
    feedback_generated_at(input, identity)
  let product_updated_at =
    read_string_field(input, "productUpdatedAt")
    |> option.unwrap(feedback_generated_at)
  case product_id, state {
    Some(product_id), Some(state) ->
      case is_valid_feedback_state(state) {
        True -> #(
          Some(ProductResourceFeedbackRecord(
            product_id: product_id,
            state: state,
            feedback_generated_at: feedback_generated_at,
            product_updated_at: product_updated_at,
            messages: read_string_list_field(input, "messages")
              |> option.unwrap([]),
          )),
          next_identity,
        )
        False -> #(None, next_identity)
      }
    _, _ -> #(None, next_identity)
  }
}

fn make_shop_resource_feedback_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(Option(ShopResourceFeedbackRecord), SyntheticIdentityRegistry) {
  let state = read_string_field(input, "state")
  case state {
    Some(state) ->
      case is_valid_feedback_state(state) {
        True -> {
          let #(id, identity_after_id) =
            synthetic_identity.make_synthetic_gid(identity, "AppFeedback")
          let #(feedback_generated_at, next_identity) =
            feedback_generated_at(input, identity_after_id)
          #(
            Some(ShopResourceFeedbackRecord(
              id: id,
              state: state,
              feedback_generated_at: feedback_generated_at,
              messages: read_string_list_field(input, "messages")
                |> option.unwrap([]),
            )),
            next_identity,
          )
        }
        False -> #(None, identity)
      }
    None -> #(None, identity)
  }
}

fn feedback_generated_at(
  input: Dict(String, ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(String, SyntheticIdentityRegistry) {
  case read_string_field(input, "feedbackGeneratedAt") {
    Some(value) -> #(value, identity)
    None -> synthetic_identity.make_synthetic_timestamp(identity)
  }
}

fn is_valid_feedback_state(state: String) -> Bool {
  state == "ACCEPTED" || state == "REQUIRES_ACTION"
}

fn handle_inventory_transfer_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  case root_name {
    "inventoryTransferCreate" ->
      handle_inventory_transfer_create(
        store,
        identity,
        root_name,
        "InventoryTransferCreatePayload",
        "DRAFT",
        field,
        fragments,
        variables,
      )
    "inventoryTransferCreateAsReadyToShip" ->
      handle_inventory_transfer_create(
        store,
        identity,
        root_name,
        "InventoryTransferCreateAsReadyToShipPayload",
        "READY_TO_SHIP",
        field,
        fragments,
        variables,
      )
    "inventoryTransferEdit" ->
      handle_inventory_transfer_edit(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferSetItems" ->
      handle_inventory_transfer_set_items(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferRemoveItems" ->
      handle_inventory_transfer_remove_items(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferMarkAsReadyToShip" ->
      handle_inventory_transfer_mark_ready(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferDuplicate" ->
      handle_inventory_transfer_duplicate(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferCancel" ->
      handle_inventory_transfer_cancel(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferDelete" ->
      handle_inventory_transfer_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    _ ->
      mutation_error_result(
        field |> get_field_response_key,
        store,
        identity,
        [],
      )
  }
}

fn handle_inventory_transfer_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root_name: String,
  payload_typename: String,
  status: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let #(transfer, user_errors, identity_after_transfer) =
    make_inventory_transfer_record(store, identity, input, status)
  case transfer, user_errors {
    Some(transfer), [] -> {
      let #(next_store, next_identity, reserve_errors) = case status {
        "READY_TO_SHIP" ->
          apply_inventory_transfer_reservation(
            store,
            identity_after_transfer,
            transfer,
            "reserve",
          )
        _ -> #(store, identity_after_transfer, [])
      }
      case reserve_errors {
        [] -> {
          let #(_, next_store) =
            store.upsert_staged_inventory_transfer(next_store, transfer)
          mutation_result(
            key,
            inventory_transfer_payload(
              next_store,
              payload_typename,
              "inventoryTransfer",
              Some(transfer),
              [],
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            inventory_transfer_staged_ids(transfer),
          )
        }
        errors ->
          mutation_result(
            key,
            inventory_transfer_payload(
              store,
              payload_typename,
              "inventoryTransfer",
              None,
              [],
              errors,
              field,
              fragments,
            ),
            store,
            identity_after_transfer,
            [],
          )
      }
    }
    _, errors ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          payload_typename,
          "inventoryTransfer",
          None,
          [],
          errors,
          field,
          fragments,
        ),
        store,
        identity_after_transfer,
        [],
      )
  }
}

fn handle_inventory_transfer_edit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let transfer_id = graphql_helpers.read_arg_string(args, "id")
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferEditPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let next_transfer =
        InventoryTransferRecord(
          ..transfer,
          reference_name: case read_string_field(input, "referenceName") {
            Some(value) -> Some(value)
            None -> transfer.reference_name
          },
          note: case read_string_field(input, "note") {
            Some(value) -> Some(value)
            None -> transfer.note
          },
          tags: read_string_list_field(input, "tags")
            |> option.unwrap(transfer.tags),
          date_created: read_string_field(input, "dateCreated")
            |> option.unwrap(transfer.date_created),
          origin: case read_string_field(input, "originId") {
            Some(id) ->
              make_inventory_transfer_location_snapshot(
                store,
                Some(id),
                identity,
              )
            None -> transfer.origin
          },
          destination: case read_string_field(input, "destinationId") {
            Some(id) ->
              make_inventory_transfer_location_snapshot(
                store,
                Some(id),
                identity,
              )
            None -> transfer.destination
          },
        )
      let #(_, next_store) =
        store.upsert_staged_inventory_transfer(store, next_transfer)
      mutation_result(
        key,
        inventory_transfer_payload(
          next_store,
          "InventoryTransferEditPayload",
          "inventoryTransfer",
          Some(next_transfer),
          [],
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        inventory_transfer_staged_ids(next_transfer),
      )
    }
  }
}

fn handle_inventory_transfer_set_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let transfer_id = read_string_field(input, "id")
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferSetItemsPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let line_item_inputs = read_inventory_transfer_line_item_inputs(input)
      let user_errors =
        validate_inventory_transfer_line_items(store, line_item_inputs)
      let prior_items = transfer.line_items
      let #(updated_line_items, identity_after_items) =
        make_inventory_transfer_line_items_reusing_ids(
          store,
          identity,
          line_item_inputs,
          prior_items,
        )
      let updates =
        list.map(updated_line_items, fn(line_item) {
          let prior_quantity =
            find_inventory_transfer_line_item_by_item_id(
              prior_items,
              line_item.inventory_item_id,
            )
            |> option.map(fn(prior) { prior.total_quantity })
            |> option.unwrap(0)
          InventoryTransferLineItemUpdate(
            inventory_item_id: line_item.inventory_item_id,
            new_quantity: line_item.total_quantity,
            delta_quantity: line_item.total_quantity - prior_quantity,
          )
        })
      let deltas =
        inventory_transfer_set_item_deltas(prior_items, updated_line_items)
      let next_transfer =
        InventoryTransferRecord(..transfer, line_items: updated_line_items)
      case user_errors {
        [] -> {
          let #(next_store, next_identity, reserve_errors) = case
            inventory_transfer_has_reserved_origin_inventory(transfer)
          {
            True ->
              apply_inventory_transfer_reservation_deltas(
                store,
                identity_after_items,
                transfer,
                deltas,
              )
            False -> #(store, identity_after_items, [])
          }
          case reserve_errors {
            [] -> {
              let #(_, next_store) =
                store.upsert_staged_inventory_transfer(
                  next_store,
                  next_transfer,
                )
              mutation_result(
                key,
                inventory_transfer_payload(
                  next_store,
                  "InventoryTransferSetItemsPayload",
                  "inventoryTransfer",
                  Some(next_transfer),
                  updates,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                inventory_transfer_staged_ids(next_transfer),
              )
            }
            errors ->
              mutation_result(
                key,
                inventory_transfer_payload(
                  store,
                  "InventoryTransferSetItemsPayload",
                  "inventoryTransfer",
                  Some(transfer),
                  [],
                  errors,
                  field,
                  fragments,
                ),
                store,
                identity_after_items,
                [],
              )
          }
        }
        errors ->
          mutation_result(
            key,
            inventory_transfer_payload(
              store,
              "InventoryTransferSetItemsPayload",
              "inventoryTransfer",
              Some(transfer),
              [],
              errors,
              field,
              fragments,
            ),
            store,
            identity_after_items,
            [],
          )
      }
    }
  }
}

fn handle_inventory_transfer_remove_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let transfer_id = read_string_field(input, "id")
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferRemoveItemsPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let remove_ids =
        read_string_list_field(input, "transferLineItemIds")
        |> option.unwrap([])
      let unknown =
        list.any(remove_ids, fn(id) {
          find_inventory_transfer_line_item(transfer.line_items, id) == None
        })
      let user_errors = case unknown {
        True -> [
          ProductUserError(
            ["input", "transferLineItemIds"],
            "The inventory transfer line item can't be found.",
            Some("LINE_ITEM_NOT_FOUND"),
          ),
        ]
        False -> []
      }
      let removed_items =
        list.filter(transfer.line_items, fn(line_item) {
          list.contains(remove_ids, line_item.id)
        })
      let next_items =
        list.filter(transfer.line_items, fn(line_item) {
          !list.contains(remove_ids, line_item.id)
        })
      let updates =
        list.map(removed_items, fn(line_item) {
          InventoryTransferLineItemUpdate(
            inventory_item_id: line_item.inventory_item_id,
            new_quantity: 0,
            delta_quantity: 0 - line_item.total_quantity,
          )
        })
      let next_transfer =
        InventoryTransferRecord(..transfer, line_items: next_items)
      case user_errors {
        [] -> {
          let #(next_store, next_identity, reserve_errors) = case
            inventory_transfer_has_reserved_origin_inventory(transfer)
          {
            True ->
              apply_inventory_transfer_reservation_deltas(
                store,
                identity,
                transfer,
                list.map(removed_items, fn(line_item) {
                  #(line_item, 0 - line_item.total_quantity)
                }),
              )
            False -> #(store, identity, [])
          }
          case reserve_errors {
            [] -> {
              let #(_, next_store) =
                store.upsert_staged_inventory_transfer(
                  next_store,
                  next_transfer,
                )
              mutation_result(
                key,
                inventory_transfer_payload(
                  next_store,
                  "InventoryTransferRemoveItemsPayload",
                  "inventoryTransfer",
                  Some(next_transfer),
                  updates,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                inventory_transfer_staged_ids(next_transfer),
              )
            }
            errors ->
              mutation_result(
                key,
                inventory_transfer_payload(
                  store,
                  "InventoryTransferRemoveItemsPayload",
                  "inventoryTransfer",
                  Some(transfer),
                  [],
                  errors,
                  field,
                  fragments,
                ),
                store,
                next_identity,
                [],
              )
          }
        }
        errors ->
          mutation_result(
            key,
            inventory_transfer_payload(
              store,
              "InventoryTransferRemoveItemsPayload",
              "inventoryTransfer",
              Some(transfer),
              [],
              errors,
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    }
  }
}

fn handle_inventory_transfer_mark_ready(
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
        inventory_transfer_payload(
          store,
          "InventoryTransferMarkAsReadyToShipPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let #(next_store, next_identity, user_errors) = case
        transfer.status == "DRAFT"
      {
        True ->
          apply_inventory_transfer_reservation(
            store,
            identity,
            transfer,
            "reserve",
          )
        False -> #(store, identity, [])
      }
      case user_errors {
        [] -> {
          let next_transfer =
            InventoryTransferRecord(..transfer, status: "READY_TO_SHIP")
          let #(_, next_store) =
            store.upsert_staged_inventory_transfer(next_store, next_transfer)
          mutation_result(
            key,
            inventory_transfer_payload(
              next_store,
              "InventoryTransferMarkAsReadyToShipPayload",
              "inventoryTransfer",
              Some(next_transfer),
              [],
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            inventory_transfer_staged_ids(next_transfer),
          )
        }
        errors ->
          mutation_result(
            key,
            inventory_transfer_payload(
              store,
              "InventoryTransferMarkAsReadyToShipPayload",
              "inventoryTransfer",
              None,
              [],
              errors,
              field,
              fragments,
            ),
            store,
            next_identity,
            [],
          )
      }
    }
  }
}

fn handle_inventory_transfer_cancel(
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
        inventory_transfer_payload(
          store,
          "InventoryTransferCancelPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let #(next_store, next_identity, _) = case
        transfer.status == "READY_TO_SHIP"
      {
        True ->
          apply_inventory_transfer_reservation(
            store,
            identity,
            transfer,
            "release",
          )
        False -> #(store, identity, [])
      }
      let next_transfer =
        InventoryTransferRecord(..transfer, status: "CANCELED")
      let #(_, next_store) =
        store.upsert_staged_inventory_transfer(next_store, next_transfer)
      mutation_result(
        key,
        inventory_transfer_payload(
          next_store,
          "InventoryTransferCancelPayload",
          "inventoryTransfer",
          Some(next_transfer),
          [],
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        inventory_transfer_staged_ids(next_transfer),
      )
    }
  }
}

fn handle_inventory_transfer_duplicate(
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
        inventory_transfer_payload(
          store,
          "InventoryTransferDuplicatePayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let #(id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryTransfer")
      let #(line_items, next_identity) =
        duplicate_inventory_transfer_line_items(
          transfer.line_items,
          identity_after_id,
        )
      let transfer_index =
        list.length(store.list_effective_inventory_transfers(store)) + 1
      let duplicated =
        InventoryTransferRecord(
          ..transfer,
          id: id,
          name: "#T" <> pad_start_zero(int.to_string(transfer_index), 4),
          status: "DRAFT",
          line_items: line_items,
        )
      let #(_, next_store) =
        store.upsert_staged_inventory_transfer(store, duplicated)
      mutation_result(
        key,
        inventory_transfer_payload(
          next_store,
          "InventoryTransferDuplicatePayload",
          "inventoryTransfer",
          Some(duplicated),
          [],
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        inventory_transfer_staged_ids(duplicated),
      )
    }
  }
}

fn handle_inventory_transfer_delete(
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

fn read_inventory_transfer_line_item_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventoryTransferLineItemInput) {
  read_object_list_field(input, "lineItems")
  |> list.map(fn(fields) {
    InventoryTransferLineItemInput(
      inventory_item_id: read_string_field(fields, "inventoryItemId"),
      quantity: read_int_field(fields, "quantity"),
    )
  })
}

fn validate_inventory_transfer_line_items(
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

fn make_inventory_transfer_record(
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

fn make_inventory_transfer_location_snapshot(
  store: Store,
  location_id: Option(String),
  identity: SyntheticIdentityRegistry,
) -> Option(InventoryTransferLocationSnapshotRecord) {
  case location_id {
    Some(id) -> {
      let name = case store.get_effective_location_by_id(store, id) {
        Some(location) -> location.name
        None -> id
      }
      let #(snapshotted_at, _) =
        synthetic_identity.make_synthetic_timestamp(identity)
      Some(InventoryTransferLocationSnapshotRecord(
        id: Some(id),
        name: name,
        snapshotted_at: snapshotted_at,
      ))
    }
    None -> None
  }
}

fn make_inventory_transfer_line_items(
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

fn make_inventory_transfer_line_items_reusing_ids(
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

fn make_inventory_transfer_line_item(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: InventoryTransferLineItemInput,
) -> #(Option(InventoryTransferLineItemRecord), SyntheticIdentityRegistry) {
  case input.inventory_item_id, input.quantity {
    Some(inventory_item_id), Some(quantity) -> {
      let #(id, next_identity) =
        synthetic_identity.make_proxy_synthetic_gid(
          identity,
          "InventoryTransferLineItem",
        )
      let variant =
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      let title = case variant {
        Some(variant) ->
          case store.get_effective_product_by_id(store, variant.product_id) {
            Some(product) -> Some(product.title)
            None -> Some(variant.title)
          }
        None -> None
      }
      #(
        Some(InventoryTransferLineItemRecord(
          id: id,
          inventory_item_id: inventory_item_id,
          title: title,
          total_quantity: quantity,
          shipped_quantity: 0,
          picked_for_shipment_quantity: 0,
        )),
        next_identity,
      )
    }
    _, _ -> #(None, identity)
  }
}

fn duplicate_inventory_transfer_line_items(
  line_items: List(InventoryTransferLineItemRecord),
  identity: SyntheticIdentityRegistry,
) -> #(List(InventoryTransferLineItemRecord), SyntheticIdentityRegistry) {
  let #(reversed, next_identity) =
    list.fold(line_items, #([], identity), fn(acc, line_item) {
      let #(records, current_identity) = acc
      let #(id, identity_after_id) =
        synthetic_identity.make_proxy_synthetic_gid(
          current_identity,
          "InventoryTransferLineItem",
        )
      #(
        [InventoryTransferLineItemRecord(..line_item, id: id), ..records],
        identity_after_id,
      )
    })
  #(list.reverse(reversed), next_identity)
}

fn inventory_transfer_set_item_deltas(
  prior_items: List(InventoryTransferLineItemRecord),
  updated_items: List(InventoryTransferLineItemRecord),
) -> List(#(InventoryTransferLineItemRecord, Int)) {
  let updated_deltas =
    list.map(updated_items, fn(line_item) {
      let prior_quantity =
        find_inventory_transfer_line_item_by_item_id(
          prior_items,
          line_item.inventory_item_id,
        )
        |> option.map(fn(prior) { prior.total_quantity })
        |> option.unwrap(0)
      #(line_item, line_item.total_quantity - prior_quantity)
    })
  let removed_deltas =
    prior_items
    |> list.filter(fn(line_item) {
      find_inventory_transfer_line_item_by_item_id(
        updated_items,
        line_item.inventory_item_id,
      )
      == None
    })
    |> list.map(fn(line_item) { #(line_item, 0 - line_item.total_quantity) })
  list.append(updated_deltas, removed_deltas)
}

fn find_inventory_transfer_line_item(
  line_items: List(InventoryTransferLineItemRecord),
  id: String,
) -> Option(InventoryTransferLineItemRecord) {
  line_items
  |> list.find(fn(line_item) { line_item.id == id })
  |> option.from_result
}

fn get_inventory_transfer_by_optional_id(
  store: Store,
  transfer_id: Option(String),
) -> Option(InventoryTransferRecord) {
  case transfer_id {
    Some(id) -> store.get_effective_inventory_transfer_by_id(store, id)
    None -> None
  }
}

fn find_inventory_transfer_line_item_by_item_id(
  line_items: List(InventoryTransferLineItemRecord),
  inventory_item_id: String,
) -> Option(InventoryTransferLineItemRecord) {
  line_items
  |> list.find(fn(line_item) {
    line_item.inventory_item_id == inventory_item_id
  })
  |> option.from_result
}

fn inventory_transfer_has_reserved_origin_inventory(
  transfer: InventoryTransferRecord,
) -> Bool {
  transfer.status == "READY_TO_SHIP" || transfer.status == "IN_PROGRESS"
}

fn apply_inventory_transfer_reservation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  transfer: InventoryTransferRecord,
  direction: String,
) -> #(Store, SyntheticIdentityRegistry, List(ProductUserError)) {
  let deltas =
    list.map(transfer.line_items, fn(line_item) {
      let quantity = case direction {
        "release" -> 0 - line_item.total_quantity
        _ -> line_item.total_quantity
      }
      #(line_item, quantity)
    })
  apply_inventory_transfer_reservation_deltas(store, identity, transfer, deltas)
}

fn apply_inventory_transfer_reservation_deltas(
  store: Store,
  identity: SyntheticIdentityRegistry,
  transfer: InventoryTransferRecord,
  deltas: List(#(InventoryTransferLineItemRecord, Int)),
) -> #(Store, SyntheticIdentityRegistry, List(ProductUserError)) {
  let result =
    list.fold(deltas, Ok(#(store, identity)), fn(acc, delta) {
      case acc {
        Error(errors) -> Error(errors)
        Ok(state) -> {
          let #(current_store, current_identity) = state
          let #(line_item, delta_quantity) = delta
          case delta_quantity {
            0 -> Ok(#(current_store, current_identity))
            _ ->
              apply_inventory_transfer_reservation_delta(
                current_store,
                current_identity,
                transfer,
                line_item,
                delta_quantity,
              )
          }
        }
      }
    })
  case result {
    Ok(state) -> {
      let #(next_store, next_identity) = state
      #(next_store, next_identity, [])
    }
    Error(errors) -> #(store, identity, errors)
  }
}

fn apply_inventory_transfer_reservation_delta(
  store: Store,
  identity: SyntheticIdentityRegistry,
  transfer: InventoryTransferRecord,
  line_item: InventoryTransferLineItemRecord,
  delta_quantity: Int,
) -> Result(#(Store, SyntheticIdentityRegistry), List(ProductUserError)) {
  case find_inventory_transfer_origin_level(store, transfer, line_item) {
    None -> Error([inventory_transfer_origin_state_error()])
    Some(target) -> {
      let #(variant, level) = target
      let available = inventory_quantity_amount(level.quantities, "available")
      let reserved = inventory_quantity_amount(level.quantities, "reserved")
      case delta_quantity > 0 && available < delta_quantity {
        True -> Error([inventory_transfer_origin_state_error()])
        False -> {
          let quantities =
            level.quantities
            |> write_inventory_quantity_amount(
              "available",
              available - delta_quantity,
            )
            |> write_inventory_quantity_amount(
              "reserved",
              int.max(0, reserved + delta_quantity),
            )
          let next_level = InventoryLevelRecord(..level, quantities: quantities)
          let next_levels =
            replace_inventory_level(
              variant_inventory_levels(variant),
              level.location.id,
              next_level,
            )
          Ok(#(
            stage_variant_inventory_levels(store, variant, next_levels),
            identity,
          ))
        }
      }
    }
  }
}

fn find_inventory_transfer_origin_level(
  store: Store,
  transfer: InventoryTransferRecord,
  line_item: InventoryTransferLineItemRecord,
) -> Option(#(ProductVariantRecord, InventoryLevelRecord)) {
  case
    store.find_effective_variant_by_inventory_item_id(
      store,
      line_item.inventory_item_id,
    ),
    transfer.origin
  {
    Some(variant), Some(origin) ->
      case origin.id {
        Some(location_id) ->
          case
            find_inventory_level(variant_inventory_levels(variant), location_id)
          {
            Some(level) -> Some(#(variant, level))
            None -> None
          }
        None -> None
      }
    _, _ -> None
  }
}

fn inventory_transfer_origin_state_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "Cannot mark the transfer as ready to ship as the line items contain following errors: The item is not stocked at the origin location.",
    Some("INVENTORY_STATE_NOT_ACTIVE"),
  )
}

fn inventory_transfer_not_found_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "The inventory transfer can't be found.",
    Some("TRANSFER_NOT_FOUND"),
  )
}

fn inventory_transfer_staged_ids(
  transfer: InventoryTransferRecord,
) -> List(String) {
  [transfer.id, ..list.map(transfer.line_items, fn(line_item) { line_item.id })]
}

fn pad_start_zero(value: String, width: Int) -> String {
  let length = string.length(value)
  case length >= width {
    True -> value
    False -> string.repeat("0", width - length) <> value
  }
}

fn handle_inventory_shipment_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let status = case root_name {
    "inventoryShipmentCreateInTransit" -> "IN_TRANSIT"
    _ -> "DRAFT"
  }
  let typename = case root_name {
    "inventoryShipmentCreateInTransit" ->
      "InventoryShipmentCreateInTransitPayload"
    _ -> "InventoryShipmentCreatePayload"
  }
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let movement_id = read_string_field(input, "movementId")
  let line_item_inputs = read_object_list_field(input, "lineItems")
  let user_errors =
    validate_inventory_shipment_line_item_inputs(store, line_item_inputs, [
      "input",
      "lineItems",
    ])
  let user_errors = case movement_id {
    Some(_) -> user_errors
    None ->
      list.append(user_errors, [
        ProductUserError(
          ["input", "movementId"],
          "Movement id is required.",
          Some("BLANK"),
        ),
      ])
  }
  case user_errors, movement_id {
    [], Some(movement_id) -> {
      let #(now, identity_after_timestamp) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(shipment_id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(
          identity_after_timestamp,
          "InventoryShipment",
        )
      let #(line_items, identity_after_line_items) =
        make_inventory_shipment_line_items(identity_after_id, line_item_inputs)
      let shipment =
        InventoryShipmentRecord(
          id: shipment_id,
          movement_id: movement_id,
          name: "#S"
            <> int.to_string(
            list.length(store.list_effective_inventory_shipments(store)) + 1,
          ),
          status: status,
          created_at: now,
          updated_at: now,
          tracking: inventory_shipment_tracking_from_input(input),
          line_items: line_items,
        )
      let #(staged_shipment, staged_store, next_identity) = case status {
        "IN_TRANSIT" ->
          stage_inventory_shipment_with_incoming(
            store,
            identity_after_line_items,
            shipment,
          )
        _ -> {
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(store, shipment)
          #(staged, staged_store, identity_after_line_items)
        }
      }
      mutation_result(
        key,
        inventory_shipment_create_payload(
          staged_store,
          typename,
          Some(staged_shipment),
          [],
          field,
          fragments,
        ),
        staged_store,
        next_identity,
        [
          staged_shipment.id,
          ..list.map(staged_shipment.line_items, fn(item) { item.id })
        ],
      )
    }
    _, _ ->
      mutation_result(
        key,
        inventory_shipment_create_payload(
          store,
          typename,
          None,
          user_errors,
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

fn handle_inventory_shipment_set_tracking(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  let existing =
    option.then(id, fn(id) {
      store.get_effective_inventory_shipment_by_id(store, id)
    })
  case existing {
    None ->
      mutation_result(
        key,
        inventory_shipment_payload(
          store,
          "InventoryShipmentSetTrackingPayload",
          None,
          [inventory_shipment_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let user_errors = case shipment.status == "RECEIVED" {
        True -> [
          ProductUserError(
            ["id"],
            "Received shipments cannot be updated.",
            Some("INVALID_STATUS"),
          ),
        ]
        False -> []
      }
      case user_errors {
        [] -> {
          let #(now, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              tracking: inventory_shipment_tracking_from_argument(args),
              updated_at: now,
            )
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_payload(
              staged_store,
              "InventoryShipmentSetTrackingPayload",
              Some(staged),
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_payload(
              store,
              "InventoryShipmentSetTrackingPayload",
              None,
              user_errors,
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    }
  }
}

fn handle_inventory_shipment_mark_in_transit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  let existing =
    option.then(id, fn(id) {
      store.get_effective_inventory_shipment_by_id(store, id)
    })
  case existing {
    None ->
      mutation_result(
        key,
        inventory_shipment_payload(
          store,
          "InventoryShipmentMarkInTransitPayload",
          None,
          [inventory_shipment_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) ->
      case shipment.status == "DRAFT" {
        False ->
          mutation_result(
            key,
            inventory_shipment_payload(
              store,
              "InventoryShipmentMarkInTransitPayload",
              None,
              [
                ProductUserError(
                  ["id"],
                  "Only draft shipments can be marked in transit.",
                  Some("INVALID_STATUS"),
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
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              status: "IN_TRANSIT",
              updated_at: now,
            )
          let #(staged, staged_store, next_identity) =
            stage_inventory_shipment_with_incoming(
              store,
              identity_after_timestamp,
              next_shipment,
            )
          mutation_result(
            key,
            inventory_shipment_payload(
              staged_store,
              "InventoryShipmentMarkInTransitPayload",
              Some(staged),
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
      }
  }
}

fn handle_inventory_shipment_add_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  let existing =
    option.then(id, fn(id) {
      store.get_effective_inventory_shipment_by_id(store, id)
    })
  case existing {
    None ->
      mutation_result(
        key,
        inventory_shipment_add_items_payload(
          store,
          None,
          [],
          [inventory_shipment_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let line_item_inputs = read_arg_object_list(args, "lineItems")
      let user_errors = case shipment.status == "RECEIVED" {
        True -> [
          ProductUserError(
            ["id"],
            "Received shipments cannot be updated.",
            Some("INVALID_STATUS"),
          ),
        ]
        False ->
          validate_inventory_shipment_line_item_inputs(store, line_item_inputs, [
            "lineItems",
          ])
      }
      case user_errors {
        [] -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let #(added_items, identity_after_items) =
            make_inventory_shipment_line_items(
              identity_after_timestamp,
              line_item_inputs,
            )
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              updated_at: now,
              line_items: list.append(shipment.line_items, added_items),
            )
          let deltas = case shipment_has_unreceived_incoming(shipment) {
            True ->
              added_items
              |> list.map(fn(line_item) {
                InventoryShipmentDelta(
                  inventory_item_id: line_item.inventory_item_id,
                  incoming: line_item.quantity,
                  available: None,
                )
              })
            False -> []
          }
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(store, identity_after_items, deltas)
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(next_store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_add_items_payload(
              staged_store,
              Some(staged),
              added_items,
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id, ..list.map(added_items, fn(item) { item.id })],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_add_items_payload(
              store,
              None,
              [],
              user_errors,
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    }
  }
}

fn handle_inventory_shipment_remove_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  let existing =
    option.then(id, fn(id) {
      store.get_effective_inventory_shipment_by_id(store, id)
    })
  case existing {
    None ->
      mutation_result(
        key,
        inventory_shipment_payload(
          store,
          "InventoryShipmentRemoveItemsPayload",
          None,
          [inventory_shipment_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let ids = read_arg_string_list(args, "lineItems")
      let has_unknown =
        list.any(ids, fn(id) {
          find_inventory_shipment_line_item(shipment.line_items, id) == None
        })
      let user_errors = case has_unknown, shipment.status == "RECEIVED" {
        True, _ -> [
          ProductUserError(
            ["lineItems"],
            "One or more shipment line items could not be found.",
            Some("NOT_FOUND"),
          ),
        ]
        _, True -> [
          ProductUserError(
            ["id"],
            "Received shipments cannot be updated.",
            Some("INVALID_STATUS"),
          ),
        ]
        _, _ -> []
      }
      case user_errors {
        [] -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let removed =
            shipment.line_items
            |> list.filter(fn(line_item) { list.contains(ids, line_item.id) })
          let remaining =
            shipment.line_items
            |> list.filter(fn(line_item) { !list.contains(ids, line_item.id) })
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              updated_at: now,
              line_items: remaining,
            )
          let deltas = case shipment_has_unreceived_incoming(shipment) {
            True ->
              removed
              |> list.map(fn(line_item) {
                InventoryShipmentDelta(
                  inventory_item_id: line_item.inventory_item_id,
                  incoming: 0 - shipment_line_item_unreceived(line_item),
                  available: None,
                )
              })
            False -> []
          }
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(
              store,
              identity_after_timestamp,
              deltas,
            )
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(next_store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_payload(
              staged_store,
              "InventoryShipmentRemoveItemsPayload",
              Some(staged),
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_payload(
              store,
              "InventoryShipmentRemoveItemsPayload",
              None,
              user_errors,
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    }
  }
}

fn handle_inventory_shipment_receive(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  let existing =
    option.then(id, fn(id) {
      store.get_effective_inventory_shipment_by_id(store, id)
    })
  case existing {
    None ->
      mutation_result(
        key,
        inventory_shipment_receive_payload(
          store,
          None,
          [
            ProductUserError(
              ["id"],
              "The specified inventory shipment could not be found.",
              Some("NOT_FOUND"),
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let receive_inputs = read_arg_object_list(args, "lineItems")
      let #(next_line_items, user_errors, inventory_deltas) =
        apply_inventory_shipment_receive_inputs(shipment, receive_inputs)
      case user_errors {
        [] -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              status: inventory_shipment_status_after_receive(next_line_items),
              updated_at: now,
              line_items: next_line_items,
            )
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(
              store,
              identity_after_timestamp,
              inventory_deltas,
            )
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(next_store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_receive_payload(
              staged_store,
              Some(staged),
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_receive_payload(
              store,
              None,
              user_errors,
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    }
  }
}

fn handle_inventory_shipment_update_item_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  let existing =
    option.then(id, fn(id) {
      store.get_effective_inventory_shipment_by_id(store, id)
    })
  case existing {
    None ->
      mutation_result(
        key,
        inventory_shipment_update_item_quantities_payload(
          store,
          None,
          [],
          [
            ProductUserError(
              ["id"],
              "The specified inventory shipment could not be found.",
              Some("NOT_FOUND"),
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let updates = read_arg_object_list(args, "items")
      let #(next_line_items, updated_line_items, user_errors, deltas) =
        apply_inventory_shipment_quantity_updates(shipment, updates)
      case user_errors {
        [] -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              updated_at: now,
              line_items: next_line_items,
            )
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(
              store,
              identity_after_timestamp,
              deltas,
            )
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(next_store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_update_item_quantities_payload(
              staged_store,
              Some(staged),
              updated_line_items,
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_update_item_quantities_payload(
              store,
              None,
              [],
              user_errors,
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    }
  }
}

fn handle_inventory_shipment_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  let existing =
    option.then(id, fn(id) {
      store.get_effective_inventory_shipment_by_id(store, id)
    })
  case existing {
    None ->
      mutation_result(
        key,
        inventory_shipment_delete_payload(
          None,
          [
            ProductUserError(
              ["id"],
              "The specified inventory shipment could not be found.",
              Some("NOT_FOUND"),
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let user_errors = case shipment.status == "RECEIVED" {
        True -> [
          ProductUserError(
            ["id"],
            "Received shipments cannot be deleted.",
            Some("INVALID_STATUS"),
          ),
        ]
        False -> []
      }
      case user_errors {
        [] -> {
          let deltas =
            shipment.line_items
            |> list.map(fn(line_item) {
              InventoryShipmentDelta(
                inventory_item_id: line_item.inventory_item_id,
                incoming: 0 - shipment_line_item_unreceived(line_item),
                available: None,
              )
            })
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(store, identity, deltas)
          let deleted_store =
            store.delete_staged_inventory_shipment(next_store, shipment.id)
          mutation_result(
            key,
            inventory_shipment_delete_payload(
              Some(shipment.id),
              [],
              field,
              fragments,
            ),
            deleted_store,
            next_identity,
            [shipment.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_delete_payload(
              None,
              user_errors,
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    }
  }
}

type InventoryShipmentDelta {
  InventoryShipmentDelta(
    inventory_item_id: String,
    incoming: Int,
    available: Option(Int),
  )
}

fn validate_inventory_shipment_line_item_inputs(
  store: Store,
  line_items: List(Dict(String, ResolvedValue)),
  field_prefix: List(String),
) -> List(ProductUserError) {
  let initial = case line_items {
    [] -> [
      ProductUserError(
        field_prefix,
        "At least one line item is required.",
        Some("BLANK"),
      ),
    ]
    _ -> []
  }
  list.fold(enumerate_items(line_items), initial, fn(errors, pair) {
    let #(line_item, index) = pair
    let inventory_item_id = read_string_field(line_item, "inventoryItemId")
    let quantity = read_int_field(line_item, "quantity")
    let errors = case inventory_item_id {
      Some(id) ->
        case store.find_effective_variant_by_inventory_item_id(store, id) {
          Some(_) -> errors
          None ->
            list.append(errors, [
              ProductUserError(
                list.append(field_prefix, [
                  int.to_string(index),
                  "inventoryItemId",
                ]),
                "The specified inventory item could not be found.",
                Some("NOT_FOUND"),
              ),
            ])
        }
      None ->
        list.append(errors, [
          ProductUserError(
            list.append(field_prefix, [int.to_string(index), "inventoryItemId"]),
            "The specified inventory item could not be found.",
            Some("NOT_FOUND"),
          ),
        ])
    }
    case quantity {
      Some(quantity) if quantity > 0 -> errors
      _ ->
        list.append(errors, [
          ProductUserError(
            list.append(field_prefix, [int.to_string(index), "quantity"]),
            "Quantity must be greater than 0.",
            Some("INVALID"),
          ),
        ])
    }
  })
}

fn make_inventory_shipment_line_items(
  identity: SyntheticIdentityRegistry,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(InventoryShipmentLineItemRecord), SyntheticIdentityRegistry) {
  let #(reversed, next_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(records, current_identity) = acc
      let #(id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "InventoryShipmentLineItem",
        )
      let assert Some(inventory_item_id) =
        read_string_field(input, "inventoryItemId")
      let assert Some(quantity) = read_int_field(input, "quantity")
      #(
        [
          InventoryShipmentLineItemRecord(
            id: id,
            inventory_item_id: inventory_item_id,
            quantity: quantity,
            accepted_quantity: 0,
            rejected_quantity: 0,
          ),
          ..records
        ],
        identity_after_id,
      )
    })
  #(list.reverse(reversed), next_identity)
}

fn inventory_shipment_tracking_from_input(
  input: Dict(String, ResolvedValue),
) -> Option(InventoryShipmentTrackingRecord) {
  case dict.get(input, "trackingInput") {
    Ok(ObjectVal(tracking)) -> inventory_shipment_tracking_from_fields(tracking)
    _ -> None
  }
}

fn inventory_shipment_tracking_from_argument(
  args: Dict(String, ResolvedValue),
) -> Option(InventoryShipmentTrackingRecord) {
  case dict.get(args, "tracking") {
    Ok(ObjectVal(tracking)) -> inventory_shipment_tracking_from_fields(tracking)
    _ -> None
  }
}

fn inventory_shipment_tracking_from_fields(
  tracking: Dict(String, ResolvedValue),
) -> Option(InventoryShipmentTrackingRecord) {
  Some(InventoryShipmentTrackingRecord(
    tracking_number: read_string_field(tracking, "trackingNumber"),
    company: read_string_field(tracking, "company"),
    tracking_url: read_string_field(tracking, "trackingUrl"),
    arrives_at: read_string_field(tracking, "arrivesAt"),
  ))
}

fn stage_inventory_shipment_with_incoming(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shipment: InventoryShipmentRecord,
) -> #(InventoryShipmentRecord, Store, SyntheticIdentityRegistry) {
  let previous =
    store.get_effective_inventory_shipment_by_id(store, shipment.id)
  let should_add_incoming = case previous {
    Some(previous) ->
      previous.status != "IN_TRANSIT" && shipment.status == "IN_TRANSIT"
    None -> shipment.status == "IN_TRANSIT"
  }
  let deltas = case should_add_incoming {
    True ->
      shipment.line_items
      |> list.map(fn(line_item) {
        InventoryShipmentDelta(
          inventory_item_id: line_item.inventory_item_id,
          incoming: shipment_line_item_unreceived(line_item),
          available: None,
        )
      })
    False -> []
  }
  let #(next_store, next_identity) =
    apply_inventory_shipment_deltas(store, identity, deltas)
  let #(staged, staged_store) =
    store.upsert_staged_inventory_shipment(next_store, shipment)
  #(staged, staged_store, next_identity)
}

fn apply_inventory_shipment_receive_inputs(
  shipment: InventoryShipmentRecord,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(
  List(InventoryShipmentLineItemRecord),
  List(ProductUserError),
  List(InventoryShipmentDelta),
) {
  let initial = #(shipment.line_items, [], [])
  list.fold(enumerate_items(inputs), initial, fn(acc, pair) {
    let #(line_items, errors, deltas) = acc
    let #(input, index) = pair
    let line_item_id = read_string_field(input, "shipmentLineItemId")
    let quantity = read_int_field(input, "quantity")
    let reason = read_string_field(input, "reason")
    let current =
      option.then(line_item_id, fn(id) {
        find_inventory_shipment_line_item(shipment.line_items, id)
      })
    case current, line_item_id, quantity, reason {
      None, _, _, _ -> #(
        line_items,
        list.append(errors, [
          ProductUserError(
            ["lineItems", int.to_string(index), "shipmentLineItemId"],
            "Shipment line item could not be found.",
            Some("NOT_FOUND"),
          ),
        ]),
        deltas,
      )
      Some(current), Some(id), Some(quantity), Some(reason) -> {
        let valid_quantity =
          quantity > 0 && quantity <= shipment_line_item_unreceived(current)
        let valid_reason = reason == "ACCEPTED" || reason == "REJECTED"
        case valid_quantity, valid_reason {
          True, True -> {
            let next_line_items =
              line_items
              |> list.map(fn(line_item) {
                case line_item.id == id {
                  True ->
                    case reason {
                      "ACCEPTED" ->
                        InventoryShipmentLineItemRecord(
                          ..line_item,
                          accepted_quantity: line_item.accepted_quantity
                            + quantity,
                        )
                      _ ->
                        InventoryShipmentLineItemRecord(
                          ..line_item,
                          rejected_quantity: line_item.rejected_quantity
                            + quantity,
                        )
                    }
                  False -> line_item
                }
              })
            let delta =
              InventoryShipmentDelta(
                inventory_item_id: current.inventory_item_id,
                incoming: 0 - quantity,
                available: case reason {
                  "ACCEPTED" -> Some(quantity)
                  _ -> None
                },
              )
            #(next_line_items, errors, list.append(deltas, [delta]))
          }
          False, _ -> #(
            line_items,
            list.append(errors, [
              ProductUserError(
                ["lineItems", int.to_string(index), "quantity"],
                "Quantity must be greater than 0 and no more than the unreceived quantity.",
                Some("INVALID"),
              ),
            ]),
            deltas,
          )
          _, False -> #(
            line_items,
            list.append(errors, [
              ProductUserError(
                ["lineItems", int.to_string(index), "reason"],
                "Receive reason is required.",
                Some("BLANK"),
              ),
            ]),
            deltas,
          )
        }
      }
      Some(_), _, _, None -> #(
        line_items,
        list.append(errors, [
          ProductUserError(
            ["lineItems", int.to_string(index), "reason"],
            "Receive reason is required.",
            Some("BLANK"),
          ),
        ]),
        deltas,
      )
      _, _, _, _ -> #(
        line_items,
        list.append(errors, [
          ProductUserError(
            ["lineItems", int.to_string(index), "quantity"],
            "Quantity must be greater than 0 and no more than the unreceived quantity.",
            Some("INVALID"),
          ),
        ]),
        deltas,
      )
    }
  })
}

fn apply_inventory_shipment_quantity_updates(
  shipment: InventoryShipmentRecord,
  updates: List(Dict(String, ResolvedValue)),
) -> #(
  List(InventoryShipmentLineItemRecord),
  List(InventoryShipmentLineItemRecord),
  List(ProductUserError),
  List(InventoryShipmentDelta),
) {
  let initial = #(shipment.line_items, [], [], [])
  list.fold(enumerate_items(updates), initial, fn(acc, pair) {
    let #(line_items, updated, errors, deltas) = acc
    let #(input, index) = pair
    let line_item_id = read_string_field(input, "shipmentLineItemId")
    let quantity = read_int_field(input, "quantity")
    let current =
      option.then(line_item_id, fn(id) {
        find_inventory_shipment_line_item(line_items, id)
      })
    case current, line_item_id, quantity {
      Some(current), Some(id), Some(quantity)
        if quantity >= current.accepted_quantity + current.rejected_quantity
      -> {
        let incoming_delta = case shipment_has_unreceived_incoming(shipment) {
          True -> quantity - current.quantity
          False -> 0
        }
        let next_line_item =
          InventoryShipmentLineItemRecord(..current, quantity: quantity)
        let next_line_items =
          line_items
          |> list.map(fn(line_item) {
            case line_item.id == id {
              True -> next_line_item
              False -> line_item
            }
          })
        let next_deltas = case incoming_delta == 0 {
          True -> deltas
          False ->
            list.append(deltas, [
              InventoryShipmentDelta(
                inventory_item_id: current.inventory_item_id,
                incoming: incoming_delta,
                available: None,
              ),
            ])
        }
        #(
          next_line_items,
          list.append(updated, [next_line_item]),
          errors,
          next_deltas,
        )
      }
      None, _, _ -> #(
        line_items,
        updated,
        list.append(errors, [
          ProductUserError(
            ["items", int.to_string(index), "shipmentLineItemId"],
            "Shipment line item could not be found.",
            Some("NOT_FOUND"),
          ),
        ]),
        deltas,
      )
      _, _, _ -> #(
        line_items,
        updated,
        list.append(errors, [
          ProductUserError(
            ["items", int.to_string(index), "quantity"],
            "Quantity cannot be less than received quantity.",
            Some("INVALID"),
          ),
        ]),
        deltas,
      )
    }
  })
}

fn find_inventory_shipment_line_item(
  line_items: List(InventoryShipmentLineItemRecord),
  id: String,
) -> Option(InventoryShipmentLineItemRecord) {
  line_items
  |> list.find(fn(line_item) { line_item.id == id })
  |> option.from_result
}

fn inventory_shipment_status_after_receive(
  line_items: List(InventoryShipmentLineItemRecord),
) -> String {
  let total =
    list.fold(line_items, 0, fn(sum, line_item) { sum + line_item.quantity })
  let received =
    list.fold(line_items, 0, fn(sum, line_item) {
      sum + line_item.accepted_quantity + line_item.rejected_quantity
    })
  case received <= 0 {
    True -> "IN_TRANSIT"
    False ->
      case received >= total {
        True -> "RECEIVED"
        False -> "PARTIALLY_RECEIVED"
      }
  }
}

fn shipment_has_unreceived_incoming(shipment: InventoryShipmentRecord) -> Bool {
  shipment.status == "IN_TRANSIT" || shipment.status == "PARTIALLY_RECEIVED"
}

fn inventory_shipment_not_found_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "The specified inventory shipment could not be found.",
    Some("NOT_FOUND"),
  )
}

fn apply_inventory_shipment_deltas(
  store: Store,
  identity: SyntheticIdentityRegistry,
  deltas: List(InventoryShipmentDelta),
) -> #(Store, SyntheticIdentityRegistry) {
  list.fold(deltas, #(store, identity), fn(acc, delta) {
    let #(current_store, current_identity) = acc
    adjust_inventory_item_quantities(
      current_store,
      current_identity,
      delta.inventory_item_id,
      delta.incoming,
      delta.available,
    )
  })
}

fn adjust_inventory_item_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inventory_item_id: String,
  incoming_delta: Int,
  available_delta: Option(Int),
) -> #(Store, SyntheticIdentityRegistry) {
  case
    store.find_effective_variant_by_inventory_item_id(store, inventory_item_id)
  {
    None -> #(store, identity)
    Some(variant) -> {
      let levels = case variant_inventory_levels(variant) {
        [] -> [default_shipment_inventory_level(variant, inventory_item_id)]
        levels -> levels
      }
      let target = case levels {
        [first, ..] -> first
        [] -> default_shipment_inventory_level(variant, inventory_item_id)
      }
      let #(target, identity_after_incoming) =
        write_inventory_quantity_delta(
          target,
          identity,
          "incoming",
          incoming_delta,
        )
      let #(target, next_identity) = case available_delta {
        Some(delta) -> {
          let #(with_available, identity_after_available) =
            write_inventory_quantity_delta(
              target,
              identity_after_incoming,
              "available",
              delta,
            )
          write_inventory_quantity_delta(
            with_available,
            identity_after_available,
            "on_hand",
            delta,
          )
        }
        None -> #(target, identity_after_incoming)
      }
      let next_levels = replace_first_inventory_level(levels, target)
      #(
        stage_variant_inventory_levels(store, variant, next_levels),
        next_identity,
      )
    }
  }
}

fn default_shipment_inventory_level(
  variant: ProductVariantRecord,
  inventory_item_id: String,
) -> InventoryLevelRecord {
  let available = variant.inventory_quantity |> option.unwrap(0)
  InventoryLevelRecord(
    id: "gid://shopify/InventoryLevel/"
      <> inventory_item_legacy_id(inventory_item_id)
      <> "?inventory_item_id="
      <> inventory_item_id,
    location: InventoryLocationRecord(
      id: "gid://shopify/Location/1",
      name: "Default location",
    ),
    quantities: [
      InventoryQuantityRecord(
        name: "available",
        quantity: available,
        updated_at: None,
      ),
      InventoryQuantityRecord(
        name: "on_hand",
        quantity: available,
        updated_at: None,
      ),
      InventoryQuantityRecord(name: "incoming", quantity: 0, updated_at: None),
    ],
    is_active: Some(True),
    cursor: None,
  )
}

fn inventory_item_legacy_id(inventory_item_id: String) -> String {
  let tail = case list.last(string.split(inventory_item_id, "/")) {
    Ok(value) -> value
    Error(_) -> inventory_item_id
  }
  case string.split(tail, "?") {
    [id, ..] -> id
    [] -> tail
  }
}

fn write_inventory_quantity_delta(
  level: InventoryLevelRecord,
  identity: SyntheticIdentityRegistry,
  name: String,
  delta: Int,
) -> #(InventoryLevelRecord, SyntheticIdentityRegistry) {
  let current = inventory_quantity_amount(level.quantities, name)
  let next_amount = int.max(0, current + delta)
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let quantities =
    write_inventory_quantity_with_timestamp(
      level.quantities,
      name,
      next_amount,
      Some(updated_at),
    )
  #(InventoryLevelRecord(..level, quantities: quantities), next_identity)
}

fn reactivate_inventory_level(
  level: InventoryLevelRecord,
  identity: SyntheticIdentityRegistry,
) -> #(InventoryLevelRecord, SyntheticIdentityRegistry) {
  let available = inventory_quantity_amount(level.quantities, "available")
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let quantities =
    write_inventory_quantity_with_timestamp(
      level.quantities,
      "available",
      available,
      Some(updated_at),
    )
  #(
    InventoryLevelRecord(..level, quantities: quantities, is_active: Some(True)),
    next_identity,
  )
}

fn write_inventory_quantity_with_timestamp(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
  updated_at: Option(String),
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True ->
            InventoryQuantityRecord(
              ..quantity,
              quantity: amount,
              updated_at: updated_at,
            )
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(
          name: name,
          quantity: amount,
          updated_at: updated_at,
        ),
      ])
  }
}

fn replace_first_inventory_level(
  levels: List(InventoryLevelRecord),
  next_level: InventoryLevelRecord,
) -> List(InventoryLevelRecord) {
  case levels {
    [] -> [next_level]
    [_first, ..rest] -> [next_level, ..rest]
  }
}

fn handle_product_variant_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let product_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "productId")
    None -> None
  }
  case product_id, input {
    None, _ ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [
            ProductUserError(
              ["input", "productId"],
              "Product id is required",
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
    Some(product_id), Some(input) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variant_payload(
              store,
              None,
              None,
              [
                ProductUserError(
                  ["input", "productId"],
                  "Product not found",
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
        Some(_) -> {
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let defaults = list.first(effective_variants) |> option.from_result
          let user_errors =
            validate_product_variant_scalar_input(input, ["input"])
          case user_errors {
            [] -> {
              let #(created_variant, identity_after_variant) =
                make_created_variant_record(
                  identity,
                  product_id,
                  input,
                  defaults,
                )
              let next_variants =
                list.append(effective_variants, [created_variant])
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_variant,
                  product_id,
                )
              mutation_result(
                key,
                product_variant_payload(
                  next_store,
                  product,
                  Some(created_variant),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                variant_staged_ids(created_variant),
              )
            }
            _ ->
              mutation_rejected_result(
                key,
                product_variant_payload(
                  store,
                  None,
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
          }
        }
      }
    Some(_), None ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [
            ProductUserError(
              ["input", "productId"],
              "Product id is required",
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
  }
}

fn handle_product_variant_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let variant_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case variant_id, input {
    None, _ ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [ProductUserError(["input", "id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(variant_id), Some(input) ->
      case store.get_effective_variant_by_id(store, variant_id) {
        None ->
          mutation_result(
            key,
            product_variant_payload(
              store,
              None,
              None,
              [ProductUserError(["input", "id"], "Variant not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(existing_variant) -> {
          let user_errors =
            validate_product_variant_scalar_input(input, ["input"])
          case user_errors {
            [] -> {
              let #(updated_variant, identity_after_variant) =
                update_variant_record(identity, existing_variant, input)
              let next_variants =
                store.get_effective_variants_by_product_id(
                  store,
                  existing_variant.product_id,
                )
                |> list.map(fn(variant) {
                  case variant.id == variant_id {
                    True -> updated_variant
                    False -> variant
                  }
                })
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  existing_variant.product_id,
                  next_variants,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_variant,
                  existing_variant.product_id,
                )
              mutation_result(
                key,
                product_variant_payload(
                  next_store,
                  product,
                  Some(updated_variant),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                variant_staged_ids(updated_variant),
              )
            }
            _ ->
              mutation_rejected_result(
                key,
                product_variant_payload(
                  store,
                  None,
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
          }
        }
      }
    Some(_), None ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [ProductUserError(["input", "id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

fn handle_product_variant_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        product_variant_delete_payload(
          None,
          [ProductUserError(["id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(variant_id) ->
      case store.get_effective_variant_by_id(store, variant_id) {
        None ->
          mutation_result(
            key,
            product_variant_delete_payload(
              None,
              [ProductUserError(["id"], "Variant not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(existing_variant) -> {
          let next_variants =
            store.get_effective_variants_by_product_id(
              store,
              existing_variant.product_id,
            )
            |> list.filter(fn(variant) { variant.id != variant_id })
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              existing_variant.product_id,
              next_variants,
            )
          let #(_, next_store, final_identity) =
            sync_product_inventory_summary(
              next_store,
              identity,
              existing_variant.product_id,
            )
          mutation_result(
            key,
            product_variant_delete_payload(
              Some(variant_id),
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            [variant_id],
          )
        }
      }
  }
}

fn handle_product_variants_bulk_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let variant_inputs = read_arg_object_list(args, "variants")
  case list.length(variant_inputs) > max_product_variants {
    True ->
      mutation_error_result(key, store, identity, [
        max_input_size_exceeded_error(
          "productVariantsBulkCreate",
          "variants",
          list.length(variant_inputs),
          field,
          document,
        ),
      ])
    False ->
      handle_product_variants_bulk_create_valid_size(
        store,
        identity,
        key,
        args,
        variant_inputs,
        field,
        fragments,
      )
  }
}

fn handle_product_variants_bulk_create_valid_size(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, ResolvedValue),
  variant_inputs: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variants_bulk_payload(
          "ProductVariantsBulkCreatePayload",
          store,
          None,
          Some([]),
          [
            BulkVariantUserError(
              Some(["productId"]),
              "Product id is required",
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
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variants_bulk_payload(
              "ProductVariantsBulkCreatePayload",
              store,
              None,
              Some([]),
              [
                BulkVariantUserError(
                  Some(["productId"]),
                  "Product does not exist",
                  Some("PRODUCT_DOES_NOT_EXIST"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let effective_options =
            store.get_effective_options_by_product_id(store, product_id)
          let defaults = list.first(effective_variants) |> option.from_result
          let should_remove_standalone_variant =
            !list.is_empty(variant_inputs)
            && list.length(effective_variants) == 1
            && {
              graphql_helpers.read_arg_string(args, "strategy")
              == Some("REMOVE_STANDALONE_VARIANT")
              || product_has_standalone_default_variant(
                effective_options,
                effective_variants,
              )
            }
          let retained_count = case should_remove_standalone_variant {
            True -> 0
            False -> list.length(effective_variants)
          }
          let user_errors =
            validate_bulk_create_variant_batch(
              store,
              product_id,
              variant_inputs,
              retained_count,
            )
          case user_errors {
            [] -> {
              let #(created_variants, identity_after_variants) =
                make_created_variant_records(
                  identity,
                  product_id,
                  variant_inputs,
                  defaults,
                )
              let retained_variants = case should_remove_standalone_variant {
                True -> []
                False -> effective_variants
              }
              let next_variants =
                list.append(retained_variants, created_variants)
              let #(synced_options, identity_after_options) = case
                should_remove_standalone_variant
              {
                True ->
                  make_options_from_variant_selections(
                    identity_after_variants,
                    product_id,
                    next_variants,
                  )
                False -> {
                  let #(next_options, identity_after_options) =
                    upsert_variant_selections_into_options(
                      identity_after_variants,
                      product_id,
                      effective_options,
                      next_variants,
                    )
                  #(
                    sync_product_options_with_variants(
                      next_options,
                      next_variants,
                    ),
                    identity_after_options,
                  )
                }
              }
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
                |> store.replace_staged_options_for_product(
                  product_id,
                  synced_options,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_options,
                  product_id,
                )
              mutation_result(
                key,
                product_variants_bulk_payload(
                  "ProductVariantsBulkCreatePayload",
                  next_store,
                  product,
                  Some(created_variants),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                list.flat_map(created_variants, variant_staged_ids),
              )
            }
            _ ->
              mutation_rejected_result(
                key,
                product_variants_bulk_payload(
                  "ProductVariantsBulkCreatePayload",
                  store,
                  None,
                  Some([]),
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
          }
        }
      }
  }
}

fn handle_product_variants_bulk_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let updates = read_arg_object_list(args, "variants")
  case list.length(updates) > max_product_variants {
    True ->
      mutation_error_result(key, store, identity, [
        max_input_size_exceeded_error(
          "productVariantsBulkUpdate",
          "variants",
          list.length(updates),
          field,
          document,
        ),
      ])
    False ->
      handle_product_variants_bulk_update_valid_size(
        store,
        identity,
        key,
        args,
        updates,
        field,
        fragments,
      )
  }
}

fn handle_product_variants_bulk_update_valid_size(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, ResolvedValue),
  updates: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variants_bulk_payload(
          "ProductVariantsBulkUpdatePayload",
          store,
          None,
          Some([]),
          [
            BulkVariantUserError(
              Some(["productId"]),
              "Product id is required",
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
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variants_bulk_payload(
              "ProductVariantsBulkUpdatePayload",
              store,
              None,
              None,
              [
                BulkVariantUserError(
                  Some(["productId"]),
                  "Product does not exist",
                  Some("PRODUCT_DOES_NOT_EXIST"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let user_errors =
            validate_bulk_update_variant_batch(
              store,
              product_id,
              updates,
              effective_variants,
            )
          case user_errors {
            [] -> {
              let #(next_variants, updated_variants, identity_after_variants) =
                update_variant_records(identity, effective_variants, updates)
              let synced_options =
                sync_product_options_with_variants(
                  store.get_effective_options_by_product_id(store, product_id),
                  next_variants,
                )
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
                |> store.replace_staged_options_for_product(
                  product_id,
                  synced_options,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_variants,
                  product_id,
                )
              mutation_result(
                key,
                product_variants_bulk_payload(
                  "ProductVariantsBulkUpdatePayload",
                  next_store,
                  product,
                  Some(updated_variants),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                list.flat_map(updated_variants, variant_staged_ids),
              )
            }
            _ -> {
              let response_product = case user_errors {
                [BulkVariantUserError(field: None, ..), ..] -> None
                _ -> store.get_effective_product_by_id(store, product_id)
              }
              mutation_rejected_result(
                key,
                product_variants_bulk_payload(
                  "ProductVariantsBulkUpdatePayload",
                  store,
                  response_product,
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
            }
          }
        }
      }
  }
}

fn handle_product_variants_bulk_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variants_bulk_delete_payload(
          store,
          None,
          [
            BulkVariantUserError(
              Some(["productId"]),
              "Product id is required",
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
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variants_bulk_delete_payload(
              store,
              None,
              [
                BulkVariantUserError(
                  Some(["productId"]),
                  "Product does not exist",
                  Some("PRODUCT_DOES_NOT_EXIST"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let variant_ids = read_arg_string_list(args, "variantsIds")
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          case
            first_bulk_delete_missing_variant(variant_ids, effective_variants)
          {
            Some(index) ->
              mutation_result(
                key,
                product_variants_bulk_delete_payload(
                  store,
                  None,
                  [
                    BulkVariantUserError(
                      Some(["variantsIds", int.to_string(index)]),
                      "At least one variant does not belong to the product",
                      Some(
                        "AT_LEAST_ONE_VARIANT_DOES_NOT_BELONG_TO_THE_PRODUCT",
                      ),
                    ),
                  ],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            None -> {
              let next_variants =
                effective_variants
                |> list.filter(fn(variant) {
                  !list.contains(variant_ids, variant.id)
                })
              let synced_options =
                sync_product_options_with_variants(
                  store.get_effective_options_by_product_id(store, product_id),
                  next_variants,
                )
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
                |> store.replace_staged_options_for_product(
                  product_id,
                  synced_options,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(next_store, identity, product_id)
              mutation_result(
                key,
                product_variants_bulk_delete_payload(
                  next_store,
                  product,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                variant_ids,
              )
            }
          }
        }
      }
  }
}

fn handle_product_variants_bulk_reorder(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variants_bulk_reorder_payload(
          store,
          None,
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variants_bulk_reorder_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let #(positions, user_errors) =
            read_product_variant_positions(read_arg_object_list(
              args,
              "positions",
            ))
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let missing_errors =
            validate_product_variant_positions(effective_variants, positions)
          let all_errors = list.append(user_errors, missing_errors)
          case all_errors {
            [_, ..] ->
              mutation_result(
                key,
                product_variants_bulk_reorder_payload(
                  store,
                  None,
                  all_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            [] -> {
              let next_variants =
                apply_sequential_variant_reorder(effective_variants, positions)
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(next_store, identity, product_id)
              mutation_result(
                key,
                product_variants_bulk_reorder_payload(
                  next_store,
                  product,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                list.map(next_variants, fn(variant) { variant.id }),
              )
            }
          }
        }
      }
  }
}

fn handle_inventory_adjust_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  uses_202604_contract: Bool,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case
    inventory_adjust_202604_contract_error(
      uses_202604_contract,
      input,
      field,
      variables,
    )
  {
    Some(error) ->
      mutation_error_with_null_data_result(key, store, identity, [
        error,
      ])
    None -> {
      let quantity_name = read_non_empty_string_field(input, "name")
      let reason = read_non_empty_string_field(input, "reason")
      let changes = read_inventory_adjustment_change_inputs(input)
      case quantity_name, reason, changes {
        None, _, _ ->
          inventory_quantity_mutation_result(
            key,
            "InventoryAdjustQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "name"],
                "Inventory quantity name is required",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        _, None, _ ->
          inventory_quantity_mutation_result(
            key,
            "InventoryAdjustQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "reason"],
                "Inventory adjustment reason is required",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        _, _, [] ->
          inventory_quantity_mutation_result(
            key,
            "InventoryAdjustQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "changes"],
                "At least one inventory adjustment is required",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        Some(name), Some(reason), changes -> {
          case validate_inventory_adjust_inputs(name, changes) {
            [_, ..] as errors ->
              inventory_quantity_mutation_result(
                key,
                "InventoryAdjustQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            [] -> {
              let result =
                apply_inventory_adjust_quantities(
                  store,
                  identity,
                  input,
                  name,
                  reason,
                  changes,
                  uses_202604_contract,
                )
              case result {
                Error(errors) ->
                  inventory_quantity_mutation_result(
                    key,
                    "InventoryAdjustQuantitiesPayload",
                    store,
                    identity,
                    None,
                    errors,
                    field,
                    fragments,
                    [],
                  )
                Ok(applied) -> {
                  let #(next_store, next_identity, group, staged_ids) = applied
                  inventory_quantity_mutation_result(
                    key,
                    "InventoryAdjustQuantitiesPayload",
                    next_store,
                    next_identity,
                    Some(group),
                    [],
                    field,
                    fragments,
                    staged_ids,
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

fn handle_inventory_activate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inventory_item_id = read_string_field(args, "inventoryItemId")
  let location_id = read_string_field(args, "locationId")
  let user_errors = case dict.get(args, "available") {
    Ok(_) -> [
      ProductUserError(
        ["available"],
        "Not allowed to set available quantity when the item is already active at the location.",
        None,
      ),
    ]
    Error(_) -> []
  }
  let resolved = case inventory_item_id, location_id, user_errors {
    Some(inventory_item_id), Some(location_id), [] -> {
      case
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      {
        Some(variant) ->
          case
            find_inventory_level(variant_inventory_levels(variant), location_id)
          {
            Some(level) -> Some(#(variant, level))
            None -> None
          }
        None -> None
      }
    }
    _, _, _ -> None
  }
  let activation_result = case resolved, user_errors {
    Some(#(variant, level)), [] -> {
      case inventory_level_is_active(level) {
        True -> #(store, identity, resolved)
        False -> {
          let #(next_level, next_identity) =
            reactivate_inventory_level(level, identity)
          let next_levels =
            replace_inventory_level(
              variant_inventory_levels(variant),
              level.location.id,
              next_level,
            )
          let next_variant = variant_with_inventory_levels(variant, next_levels)
          #(
            stage_variant_inventory_levels(store, variant, next_levels),
            next_identity,
            Some(#(next_variant, next_level)),
          )
        }
      }
    }
    _, _ -> #(store, identity, resolved)
  }
  let #(next_store, next_identity, next_resolved) = activation_result
  let staged_ids = inventory_activate_staged_ids(next_resolved)
  mutation_result(
    key,
    inventory_activate_payload(
      next_store,
      next_resolved,
      user_errors,
      field,
      fragments,
    ),
    next_store,
    next_identity,
    staged_ids,
  )
}

fn handle_inventory_deactivate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inventory_level_id = read_string_field(args, "inventoryLevelId")
  let target = case inventory_level_id {
    Some(inventory_level_id) ->
      find_inventory_level_target(store, inventory_level_id)
    None -> None
  }
  let user_errors = case target {
    Some(#(variant, level)) -> {
      let active_levels =
        variant_inventory_levels(variant)
        |> active_inventory_levels
      case list.length(active_levels) <= 1 {
        True -> [
          NullableFieldUserError(
            None,
            "The product couldn't be unstocked from "
              <> level.location.name
              <> " because products need to be stocked at a minimum of 1 location.",
          ),
        ]
        False -> []
      }
    }
    None -> []
  }
  let next_store = case target, user_errors {
    Some(#(variant, level)), [] -> {
      let next_level = InventoryLevelRecord(..level, is_active: Some(False))
      let next_levels =
        variant_inventory_levels(variant)
        |> replace_inventory_level(level.location.id, next_level)
      stage_variant_inventory_levels(store, variant, next_levels)
    }
    _, _ -> store
  }
  let staged_ids = case target, user_errors {
    Some(#(_variant, level)), [] -> [level.id]
    _, _ -> []
  }
  mutation_result(
    key,
    inventory_deactivate_payload(user_errors, field, fragments),
    next_store,
    identity,
    staged_ids,
  )
}

fn handle_inventory_bulk_toggle_activation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inventory_item_id = read_string_field(args, "inventoryItemId")
  let first_update =
    read_arg_object_list(args, "inventoryItemUpdates")
    |> list.first
    |> option.from_result
  let location_id = case first_update {
    Some(update) -> read_string_field(update, "locationId")
    None -> None
  }
  let activate = case first_update {
    Some(update) -> read_bool_field(update, "activate")
    None -> None
  }
  let variant = case inventory_item_id {
    Some(inventory_item_id) ->
      store.find_effective_variant_by_inventory_item_id(
        store,
        inventory_item_id,
      )
    None -> None
  }
  let target = case variant, location_id {
    Some(variant), Some(location_id) ->
      case
        find_inventory_level(variant_inventory_levels(variant), location_id)
      {
        Some(level) -> Some(#(variant, level))
        None -> None
      }
    _, _ -> None
  }
  let user_errors = case variant, location_id, target, activate {
    Some(_variant), Some(_location_id), None, _ -> [
      ProductUserError(
        ["inventoryItemUpdates", "0", "locationId"],
        "The quantity couldn't be updated because the location was not found.",
        Some("LOCATION_NOT_FOUND"),
      ),
    ]
    Some(variant),
      Some(_location_id),
      Some(#(_target_variant, level)),
      Some(False)
    -> {
      case list.length(variant_inventory_levels(variant)) <= 1 {
        True -> [
          ProductUserError(
            ["inventoryItemUpdates", "0", "locationId"],
            "The variant couldn't be unstocked from "
              <> level.location.name
              <> " because products need to be stocked at a minimum of 1 location.",
            Some("CANNOT_DEACTIVATE_FROM_ONLY_LOCATION"),
          ),
        ]
        False -> []
      }
    }
    _, _, _, _ -> []
  }
  let outcome = case target, activate, user_errors {
    Some(#(variant, level)), Some(False), [] -> {
      let next_levels =
        variant_inventory_levels(variant)
        |> list.filter(fn(candidate) { candidate.id != level.id })
      let next_store =
        stage_variant_inventory_levels(store, variant, next_levels)
      #(
        next_store,
        store.find_effective_variant_by_inventory_item_id(
          next_store,
          option.unwrap(inventory_item_id, ""),
        ),
        Some([]),
        [level.id],
      )
    }
    Some(#(variant, level)), _, [] -> #(
      store,
      Some(variant),
      Some([#(variant, level)]),
      case variant.inventory_item {
        Some(item) -> [level.id, item.id]
        None -> [level.id]
      },
    )
    _, _, [] -> #(store, variant, None, [])
    _, _, _ -> #(store, None, None, [])
  }
  let #(next_store, payload_variant, response_levels, staged_ids) = outcome
  mutation_result(
    key,
    inventory_bulk_toggle_activation_payload(
      next_store,
      payload_variant,
      response_levels,
      user_errors,
      field,
      fragments,
    ),
    next_store,
    identity,
    staged_ids,
  )
}

fn handle_inventory_item_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inventory_item_id = read_string_field(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let existing_variant = case inventory_item_id {
    Some(inventory_item_id) ->
      store.find_effective_variant_by_inventory_item_id(
        store,
        inventory_item_id,
      )
    None -> None
  }
  case existing_variant {
    Some(variant) ->
      case variant.inventory_item {
        Some(existing_item) -> {
          let #(next_item, _) =
            read_variant_inventory_item(
              identity,
              Some(input),
              Some(existing_item),
            )
          case next_item {
            Some(next_item) -> {
              let next_variant =
                ProductVariantRecord(..variant, inventory_item: Some(next_item))
              let next_variants =
                store.get_effective_variants_by_product_id(
                  store,
                  variant.product_id,
                )
                |> list.map(fn(candidate) {
                  case candidate.id == variant.id {
                    True -> next_variant
                    False -> candidate
                  }
                })
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  variant.product_id,
                  next_variants,
                )
              let #(_, synced_store, synced_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity,
                  variant.product_id,
                )
              let updated_variant =
                store.get_effective_variant_by_id(synced_store, variant.id)
                |> option.unwrap(next_variant)
              mutation_result(
                key,
                inventory_item_update_payload(
                  synced_store,
                  Some(updated_variant),
                  [],
                  field,
                  fragments,
                ),
                synced_store,
                synced_identity,
                variant_staged_ids(updated_variant),
              )
            }
            None ->
              inventory_item_update_missing_result(
                key,
                store,
                identity,
                field,
                fragments,
              )
          }
        }
        None ->
          inventory_item_update_missing_result(
            key,
            store,
            identity,
            field,
            fragments,
          )
      }
    None ->
      inventory_item_update_missing_result(
        key,
        store,
        identity,
        field,
        fragments,
      )
  }
}

fn handle_inventory_set_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  uses_202604_contract: Bool,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let quantity_name = read_non_empty_string_field(input, "name")
  let reason = read_non_empty_string_field(input, "reason")
  let quantities = read_inventory_set_quantity_inputs(input)
  let ignore_compare_quantity =
    read_bool_field(input, "ignoreCompareQuantity") == Some(True)
  case
    inventory_set_202604_contract_error(
      uses_202604_contract,
      input,
      field,
      variables,
    )
  {
    Some(error) ->
      mutation_error_with_null_data_result(key, store, identity, [
        error,
      ])
    None ->
      case quantity_name, reason, quantities {
        None, _, _ ->
          inventory_quantity_mutation_result(
            key,
            "InventorySetQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "name"],
                "Inventory quantity name is required",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        Some(name), _, _ -> {
          case valid_inventory_set_quantity_name(name) {
            False ->
              inventory_quantity_mutation_result(
                key,
                "InventorySetQuantitiesPayload",
                store,
                identity,
                None,
                [invalid_inventory_set_quantity_name_error()],
                field,
                fragments,
                [],
              )
            True ->
              handle_valid_inventory_set_quantities(
                store,
                identity,
                key,
                input,
                name,
                reason,
                quantities,
                ignore_compare_quantity,
                uses_202604_contract,
                field,
                fragments,
              )
          }
        }
      }
  }
}

fn handle_valid_inventory_set_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: Option(String),
  quantities: List(InventorySetQuantityInput),
  ignore_compare_quantity: Bool,
  uses_202604_contract: Bool,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  case reason, quantities {
    None, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventorySetQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "reason"],
            "Inventory adjustment reason is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    _, [] ->
      inventory_quantity_mutation_result(
        key,
        "InventorySetQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "quantities"],
            "At least one inventory quantity is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    Some(reason), quantities -> {
      case
        !uses_202604_contract
        && !ignore_compare_quantity
        && list.any(quantities, fn(quantity) {
          quantity.compare_quantity == None
        })
      {
        True ->
          inventory_quantity_mutation_result(
            key,
            "InventorySetQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "ignoreCompareQuantity"],
                "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity.",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        False -> {
          let result =
            apply_inventory_set_quantities(
              store,
              identity,
              input,
              name,
              reason,
              quantities,
              ignore_compare_quantity,
              uses_202604_contract,
            )
          case result {
            Error(errors) ->
              inventory_quantity_mutation_result(
                key,
                "InventorySetQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            Ok(applied) -> {
              let #(next_store, next_identity, group, staged_ids) = applied
              inventory_quantity_mutation_result(
                key,
                "InventorySetQuantitiesPayload",
                next_store,
                next_identity,
                Some(group),
                [],
                field,
                fragments,
                staged_ids,
              )
            }
          }
        }
      }
    }
  }
}

fn handle_inventory_move_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let reason = read_non_empty_string_field(input, "reason")
  let changes = read_inventory_move_quantity_inputs(input)
  case reason, changes {
    None, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventoryMoveQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "reason"],
            "Inventory adjustment reason is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    _, [] ->
      inventory_quantity_mutation_result(
        key,
        "InventoryMoveQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "changes"],
            "At least one inventory quantity move is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    Some(reason), changes -> {
      case validate_inventory_move_inputs(changes) {
        [_, ..] as errors ->
          inventory_quantity_mutation_result(
            key,
            "InventoryMoveQuantitiesPayload",
            store,
            identity,
            None,
            errors,
            field,
            fragments,
            [],
          )
        [] -> {
          let result =
            apply_inventory_move_quantities(
              store,
              identity,
              input,
              reason,
              changes,
            )
          case result {
            Error(errors) ->
              inventory_quantity_mutation_result(
                key,
                "InventoryMoveQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            Ok(applied) -> {
              let #(next_store, next_identity, group, staged_ids) = applied
              inventory_quantity_mutation_result(
                key,
                "InventoryMoveQuantitiesPayload",
                next_store,
                next_identity,
                Some(group),
                [],
                field,
                fragments,
                staged_ids,
              )
            }
          }
        }
      }
    }
  }
}

fn product_update_validation_error(
  input: Dict(String, ResolvedValue),
) -> Option(ProductUserError) {
  case read_string_field(input, "title") {
    Some(title) ->
      case string.length(string.trim(title)) == 0 {
        True -> Some(ProductUserError(["title"], "Title can't be blank", None))
        False -> product_update_handle_validation_error(input)
      }
    None -> product_update_handle_validation_error(input)
  }
}

fn product_create_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  let product_errors = case product_create_validation_error(input) {
    Some(error) -> [error]
    None -> []
  }

  list.append(product_errors, product_create_variant_errors(input))
}

fn product_create_validation_error(
  input: Dict(String, ResolvedValue),
) -> Option(ProductUserError) {
  case read_string_field(input, "title") {
    Some(title) ->
      case string.length(string.trim(title)) == 0 {
        True -> Some(ProductUserError(["title"], "Title can't be blank", None))
        False -> product_create_handle_validation_error(input)
      }
    None -> Some(ProductUserError(["title"], "Title can't be blank", None))
  }
}

fn product_create_variant_errors(
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

fn product_create_handle_validation_error(
  input: Dict(String, ResolvedValue),
) -> Option(ProductUserError) {
  case read_explicit_product_handle(input) {
    Some(handle) ->
      case string.length(handle) > 255 {
        True ->
          Some(ProductUserError(
            ["handle"],
            "Handle is too long (maximum is 255 characters)",
            None,
          ))
        False -> None
      }
    None -> None
  }
}

fn product_update_handle_validation_error(
  input: Dict(String, ResolvedValue),
) -> Option(ProductUserError) {
  case read_string_field(input, "handle") {
    Some(handle) ->
      case string.length(handle) > 255 {
        True ->
          Some(ProductUserError(
            ["handle"],
            "Handle is too long (maximum is 255 characters)",
            None,
          ))
        False -> None
      }
    None -> None
  }
}

fn handle_collection_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let collection_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case collection_id {
    None ->
      mutation_result(
        key,
        collection_update_payload(
          store,
          None,
          [
            ProductUserError(["input", "id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id), input {
        None, _ ->
          mutation_result(
            key,
            collection_update_payload(
              store,
              None,
              [
                ProductUserError(["input", "id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection), Some(input) -> {
          let #(next_collection, next_identity) =
            updated_collection_record(identity, collection, input)
          let next_store =
            store.upsert_staged_collections(store, [next_collection])
          mutation_result(
            key,
            collection_update_payload(
              next_store,
              Some(next_collection),
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            [next_collection.id],
          )
        }
        Some(_), None ->
          mutation_result(
            key,
            collection_update_payload(
              store,
              None,
              [
                ProductUserError(
                  ["input", "id"],
                  "Collection id is required",
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
      }
  }
}

fn handle_collection_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case read_non_empty_string_field(input, "title") {
    None ->
      mutation_result(
        key,
        collection_create_payload(
          store,
          None,
          [
            ProductUserError(
              ["input", "title"],
              "Collection title is required",
              None,
            ),
          ],
          field,
          fragments,
          variables,
          None,
        ),
        store,
        identity,
        [],
      )
    Some(_) -> {
      let #(collection, next_identity) =
        created_collection_record(store, identity, input)
      let product_ids = read_collection_product_ids(input)
      let result = case product_ids {
        [] -> #(store, Some(collection), [])
        _ ->
          stage_collection_product_memberships(
            store,
            collection,
            product_ids,
            AppendProducts,
          )
      }
      let #(membership_store, result_collection, user_errors) = result
      case user_errors {
        [] -> {
          let staged_collection =
            result_collection
            |> option.unwrap(collection)
          let next_store =
            store.upsert_staged_collections(membership_store, [
              staged_collection,
            ])
          mutation_result(
            key,
            collection_create_payload(
              next_store,
              Some(staged_collection),
              [],
              field,
              fragments,
              variables,
              Some(0),
            ),
            next_store,
            next_identity,
            [staged_collection.id],
          )
        }
        _ ->
          mutation_result(
            key,
            collection_create_payload(
              store,
              None,
              user_errors,
              field,
              fragments,
              variables,
              None,
            ),
            store,
            next_identity,
            [],
          )
      }
    }
  }
}

fn handle_collection_add_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_add_products_payload(
          store,
          None,
          [
            ProductUserError(["id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_add_products_payload(
              store,
              None,
              [
                ProductUserError(["id"], "Collection does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let result =
            add_products_to_collection(
              store,
              collection,
              read_arg_string_list(args, "productIds"),
              AppendProducts,
            )
          let #(next_store, result_collection, user_errors) = result
          let staged_ids = case user_errors, result_collection {
            [], Some(record) -> [record.id]
            _, _ -> []
          }
          mutation_result(
            key,
            collection_add_products_payload(
              next_store,
              result_collection,
              user_errors,
              field,
              fragments,
            ),
            next_store,
            identity,
            staged_ids,
          )
        }
      }
  }
}

fn handle_collection_add_products_v2(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_add_products_v2_payload(
          None,
          [ProductUserError(["id"], "Collection id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_add_products_v2_payload(
              None,
              [ProductUserError(["id"], "Collection does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let placement = case collection.sort_order {
            Some("MANUAL") -> AppendProducts
            _ -> PrependReverseProducts
          }
          let #(next_store, result_collection, user_errors) =
            add_products_to_collection(
              store,
              collection,
              read_arg_string_list(args, "productIds"),
              placement,
            )
          case user_errors, result_collection {
            [], Some(record) -> {
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              mutation_result(
                key,
                collection_add_products_v2_payload(
                  Some(job_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [record.id],
              )
            }
            _, _ ->
              mutation_result(
                key,
                collection_add_products_v2_payload(
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
          }
        }
      }
  }
}

fn add_products_to_collection(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
  placement: CollectionProductPlacement,
) -> #(Store, Option(CollectionRecord), List(ProductUserError)) {
  let normalized_product_ids = dedupe_preserving_order(product_ids)
  case normalized_product_ids {
    [] -> #(store, None, [
      ProductUserError(
        ["productIds"],
        "At least one product id is required",
        None,
      ),
    ])
    _ ->
      case collection.is_smart {
        True -> #(store, None, [
          ProductUserError(
            ["id"],
            "Can't manually add products to a smart collection",
            None,
          ),
        ])
        False ->
          case
            list.find(normalized_product_ids, fn(product_id) {
              product_already_in_collection(store, collection.id, product_id)
            })
          {
            Ok(_) -> #(store, None, [
              ProductUserError(
                ["productIds"],
                "Product is already in the collection",
                None,
              ),
            ])
            Error(_) ->
              stage_collection_product_memberships(
                store,
                collection,
                normalized_product_ids,
                placement,
              )
          }
      }
  }
}

fn stage_collection_product_memberships(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
  placement: CollectionProductPlacement,
) -> #(Store, Option(CollectionRecord), List(ProductUserError)) {
  let existing_product_ids =
    product_ids
    |> list.filter(fn(product_id) {
      case store.get_effective_product_by_id(store, product_id) {
        Some(_) -> True
        None -> False
      }
    })
  case existing_product_ids {
    [] -> #(store, Some(collection), [])
    _ -> {
      let existing_positions =
        store.list_effective_products_for_collection(store, collection.id)
        |> list.map(fn(entry) {
          let #(_, membership) = entry
          membership.position
        })
      let first_position = case placement {
        AppendProducts ->
          case existing_positions {
            [] -> 0
            _ -> {
              list.fold(existing_positions, -1, int.max) + 1
            }
          }
        PrependReverseProducts ->
          case existing_positions {
            [] -> 0
            [first, ..rest] ->
              list.fold(rest, first, int.min)
              - list.length(existing_product_ids)
          }
      }
      let positioned_product_ids = case placement {
        AppendProducts -> existing_product_ids
        PrependReverseProducts -> list.reverse(existing_product_ids)
      }
      let existing_memberships =
        store.list_effective_products_for_collection(store, collection.id)
      let memberships =
        positioned_product_ids
        |> enumerate_strings()
        |> list.map(fn(entry) {
          let #(product_id, index) = entry
          ProductCollectionRecord(
            collection_id: collection.id,
            product_id: product_id,
            position: first_position + index,
            cursor: None,
          )
        })
      let next_count =
        list.length(existing_memberships) + list.length(memberships)
      let next_collection =
        CollectionRecord(..collection, products_count: Some(next_count))
      let next_store =
        store
        |> store.upsert_staged_collections([next_collection])
        |> store.upsert_staged_product_collections(memberships)
      #(next_store, Some(next_collection), [])
    }
  }
}

fn handle_collection_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let collection_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case collection_id {
    None ->
      mutation_result(
        key,
        collection_delete_payload(
          None,
          [
            ProductUserError(["input", "id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_delete_payload(
              None,
              [
                ProductUserError(["input", "id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let next_store = store.delete_staged_collection(store, collection_id)
          mutation_result(
            key,
            collection_delete_payload(Some(collection_id), [], field, fragments),
            next_store,
            identity,
            [collection_id],
          )
        }
      }
  }
}

fn handle_collection_remove_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_remove_products_payload(
          None,
          [
            ProductUserError(["id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_remove_products_payload(
              None,
              [
                ProductUserError(["id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let next_store =
            remove_products_from_collection(
              store,
              collection,
              read_arg_string_list(args, "productIds"),
            )
          let next_count =
            store.list_effective_products_for_collection(
              next_store,
              collection.id,
            )
            |> list.length
          let next_collection =
            CollectionRecord(..collection, products_count: Some(next_count))
          let next_store =
            store.upsert_staged_collections(next_store, [next_collection])
          let #(job_id, next_identity) =
            synthetic_identity.make_synthetic_gid(identity, "Job")
          mutation_result(
            key,
            collection_remove_products_payload(
              Some(job_id),
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            [collection.id],
          )
        }
      }
  }
}

fn remove_products_from_collection(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
) -> Store {
  let normalized_product_ids = dedupe_preserving_order(product_ids)
  list.fold(normalized_product_ids, store, fn(current_store, product_id) {
    case store.get_effective_product_by_id(current_store, product_id) {
      None -> current_store
      Some(_) -> {
        let next_memberships =
          store.list_effective_collections_for_product(
            current_store,
            product_id,
          )
          |> list.filter(fn(entry) {
            let #(existing_collection, _) = entry
            existing_collection.id != collection.id
          })
          |> list.map(fn(entry) {
            let #(_, membership) = entry
            membership
          })
        store.replace_staged_collections_for_product(
          current_store,
          product_id,
          next_memberships,
        )
      }
    }
  })
}

fn handle_collection_reorder_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_reorder_products_payload(
          None,
          [
            ProductUserError(["id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_reorder_products_payload(
              None,
              [
                ProductUserError(
                  ["id"],
                  "Collection not found",
                  Some("COLLECTION_NOT_FOUND"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let result =
            reorder_collection_products(
              store,
              collection,
              read_arg_object_list(args, "moves"),
            )
          let #(next_store, user_errors) = result
          case user_errors {
            [] -> {
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              mutation_result(
                key,
                collection_reorder_products_payload(
                  Some(job_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [collection.id],
              )
            }
            _ ->
              mutation_result(
                key,
                collection_reorder_products_payload(
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
          }
        }
      }
  }
}

fn reorder_collection_products(
  store: Store,
  collection: CollectionRecord,
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(Store, List(ProductUserError)) {
  case
    collection.is_smart
    || case collection.sort_order {
      Some(sort_order) -> sort_order != "MANUAL"
      None -> False
    }
  {
    True -> #(store, [
      ProductUserError(
        ["id"],
        "Can't reorder products unless collection is manually sorted",
        Some("MANUALLY_SORTED_COLLECTION"),
      ),
    ])
    False -> {
      let #(moves, user_errors) = read_collection_product_moves(raw_moves)
      let ordered_entries =
        store.list_effective_products_for_collection(store, collection.id)
      let product_ids_in_collection =
        ordered_entries
        |> list.map(fn(entry) {
          let #(product, _) = entry
          product.id
        })
      let user_errors =
        list.fold(enumerate_items(moves), user_errors, fn(errors, entry) {
          let #(move, index) = entry
          let CollectionProductMove(id: product_id, new_position: _) = move
          case store.get_effective_product_by_id(store, product_id) {
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "id"],
                  "Product does not exist",
                  Some("INVALID_MOVE"),
                ),
              ])
            Some(_) ->
              case list.contains(product_ids_in_collection, product_id) {
                True -> errors
                False ->
                  list.append(errors, [
                    ProductUserError(
                      ["moves", int.to_string(index), "id"],
                      "Product is not in the collection",
                      Some("INVALID_MOVE"),
                    ),
                  ])
              }
          }
        })
      case user_errors {
        [] -> {
          let reordered_entries =
            apply_collection_product_moves(ordered_entries, moves)
          let next_store =
            reordered_entries
            |> enumerate_items()
            |> list.fold(store, fn(current_store, entry) {
              let #(#(product, _), position) = entry
              let next_memberships =
                store.list_effective_collections_for_product(
                  current_store,
                  product.id,
                )
                |> list.map(fn(collection_entry) {
                  let #(existing_collection, membership) = collection_entry
                  case existing_collection.id == collection.id {
                    True ->
                      ProductCollectionRecord(..membership, position: position)
                    False -> membership
                  }
                })
              store.replace_staged_collections_for_product(
                current_store,
                product.id,
                next_memberships,
              )
            })
          #(next_store, [])
        }
        _ -> #(store, user_errors)
      }
    }
  }
}

fn read_collection_product_moves(
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(List(CollectionProductMove), List(ProductUserError)) {
  case raw_moves {
    [] -> #([], [
      ProductUserError(
        ["moves"],
        "At least one move is required",
        Some("INVALID_MOVE"),
      ),
    ])
    _ -> {
      let too_many_errors = case list.length(raw_moves) > 250 {
        True -> [
          ProductUserError(
            ["moves"],
            "Too many moves were provided",
            Some("INVALID_MOVE"),
          ),
        ]
        False -> []
      }
      let result =
        raw_moves
        |> enumerate_items()
        |> list.fold(#([], too_many_errors), fn(acc, entry) {
          let #(moves, errors) = acc
          let #(raw_move, index) = entry
          let product_id = read_string_field(raw_move, "id")
          let new_position = read_collection_reorder_position(raw_move)
          let errors = case product_id {
            Some(_) -> errors
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "id"],
                  "Product id is required",
                  Some("INVALID_MOVE"),
                ),
              ])
          }
          let errors = case new_position {
            Some(_) -> errors
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "newPosition"],
                  "Position is invalid",
                  Some("INVALID_MOVE"),
                ),
              ])
          }
          case product_id, new_position {
            Some(id), Some(position) -> #(
              list.append(moves, [
                CollectionProductMove(id: id, new_position: position),
              ]),
              errors,
            )
            _, _ -> #(moves, errors)
          }
        })
      result
    }
  }
}

fn read_collection_reorder_position(
  fields: Dict(String, ResolvedValue),
) -> Option(Int) {
  case dict.get(fields, "newPosition") {
    Ok(IntVal(value)) -> Some(int.max(0, value))
    Ok(StringVal(value)) -> parse_unsigned_int_string(value)
    _ -> None
  }
}

fn parse_unsigned_int_string(value: String) -> Option(Int) {
  let trimmed = string.trim(value)
  case
    string.length(trimmed) > 0
    && list.all(string.to_graphemes(trimmed), is_decimal_digit)
  {
    False -> None
    True ->
      case int.parse(trimmed) {
        Ok(parsed) -> Some(parsed)
        Error(_) -> None
      }
  }
}

fn is_decimal_digit(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

fn apply_collection_product_moves(
  entries: List(#(ProductRecord, ProductCollectionRecord)),
  moves: List(CollectionProductMove),
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  list.fold(moves, entries, fn(current_entries, move) {
    let CollectionProductMove(id: product_id, new_position: new_position) = move
    case
      list.find(current_entries, fn(entry) {
        let #(product, _) = entry
        product.id == product_id
      })
    {
      Error(_) -> current_entries
      Ok(entry) -> {
        let without_entry =
          current_entries
          |> list.filter(fn(candidate) {
            let #(product, _) = candidate
            product.id != product_id
          })
        insert_collection_entry(without_entry, entry, new_position)
      }
    }
  })
}

fn insert_collection_entry(
  entries: List(#(ProductRecord, ProductCollectionRecord)),
  entry: #(ProductRecord, ProductCollectionRecord),
  position: Int,
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  let insertion_index = int.min(position, list.length(entries))
  let before = list.take(entries, insertion_index)
  let after = list.drop(entries, insertion_index)
  list.append(before, [entry, ..after])
}

fn product_already_in_collection(
  store: Store,
  collection_id: String,
  product_id: String,
) -> Bool {
  store.list_effective_collections_for_product(store, product_id)
  |> list.any(fn(entry) {
    let #(collection, _) = entry
    collection.id == collection_id
  })
}

fn enumerate_strings(values: List(String)) -> List(#(String, Int)) {
  values
  |> enumerate_items()
}

fn handle_tags_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        tags_update_payload(
          store,
          is_add,
          None,
          [ProductUserError(["id"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            tags_update_payload(
              store,
              is_add,
              None,
              [ProductUserError(["id"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) -> {
          let tags = read_tag_inputs(args, is_add)
          case tags {
            [] ->
              mutation_result(
                key,
                tags_update_payload(
                  store,
                  is_add,
                  None,
                  [
                    ProductUserError(
                      ["tags"],
                      "At least one tag is required",
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
            _ -> {
              let next_tags = case is_add {
                True -> normalize_product_tags(list.append(product.tags, tags))
                False ->
                  normalize_product_tags(
                    list.filter(product.tags, fn(tag) {
                      !list.contains(tags, tag)
                    }),
                  )
              }
              let next_product = ProductRecord(..product, tags: next_tags)
              let #(_, next_store) =
                store.upsert_staged_product(store, next_product)
              mutation_result(
                key,
                tags_update_payload(
                  next_store,
                  is_add,
                  Some(next_product),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                identity,
                [next_product.id],
              )
            }
          }
        }
      }
  }
}

fn product_delete_input_error(
  document: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  case find_argument(arguments, "input") {
    Some(argument) ->
      case argument.value {
        ObjectValue(fields: fields, loc: loc) ->
          case find_object_field(fields, "id") {
            None ->
              Some(build_product_delete_missing_input_id_error(loc, document))
            Some(ObjectField(value: NullValue(..), ..)) ->
              Some(build_product_delete_null_input_id_error(loc, document))
            Some(_) -> None
          }
        VariableValue(variable: variable) -> {
          let args = graphql_helpers.field_args(field, variables)
          let input = graphql_helpers.read_arg_object(args, "input")
          let invalid = case input {
            Some(input) ->
              case dict.get(input, "id") {
                Ok(StringVal(_)) -> False
                _ -> True
              }
            None -> True
          }
          case invalid {
            False -> None
            True ->
              Some(build_product_delete_invalid_variable_error(
                variable.name.value,
                resolved_input_to_json(input),
                find_variable_definition_location(document, variable.name.value),
                document,
              ))
          }
        }
        _ -> None
      }
    None -> None
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

fn build_product_delete_missing_input_id_error(
  loc: Option(Location),
  document: String,
) -> Json {
  product_delete_input_object_error(
    "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
    loc,
    document,
    [
      #("code", json.string("missingRequiredInputObjectAttribute")),
      #("argumentName", json.string("id")),
      #("argumentType", json.string("ID!")),
      #("inputObjectType", json.string("ProductDeleteInput")),
    ],
  )
}

fn build_product_delete_null_input_id_error(
  loc: Option(Location),
  document: String,
) -> Json {
  product_delete_input_object_error(
    "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
    loc,
    document,
    [
      #("code", json.string("argumentLiteralsIncompatible")),
      #("typeName", json.string("InputObject")),
      #("argumentName", json.string("id")),
    ],
  )
}

fn product_delete_input_object_error(
  message: String,
  loc: Option(Location),
  document: String,
  extensions: List(#(String, Json)),
) -> Json {
  let base = [#("message", json.string(message))]
  let with_locations = case locations_payload(loc, document) {
    Some(locations) -> list.append(base, [#("locations", locations)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(["mutation", "productDelete", "input", "id"], json.string),
      ),
      #("extensions", json.object(extensions)),
    ]),
  )
}

fn build_product_delete_invalid_variable_error(
  variable_name: String,
  value: Json,
  loc: Option(Location),
  document: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)",
      ),
    ),
  ]
  let with_locations = case locations_payload(loc, document) {
    Some(locations) -> list.append(base, [#("locations", locations)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", value),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #("path", json.array(["id"], json.string)),
                #("explanation", json.string("Expected value to not be null")),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

fn find_variable_definition_location(
  document: String,
  variable_name: String,
) -> Option(Location) {
  case parser.parse(graphql_source.new(document)) {
    Ok(parsed) ->
      find_variable_definition_location_in_definitions(
        parsed.definitions,
        variable_name,
      )
    Error(_) -> None
  }
}

fn find_variable_definition_location_in_definitions(
  definitions: List(Definition),
  variable_name: String,
) -> Option(Location) {
  case definitions {
    [] -> None
    [definition, ..rest] ->
      case definition {
        OperationDefinition(variable_definitions: definitions, ..) ->
          case find_variable_definition(definitions, variable_name) {
            Some(location) -> Some(location)
            None ->
              find_variable_definition_location_in_definitions(
                rest,
                variable_name,
              )
          }
        _ ->
          find_variable_definition_location_in_definitions(rest, variable_name)
      }
  }
}

fn find_variable_definition(
  definitions: List(VariableDefinition),
  variable_name: String,
) -> Option(Location) {
  case definitions {
    [] -> None
    [definition, ..rest] -> {
      let VariableDefinition(variable: variable, loc: loc, ..) = definition
      case variable.name.value == variable_name {
        True -> loc
        False -> find_variable_definition(rest, variable_name)
      }
    }
  }
}

fn locations_payload(loc: Option(Location), document: String) -> Option(Json) {
  option.map(loc, graphql_helpers.locations_json(_, document))
}

fn resolved_input_to_json(input: Option(Dict(String, ResolvedValue))) -> Json {
  case input {
    Some(fields) ->
      json.object(
        list.map(dict.to_list(fields), fn(entry) {
          let #(key, value) = entry
          #(key, resolved_value_to_json(value))
        }),
      )
    None -> json.null()
  }
}

fn resolved_value_to_json(value: ResolvedValue) -> Json {
  case value {
    StringVal(value) -> json.string(value)
    IntVal(value) -> json.int(value)
    FloatVal(value) -> json.float(value)
    BoolVal(value) -> json.bool(value)
    NullVal -> json.null()
    ListVal(values) -> json.array(values, resolved_value_to_json)
    ObjectVal(fields) ->
      json.object(
        list.map(dict.to_list(fields), fn(entry) {
          let #(key, value) = entry
          #(key, resolved_value_to_json(value))
        }),
      )
  }
}

fn product_change_status_null_product_id_error(
  document: String,
  operation_path: String,
  field: Selection,
) -> Option(Json) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  case find_argument(arguments, "productId") {
    Some(argument) ->
      case argument.value {
        NullValue(..) -> {
          let field_loc = case field {
            Field(loc: loc, ..) -> loc
            _ -> None
          }
          Some(build_null_argument_error(
            "productChangeStatus",
            "productId",
            "ID!",
            operation_path,
            field_loc,
            document,
          ))
        }
        _ -> None
      }
    None -> None
  }
}

fn is_valid_product_status(status: Option(String)) -> Bool {
  case status {
    Some("ACTIVE") | Some("ARCHIVED") | Some("DRAFT") -> True
    _ -> False
  }
}

fn stage_product_options_reorder(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_inputs: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let #(next_options, user_errors) =
    reorder_product_options(existing_options, option_inputs)
  case user_errors {
    [_, ..] ->
      mutation_result(
        key,
        product_options_reorder_payload(
          store,
          Some(product),
          user_errors,
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    [] -> {
      let next_variants =
        store.get_effective_variants_by_product_id(store, product_id)
        |> reorder_variant_selections_for_options(next_options)
      let synced_options =
        sync_product_options_with_variants(next_options, next_variants)
      let next_store =
        store
        |> store.replace_staged_options_for_product(product_id, synced_options)
        |> store.replace_staged_variants_for_product(product_id, next_variants)
      mutation_result(
        key,
        product_options_reorder_payload(
          next_store,
          store.get_effective_product_by_id(next_store, product_id),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        list.map(next_options, fn(option) { option.id }),
      )
    }
  }
}

fn stage_product_options_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_ids: List(String),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let existing_ids = list.map(existing_options, fn(option) { option.id })
  let unknown_errors = unknown_option_errors(option_ids, existing_ids)
  case unknown_errors {
    [_, ..] ->
      mutation_result(
        key,
        product_options_delete_payload(
          store,
          [],
          Some(product),
          unknown_errors,
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    [] -> {
      let deleted_ids =
        existing_options
        |> list.filter(fn(option) { list.contains(option_ids, option.id) })
        |> list.map(fn(option) { option.id })
      let remaining_options =
        existing_options
        |> list.filter(fn(option) { !list.contains(option_ids, option.id) })
        |> position_options(1, [])
      let #(next_options, next_variants, final_identity, restored_ids) = case
        remaining_options
      {
        [] -> restore_default_option_state(identity, product, existing_variants)
        [_, ..] -> #(remaining_options, existing_variants, identity, [])
      }
      let synced_options =
        sync_product_options_with_variants(next_options, next_variants)
      let next_store =
        store
        |> store.replace_staged_variants_for_product(product_id, next_variants)
        |> store.replace_staged_options_for_product(product_id, synced_options)
      mutation_result(
        key,
        product_options_delete_payload(
          next_store,
          deleted_ids,
          store.get_effective_product_by_id(next_store, product_id),
          [],
          field,
          fragments,
        ),
        next_store,
        final_identity,
        list.append(deleted_ids, restored_ids),
      )
    }
  }
}

fn handle_product_option_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_option_update_payload(
          store,
          None,
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_option_update_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          case graphql_helpers.read_arg_object(args, "option") {
            None ->
              mutation_result(
                key,
                product_option_update_payload(
                  store,
                  None,
                  [
                    ProductUserError(
                      ["option", "id"],
                      "Option id is required",
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
            Some(option_input) ->
              stage_product_option_update(
                store,
                identity,
                key,
                product,
                option_input,
                args,
                field,
                fragments,
              )
          }
      }
  }
}

fn stage_product_option_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_input: Dict(String, ResolvedValue),
  args: Dict(String, ResolvedValue),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  case read_string_field(option_input, "id") {
    None ->
      mutation_result(
        key,
        product_option_update_payload(
          store,
          None,
          [
            ProductUserError(["option", "id"], "Option id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(option_id) ->
      case find_product_option(existing_options, product_id, option_id) {
        None ->
          mutation_result(
            key,
            product_option_update_payload(
              store,
              Some(product),
              [ProductUserError(["option"], "Option does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(target_option) -> {
          let #(updated_option, renamed_values, identity_after_values, new_ids) =
            update_product_option_record(
              identity,
              target_option,
              option_input,
              read_arg_object_list(args, "optionValuesToAdd"),
              read_arg_object_list(args, "optionValuesToUpdate"),
              read_arg_string_list(args, "optionValuesToDelete"),
            )
          let next_options =
            existing_options
            |> list.filter(fn(option) { option.id != option_id })
            |> insert_option_at_position(
              updated_option,
              read_int_field(option_input, "position"),
            )
          let next_variants =
            existing_variants
            |> remap_variant_selections_for_option_update(
              target_option.name,
              updated_option.name,
              renamed_values,
            )
            |> reorder_variant_selections_for_options(next_options)
          let synced_options =
            sync_product_options_with_variants(next_options, next_variants)
          let next_store =
            store
            |> store.replace_staged_variants_for_product(
              product_id,
              next_variants,
            )
            |> store.replace_staged_options_for_product(
              product_id,
              synced_options,
            )
          mutation_result(
            key,
            product_option_update_payload(
              next_store,
              store.get_effective_product_by_id(next_store, product_id),
              [],
              field,
              fragments,
            ),
            next_store,
            identity_after_values,
            list.append([option_id], new_ids),
          )
        }
      }
  }
}

fn stage_product_options_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  option_inputs: List(Dict(String, ResolvedValue)),
  should_create_option_variants: Bool,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let replacing_default =
    product_uses_only_default_option_state(existing_options, existing_variants)
  let starting_options = case replacing_default {
    True -> []
    False -> existing_options
  }
  let #(created_options, identity_after_options) =
    make_created_option_records(identity, product_id, option_inputs)
  let next_options =
    list.append(starting_options, created_options)
    |> sort_and_position_options
  let next_variants = case replacing_default, existing_variants {
    True, [first_variant, ..] -> [
      remap_variant_to_first_option_values(first_variant, next_options),
    ]
    _, _ ->
      map_variants_to_first_new_option_values(
        existing_variants,
        created_options,
      )
  }
  let #(next_variants, final_identity) = case should_create_option_variants {
    True ->
      create_variants_for_option_value_combinations(
        identity_after_options,
        product_id,
        next_options,
        created_options,
        next_variants,
      )
    False -> #(next_variants, identity_after_options)
  }
  let synced_options =
    sync_product_options_with_variants(next_options, next_variants)
  let next_store =
    store
    |> store.replace_staged_variants_for_product(product_id, next_variants)
    |> store.replace_staged_options_for_product(product_id, synced_options)
  let staged_ids =
    list.append(
      list.map(created_options, fn(option) { option.id }),
      list.flat_map(created_options, fn(option) {
        list.map(option.option_values, fn(value) { value.id })
      }),
    )
  mutation_result(
    key,
    product_options_create_payload(
      next_store,
      store.get_effective_product_by_id(next_store, product_id),
      [],
      field,
      fragments,
    ),
    next_store,
    final_identity,
    staged_ids,
  )
}

fn handle_selling_plan_group_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "sellingPlanGroupCreate" -> {
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let resources =
        graphql_helpers.read_arg_object(args, "resources")
        |> option.unwrap(dict.new())
      let #(group, next_identity) =
        make_selling_plan_group_record(identity, input, None, resources)
      let #(_, next_store) =
        store.upsert_staged_selling_plan_group(store, group)
      mutation_result(
        key,
        selling_plan_group_mutation_payload(
          next_store,
          field,
          variables,
          fragments,
          Some(group),
          [],
          None,
          None,
          None,
          None,
        ),
        next_store,
        next_identity,
        selling_plan_group_staged_ids(group),
      )
    }
    "sellingPlanGroupUpdate" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              Some(None),
              None,
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(existing) -> {
          let input =
            graphql_helpers.read_arg_object(args, "input")
            |> option.unwrap(dict.new())
          let deleted_plan_ids =
            read_string_list_field(input, "sellingPlansToDelete")
            |> option.unwrap([])
            |> list.filter(fn(plan_id) {
              list.any(existing.selling_plans, fn(plan) { plan.id == plan_id })
            })
          let #(group, next_identity) =
            make_selling_plan_group_record(
              identity,
              input,
              Some(existing),
              dict.new(),
            )
          let #(_, next_store) =
            store.upsert_staged_selling_plan_group(store, group)
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              Some(group),
              [],
              Some(Some(deleted_plan_ids)),
              None,
              None,
              None,
            ),
            next_store,
            next_identity,
            selling_plan_group_staged_ids(group),
          )
        }
      }
    }
    "sellingPlanGroupDelete" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              Some(None),
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          let next_store =
            store.delete_staged_selling_plan_group(store, group.id)
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              None,
              [],
              None,
              Some(Some(group.id)),
              None,
              None,
            ),
            next_store,
            identity,
            [group.id],
          )
        }
      }
    }
    "sellingPlanGroupAddProducts" | "sellingPlanGroupAddProductVariants" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              None,
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          let next_group = case root_name {
            "sellingPlanGroupAddProducts" ->
              SellingPlanGroupRecord(
                ..group,
                product_ids: dedupe_preserving_order(list.append(
                  group.product_ids,
                  read_arg_string_list(args, "productIds"),
                )),
              )
            _ ->
              SellingPlanGroupRecord(
                ..group,
                product_variant_ids: dedupe_preserving_order(list.append(
                  group.product_variant_ids,
                  read_arg_string_list(args, "productVariantIds"),
                )),
              )
          }
          let #(_, next_store) =
            store.upsert_staged_selling_plan_group(store, next_group)
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              Some(next_group),
              [],
              None,
              None,
              None,
              None,
            ),
            next_store,
            identity,
            [next_group.id],
          )
        }
      }
    }
    "sellingPlanGroupRemoveProducts"
    | "sellingPlanGroupRemoveProductVariants" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              None,
              case root_name {
                "sellingPlanGroupRemoveProducts" -> Some(None)
                _ -> None
              },
              case root_name {
                "sellingPlanGroupRemoveProductVariants" -> Some(None)
                _ -> None
              },
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          case root_name {
            "sellingPlanGroupRemoveProducts" -> {
              let requested = read_arg_string_list(args, "productIds")
              let removed =
                group.product_ids
                |> list.filter(fn(product_id) {
                  list.contains(requested, product_id)
                })
              let next_group =
                SellingPlanGroupRecord(
                  ..group,
                  product_ids: group.product_ids
                    |> list.filter(fn(product_id) {
                      !list.contains(requested, product_id)
                    }),
                )
              let #(_, next_store) =
                store.upsert_staged_selling_plan_group(store, next_group)
              mutation_result(
                key,
                selling_plan_group_mutation_payload(
                  next_store,
                  field,
                  variables,
                  fragments,
                  None,
                  [],
                  None,
                  None,
                  Some(Some(removed)),
                  None,
                ),
                next_store,
                identity,
                [next_group.id],
              )
            }
            _ -> {
              let requested = read_arg_string_list(args, "productVariantIds")
              let removed =
                group.product_variant_ids
                |> list.filter(fn(variant_id) {
                  list.contains(requested, variant_id)
                })
              let next_group =
                SellingPlanGroupRecord(
                  ..group,
                  product_variant_ids: group.product_variant_ids
                    |> list.filter(fn(variant_id) {
                      !list.contains(requested, variant_id)
                    }),
                )
              let #(_, next_store) =
                store.upsert_staged_selling_plan_group(store, next_group)
              mutation_result(
                key,
                selling_plan_group_mutation_payload(
                  next_store,
                  field,
                  variables,
                  fragments,
                  None,
                  [],
                  None,
                  None,
                  None,
                  Some(Some(removed)),
                ),
                next_store,
                identity,
                [next_group.id],
              )
            }
          }
        }
      }
    }
    _ -> mutation_result(key, json.null(), store, identity, [])
  }
}

fn handle_product_selling_plan_group_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "productJoinSellingPlanGroups" | "productLeaveSellingPlanGroups" -> {
      let product_id = graphql_helpers.read_arg_string(args, "id")
      case
        product_id
        |> option.then(fn(id) { store.get_effective_product_by_id(store, id) })
      {
        None ->
          mutation_result(
            key,
            product_selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              None,
              [
                ProductUserError(
                  ["id"],
                  "Product does not exist.",
                  Some("PRODUCT_DOES_NOT_EXIST"),
                ),
              ],
            ),
            store,
            identity,
            [],
          )
        Some(product) -> {
          let #(next_store, errors, staged_ids) =
            update_product_selling_plan_group_membership(
              store,
              product.id,
              read_arg_string_list(args, "sellingPlanGroupIds"),
              root_name == "productJoinSellingPlanGroups",
            )
          mutation_result(
            key,
            product_selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              store.get_effective_product_by_id(next_store, product.id),
              None,
              errors,
            ),
            next_store,
            identity,
            staged_ids,
          )
        }
      }
    }
    "productVariantJoinSellingPlanGroups"
    | "productVariantLeaveSellingPlanGroups" -> {
      let variant_id = graphql_helpers.read_arg_string(args, "id")
      case
        variant_id
        |> option.then(fn(id) { store.get_effective_variant_by_id(store, id) })
      {
        None ->
          mutation_result(
            key,
            product_selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              None,
              [
                ProductUserError(
                  ["id"],
                  "Product variant does not exist.",
                  Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ),
              ],
            ),
            store,
            identity,
            [],
          )
        Some(variant) -> {
          let #(next_store, errors, staged_ids) =
            update_variant_selling_plan_group_membership(
              store,
              variant.id,
              read_arg_string_list(args, "sellingPlanGroupIds"),
              root_name == "productVariantJoinSellingPlanGroups",
            )
          mutation_result(
            key,
            product_selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              None,
              store.get_effective_variant_by_id(next_store, variant.id),
              errors,
            ),
            next_store,
            identity,
            staged_ids,
          )
        }
      }
    }
    _ -> mutation_result(key, json.null(), store, identity, [])
  }
}

fn update_product_selling_plan_group_membership(
  store: Store,
  product_id: String,
  group_ids: List(String),
  join: Bool,
) -> #(Store, List(ProductUserError), List(String)) {
  let #(next_store, errors, staged_ids) =
    list.fold(group_ids, #(store, [], []), fn(acc, group_id) {
      let #(current_store, current_errors, current_ids) = acc
      case
        store.get_effective_selling_plan_group_by_id(current_store, group_id)
      {
        None -> #(
          current_store,
          list.append(current_errors, [
            selling_plan_group_does_not_exist_error(),
          ]),
          current_ids,
        )
        Some(group) -> {
          let next_product_ids = case join {
            True ->
              dedupe_preserving_order(
                list.append(group.product_ids, [
                  product_id,
                ]),
              )
            False ->
              group.product_ids
              |> list.filter(fn(existing_id) { existing_id != product_id })
          }
          let next_group =
            SellingPlanGroupRecord(..group, product_ids: next_product_ids)
          let #(_, updated_store) =
            store.upsert_staged_selling_plan_group(current_store, next_group)
          #(updated_store, current_errors, list.append(current_ids, [group_id]))
        }
      }
    })
  #(next_store, errors, staged_ids)
}

fn update_variant_selling_plan_group_membership(
  store: Store,
  variant_id: String,
  group_ids: List(String),
  join: Bool,
) -> #(Store, List(ProductUserError), List(String)) {
  let #(next_store, errors, staged_ids) =
    list.fold(group_ids, #(store, [], []), fn(acc, group_id) {
      let #(current_store, current_errors, current_ids) = acc
      case
        store.get_effective_selling_plan_group_by_id(current_store, group_id)
      {
        None -> #(
          current_store,
          list.append(current_errors, [
            selling_plan_group_does_not_exist_error(),
          ]),
          current_ids,
        )
        Some(group) -> {
          let next_variant_ids = case join {
            True ->
              dedupe_preserving_order(
                list.append(group.product_variant_ids, [
                  variant_id,
                ]),
              )
            False ->
              group.product_variant_ids
              |> list.filter(fn(existing_id) { existing_id != variant_id })
          }
          let next_group =
            SellingPlanGroupRecord(
              ..group,
              product_variant_ids: next_variant_ids,
            )
          let #(_, updated_store) =
            store.upsert_staged_selling_plan_group(current_store, next_group)
          #(updated_store, current_errors, list.append(current_ids, [group_id]))
        }
      }
    })
  #(next_store, errors, staged_ids)
}

fn make_selling_plan_group_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  existing: Option(SellingPlanGroupRecord),
  resources: Dict(String, ResolvedValue),
) -> #(SellingPlanGroupRecord, SyntheticIdentityRegistry) {
  let current_plans = case existing {
    Some(group) -> group.selling_plans
    None -> []
  }
  let #(created_plans, identity_after_creates) =
    read_object_list_field(input, "sellingPlansToCreate")
    |> list.fold(#(current_plans, identity), fn(acc, plan_input) {
      let #(plans, current_identity) = acc
      let #(plan, next_identity) =
        make_selling_plan_record(current_identity, plan_input, None)
      #(list.append(plans, [plan]), next_identity)
    })
  let #(updated_plans, identity_after_updates) =
    read_object_list_field(input, "sellingPlansToUpdate")
    |> list.fold(#(created_plans, identity_after_creates), fn(acc, plan_input) {
      let #(plans, current_identity) = acc
      case read_string_field(plan_input, "id") {
        None -> acc
        Some(plan_id) ->
          case find_selling_plan(plans, plan_id) {
            None -> acc
            Some(existing_plan) -> {
              let #(next_plan, next_identity) =
                make_selling_plan_record(
                  current_identity,
                  plan_input,
                  Some(existing_plan),
                )
              #(replace_selling_plan(plans, next_plan), next_identity)
            }
          }
      }
    })
  let deleted_plan_ids =
    read_string_list_field(input, "sellingPlansToDelete") |> option.unwrap([])
  let plans =
    updated_plans
    |> list.filter(fn(plan) { !list.contains(deleted_plan_ids, plan.id) })
  let #(created_at, identity_after_timestamp) = case existing {
    Some(group) -> #(group.created_at, identity_after_updates)
    None -> {
      let #(timestamp, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity_after_updates)
      #(Some(timestamp), next_identity)
    }
  }
  let #(id, next_identity) = case existing {
    Some(group) -> #(group.id, identity_after_timestamp)
    None ->
      synthetic_identity.make_proxy_synthetic_gid(
        identity_after_timestamp,
        "SellingPlanGroup",
      )
  }
  let existing_product_ids = case existing {
    Some(group) -> group.product_ids
    None -> []
  }
  let existing_variant_ids = case existing {
    Some(group) -> group.product_variant_ids
    None -> []
  }
  let group =
    SellingPlanGroupRecord(
      id: id,
      app_id: read_string_field(input, "appId")
        |> option.or(existing_group_app_id(existing)),
      name: read_string_field(input, "name")
        |> option.unwrap(existing_group_name(existing)),
      merchant_code: read_string_field(input, "merchantCode")
        |> option.unwrap(existing_group_merchant_code(existing)),
      description: read_string_field(input, "description")
        |> option.or(existing_group_description(existing)),
      options: read_string_list_field(input, "options")
        |> option.unwrap(existing_group_options(existing)),
      position: read_int_field(input, "position")
        |> option.or(existing_group_position(existing)),
      summary: summarize_selling_plan_group(plans),
      created_at: created_at,
      product_ids: dedupe_preserving_order(list.append(
        existing_product_ids,
        read_string_list_field(resources, "productIds") |> option.unwrap([]),
      )),
      product_variant_ids: dedupe_preserving_order(list.append(
        existing_variant_ids,
        read_string_list_field(resources, "productVariantIds")
          |> option.unwrap([]),
      )),
      selling_plans: plans,
      cursor: case existing {
        Some(group) -> group.cursor
        None -> None
      },
    )
  #(group, next_identity)
}

fn make_selling_plan_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  existing: Option(SellingPlanRecord),
) -> #(SellingPlanRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) = case read_string_field(input, "id"), existing {
    Some(id), _ -> #(id, identity)
    None, Some(plan) -> #(plan.id, identity)
    None, None ->
      synthetic_identity.make_proxy_synthetic_gid(identity, "SellingPlan")
  }
  let previous = case existing {
    Some(plan) -> plan.data
    None -> CapturedObject([])
  }
  let #(created_at, next_identity) = case
    captured_string_field(previous, "createdAt")
  {
    Some(value) -> #(value, identity_after_id)
    None -> synthetic_identity.make_synthetic_timestamp(identity_after_id)
  }
  let data =
    CapturedObject([
      #("__typename", CapturedString("SellingPlan")),
      #("id", CapturedString(id)),
      #(
        "name",
        CapturedString(
          read_string_field(input, "name")
          |> option.or(captured_string_field(previous, "name"))
          |> option.unwrap("Selling plan"),
        ),
      ),
      #(
        "description",
        optional_captured_string(
          read_string_field(input, "description")
          |> option.or(captured_string_field(previous, "description")),
        ),
      ),
      #(
        "options",
        CapturedArray(
          read_string_list_field(input, "options")
          |> option.or(captured_string_array_field(previous, "options"))
          |> option.unwrap([])
          |> list.map(CapturedString),
        ),
      ),
      #(
        "position",
        optional_captured_int(
          read_int_field(input, "position")
          |> option.or(captured_int_field(previous, "position")),
        ),
      ),
      #(
        "category",
        optional_captured_string(
          read_string_field(input, "category")
          |> option.or(captured_string_field(previous, "category")),
        ),
      ),
      #("createdAt", CapturedString(created_at)),
      #(
        "billingPolicy",
        selling_plan_billing_policy(
          read_object_field(input, "billingPolicy") |> option.unwrap(dict.new()),
          captured_object_field(previous, "billingPolicy"),
        ),
      ),
      #(
        "deliveryPolicy",
        selling_plan_delivery_policy(
          read_object_field(input, "deliveryPolicy")
            |> option.unwrap(dict.new()),
          captured_object_field(previous, "deliveryPolicy"),
        ),
      ),
      #(
        "inventoryPolicy",
        selling_plan_inventory_policy(
          read_object_field(input, "inventoryPolicy")
            |> option.unwrap(dict.new()),
          captured_object_field(previous, "inventoryPolicy"),
        ),
      ),
      #("pricingPolicies", case dict.has_key(input, "pricingPolicies") {
        True ->
          CapturedArray(list.map(
            read_object_list_field(input, "pricingPolicies"),
            selling_plan_pricing_policy,
          ))
        False -> CapturedArray([])
      }),
    ])
  #(SellingPlanRecord(id: id, data: data), next_identity)
}

fn selling_plan_billing_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  case read_object_field(input, "recurring") {
    Some(recurring) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringBillingPolicy")),
        #(
          "interval",
          optional_captured_string(read_string_field(recurring, "interval")),
        ),
        #(
          "intervalCount",
          optional_captured_int(read_int_field(recurring, "intervalCount")),
        ),
        #(
          "minCycles",
          optional_captured_int(read_int_field(recurring, "minCycles")),
        ),
        #(
          "maxCycles",
          optional_captured_int(read_int_field(recurring, "maxCycles")),
        ),
      ])
    None ->
      case read_object_field(input, "fixed") {
        Some(fixed) ->
          CapturedObject([
            #("__typename", CapturedString("SellingPlanFixedBillingPolicy")),
            #(
              "checkoutCharge",
              captured_object_or_null(read_object_field(fixed, "checkoutCharge")),
            ),
            #(
              "remainingBalanceChargeTrigger",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeTrigger",
              )),
            ),
            #(
              "remainingBalanceChargeExactTime",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeExactTime",
              )),
            ),
            #(
              "remainingBalanceChargeTimeAfterCheckout",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeTimeAfterCheckout",
              )),
            ),
          ])
        None ->
          existing
          |> option.unwrap(
            CapturedObject([
              #(
                "__typename",
                CapturedString("SellingPlanRecurringBillingPolicy"),
              ),
            ]),
          )
      }
  }
}

fn selling_plan_delivery_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  case read_object_field(input, "recurring") {
    Some(recurring) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringDeliveryPolicy")),
        #(
          "interval",
          optional_captured_string(read_string_field(recurring, "interval")),
        ),
        #(
          "intervalCount",
          optional_captured_int(read_int_field(recurring, "intervalCount")),
        ),
        #("cutoff", optional_captured_int(read_int_field(recurring, "cutoff"))),
        #(
          "intent",
          CapturedString(
            read_string_field(recurring, "intent")
            |> option.unwrap("FULFILLMENT_BEGIN"),
          ),
        ),
        #(
          "preAnchorBehavior",
          CapturedString(
            read_string_field(recurring, "preAnchorBehavior")
            |> option.unwrap("ASAP"),
          ),
        ),
      ])
    None ->
      case read_object_field(input, "fixed") {
        Some(fixed) ->
          CapturedObject([
            #("__typename", CapturedString("SellingPlanFixedDeliveryPolicy")),
            #("cutoff", optional_captured_int(read_int_field(fixed, "cutoff"))),
            #(
              "fulfillmentTrigger",
              optional_captured_string(read_string_field(
                fixed,
                "fulfillmentTrigger",
              )),
            ),
            #(
              "fulfillmentExactTime",
              optional_captured_string(read_string_field(
                fixed,
                "fulfillmentExactTime",
              )),
            ),
            #(
              "intent",
              optional_captured_string(read_string_field(fixed, "intent")),
            ),
            #(
              "preAnchorBehavior",
              optional_captured_string(read_string_field(
                fixed,
                "preAnchorBehavior",
              )),
            ),
          ])
        None ->
          existing
          |> option.unwrap(
            CapturedObject([
              #(
                "__typename",
                CapturedString("SellingPlanRecurringDeliveryPolicy"),
              ),
            ]),
          )
      }
  }
}

fn selling_plan_inventory_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "reserve",
      optional_captured_string(
        read_string_field(input, "reserve")
        |> option.or(
          option.then(existing, fn(value) {
            captured_string_field(value, "reserve")
          }),
        ),
      ),
    ),
  ])
}

fn selling_plan_pricing_policy(
  input: Dict(String, ResolvedValue),
) -> CapturedJsonValue {
  case read_object_field(input, "fixed") {
    Some(fixed) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanFixedPricingPolicy")),
        #(
          "adjustmentType",
          optional_captured_string(read_string_field(fixed, "adjustmentType")),
        ),
        #(
          "adjustmentValue",
          selling_plan_policy_value(
            read_object_field(fixed, "adjustmentValue")
            |> option.unwrap(dict.new()),
          ),
        ),
      ])
    None -> {
      let recurring =
        read_object_field(input, "recurring") |> option.unwrap(dict.new())
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringPricingPolicy")),
        #(
          "adjustmentType",
          optional_captured_string(read_string_field(
            recurring,
            "adjustmentType",
          )),
        ),
        #(
          "adjustmentValue",
          selling_plan_policy_value(
            read_object_field(recurring, "adjustmentValue")
            |> option.unwrap(dict.new()),
          ),
        ),
        #(
          "afterCycle",
          optional_captured_int(read_int_field(recurring, "afterCycle")),
        ),
      ])
    }
  }
}

fn selling_plan_policy_value(
  input: Dict(String, ResolvedValue),
) -> CapturedJsonValue {
  case read_number_captured_field(input, "fixedValue") {
    Some(value) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanPricingPolicyFixedValue")),
        #("fixedValue", value),
      ])
    None ->
      case read_string_field(input, "fixedValue") {
        Some(value) ->
          CapturedObject([
            #(
              "__typename",
              CapturedString("SellingPlanPricingPolicyFixedValue"),
            ),
            #("fixedValue", CapturedString(value)),
          ])
        None ->
          CapturedObject([
            #(
              "__typename",
              CapturedString("SellingPlanPricingPolicyPercentageValue"),
            ),
            #(
              "percentage",
              read_number_captured_field(input, "percentage")
                |> option.unwrap(CapturedNull),
            ),
          ])
      }
  }
}

fn selling_plan_group_mutation_payload(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  group: Option(SellingPlanGroupRecord),
  user_errors: List(ProductUserError),
  deleted_selling_plan_ids: Option(Option(List(String))),
  deleted_selling_plan_group_id: Option(Option(String)),
  removed_product_ids: Option(Option(List(String))),
  removed_product_variant_ids: Option(Option(List(String))),
) -> Json {
  json.object(
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "sellingPlanGroup" ->
              case group {
                Some(group) ->
                  serialize_selling_plan_group_object(
                    store,
                    group,
                    get_selected_child_fields(
                      selection,
                      default_selected_field_options(),
                    ),
                    variables,
                    fragments,
                  )
                None -> json.null()
              }
            "userErrors" ->
              serialize_product_user_errors_json(user_errors, selection)
            "deletedSellingPlanIds" ->
              optional_string_list_json(deleted_selling_plan_ids)
            "deletedSellingPlanGroupId" ->
              optional_string_value_json(deleted_selling_plan_group_id)
            "removedProductIds" ->
              optional_string_list_json(removed_product_ids)
            "removedProductVariantIds" ->
              optional_string_list_json(removed_product_variant_ids)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn product_selling_plan_group_mutation_payload(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  product: Option(ProductRecord),
  variant: Option(ProductVariantRecord),
  user_errors: List(ProductUserError),
) -> Json {
  json.object(
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "product" ->
              case product {
                Some(product) ->
                  serialize_product_selection(
                    store,
                    product,
                    selection,
                    variables,
                    fragments,
                  )
                None -> json.null()
              }
            "productVariant" ->
              case variant {
                Some(variant) ->
                  serialize_product_variant_object(
                    store,
                    variant,
                    get_selected_child_fields(
                      selection,
                      default_selected_field_options(),
                    ),
                    selection,
                    variables,
                    fragments,
                  )
                None -> json.null()
              }
            "userErrors" ->
              serialize_product_user_errors_json(user_errors, selection)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn optional_string_list_json(value: Option(Option(List(String)))) -> Json {
  case value {
    Some(Some(items)) -> json.array(items, json.string)
    _ -> json.null()
  }
}

fn optional_string_value_json(value: Option(Option(String))) -> Json {
  case value {
    Some(Some(item)) -> json.string(item)
    _ -> json.null()
  }
}

fn serialize_product_user_errors_json(
  errors: List(ProductUserError),
  field: Selection,
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  json.array(errors, fn(error) {
    let ProductUserError(field: path, message: message, code: code) = error
    json.object(
      list.map(selections, fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "field" -> json.array(path, json.string)
              "message" -> json.string(message)
              "code" -> optional_string_json(code)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      }),
    )
  })
}

fn selling_plan_group_does_not_exist_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "Selling plan group does not exist.",
    Some("GROUP_DOES_NOT_EXIST"),
  )
}

fn selling_plan_group_staged_ids(
  group: SellingPlanGroupRecord,
) -> List(String) {
  [group.id, ..list.map(group.selling_plans, fn(plan) { plan.id })]
}

fn find_selling_plan(
  plans: List(SellingPlanRecord),
  id: String,
) -> Option(SellingPlanRecord) {
  plans
  |> list.find(fn(plan) { plan.id == id })
  |> option.from_result
}

fn replace_selling_plan(
  plans: List(SellingPlanRecord),
  next_plan: SellingPlanRecord,
) -> List(SellingPlanRecord) {
  list.map(plans, fn(plan) {
    case plan.id == next_plan.id {
      True -> next_plan
      False -> plan
    }
  })
}

fn summarize_selling_plan_group(
  plans: List(SellingPlanRecord),
) -> Option(String) {
  let percentage =
    plans
    |> list.find_map(fn(plan) {
      first_selling_plan_percentage(plan.data) |> option_to_result
    })
    |> option.from_result
    |> option.unwrap("")
  Some(
    int.to_string(list.length(plans))
    <> " delivery frequency, "
    <> percentage
    <> " discount",
  )
}

fn first_selling_plan_percentage(value: CapturedJsonValue) -> Option(String) {
  case captured_object_field(value, "pricingPolicies") {
    Some(CapturedArray(policies)) ->
      policies
      |> list.find_map(fn(policy) {
        case
          captured_object_field(policy, "adjustmentValue")
          |> option.then(fn(adjustment) {
            captured_number_string_field(adjustment, "percentage")
          })
        {
          Some(value) -> Ok(value <> "%")
          None -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn existing_group_app_id(
  group: Option(SellingPlanGroupRecord),
) -> Option(String) {
  case group {
    Some(group) -> group.app_id
    None -> None
  }
}

fn existing_group_name(group: Option(SellingPlanGroupRecord)) -> String {
  case group {
    Some(group) -> group.name
    None -> "Selling plan group"
  }
}

fn existing_group_merchant_code(
  group: Option(SellingPlanGroupRecord),
) -> String {
  case group {
    Some(group) -> group.merchant_code
    None -> "selling-plan-group"
  }
}

fn existing_group_description(
  group: Option(SellingPlanGroupRecord),
) -> Option(String) {
  case group {
    Some(group) -> group.description
    None -> None
  }
}

fn existing_group_options(
  group: Option(SellingPlanGroupRecord),
) -> List(String) {
  case group {
    Some(group) -> group.options
    None -> []
  }
}

fn existing_group_position(
  group: Option(SellingPlanGroupRecord),
) -> Option(Int) {
  case group {
    Some(group) -> group.position
    None -> None
  }
}

fn optional_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

fn optional_captured_int(value: Option(Int)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedInt(value)
    None -> CapturedNull
  }
}

fn captured_object_or_null(
  value: Option(Dict(String, ResolvedValue)),
) -> CapturedJsonValue {
  case value {
    Some(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, value) = pair
          #(key, resolved_value_to_captured(value))
        }),
      )
    None -> CapturedNull
  }
}

fn resolved_value_to_captured(value: ResolvedValue) -> CapturedJsonValue {
  case value {
    NullVal -> CapturedNull
    BoolVal(value) -> CapturedBool(value)
    IntVal(value) -> CapturedInt(value)
    FloatVal(value) -> CapturedFloat(value)
    StringVal(value) -> CapturedString(value)
    ListVal(values) ->
      CapturedArray(list.map(values, resolved_value_to_captured))
    ObjectVal(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, value) = pair
          #(key, resolved_value_to_captured(value))
        }),
      )
  }
}

fn read_number_captured_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(CapturedJsonValue) {
  case dict.get(input, name) {
    Ok(IntVal(value)) -> Some(CapturedInt(value))
    Ok(FloatVal(value)) -> Some(CapturedFloat(value))
    _ -> None
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

fn captured_int_field(value: CapturedJsonValue, name: String) -> Option(Int) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(value)
    _ -> None
  }
}

fn captured_string_array_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(List(String)) {
  case captured_object_field(value, name) {
    Some(CapturedArray(items)) ->
      Some(
        list.filter_map(items, fn(item) {
          case item {
            CapturedString(value) -> Ok(value)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

fn captured_number_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(int.to_string(value))
    Some(CapturedFloat(value)) -> Some(float.to_string(value))
    _ -> None
  }
}

fn mutation_result(
  key: String,
  payload: Json,
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_resource_ids: List(String),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: payload,
    store: store,
    identity: identity,
    staged_resource_ids: staged_resource_ids,
    top_level_errors: [],
    top_level_error_data_entries: [],
    staging_failed: False,
  )
}

fn mutation_rejected_result(
  key: String,
  payload: Json,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: payload,
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    top_level_error_data_entries: [],
    staging_failed: True,
  )
}

fn mutation_error_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  errors: List(Json),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: errors,
    top_level_error_data_entries: [],
    staging_failed: False,
  )
}

fn mutation_error_with_null_data_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  errors: List(Json),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: errors,
    top_level_error_data_entries: [#(key, json.null())],
    staging_failed: False,
  )
}

fn product_delete_payload(
  deleted_product_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let deleted_value = case deleted_product_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductDeletePayload")),
      #("deletedProductId", deleted_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_update_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductUpdatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_create_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductCreatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_publication_payload(
  typename: String,
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn publication_mutation_payload(
  store: Store,
  typename: String,
  publication: Option(PublicationRecord),
  deleted_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let publication_value = case publication {
    Some(record) -> publication_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("publication", publication_value),
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn publishable_mutation_payload(
  store: Store,
  publishable: Option(SourceValue),
  user_errors: List(ProductUserError),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let selected_publication =
    selected_publication_id(
      get_selected_child_fields(field, default_selected_field_options()),
      variables,
    )
  let publishable_value = case publishable {
    Some(SrcObject(source)) ->
      case dict.get(source, "__typename") {
        Ok(SrcString("Product")) ->
          case dict.get(source, "id") {
            Ok(SrcString(id)) ->
              case store.get_effective_product_by_id(store, id) {
                Some(product) ->
                  product_source_with_store_and_publication(
                    store,
                    product,
                    selected_publication,
                  )
                None -> SrcObject(source)
              }
            _ -> SrcObject(source)
          }
        Ok(SrcString("Collection")) ->
          case dict.get(source, "id") {
            Ok(SrcString(id)) ->
              case store.get_effective_collection_by_id(store, id) {
                Some(collection) ->
                  collection_source_with_store_and_publication(
                    store,
                    collection,
                    selected_publication,
                  )
                None -> SrcObject(source)
              }
            _ -> SrcObject(source)
          }
        _ -> SrcObject(source)
      }
    Some(value) -> value
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("PublishablePublishPayload")),
      #("publishable", publishable_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_feed_create_payload(
  feed: ProductFeedRecord,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductFeedCreatePayload")),
      #("productFeed", product_feed_source(feed)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_feed_delete_payload(
  deleted_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductFeedDeletePayload")),
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_full_sync_payload(
  id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductFullSyncPayload")),
      #("id", graphql_helpers.option_string_source(id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_bundle_mutation_payload(
  root_name: String,
  user_errors: List(NullableFieldUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let typename = case root_name {
    "productBundleUpdate" -> "ProductBundleUpdatePayload"
    _ -> "ProductBundleCreatePayload"
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("productBundleOperation", SrcNull),
      #("userErrors", nullable_field_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn combined_listing_update_payload(
  product: SourceValue,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CombinedListingUpdatePayload")),
      #("product", product),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variant_relationship_bulk_update_payload(
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let parent_product_variants = case user_errors {
    [] -> SrcList([])
    _ -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantRelationshipBulkUpdatePayload")),
      #("parentProductVariants", parent_product_variants),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn bulk_product_resource_feedback_create_payload(
  feedback: List(ProductResourceFeedbackRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("BulkProductResourceFeedbackCreatePayload")),
      #(
        "feedback",
        SrcList(list.map(feedback, product_resource_feedback_source)),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn shop_resource_feedback_create_payload(
  feedback: Option(ShopResourceFeedbackRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let feedback_value = case feedback {
    Some(record) -> shop_resource_feedback_source(record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ShopResourceFeedbackCreatePayload")),
      #("feedback", feedback_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_shipment_create_payload(
  store: Store,
  typename: String,
  shipment: Option(InventoryShipmentRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("inventoryShipment", shipment_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_transfer_payload(
  store: Store,
  typename: String,
  transfer_field: String,
  transfer: Option(InventoryTransferRecord),
  line_item_updates: List(InventoryTransferLineItemUpdate),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let transfer_value = case transfer {
    Some(record) -> inventory_transfer_source(store, record)
    None -> SrcNull
  }
  let updates =
    SrcList(list.map(
      line_item_updates,
      inventory_transfer_line_item_update_source,
    ))
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #(transfer_field, transfer_value),
      #("updatedLineItems", updates),
      #("removedQuantities", updates),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_transfer_line_item_update_source(
  update: InventoryTransferLineItemUpdate,
) -> SourceValue {
  src_object([
    #("inventoryItemId", SrcString(update.inventory_item_id)),
    #("newQuantity", SrcInt(update.new_quantity)),
    #("deltaQuantity", SrcInt(update.delta_quantity)),
  ])
}

fn inventory_transfer_delete_payload(
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

fn inventory_shipment_receive_payload(
  store: Store,
  shipment: Option(InventoryShipmentRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryShipmentReceivePayload")),
      #("inventoryShipment", shipment_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_shipment_payload(
  store: Store,
  typename: String,
  shipment: Option(InventoryShipmentRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("inventoryShipment", shipment_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_shipment_add_items_payload(
  store: Store,
  shipment: Option(InventoryShipmentRecord),
  added_items: List(InventoryShipmentLineItemRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryShipmentAddItemsPayload")),
      #(
        "addedItems",
        SrcList(
          list.map(added_items, fn(line_item) {
            inventory_shipment_line_item_source(store, line_item)
          }),
        ),
      ),
      #("inventoryShipment", shipment_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_shipment_update_item_quantities_payload(
  store: Store,
  shipment: Option(InventoryShipmentRecord),
  updated_line_items: List(InventoryShipmentLineItemRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryShipmentUpdateItemQuantitiesPayload")),
      #("shipment", shipment_value),
      #(
        "updatedLineItems",
        SrcList(
          list.map(updated_line_items, fn(line_item) {
            inventory_shipment_line_item_source(store, line_item)
          }),
        ),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_shipment_delete_payload(
  id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryShipmentDeletePayload")),
      #("id", graphql_helpers.option_string_source(id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variant_payload(
  store: Store,
  product: Option(ProductRecord),
  variant: Option(ProductVariantRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  let variant_value = case variant {
    Some(record) -> product_variant_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantPayload")),
      #("product", product_value),
      #("productVariant", variant_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variant_delete_payload(
  deleted_product_variant_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let deleted_value = case deleted_product_variant_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantDeletePayload")),
      #("deletedProductVariantId", deleted_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variants_bulk_payload(
  typename: String,
  store: Store,
  product: Option(ProductRecord),
  variants: Option(List(ProductVariantRecord)),
  user_errors: List(BulkVariantUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  let variants_value = case variants {
    Some(records) ->
      SrcList(
        list.map(records, fn(variant) { product_variant_source(store, variant) }),
      )
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("product", product_value),
      #("productVariants", variants_value),
      #("userErrors", bulk_variant_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variants_bulk_delete_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(BulkVariantUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantsBulkDeletePayload")),
      #("product", product_value),
      #("userErrors", bulk_variant_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variants_bulk_reorder_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantsBulkReorderPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_quantity_mutation_result(
  key: String,
  typename: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  group: Option(InventoryAdjustmentGroup),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
  staged_ids: List(String),
) -> MutationFieldResult {
  mutation_result(
    key,
    inventory_quantity_payload(
      typename,
      store,
      group,
      user_errors,
      field,
      fragments,
    ),
    store,
    identity,
    staged_ids,
  )
}

fn inventory_quantity_payload(
  typename: String,
  store: Store,
  group: Option(InventoryAdjustmentGroup),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #(
        "inventoryAdjustmentGroup",
        inventory_adjustment_group_source(store, group),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_activate_payload(
  store: Store,
  resolved: Option(#(ProductVariantRecord, InventoryLevelRecord)),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_level = case resolved {
    Some(#(variant, level)) ->
      inventory_level_source_with_item(store, variant, level)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryActivatePayload")),
      #("inventoryLevel", inventory_level),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_deactivate_payload(
  user_errors: List(NullableFieldUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryDeactivatePayload")),
      #("userErrors", nullable_field_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_bulk_toggle_activation_payload(
  store: Store,
  variant: Option(ProductVariantRecord),
  levels: Option(List(#(ProductVariantRecord, InventoryLevelRecord))),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_item = case variant {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  let inventory_levels = case levels {
    Some(levels) ->
      SrcList(
        list.map(levels, fn(level) {
          let #(variant, level) = level
          inventory_level_source_with_item(store, variant, level)
        }),
      )
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryBulkToggleActivationPayload")),
      #("inventoryItem", inventory_item),
      #("inventoryLevels", inventory_levels),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_item_update_missing_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  mutation_result(
    key,
    inventory_item_update_payload(
      store,
      None,
      [
        ProductUserError(
          ["id"],
          "The product couldn't be updated because it does not exist.",
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
}

fn inventory_item_update_payload(
  store: Store,
  variant: Option(ProductVariantRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_item = case variant {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryItemUpdatePayload")),
      #("inventoryItem", inventory_item),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_add_products_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let collection_value = case collection {
    Some(record) -> collection_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionAddProductsPayload")),
      #("collection", collection_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_add_products_v2_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionAddProductsV2Payload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_create_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  products_count_override: Option(Int),
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "__typename" -> json.string("CollectionCreatePayload")
              "collection" ->
                case collection {
                  Some(record) ->
                    serialize_collection_object_with_options(
                      store,
                      record,
                      get_selected_child_fields(
                        selection,
                        default_selected_field_options(),
                      ),
                      variables,
                      fragments,
                      products_count_override,
                    )
                  None -> json.null()
                }
              "userErrors" ->
                project_graphql_value(
                  user_errors_source(user_errors),
                  get_selected_child_fields(
                    selection,
                    default_selected_field_options(),
                  ),
                  fragments,
                )
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn collection_update_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let collection_value = case collection {
    Some(record) -> collection_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionUpdatePayload")),
      #("collection", collection_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_delete_payload(
  deleted_collection_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let deleted_value = case deleted_collection_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionDeletePayload")),
      #("deletedCollectionId", deleted_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_remove_products_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionRemoveProductsPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_reorder_products_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionReorderProductsPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn job_source(id: String, done: Bool) -> SourceValue {
  src_object([
    #("__typename", SrcString("Job")),
    #("id", SrcString(id)),
    #("done", SrcBool(done)),
  ])
}

fn inventory_adjustment_group_source(
  store: Store,
  group: Option(InventoryAdjustmentGroup),
) -> SourceValue {
  case group {
    None -> SrcNull
    Some(group) ->
      src_object([
        #("__typename", SrcString("InventoryAdjustmentGroup")),
        #("id", SrcString(group.id)),
        #("createdAt", SrcString(group.created_at)),
        #("reason", SrcString(group.reason)),
        #(
          "referenceDocumentUri",
          graphql_helpers.option_string_source(group.reference_document_uri),
        ),
        #("app", inventory_adjustment_app_source()),
        #(
          "changes",
          SrcList(
            list.map(group.changes, fn(change) {
              inventory_adjustment_change_source(store, change)
            }),
          ),
        ),
      ])
  }
}

fn inventory_adjustment_change_source(
  store: Store,
  change: InventoryAdjustmentChange,
) -> SourceValue {
  let location = inventory_change_location(store, change)
  let item = case
    store.find_effective_variant_by_inventory_item_id(
      store,
      change.inventory_item_id,
    )
  {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("InventoryChange")),
    #("name", SrcString(change.name)),
    #("delta", SrcInt(change.delta)),
    #(
      "quantityAfterChange",
      graphql_helpers.option_int_source(change.quantity_after_change),
    ),
    #(
      "ledgerDocumentUri",
      graphql_helpers.option_string_source(change.ledger_document_uri),
    ),
    #("item", item),
    #(
      "location",
      src_object([
        #("__typename", SrcString("Location")),
        #("id", SrcString(location.id)),
        #("name", SrcString(location.name)),
      ]),
    ),
  ])
}

fn inventory_adjustment_app_source() -> SourceValue {
  src_object([
    #("__typename", SrcString("App")),
    #("id", SrcNull),
    #("title", SrcString("hermes-conformance-products")),
    #("apiKey", SrcNull),
    #("handle", SrcString("hermes-conformance-products")),
  ])
}

fn tags_update_payload(
  store: Store,
  is_add: Bool,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  let typename = case is_add {
    True -> "TagsAddPayload"
    False -> "TagsRemovePayload"
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("node", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_options_delete_payload(
  store: Store,
  deleted_option_ids: List(String),
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionsDeletePayload")),
      #("deletedOptionsIds", SrcList(list.map(deleted_option_ids, SrcString))),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_change_status_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductChangeStatusPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_options_reorder_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionsReorderPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_option_update_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionUpdatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_options_create_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionsCreatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn user_errors_source(errors: List(ProductUserError)) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let ProductUserError(field: field, message: message, code: code) = error
      src_object([
        #("field", SrcList(list.map(field, SrcString))),
        #("message", SrcString(message)),
        #("code", graphql_helpers.option_string_source(code)),
      ])
    }),
  )
}

fn bulk_variant_user_errors_source(
  errors: List(BulkVariantUserError),
) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let BulkVariantUserError(field: field, message: message, code: code) =
        error
      let field_value = case field {
        Some(field) -> SrcList(list.map(field, SrcString))
        None -> SrcNull
      }
      src_object([
        #("field", field_value),
        #("message", SrcString(message)),
        #("code", graphql_helpers.option_string_source(code)),
      ])
    }),
  )
}

fn nullable_field_user_errors_source(
  errors: List(NullableFieldUserError),
) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let NullableFieldUserError(field: field, message: message) = error
      let field_value = case field {
        Some(field) -> SrcList(list.map(field, SrcString))
        None -> SrcNull
      }
      src_object([
        #("field", field_value),
        #("message", SrcString(message)),
      ])
    }),
  )
}

fn read_arg_object_list(
  args: Dict(String, ResolvedValue),
  name: String,
) -> List(Dict(String, ResolvedValue)) {
  case dict.get(args, name) {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(input) -> Ok(input)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_arg_string_list(
  args: Dict(String, ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(args, name) {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          StringVal(input) -> Ok(input)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_collection_product_ids(
  input: Dict(String, ResolvedValue),
) -> List(String) {
  read_string_list_field(input, "products")
  |> option.unwrap([])
  |> dedupe_preserving_order
}

fn read_inventory_set_quantity_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventorySetQuantityInput) {
  case dict.get(input, "quantities") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventorySetQuantityInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              location_id: read_string_field(fields, "locationId"),
              quantity: read_int_field(fields, "quantity"),
              compare_quantity: read_int_field(fields, "compareQuantity"),
              change_from_quantity: read_int_field(fields, "changeFromQuantity"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_inventory_adjustment_change_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventoryAdjustmentChangeInput) {
  case dict.get(input, "changes") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventoryAdjustmentChangeInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              location_id: read_string_field(fields, "locationId"),
              ledger_document_uri: read_string_field(
                fields,
                "ledgerDocumentUri",
              ),
              delta: read_int_field(fields, "delta"),
              change_from_quantity: read_int_field(fields, "changeFromQuantity"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_inventory_move_quantity_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventoryMoveQuantityInput) {
  case dict.get(input, "changes") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventoryMoveQuantityInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              quantity: read_int_field(fields, "quantity"),
              from: read_inventory_move_terminal(fields, "from"),
              to: read_inventory_move_terminal(fields, "to"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_inventory_move_terminal(
  input: Dict(String, ResolvedValue),
  name: String,
) -> InventoryMoveTerminalInput {
  case read_object_field(input, name) {
    Some(fields) ->
      InventoryMoveTerminalInput(
        location_id: read_string_field(fields, "locationId"),
        name: read_string_field(fields, "name"),
        ledger_document_uri: read_string_field(fields, "ledgerDocumentUri"),
      )
    None ->
      InventoryMoveTerminalInput(
        location_id: None,
        name: None,
        ledger_document_uri: None,
      )
  }
}

fn read_tag_inputs(
  args: Dict(String, ResolvedValue),
  allow_comma_separated_string: Bool,
) -> List(String) {
  let values = case dict.get(args, "tags") {
    Ok(ListVal(items)) ->
      list.filter_map(items, fn(value) {
        case value {
          StringVal(tag) -> trimmed_non_empty(tag)
          _ -> Error(Nil)
        }
      })
    Ok(StringVal(raw)) ->
      case allow_comma_separated_string {
        True ->
          string.split(raw, on: ",")
          |> list.filter_map(trimmed_non_empty)
        False -> []
      }
    _ -> []
  }
  dedupe_preserving_order(values)
}

fn trimmed_non_empty(value: String) -> Result(String, Nil) {
  let trimmed = string.trim(value)
  case string.length(trimmed) > 0 {
    True -> Ok(trimmed)
    False -> Error(Nil)
  }
}

fn normalize_product_tags(tags: List(String)) -> List(String) {
  tags
  |> list.filter_map(trimmed_non_empty)
  |> list.fold(dict.new(), fn(seen, tag) { dict.insert(seen, tag, True) })
  |> dict.keys()
  |> list.sort(string.compare)
}

fn dedupe_preserving_order(values: List(String)) -> List(String) {
  let #(reversed, _) =
    list.fold(values, #([], dict.new()), fn(acc, value) {
      let #(items, seen) = acc
      case dict.has_key(seen, value) {
        True -> #(items, seen)
        False -> #([value, ..items], dict.insert(seen, value, True))
      }
    })
  list.reverse(reversed)
}

fn updated_product_record(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  #(
    ProductRecord(
      ..product,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(product.title),
      handle: read_non_empty_string_field(input, "handle")
        |> option.unwrap(product.handle),
      status: read_product_status_field(input) |> option.unwrap(product.status),
      vendor: read_string_field(input, "vendor") |> option.or(product.vendor),
      product_type: read_string_field(input, "productType")
        |> option.or(product.product_type),
      tags: read_string_list_field(input, "tags")
        |> option.map(normalize_product_tags)
        |> option.unwrap(product.tags),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.unwrap(product.description_html),
      template_suffix: read_string_field(input, "templateSuffix")
        |> option.or(product.template_suffix),
      seo: updated_product_seo(product.seo, input),
      updated_at: Some(updated_at),
    ),
    next_identity,
  )
}

fn created_product_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap("Untitled product")
  let #(created_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, next_identity) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity_after_timestamp,
      "Product",
    )
  let base_handle = case read_explicit_product_handle(input) {
    Some(handle) -> handle
    None -> slugify_product_handle(title)
  }
  #(
    ProductRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: ensure_unique_product_handle(store, base_handle),
      status: read_product_status_field(input) |> option.unwrap("ACTIVE"),
      vendor: read_string_field(input, "vendor"),
      product_type: read_string_field(input, "productType"),
      tags: read_string_list_field(input, "tags")
        |> option.map(normalize_product_tags)
        |> option.unwrap([]),
      total_inventory: Some(0),
      tracks_inventory: Some(False),
      created_at: Some(created_at),
      updated_at: Some(created_at),
      published_at: None,
      description_html: read_string_field(input, "descriptionHtml")
        |> option.unwrap(""),
      online_store_preview_url: None,
      template_suffix: read_string_field(input, "templateSuffix"),
      seo: updated_product_seo(
        ProductSeoRecord(title: None, description: None),
        input,
      ),
      category: None,
      publication_ids: [],
      contextual_pricing: None,
      cursor: None,
    ),
    next_identity,
  )
}

fn duplicated_product_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  source_product: ProductRecord,
  new_title: Option(String),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let title = new_title |> option.unwrap(source_product.title <> " Copy")
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "Product")
  let base_handle = slugify_product_handle(title)
  let handle = ensure_unique_product_handle(store, base_handle)
  #(
    ProductRecord(
      ..source_product,
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: handle,
      status: "DRAFT",
      cursor: None,
    ),
    next_identity,
  )
}

fn apply_product_set_graph(
  store: Store,
  identity: SyntheticIdentityRegistry,
  existing: Option(ProductRecord),
  product_id: String,
  input: Dict(String, ResolvedValue),
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  let #(store, identity, option_ids) = case
    dict.has_key(input, "productOptions")
  {
    True -> {
      let #(options, next_identity, ids) =
        product_set_option_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "productOptions"),
        )
      let next_store =
        store.replace_staged_options_for_product(store, product_id, options)
      #(next_store, next_identity, ids)
    }
    False ->
      case existing {
        Some(_) -> #(store, identity, [])
        None ->
          case store.get_effective_options_by_product_id(store, product_id) {
            [] -> {
              let assert Some(product) =
                store.get_effective_product_by_id(store, product_id)
              let #(option, next_identity, ids) =
                make_default_option_record(identity, product)
              let next_store =
                store.replace_staged_options_for_product(store, product_id, [
                  option,
                ])
              #(next_store, next_identity, ids)
            }
            _ -> #(store, identity, [])
          }
      }
  }
  let #(store, identity, variant_ids) = case dict.has_key(input, "variants") {
    True -> {
      let #(variants, next_identity, ids) =
        product_set_variant_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "variants"),
        )
      let synced_options =
        sync_product_options_with_variants(
          store.get_effective_options_by_product_id(store, product_id),
          variants,
        )
      let next_store =
        store
        |> store.replace_staged_variants_for_product(product_id, variants)
        |> store.replace_staged_options_for_product(product_id, synced_options)
      #(next_store, next_identity, ids)
    }
    False ->
      case existing {
        Some(_) -> #(store, identity, [])
        None ->
          case store.get_effective_variants_by_product_id(store, product_id) {
            [] -> {
              let assert Some(product) =
                store.get_effective_product_by_id(store, product_id)
              let #(variant, next_identity, ids) =
                make_default_variant_record(identity, product)
              let next_store =
                store.replace_staged_variants_for_product(store, product_id, [
                  variant,
                ])
              #(next_store, next_identity, ids)
            }
            _ -> #(store, identity, [])
          }
      }
  }
  let #(store, identity, metafield_ids) = case
    dict.has_key(input, "metafields")
  {
    True -> {
      let #(metafields, next_identity, ids) =
        product_set_metafield_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "metafields"),
        )
      let next_store =
        store.replace_staged_metafields_for_owner(store, product_id, metafields)
      #(next_store, next_identity, ids)
    }
    False -> #(store, identity, [])
  }
  #(
    store,
    identity,
    list.append(option_ids, list.append(variant_ids, metafield_ids)),
  )
}

fn product_set_option_records(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let #(reversed, final_identity, ids) =
    inputs
    |> enumerate_items()
    |> list.fold(#([], identity, []), fn(acc, pair) {
      let #(records, current_identity, collected_ids) = acc
      let #(input, index) = pair
      let existing = case read_string_field(input, "id") {
        Some(id) -> find_product_option(existing_options, product_id, id)
        None -> None
      }
      let #(option_id, identity_after_option, option_ids) = case existing {
        Some(option) -> #(option.id, current_identity, [option.id])
        None -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(
              current_identity,
              "ProductOption",
            )
          #(id, next_identity, [id])
        }
      }
      let #(values, next_identity, value_ids) =
        product_set_option_value_records(
          identity_after_option,
          option.map(existing, fn(option) { option.option_values }),
          read_object_list_field(input, "values"),
        )
      let option_record =
        ProductOptionRecord(
          id: option_id,
          product_id: product_id,
          name: read_non_empty_string_field(input, "name")
            |> option.unwrap(
              option.map(existing, fn(option) { option.name })
              |> option.unwrap(""),
            ),
          position: read_int_field(input, "position")
            |> option.unwrap(index + 1),
          option_values: values,
        )
      #(
        [option_record, ..records],
        next_identity,
        list.append(collected_ids, list.append(option_ids, value_ids)),
      )
    })
  #(list.reverse(reversed), final_identity, ids)
}

fn product_set_option_value_records(
  identity: SyntheticIdentityRegistry,
  existing_values: Option(List(ProductOptionValueRecord)),
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionValueRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_values = existing_values |> option.unwrap([])
  let #(reversed, final_identity, ids) =
    list.fold(inputs, #([], identity, []), fn(acc, input) {
      let #(records, current_identity, collected_ids) = acc
      let existing = case read_string_field(input, "id") {
        Some(id) ->
          existing_values
          |> list.find(fn(value) { value.id == id })
          |> option.from_result
        None -> None
      }
      let #(value_id, next_identity, ids) = case existing {
        Some(value) -> #(value.id, current_identity, [value.id])
        None -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(
              current_identity,
              "ProductOptionValue",
            )
          #(id, next_identity, [id])
        }
      }
      let value =
        ProductOptionValueRecord(
          id: value_id,
          name: read_non_empty_string_field(input, "name")
            |> option.unwrap(
              option.map(existing, fn(value) { value.name })
              |> option.unwrap("Option value"),
            ),
          has_variants: option.map(existing, fn(value) { value.has_variants })
            |> option.unwrap(False),
        )
      #([value, ..records], next_identity, list.append(collected_ids, ids))
    })
  #(list.reverse(reversed), final_identity, ids)
}

fn product_set_variant_records(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let #(reversed, final_identity, ids) =
    list.fold(inputs, #([], identity, []), fn(acc, input) {
      let #(records, current_identity, collected_ids) = acc
      let existing = case read_string_field(input, "id") {
        Some(id) ->
          existing_variants
          |> list.find(fn(variant) { variant.id == id })
          |> option.from_result
        None -> None
      }
      let #(variant, identity_after_variant) = case existing {
        Some(variant) -> update_variant_record(current_identity, variant, input)
        None ->
          make_created_variant_record(current_identity, product_id, input, None)
      }
      let variant = product_set_variant_defaults(variant)
      let #(variant, next_identity) =
        apply_product_set_inventory_quantities(
          store,
          identity_after_variant,
          variant,
          input,
        )
      #(
        [variant, ..records],
        next_identity,
        list.append(collected_ids, variant_staged_ids(variant)),
      )
    })
  #(list.reverse(reversed), final_identity, ids)
}

fn product_set_variant_defaults(
  variant: ProductVariantRecord,
) -> ProductVariantRecord {
  let inventory_item = case variant.inventory_item {
    Some(item) ->
      Some(
        InventoryItemRecord(
          ..item,
          measurement: item.measurement
            |> option.or(Some(default_inventory_item_measurement())),
        ),
      )
    None -> None
  }
  ProductVariantRecord(
    ..variant,
    taxable: variant.taxable |> option.or(Some(True)),
    inventory_policy: variant.inventory_policy |> option.or(Some("DENY")),
    inventory_item: inventory_item,
  )
}

fn default_inventory_item_measurement() -> InventoryMeasurementRecord {
  InventoryMeasurementRecord(
    weight: Some(InventoryWeightRecord(
      unit: "KILOGRAMS",
      value: InventoryWeightInt(0),
    )),
  )
}

fn apply_product_set_inventory_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  variant: ProductVariantRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let quantity_inputs = read_product_set_inventory_quantity_inputs(input)
  case quantity_inputs {
    [] -> #(variant, identity)
    _ -> {
      let #(inventory_item, identity_after_item) =
        ensure_product_set_inventory_item(identity, variant.inventory_item)
      let #(levels, next_identity) =
        product_set_inventory_levels(
          store,
          identity_after_item,
          inventory_item,
          quantity_inputs,
        )
      let available = product_set_available_quantity(quantity_inputs)
      #(
        ProductVariantRecord(
          ..variant,
          inventory_quantity: available |> option.or(variant.inventory_quantity),
          inventory_item: Some(
            InventoryItemRecord(..inventory_item, inventory_levels: levels),
          ),
        ),
        next_identity,
      )
    }
  }
}

fn ensure_product_set_inventory_item(
  identity: SyntheticIdentityRegistry,
  inventory_item: Option(InventoryItemRecord),
) -> #(InventoryItemRecord, SyntheticIdentityRegistry) {
  case inventory_item {
    Some(item) -> #(item, identity)
    None -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        InventoryItemRecord(
          id: id,
          tracked: None,
          requires_shipping: None,
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [],
        ),
        next_identity,
      )
    }
  }
}

fn read_product_set_inventory_quantity_inputs(
  input: Dict(String, ResolvedValue),
) -> List(ProductSetInventoryQuantityInput) {
  read_object_list_field(input, "inventoryQuantities")
  |> list.filter_map(fn(fields) {
    case read_int_field(fields, "quantity") {
      Some(quantity) ->
        Ok(ProductSetInventoryQuantityInput(
          location_id: read_non_empty_string_field(fields, "locationId"),
          name: read_non_empty_string_field(fields, "name")
            |> option.unwrap("available"),
          quantity: quantity,
        ))
      None -> Error(Nil)
    }
  })
}

fn product_set_available_quantity(
  inputs: List(ProductSetInventoryQuantityInput),
) -> Option(Int) {
  let quantities =
    inputs
    |> list.filter_map(fn(input) {
      case input.name == "available" {
        True -> Ok(input.quantity)
        False -> Error(Nil)
      }
    })
  case quantities {
    [] -> None
    _ ->
      Some(list.fold(quantities, 0, fn(total, quantity) { total + quantity }))
  }
}

fn product_set_inventory_levels(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inventory_item: InventoryItemRecord,
  inputs: List(ProductSetInventoryQuantityInput),
) -> #(List(InventoryLevelRecord), SyntheticIdentityRegistry) {
  inputs
  |> group_product_set_quantities_by_location
  |> list.fold(#([], identity), fn(acc, entry) {
    let #(levels, current_identity) = acc
    let #(location_id, location_inputs) = entry
    let existing =
      find_inventory_level(inventory_item.inventory_levels, location_id)
    let base_quantities = case existing {
      Some(level) -> level.quantities
      None -> []
    }
    let #(quantities, next_identity) =
      apply_product_set_level_quantities(
        current_identity,
        base_quantities,
        location_inputs,
      )
    let level =
      InventoryLevelRecord(
        id: existing
          |> option.map(fn(level) { level.id })
          |> option.unwrap(product_set_inventory_level_id(
            inventory_item.id,
            location_id,
          )),
        location: product_set_inventory_location(store, existing, location_id),
        quantities: quantities,
        is_active: Some(True),
        cursor: option.then(existing, fn(level) { level.cursor }),
      )
    #([level, ..levels], next_identity)
  })
  |> fn(result) {
    let #(levels, final_identity) = result
    #(list.reverse(levels), final_identity)
  }
}

fn group_product_set_quantities_by_location(
  inputs: List(ProductSetInventoryQuantityInput),
) -> List(#(String, List(ProductSetInventoryQuantityInput))) {
  inputs
  |> list.fold([], fn(groups, input) {
    let location_id =
      input.location_id |> option.unwrap("gid://shopify/Location/1")
    upsert_product_set_quantity_group(groups, location_id, input)
  })
}

fn upsert_product_set_quantity_group(
  groups: List(#(String, List(ProductSetInventoryQuantityInput))),
  location_id: String,
  input: ProductSetInventoryQuantityInput,
) -> List(#(String, List(ProductSetInventoryQuantityInput))) {
  case groups {
    [] -> [#(location_id, [input])]
    [first, ..rest] -> {
      let #(current_id, values) = first
      case current_id == location_id {
        True -> [#(current_id, list.append(values, [input])), ..rest]
        False -> [
          first,
          ..upsert_product_set_quantity_group(rest, location_id, input)
        ]
      }
    }
  }
}

fn apply_product_set_level_quantities(
  identity: SyntheticIdentityRegistry,
  quantities: List(InventoryQuantityRecord),
  inputs: List(ProductSetInventoryQuantityInput),
) -> #(List(InventoryQuantityRecord), SyntheticIdentityRegistry) {
  let #(next_quantities, next_identity) =
    list.fold(inputs, #(quantities, identity), fn(acc, input) {
      let #(current_quantities, current_identity) = acc
      let #(updated_at, identity_after_timestamp) =
        synthetic_identity.make_synthetic_timestamp(current_identity)
      let with_named =
        write_inventory_quantity(
          current_quantities,
          input.name,
          input.quantity,
          Some(updated_at),
        )
      let with_on_hand = case input.name == "available" {
        True ->
          write_inventory_quantity(with_named, "on_hand", input.quantity, None)
        False -> with_named
      }
      #(with_on_hand, identity_after_timestamp)
    })
  #(ensure_product_set_default_quantities(next_quantities), next_identity)
}

fn write_inventory_quantity(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
  updated_at: Option(String),
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True ->
            InventoryQuantityRecord(
              ..quantity,
              quantity: amount,
              updated_at: updated_at,
            )
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(
          name: name,
          quantity: amount,
          updated_at: updated_at,
        ),
      ])
  }
}

fn ensure_product_set_default_quantities(
  quantities: List(InventoryQuantityRecord),
) -> List(InventoryQuantityRecord) {
  quantities
  |> ensure_inventory_quantity("available", 0)
  |> ensure_inventory_quantity("on_hand", 0)
  |> ensure_inventory_quantity("incoming", 0)
}

fn ensure_inventory_quantity(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True -> quantities
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(name: name, quantity: amount, updated_at: None),
      ])
  }
}

fn product_set_inventory_location(
  store: Store,
  existing: Option(InventoryLevelRecord),
  location_id: String,
) -> InventoryLocationRecord {
  case store.get_effective_location_by_id(store, location_id) {
    Some(location) ->
      InventoryLocationRecord(id: location.id, name: location.name)
    None ->
      case existing {
        Some(level) -> level.location
        None -> InventoryLocationRecord(id: location_id, name: "")
      }
  }
}

fn product_set_inventory_level_id(
  inventory_item_id: String,
  location_id: String,
) -> String {
  let inventory_tail =
    inventory_item_id |> string.split("/") |> list.last |> result.unwrap("0")
  let location_tail =
    location_id |> string.split("/") |> list.last |> result.unwrap("0")
  "gid://shopify/InventoryLevel/"
  <> inventory_tail
  <> "-"
  <> location_tail
  <> "?inventory_item_id="
  <> inventory_item_id
}

fn product_set_metafield_records(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductMetafieldRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_metafields =
    store.get_effective_metafields_by_owner_id(store, product_id)
  let #(reversed, final_identity, ids) =
    list.fold(inputs, #([], identity, []), fn(acc, input) {
      let #(records, current_identity, collected_ids) = acc
      let existing = case read_string_field(input, "id") {
        Some(id) ->
          existing_metafields
          |> list.find(fn(metafield) { metafield.id == id })
          |> option.from_result
        None -> None
      }
      let #(metafield_id, next_identity, ids) = case existing {
        Some(metafield) -> #(metafield.id, current_identity, [metafield.id])
        None -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
          #(id, next_identity, [id])
        }
      }
      let type_ =
        read_string_field(input, "type")
        |> option.or(option.then(existing, fn(metafield) { metafield.type_ }))
      let value =
        read_string_field(input, "value")
        |> option.or(option.then(existing, fn(metafield) { metafield.value }))
      let metafield =
        ProductMetafieldRecord(
          id: metafield_id,
          owner_id: product_id,
          namespace: read_string_field(input, "namespace")
            |> option.unwrap(
              option.map(existing, fn(metafield) { metafield.namespace })
              |> option.unwrap(""),
            ),
          key: read_string_field(input, "key")
            |> option.unwrap(
              option.map(existing, fn(metafield) { metafield.key })
              |> option.unwrap(""),
            ),
          type_: type_,
          value: value,
          compare_digest: None,
          json_value: None,
          created_at: option.then(existing, fn(metafield) {
            metafield.created_at
          }),
          updated_at: option.then(existing, fn(metafield) {
            metafield.updated_at
          }),
          owner_type: Some("PRODUCT"),
        )
      #([metafield, ..records], next_identity, list.append(collected_ids, ids))
    })
  #(list.reverse(reversed), final_identity, ids)
}

fn duplicate_product_relationships(
  store: Store,
  identity: SyntheticIdentityRegistry,
  source_product_id: String,
  duplicate_product_id: String,
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  let #(options, identity_after_options, option_ids) =
    duplicate_product_options(
      identity,
      duplicate_product_id,
      store.get_effective_options_by_product_id(store, source_product_id),
    )
  let #(variants, identity_after_variants, variant_ids) =
    duplicate_product_variants(
      identity_after_options,
      duplicate_product_id,
      store.get_effective_variants_by_product_id(store, source_product_id),
    )
  let #(metafields, next_identity, metafield_ids) =
    duplicate_product_metafields(
      identity_after_variants,
      duplicate_product_id,
      store.get_effective_metafields_by_owner_id(store, source_product_id),
    )
  let memberships =
    store.list_effective_collections_for_product(store, source_product_id)
    |> list.map(fn(entry) {
      let #(_, membership) = entry
      ProductCollectionRecord(..membership, product_id: duplicate_product_id)
    })
  let next_store =
    store
    |> store.replace_staged_options_for_product(duplicate_product_id, options)
    |> store.replace_staged_variants_for_product(duplicate_product_id, variants)
    |> store.upsert_staged_product_collections(memberships)
    |> store.replace_staged_media_for_product(duplicate_product_id, [])
    |> store.replace_staged_metafields_for_owner(
      duplicate_product_id,
      metafields,
    )
  #(
    next_store,
    next_identity,
    list.append(option_ids, list.append(variant_ids, metafield_ids)),
  )
}

fn duplicate_product_options(
  identity: SyntheticIdentityRegistry,
  duplicate_product_id: String,
  options: List(ProductOptionRecord),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry, List(String)) {
  let #(reversed, next_identity, ids) =
    list.fold(options, #([], identity, []), fn(acc, option_record) {
      let #(collected, current_identity, collected_ids) = acc
      let #(option_id, identity_after_option) =
        synthetic_identity.make_synthetic_gid(current_identity, "ProductOption")
      let #(values, identity_after_values, value_ids) =
        duplicate_product_option_values(
          identity_after_option,
          option_record.option_values,
        )
      #(
        [
          ProductOptionRecord(
            ..option_record,
            id: option_id,
            product_id: duplicate_product_id,
            option_values: values,
          ),
          ..collected
        ],
        identity_after_values,
        list.append(collected_ids, [option_id, ..value_ids]),
      )
    })
  #(list.reverse(reversed), next_identity, ids)
}

fn duplicate_product_option_values(
  identity: SyntheticIdentityRegistry,
  values: List(ProductOptionValueRecord),
) -> #(List(ProductOptionValueRecord), SyntheticIdentityRegistry, List(String)) {
  let #(reversed, next_identity, ids) =
    list.fold(values, #([], identity, []), fn(acc, value_record) {
      let #(collected, current_identity, collected_ids) = acc
      let #(value_id, next_identity) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "ProductOptionValue",
        )
      #(
        [ProductOptionValueRecord(..value_record, id: value_id), ..collected],
        next_identity,
        list.append(collected_ids, [value_id]),
      )
    })
  #(list.reverse(reversed), next_identity, ids)
}

fn duplicate_product_variants(
  identity: SyntheticIdentityRegistry,
  duplicate_product_id: String,
  variants: List(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry, List(String)) {
  let #(reversed, next_identity, ids) =
    list.fold(variants, #([], identity, []), fn(acc, variant) {
      let #(collected, current_identity, collected_ids) = acc
      let #(variant_id, identity_after_variant) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "ProductVariant",
        )
      let #(inventory_item, identity_after_inventory, inventory_ids) =
        duplicate_inventory_item(identity_after_variant, variant.inventory_item)
      #(
        [
          ProductVariantRecord(
            ..variant,
            id: variant_id,
            product_id: duplicate_product_id,
            inventory_item: inventory_item,
          ),
          ..collected
        ],
        identity_after_inventory,
        list.append(collected_ids, [variant_id, ..inventory_ids]),
      )
    })
  #(list.reverse(reversed), next_identity, ids)
}

fn duplicate_inventory_item(
  identity: SyntheticIdentityRegistry,
  inventory_item: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry, List(String)) {
  case inventory_item {
    None -> #(None, identity, [])
    Some(record) -> {
      let #(inventory_item_id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..record, id: inventory_item_id)),
        next_identity,
        [inventory_item_id],
      )
    }
  }
}

fn duplicate_product_metafields(
  identity: SyntheticIdentityRegistry,
  duplicate_product_id: String,
  metafields: List(ProductMetafieldRecord),
) -> #(List(ProductMetafieldRecord), SyntheticIdentityRegistry, List(String)) {
  let #(reversed, next_identity, ids) =
    list.fold(metafields, #([], identity, []), fn(acc, metafield) {
      let #(collected, current_identity, collected_ids) = acc
      let #(metafield_id, next_identity) =
        synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
      #(
        [
          ProductMetafieldRecord(
            ..metafield,
            id: metafield_id,
            owner_id: duplicate_product_id,
          ),
          ..collected
        ],
        next_identity,
        list.append(collected_ids, [metafield_id]),
      )
    })
  #(list.reverse(reversed), next_identity, ids)
}

fn updated_collection_record(
  identity: SyntheticIdentityRegistry,
  collection: CollectionRecord,
  input: Dict(String, ResolvedValue),
) -> #(CollectionRecord, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap(collection.title)
  let handle =
    read_non_empty_string_field(input, "handle")
    |> option.unwrap(collection.handle)
  #(
    CollectionRecord(
      ..collection,
      title: title,
      handle: handle,
      updated_at: Some(updated_at),
      description: read_string_field(input, "description")
        |> option.or(collection.description),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.or(collection.description_html),
      sort_order: read_string_field(input, "sortOrder")
        |> option.or(collection.sort_order),
      template_suffix: read_string_field(input, "templateSuffix")
        |> option.or(collection.template_suffix),
      seo: updated_product_seo(collection.seo, input),
    ),
    next_identity,
  )
}

fn created_collection_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(CollectionRecord, SyntheticIdentityRegistry) {
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap("Untitled collection")
  let #(updated_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_timestamp,
      "Collection",
    )
  let handle = case read_non_empty_string_field(input, "handle") {
    Some(handle) -> normalize_product_handle(handle)
    None -> slugify_collection_handle(title)
  }
  #(
    CollectionRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: ensure_unique_collection_handle(store, handle),
      publication_ids: [],
      updated_at: Some(updated_at),
      description: read_string_field(input, "description"),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.or(Some("")),
      image: None,
      sort_order: read_string_field(input, "sortOrder")
        |> option.or(Some("MANUAL")),
      template_suffix: read_string_field(input, "templateSuffix"),
      seo: updated_product_seo(
        ProductSeoRecord(title: None, description: None),
        input,
      ),
      rule_set: None,
      products_count: Some(0),
      is_smart: False,
      cursor: None,
      title_cursor: None,
      updated_at_cursor: None,
    ),
    next_identity,
  )
}

fn slugify_collection_handle(title: String) -> String {
  let normalized = normalize_product_handle(title)
  let handle = case normalized {
    "" -> "untitled-collection"
    _ -> normalized
  }
  case string.ends_with(handle, "product") {
    True -> string.drop_end(handle, 7) <> "collection"
    False -> handle
  }
}

fn ensure_unique_collection_handle(store: Store, handle: String) -> String {
  case store.get_effective_collection_by_handle(store, handle) {
    Some(_) -> ensure_unique_collection_handle(store, handle <> "-1")
    None -> handle
  }
}

fn read_explicit_product_handle(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_non_empty_string_field(input, "handle") {
    Some(handle) -> {
      let normalized = normalize_product_handle(handle)
      case normalized {
        "" -> Some("product")
        _ -> Some(normalized)
      }
    }
    None -> None
  }
}

fn slugify_product_handle(title: String) -> String {
  let normalized = normalize_product_handle(title)
  case normalized {
    "" -> "untitled-product"
    _ -> normalized
  }
}

fn normalize_product_handle(value: String) -> String {
  value
  |> string.trim
  |> string.lowercase
  |> string.to_graphemes
  |> list.fold(#([], ""), fn(acc, grapheme) {
    let #(parts, current) = acc
    case is_handle_grapheme(grapheme) {
      True -> #(parts, current <> grapheme)
      False ->
        case current {
          "" -> #(parts, "")
          _ -> #([current, ..parts], "")
        }
    }
  })
  |> finish_handle_parts
}

fn finish_handle_parts(parts_state: #(List(String), String)) -> String {
  let #(parts, current) = parts_state
  let parts = case current {
    "" -> parts
    _ -> [current, ..parts]
  }
  parts
  |> list.reverse
  |> string.join("-")
}

fn is_handle_grapheme(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    _ -> False
  }
}

fn ensure_unique_product_handle(store: Store, handle: String) -> String {
  case product_handle_in_use(store, handle) {
    True -> ensure_unique_product_handle(store, handle <> "-1")
    False -> handle
  }
}

fn product_handle_in_use(store: Store, handle: String) -> Bool {
  store.list_effective_products(store)
  |> list.any(fn(product) { product.handle == handle })
}

fn make_product_create_option_graph(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  option_inputs: List(Dict(String, ResolvedValue)),
) -> #(
  List(ProductOptionRecord),
  ProductVariantRecord,
  SyntheticIdentityRegistry,
  List(String),
) {
  case option_inputs {
    [] -> {
      let #(default_option, identity_after_option, option_ids) =
        make_default_option_record(identity, product)
      let #(default_variant, final_identity, variant_ids) =
        make_default_variant_record(identity_after_option, product)
      #(
        [default_option],
        default_variant,
        final_identity,
        list.append(option_ids, variant_ids),
      )
    }
    _ -> {
      let #(options, identity_after_options) =
        make_created_option_records(identity, product.id, option_inputs)
      let positioned_options = sort_and_position_options(options)
      let #(default_variant, final_identity, variant_ids) =
        make_default_variant_for_options(
          identity_after_options,
          product,
          positioned_options,
        )
      let synced_options =
        sync_product_options_with_variants(positioned_options, [default_variant])
      let option_ids =
        list.append(
          list.map(synced_options, fn(option) { option.id }),
          list.flat_map(synced_options, fn(option) {
            list.map(option.option_values, fn(value) { value.id })
          }),
        )
      #(
        synced_options,
        default_variant,
        final_identity,
        list.append(option_ids, variant_ids),
      )
    }
  }
}

fn make_default_variant_for_options(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  options: List(ProductOptionRecord),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry, List(String)) {
  let selected_options =
    list.map(options, fn(option) {
      ProductVariantSelectedOptionRecord(
        name: option.name,
        value: first_option_value_name(option),
      )
    })
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_variant,
      "InventoryItem",
    )
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product.id,
      title: variant_title(selected_options),
      sku: None,
      barcode: None,
      price: None,
      compare_at_price: None,
      taxable: None,
      inventory_policy: None,
      inventory_quantity: Some(0),
      selected_options: selected_options,
      media_ids: [],
      inventory_item: Some(
        InventoryItemRecord(
          id: inventory_item_id,
          tracked: Some(False),
          requires_shipping: Some(True),
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [],
        ),
      ),
      contextual_pricing: None,
      cursor: None,
    ),
    next_identity,
    [variant_id, inventory_item_id],
  )
}

fn make_default_option_record(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
) -> #(ProductOptionRecord, SyntheticIdentityRegistry, List(String)) {
  let #(option_id, identity_after_option) =
    synthetic_identity.make_synthetic_gid(identity, "ProductOption")
  let #(value_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_option,
      "ProductOptionValue",
    )
  #(
    ProductOptionRecord(
      id: option_id,
      product_id: product.id,
      name: "Title",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: value_id,
          name: "Default Title",
          has_variants: True,
        ),
      ],
    ),
    next_identity,
    [option_id, value_id],
  )
}

fn make_default_variant_record(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
) -> #(ProductVariantRecord, SyntheticIdentityRegistry, List(String)) {
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_variant,
      "InventoryItem",
    )
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product.id,
      title: "Default Title",
      sku: None,
      barcode: None,
      price: None,
      compare_at_price: None,
      taxable: None,
      inventory_policy: None,
      inventory_quantity: Some(0),
      selected_options: [
        ProductVariantSelectedOptionRecord(
          name: "Title",
          value: "Default Title",
        ),
      ],
      media_ids: [],
      inventory_item: Some(
        InventoryItemRecord(
          id: inventory_item_id,
          tracked: Some(False),
          requires_shipping: Some(True),
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [],
        ),
      ),
      contextual_pricing: None,
      cursor: None,
    ),
    next_identity,
    [variant_id, inventory_item_id],
  )
}

fn make_created_variant_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
  defaults: Option(ProductVariantRecord),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let selected_options = read_variant_selected_options(input, [])
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item, final_identity) = case
    read_object_field(input, "inventoryItem")
  {
    Some(inventory_item_input) ->
      read_variant_inventory_item(
        identity_after_variant,
        Some(inventory_item_input),
        None,
      )
    None ->
      clone_default_inventory_item(
        identity_after_variant,
        option.then(defaults, fn(variant) { variant.inventory_item }),
      )
  }
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(variant_title_with_fallback(
          selected_options,
          "Default Title",
        )),
      sku: read_variant_sku(input, None),
      barcode: read_string_field(input, "barcode"),
      price: read_string_field(input, "price")
        |> option.or(option.then(defaults, fn(variant) { variant.price })),
      compare_at_price: read_string_field(input, "compareAtPrice")
        |> option.or(
          option.then(defaults, fn(variant) { variant.compare_at_price }),
        ),
      taxable: read_bool_field(input, "taxable")
        |> option.or(option.then(defaults, fn(variant) { variant.taxable })),
      inventory_policy: read_string_field(input, "inventoryPolicy")
        |> option.or(
          option.then(defaults, fn(variant) { variant.inventory_policy }),
        ),
      inventory_quantity: read_variant_inventory_quantity(input, Some(0)),
      selected_options: selected_options,
      media_ids: [],
      inventory_item: inventory_item,
      contextual_pricing: None,
      cursor: None,
    ),
    final_identity,
  )
}

fn make_created_variant_records(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  defaults: Option(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(variants, current_identity) = acc
      let #(variant, next_identity) =
        make_created_variant_record(
          current_identity,
          product_id,
          input,
          defaults,
        )
      #([variant, ..variants], next_identity)
    })
  #(list.reverse(reversed), final_identity)
}

fn validate_bulk_create_variant_batch(
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

fn validate_bulk_update_variant_batch(
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

fn validate_bulk_variant_scalar_input(
  input: Dict(String, ResolvedValue),
  variant_index: Int,
) -> List(BulkVariantUserError) {
  variant_validation_problems(input)
  |> list.flat_map(fn(problem) {
    bulk_variant_error_from_problem(problem, variant_index)
  })
}

fn validate_product_variant_scalar_input(
  input: Dict(String, ResolvedValue),
  prefix: List(String),
) -> List(ProductUserError) {
  variant_validation_problems(input)
  |> list.map(fn(problem) {
    let VariantValidationProblem(
      suffix: suffix,
      message: message,
      product_code: code,
      ..,
    ) = problem
    ProductUserError(list.append(prefix, suffix), message, code)
  })
}

fn bulk_variant_error_from_problem(
  problem: VariantValidationProblem,
  variant_index: Int,
) -> List(BulkVariantUserError) {
  let VariantValidationProblem(
    kind: kind,
    bulk_suffix: suffix,
    message: message,
    bulk_code: code,
    ..,
  ) = problem
  let field = Some(["variants", int.to_string(variant_index), ..suffix])
  let primary = BulkVariantUserError(field, message, code)
  case kind {
    "sku_too_long" -> [
      primary,
      BulkVariantUserError(
        Some(["variants", int.to_string(variant_index)]),
        "is too long (maximum is 255 characters)",
        None,
      ),
    ]
    _ -> [primary]
  }
}

fn variant_validation_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  []
  |> list.append(variant_price_problems(input))
  |> list.append(variant_compare_at_price_problems(input))
  |> list.append(variant_weight_problems(input))
  |> list.append(variant_quantity_problems(input))
  |> list.append(variant_text_length_problems(input))
  |> list.append(variant_option_value_length_problems(input))
}

fn variant_price_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  case read_numeric_field(input, "price") {
    NumericNull -> [
      VariantValidationProblem(
        "price_blank",
        ["price"],
        ["price"],
        "Price can't be blank",
        Some("INVALID"),
        Some("INVALID"),
      ),
    ]
    NumericValue(value) if value <. 0.0 -> [
      VariantValidationProblem(
        "price_negative",
        ["price"],
        ["price"],
        "Price must be greater than or equal to 0",
        Some("GREATER_THAN_OR_EQUAL_TO"),
        Some("GREATER_THAN_OR_EQUAL_TO"),
      ),
    ]
    NumericValue(value) if value >=. max_variant_price -> [
      VariantValidationProblem(
        "price_too_large",
        ["price"],
        ["price"],
        "Price must be less than 1000000000000000000",
        Some("INVALID_INPUT"),
        Some("INVALID_INPUT"),
      ),
    ]
    NumericNotANumber -> [
      VariantValidationProblem(
        "price_not_a_number",
        ["price"],
        ["price"],
        "Price is not a number",
        Some("NOT_A_NUMBER"),
        Some("NOT_A_NUMBER"),
      ),
    ]
    NumericMissing | NumericValue(_) -> []
  }
}

fn variant_compare_at_price_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  case read_numeric_field(input, "compareAtPrice") {
    NumericValue(value) if value >=. max_variant_price -> [
      VariantValidationProblem(
        "compare_at_price_too_large",
        ["compareAtPrice"],
        ["compareAtPrice"],
        "must be less than 1000000000000000000",
        Some("INVALID_INPUT"),
        Some("INVALID_INPUT"),
      ),
    ]
    NumericNotANumber -> [
      VariantValidationProblem(
        "compare_at_price_not_a_number",
        ["compareAtPrice"],
        ["compareAtPrice"],
        "Compare at price is not a number",
        Some("NOT_A_NUMBER"),
        Some("NOT_A_NUMBER"),
      ),
    ]
    NumericMissing | NumericNull | NumericValue(_) -> []
  }
}

fn variant_weight_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  let top_level_errors =
    variant_weight_value_problems(read_numeric_field(input, "weight"))
    |> list.append(variant_top_level_weight_unit_problems(input))
  case read_variant_weight_input(input) {
    None -> top_level_errors
    Some(weight) -> {
      top_level_errors
      |> list.append(
        variant_weight_value_problems(read_numeric_field(weight, "value")),
      )
      |> list.append(variant_weight_unit_problems(weight))
    }
  }
}

fn variant_weight_value_problems(
  read: NumericRead,
) -> List(VariantValidationProblem) {
  case read {
    NumericValue(value) if value <. 0.0 -> [
      VariantValidationProblem(
        "weight_negative",
        [],
        [],
        "Weight must be greater than or equal to 0",
        Some("GREATER_THAN_OR_EQUAL_TO"),
        Some("GREATER_THAN_OR_EQUAL_TO"),
      ),
    ]
    NumericValue(value) if value >=. max_variant_weight -> [
      VariantValidationProblem(
        "weight_too_large",
        [],
        [],
        "Weight must be less than 2000000000",
        Some("INVALID_INPUT"),
        Some("INVALID_INPUT"),
      ),
    ]
    NumericNotANumber -> [
      VariantValidationProblem(
        "weight_not_a_number",
        [],
        [],
        "Weight is not a number",
        Some("NOT_A_NUMBER"),
        Some("NOT_A_NUMBER"),
      ),
    ]
    NumericMissing | NumericNull | NumericValue(_) -> []
  }
}

fn variant_weight_unit_problems(
  weight: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  case read_string_field(weight, "unit") {
    Some(unit) ->
      case valid_weight_unit(unit) {
        True -> []
        False -> [
          VariantValidationProblem(
            "weight_unit_invalid",
            [],
            [],
            "Weight unit is not included in the list",
            Some("INVALID_INPUT"),
            Some("INVALID_INPUT"),
          ),
        ]
      }
    None -> []
  }
}

fn variant_top_level_weight_unit_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  case read_string_field(input, "weightUnit") {
    Some(unit) ->
      case valid_weight_unit(unit) {
        True -> []
        False -> [
          VariantValidationProblem(
            "weight_unit_invalid",
            [],
            [],
            "Weight unit is not included in the list",
            Some("INVALID_INPUT"),
            Some("INVALID_INPUT"),
          ),
        ]
      }
    None -> []
  }
}

fn valid_weight_unit(unit: String) -> Bool {
  case unit {
    "KILOGRAMS" | "GRAMS" | "POUNDS" | "OUNCES" -> True
    _ -> False
  }
}

fn variant_quantity_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  let direct_errors = case read_quantity_field(input, "inventoryQuantity") {
    QuantityInt(quantity) ->
      variant_quantity_range_problems(quantity, ["inventoryQuantity"])
    QuantityFloat(_) -> [
      quantity_problem(
        ["inventoryQuantity"],
        "Inventory quantity must be an integer",
      ),
    ]
    QuantityNotANumber -> [
      quantity_problem(
        ["inventoryQuantity"],
        "Inventory quantity must be an integer",
      ),
    ]
    QuantityMissing | QuantityNull -> []
  }
  list.append(direct_errors, inventory_quantity_list_problems(input))
}

fn inventory_quantity_list_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  read_object_list_field(input, "inventoryQuantities")
  |> list.flat_map(fn(quantity_input) {
    let path = ["inventoryQuantities"]
    case read_quantity_field(quantity_input, "availableQuantity") {
      QuantityInt(quantity) -> variant_quantity_range_problems(quantity, path)
      QuantityFloat(_) -> [
        quantity_problem(path, "Inventory quantity must be an integer"),
      ]
      QuantityNotANumber -> [
        quantity_problem(path, "Inventory quantity must be an integer"),
      ]
      QuantityMissing | QuantityNull ->
        case read_quantity_field(quantity_input, "quantity") {
          QuantityInt(quantity) ->
            variant_quantity_range_problems(quantity, path)
          QuantityFloat(_) -> [
            quantity_problem(path, "Inventory quantity must be an integer"),
          ]
          QuantityNotANumber -> [
            quantity_problem(path, "Inventory quantity must be an integer"),
          ]
          QuantityMissing | QuantityNull -> []
        }
    }
  })
}

fn variant_quantity_range_problems(
  quantity: Int,
  suffix: List(String),
) -> List(VariantValidationProblem) {
  case quantity < min_inventory_quantity {
    True -> [
      quantity_problem(
        suffix,
        "Inventory quantity must be greater than or equal to -1000000000",
      ),
    ]
    False ->
      case quantity > max_inventory_quantity {
        True -> [
          quantity_problem(
            suffix,
            "Inventory quantity must be less than or equal to 1000000000",
          ),
        ]
        False -> []
      }
  }
}

fn quantity_problem(
  suffix: List(String),
  message: String,
) -> VariantValidationProblem {
  VariantValidationProblem(
    "inventory_quantity",
    suffix,
    case suffix {
      ["inventoryQuantity"] -> suffix
      _ -> ["inventoryQuantities"]
    },
    message,
    Some("INVALID_INPUT"),
    Some("INVALID_INPUT"),
  )
}

fn variant_text_length_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  let sku_errors = case read_variant_sku(input, None) {
    Some(sku) ->
      case string.length(sku) > max_variant_text_length {
        True -> [
          VariantValidationProblem(
            "sku_too_long",
            ["sku"],
            [],
            "SKU is too long (maximum is 255 characters)",
            Some("INVALID_INPUT"),
            Some("INVALID_INPUT"),
          ),
        ]
        False -> []
      }
    _ -> []
  }
  let barcode_errors = case read_string_field(input, "barcode") {
    Some(barcode) ->
      case string.length(barcode) > max_variant_text_length {
        True -> [
          VariantValidationProblem(
            "barcode_too_long",
            ["barcode"],
            ["barcode"],
            "Barcode is too long (maximum is 255 characters)",
            Some("INVALID_INPUT"),
            Some("INVALID_INPUT"),
          ),
        ]
        False -> []
      }
    _ -> []
  }
  list.append(sku_errors, barcode_errors)
}

fn variant_option_value_length_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  list.append(
    option_value_length_problems(input, "optionValues", "name"),
    option_value_length_problems(input, "selectedOptions", "value"),
  )
}

fn option_value_length_problems(
  input: Dict(String, ResolvedValue),
  list_field: String,
  value_field: String,
) -> List(VariantValidationProblem) {
  case dict.get(input, list_field) {
    Ok(ListVal(values)) ->
      values
      |> enumerate_items()
      |> list.filter_map(fn(pair) {
        let #(value, index) = pair
        case value {
          ObjectVal(fields) ->
            case read_string_field(fields, value_field) {
              Some(name) ->
                case string.length(name) > max_variant_text_length {
                  True ->
                    Ok(VariantValidationProblem(
                      "option_value_too_long",
                      [list_field, int.to_string(index), value_field],
                      [list_field, int.to_string(index), value_field],
                      "Option value name is too long",
                      Some("INVALID_INPUT"),
                      Some("INVALID_INPUT"),
                    ))
                  False -> Error(Nil)
                }
              _ -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_numeric_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> NumericRead {
  case dict.get(input, name) {
    Error(_) -> NumericMissing
    Ok(NullVal) -> NumericNull
    Ok(IntVal(value)) -> NumericValue(int.to_float(value))
    Ok(FloatVal(value)) -> NumericValue(value)
    Ok(StringVal(value)) ->
      case int.parse(value) {
        Ok(parsed) -> NumericValue(int.to_float(parsed))
        Error(_) ->
          case float.parse(value) {
            Ok(parsed) -> NumericValue(parsed)
            Error(_) -> NumericNotANumber
          }
      }
    _ -> NumericNotANumber
  }
}

fn read_quantity_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> QuantityRead {
  case dict.get(input, name) {
    Error(_) -> QuantityMissing
    Ok(NullVal) -> QuantityNull
    Ok(IntVal(value)) -> QuantityInt(value)
    Ok(FloatVal(value)) -> QuantityFloat(value)
    Ok(StringVal(_)) -> QuantityNotANumber
    _ -> QuantityNotANumber
  }
}

fn read_variant_weight_input(
  input: Dict(String, ResolvedValue),
) -> Option(Dict(String, ResolvedValue)) {
  use inventory_item <- option.then(read_object_field(input, "inventoryItem"))
  use measurement <- option.then(read_object_field(
    inventory_item,
    "measurement",
  ))
  read_object_field(measurement, "weight")
}

fn validate_bulk_variant_option_input(
  store: Store,
  product_id: String,
  input: Dict(String, ResolvedValue),
  variant_index: Int,
  mode: String,
) -> #(List(ProductVariantSelectedOptionRecord), List(BulkVariantUserError)) {
  let selected_options = read_variant_selected_options(input, [])
  let product_options =
    store.get_effective_options_by_product_id(store, product_id)
  let option_field_name = bulk_variant_option_field_name(input)
  let user_errors =
    validate_bulk_variant_selected_options(
      selected_options,
      product_options,
      dict.new(),
      variant_index,
      0,
      option_field_name,
      mode,
    )
  let user_errors = case user_errors {
    [] ->
      validate_bulk_variant_required_options(
        selected_options,
        product_options,
        variant_index,
        mode,
        has_variant_option_input(input),
      )
    _ -> user_errors
  }
  #(selected_options, user_errors)
}

fn validate_bulk_variant_selected_options(
  selected_options: List(ProductVariantSelectedOptionRecord),
  product_options: List(ProductOptionRecord),
  seen_names: Dict(String, Bool),
  variant_index: Int,
  option_index: Int,
  option_field_name: String,
  mode: String,
) -> List(BulkVariantUserError) {
  case selected_options {
    [] -> []
    [selected, ..rest] ->
      case dict.has_key(seen_names, selected.name) {
        True -> [
          BulkVariantUserError(
            Some(["variants", int.to_string(variant_index), option_field_name]),
            "Duplicated option name '" <> selected.name <> "'",
            Some("INVALID_INPUT"),
          ),
        ]
        False ->
          case
            !list.is_empty(product_options)
            && product_option_named(product_options, selected.name) == None
          {
            True -> [
              BulkVariantUserError(
                Some([
                  "variants",
                  int.to_string(variant_index),
                  option_field_name,
                  int.to_string(option_index),
                ]),
                "Option does not exist",
                Some(case mode {
                  "update" -> "OPTION_DOES_NOT_EXIST"
                  _ -> "INVALID_INPUT"
                }),
              ),
            ]
            False ->
              validate_bulk_variant_selected_options(
                rest,
                product_options,
                dict.insert(seen_names, selected.name, True),
                variant_index,
                option_index + 1,
                option_field_name,
                mode,
              )
          }
      }
  }
}

fn validate_bulk_variant_required_options(
  selected_options: List(ProductVariantSelectedOptionRecord),
  product_options: List(ProductOptionRecord),
  variant_index: Int,
  mode: String,
  has_option_input: Bool,
) -> List(BulkVariantUserError) {
  let should_require = mode == "create" || has_option_input
  case
    should_require,
    list.is_empty(product_options),
    list.is_empty(selected_options)
  {
    True, False, False ->
      case
        first_missing_selected_product_option(product_options, selected_options)
      {
        Some(option) -> [
          BulkVariantUserError(
            Some(["variants", int.to_string(variant_index)]),
            "You need to add option values for " <> option.name,
            Some("NEED_TO_ADD_OPTION_VALUES"),
          ),
        ]
        None -> []
      }
    _, _, _ -> []
  }
}

fn validate_bulk_create_inventory_quantities(
  store: Store,
  input: Dict(String, ResolvedValue),
  variant_index: Int,
  selected_options: List(ProductVariantSelectedOptionRecord),
) -> List(BulkVariantUserError) {
  let quantities = read_object_list_field(input, "inventoryQuantities")
  let has_invalid_location =
    quantities
    |> list.any(fn(quantity) {
      case read_string_field(quantity, "locationId") {
        Some("gid://shopify/Location/1") -> False
        Some(location_id) ->
          store.get_effective_location_by_id(store, location_id) == None
        None -> False
      }
    })
  case has_invalid_location {
    True -> [
      BulkVariantUserError(
        Some(["variants", int.to_string(variant_index), "inventoryQuantities"]),
        "Quantity for "
          <> variant_title_with_fallback(selected_options, "Default Title")
          <> " couldn't be set because the location was deleted.",
        Some("TRACKED_VARIANT_LOCATION_NOT_FOUND"),
      ),
    ]
    False -> []
  }
}

fn has_variant_option_input(input: Dict(String, ResolvedValue)) -> Bool {
  dict.has_key(input, "selectedOptions")
  || dict.has_key(input, "optionValues")
  || dict.has_key(input, "options")
}

fn bulk_variant_option_field_name(
  input: Dict(String, ResolvedValue),
) -> String {
  case dict.has_key(input, "optionValues") {
    True -> "optionValues"
    False ->
      case dict.has_key(input, "selectedOptions") {
        True -> "selectedOptions"
        False -> "options"
      }
  }
}

fn product_option_named(
  options: List(ProductOptionRecord),
  name: String,
) -> Option(ProductOptionRecord) {
  options
  |> list.find(fn(option) { option.name == name })
  |> option.from_result
}

fn first_missing_selected_product_option(
  options: List(ProductOptionRecord),
  selected_options: List(ProductVariantSelectedOptionRecord),
) -> Option(ProductOptionRecord) {
  options
  |> list.find(fn(option) {
    !list.any(selected_options, fn(selected) { selected.name == option.name })
  })
  |> option.from_result
}

fn has_variant_id(variants: List(ProductVariantRecord), id: String) -> Bool {
  list.any(variants, fn(variant) { variant.id == id })
}

fn first_bulk_delete_missing_variant(
  variant_ids: List(String),
  variants: List(ProductVariantRecord),
) -> Option(Int) {
  variant_ids
  |> enumerate_items()
  |> list.find_map(fn(pair) {
    let #(variant_id, index) = pair
    case
      has_variant_id(variants, variant_id)
      && !is_known_missing_shopify_gid(variant_id)
    {
      True -> Error(Nil)
      False ->
        case has_variant_id(variants, variant_id) {
          True -> Error(Nil)
          False -> Ok(index)
        }
    }
  })
  |> option.from_result
}

fn is_known_missing_shopify_gid(id: String) -> Bool {
  string.contains(id, "/999999999999")
}

fn update_variant_records(
  identity: SyntheticIdentityRegistry,
  variants: List(ProductVariantRecord),
  updates: List(Dict(String, ResolvedValue)),
) -> #(
  List(ProductVariantRecord),
  List(ProductVariantRecord),
  SyntheticIdentityRegistry,
) {
  let #(reversed_variants, reversed_updated, final_identity) =
    list.fold(variants, #([], [], identity), fn(acc, variant) {
      let #(next_variants, updated_variants, current_identity) = acc
      case find_variant_update(updates, variant.id) {
        Some(input) -> {
          let #(updated, next_identity) =
            update_variant_record(current_identity, variant, input)
          #(
            [updated, ..next_variants],
            [updated, ..updated_variants],
            next_identity,
          )
        }
        None -> #(
          [variant, ..next_variants],
          updated_variants,
          current_identity,
        )
      }
    })
  #(
    list.reverse(reversed_variants),
    list.reverse(reversed_updated),
    final_identity,
  )
}

fn read_product_variant_positions(
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductVariantPositionInput), List(ProductUserError)) {
  case inputs {
    [] -> #([], [
      ProductUserError(["positions"], "At least one position is required", None),
    ])
    _ -> {
      let #(reversed_positions, errors) =
        inputs
        |> enumerate_items()
        |> list.fold(#([], []), fn(acc, pair) {
          let #(positions, errors) = acc
          let #(input, index) = pair
          let path = ["positions", int.to_string(index)]
          let variant_id = graphql_helpers.read_arg_string(input, "id")
          let raw_position = read_int_field(input, "position")
          let id_errors = case variant_id {
            None -> [
              ProductUserError(
                list.append(path, ["id"]),
                "Variant id is required",
                None,
              ),
            ]
            Some(_) -> []
          }
          let position_errors = case raw_position {
            Some(position) if position >= 1 -> []
            _ -> [
              ProductUserError(
                list.append(path, ["position"]),
                "Position is invalid",
                None,
              ),
            ]
          }
          let next_positions = case variant_id, raw_position {
            Some(id), Some(position) if position >= 1 -> [
              ProductVariantPositionInput(id: id, position: position - 1),
              ..positions
            ]
            _, _ -> positions
          }
          #(
            next_positions,
            list.append(errors, list.append(id_errors, position_errors)),
          )
        })
      #(list.reverse(reversed_positions), errors)
    }
  }
}

fn validate_product_variant_positions(
  variants: List(ProductVariantRecord),
  positions: List(ProductVariantPositionInput),
) -> List(ProductUserError) {
  positions
  |> enumerate_items()
  |> list.filter_map(fn(pair) {
    let #(position, index) = pair
    case list.any(variants, fn(variant) { variant.id == position.id }) {
      True -> Error(Nil)
      False ->
        Ok(ProductUserError(
          ["positions", int.to_string(index), "id"],
          "Variant does not exist",
          None,
        ))
    }
  })
}

fn apply_sequential_variant_reorder(
  variants: List(ProductVariantRecord),
  positions: List(ProductVariantPositionInput),
) -> List(ProductVariantRecord) {
  list.fold(positions, variants, fn(current, position) {
    move_variant_to_position(current, position.id, position.position)
  })
}

fn move_variant_to_position(
  variants: List(ProductVariantRecord),
  variant_id: String,
  position: Int,
) -> List(ProductVariantRecord) {
  let #(variant, remaining) = remove_variant_by_id(variants, variant_id, [])
  case variant {
    Some(record) -> insert_variant_at_position(remaining, record, position)
    None -> variants
  }
}

fn remove_variant_by_id(
  variants: List(ProductVariantRecord),
  variant_id: String,
  reversed_before: List(ProductVariantRecord),
) -> #(Option(ProductVariantRecord), List(ProductVariantRecord)) {
  case variants {
    [] -> #(None, list.reverse(reversed_before))
    [first, ..rest] ->
      case first.id == variant_id {
        True -> #(Some(first), list.append(list.reverse(reversed_before), rest))
        False ->
          remove_variant_by_id(rest, variant_id, [first, ..reversed_before])
      }
  }
}

fn insert_variant_at_position(
  variants: List(ProductVariantRecord),
  variant: ProductVariantRecord,
  position: Int,
) -> List(ProductVariantRecord) {
  case variants, position <= 0 {
    _, True -> [variant, ..variants]
    [], False -> [variant]
    [first, ..rest], False -> [
      first,
      ..insert_variant_at_position(rest, variant, position - 1)
    ]
  }
}

fn find_variant_update(
  updates: List(Dict(String, ResolvedValue)),
  variant_id: String,
) -> Option(Dict(String, ResolvedValue)) {
  updates
  |> list.find(fn(input) {
    graphql_helpers.read_arg_string(input, "id") == Some(variant_id)
  })
  |> option.from_result
}

fn update_variant_record(
  identity: SyntheticIdentityRegistry,
  existing: ProductVariantRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let selected_options =
    read_variant_selected_options(input, existing.selected_options)
  let #(inventory_item, next_identity) =
    read_variant_inventory_item(
      identity,
      read_object_field(input, "inventoryItem"),
      existing.inventory_item,
    )
  #(
    ProductVariantRecord(
      ..existing,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(variant_title_with_fallback(
          selected_options,
          existing.title,
        )),
      sku: read_variant_sku(input, existing.sku),
      barcode: read_string_field(input, "barcode")
        |> option.or(existing.barcode),
      price: read_string_field(input, "price") |> option.or(existing.price),
      compare_at_price: read_string_field(input, "compareAtPrice")
        |> option.or(existing.compare_at_price),
      taxable: read_bool_field(input, "taxable") |> option.or(existing.taxable),
      inventory_policy: read_string_field(input, "inventoryPolicy")
        |> option.or(existing.inventory_policy),
      inventory_quantity: read_variant_inventory_quantity(
        input,
        existing.inventory_quantity,
      ),
      selected_options: selected_options,
      inventory_item: inventory_item,
    ),
    next_identity,
  )
}

fn read_variant_sku(
  input: Dict(String, ResolvedValue),
  fallback: Option(String),
) -> Option(String) {
  case read_string_field(input, "sku") {
    Some(sku) -> Some(sku)
    None ->
      case read_object_field(input, "inventoryItem") {
        Some(item) -> read_string_field(item, "sku") |> option.or(fallback)
        None -> fallback
      }
  }
}

fn read_variant_selected_options(
  input: Dict(String, ResolvedValue),
  fallback: List(ProductVariantSelectedOptionRecord),
) -> List(ProductVariantSelectedOptionRecord) {
  case dict.get(input, "selectedOptions") {
    Ok(ListVal(values)) -> {
      let selected =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) -> read_variant_selected_option(fields)
            _ -> Error(Nil)
          }
        })
      case selected {
        [] -> fallback
        _ -> selected
      }
    }
    _ -> read_variant_option_values(input, fallback)
  }
}

fn read_variant_option_values(
  input: Dict(String, ResolvedValue),
  fallback: List(ProductVariantSelectedOptionRecord),
) -> List(ProductVariantSelectedOptionRecord) {
  case dict.get(input, "optionValues") {
    Ok(ListVal(values)) -> {
      let selected =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) -> read_variant_option_value(fields)
            _ -> Error(Nil)
          }
        })
      case selected {
        [] -> fallback
        _ -> selected
      }
    }
    _ -> fallback
  }
}

fn read_variant_option_value(
  fields: Dict(String, ResolvedValue),
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  case
    read_non_empty_string_field(fields, "optionName"),
    read_non_empty_string_field(fields, "name")
  {
    Some(name), Some(value) ->
      Ok(ProductVariantSelectedOptionRecord(name: name, value: value))
    _, _ -> Error(Nil)
  }
}

fn read_variant_selected_option(
  fields: Dict(String, ResolvedValue),
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  case
    read_non_empty_string_field(fields, "name"),
    read_non_empty_string_field(fields, "value")
  {
    Some(name), Some(value) ->
      Ok(ProductVariantSelectedOptionRecord(name: name, value: value))
    _, _ -> Error(Nil)
  }
}

fn read_variant_inventory_quantity(
  input: Dict(String, ResolvedValue),
  fallback: Option(Int),
) -> Option(Int) {
  case read_int_field(input, "inventoryQuantity") {
    Some(quantity) -> Some(quantity)
    None ->
      read_inventory_quantities_available_total(input) |> option.or(fallback)
  }
}

fn read_inventory_quantities_available_total(
  input: Dict(String, ResolvedValue),
) -> Option(Int) {
  case dict.get(input, "inventoryQuantities") {
    Ok(ListVal(values)) -> {
      let quantities =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) ->
              case read_int_field(fields, "availableQuantity") {
                Some(quantity) -> Ok(quantity)
                None -> Error(Nil)
              }
            _ -> Error(Nil)
          }
        })
      case quantities {
        [] -> None
        _ ->
          Some(
            list.fold(quantities, 0, fn(total, quantity) { total + quantity }),
          )
      }
    }
    _ -> None
  }
}

fn read_variant_inventory_item(
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, ResolvedValue)),
  existing: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  case input {
    None -> #(existing, identity)
    Some(input) -> {
      let #(id, next_identity) = case existing {
        Some(item) -> #(item.id, identity)
        None -> synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      }
      let current_levels = case existing {
        Some(item) -> item.inventory_levels
        None -> []
      }
      #(
        Some(InventoryItemRecord(
          id: id,
          tracked: read_bool_field(input, "tracked")
            |> option.or(option.then(existing, fn(item) { item.tracked })),
          requires_shipping: read_bool_field(input, "requiresShipping")
            |> option.or(
              option.then(existing, fn(item) { item.requires_shipping }),
            ),
          measurement: read_inventory_measurement_input(
            input,
            option.then(existing, fn(item) { item.measurement }),
          ),
          country_code_of_origin: read_string_field(
            input,
            "countryCodeOfOrigin",
          )
            |> option.or(
              option.then(existing, fn(item) { item.country_code_of_origin }),
            ),
          province_code_of_origin: read_string_field(
            input,
            "provinceCodeOfOrigin",
          )
            |> option.or(
              option.then(existing, fn(item) { item.province_code_of_origin }),
            ),
          harmonized_system_code: read_string_field(
            input,
            "harmonizedSystemCode",
          )
            |> option.or(
              option.then(existing, fn(item) { item.harmonized_system_code }),
            ),
          inventory_levels: current_levels,
        )),
        next_identity,
      )
    }
  }
}

fn read_inventory_measurement_input(
  input: Dict(String, ResolvedValue),
  fallback: Option(InventoryMeasurementRecord),
) -> Option(InventoryMeasurementRecord) {
  case read_object_field(input, "measurement") {
    Some(measurement) ->
      Some(
        InventoryMeasurementRecord(weight: read_inventory_weight_input(
          measurement,
          option.then(fallback, fn(measurement) { measurement.weight }),
        )),
      )
    None -> fallback
  }
}

fn read_inventory_weight_input(
  input: Dict(String, ResolvedValue),
  fallback: Option(InventoryWeightRecord),
) -> Option(InventoryWeightRecord) {
  case read_object_field(input, "weight") {
    Some(weight) ->
      case
        read_string_field(weight, "unit"),
        read_inventory_weight_value_input(weight)
      {
        Some(unit), Some(value) ->
          Some(InventoryWeightRecord(unit: unit, value: value))
        _, _ -> fallback
      }
    None -> fallback
  }
}

fn read_inventory_weight_value_input(
  input: Dict(String, ResolvedValue),
) -> Option(InventoryWeightValue) {
  case dict.get(input, "value") {
    Ok(IntVal(value)) -> Some(InventoryWeightInt(value))
    Ok(FloatVal(value)) -> Some(InventoryWeightFloat(value))
    _ -> None
  }
}

fn clone_default_inventory_item(
  identity: SyntheticIdentityRegistry,
  item: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  case item {
    None -> #(None, identity)
    Some(item) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(Some(InventoryItemRecord(..item, id: id)), next_identity)
    }
  }
}

fn apply_inventory_adjust_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: String,
  changes: List(InventoryAdjustmentChangeInput),
  require_change_from_quantity: Bool,
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  let result =
    changes
    |> enumerate_items()
    |> list.try_fold(#([], [], store), fn(acc, pair) {
      let #(change, index) = pair
      let #(adjusted_changes, mirrored_changes, current_store) = acc
      case
        stage_inventory_quantity_adjust(
          current_store,
          name,
          change,
          index,
          require_change_from_quantity,
        )
      {
        Error(error) -> Error([error])
        Ok(applied) -> {
          let #(next_store, adjusted_change, mirrored) = applied
          Ok(#(
            list.append(adjusted_changes, [adjusted_change]),
            list.append(mirrored_changes, mirrored),
            next_store,
          ))
        }
      }
    })
  case result {
    Error(errors) -> Error(errors)
    Ok(done) -> {
      let #(adjusted_changes, mirrored_changes, next_store) = done
      let #(group, next_identity) =
        make_inventory_adjustment_group(
          identity,
          reason,
          reference_document_uri,
          list.append(adjusted_changes, mirrored_changes),
        )
      Ok(#(
        next_store,
        next_identity,
        group,
        inventory_adjustment_staged_ids(group),
      ))
    }
  }
}

fn apply_inventory_set_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: String,
  quantities: List(InventorySetQuantityInput),
  ignore_compare_quantity: Bool,
  use_change_from_quantity: Bool,
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  case validate_inventory_set_quantity_inputs(quantities) {
    [_, ..] as errors -> Error(errors)
    [] -> {
      let initial = #([], [], store)
      let result =
        quantities
        |> enumerate_items()
        |> list.try_fold(initial, fn(acc, pair) {
          let #(quantity, index) = pair
          let #(changes, mirrored_changes, current_store) = acc
          let assert Some(inventory_item_id) = quantity.inventory_item_id
          let assert Some(location_id) = quantity.location_id
          let assert Some(next_quantity) = quantity.quantity
          case
            stage_inventory_quantity_set(
              current_store,
              inventory_item_id,
              location_id,
              name,
              next_quantity,
              ignore_compare_quantity,
              quantity_compare_quantity(quantity, use_change_from_quantity),
              use_change_from_quantity,
              index,
            )
          {
            Error(error) -> Error([error])
            Ok(applied) -> {
              let #(next_store, delta) = applied
              let change =
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: name,
                  delta: delta,
                  quantity_after_change: None,
                  ledger_document_uri: None,
                )
              let #(changes_to_append, mirrored) =
                inventory_set_quantity_changes(
                  inventory_item_id,
                  location_id,
                  name,
                  delta,
                  change,
                )
              Ok(#(
                list.append(changes, changes_to_append),
                list.append(mirrored_changes, mirrored),
                next_store,
              ))
            }
          }
        })
      case result {
        Error(errors) -> Error(errors)
        Ok(done) -> {
          let #(changes, mirrored_changes, next_store) = done
          let #(group, next_identity) =
            make_inventory_adjustment_group(
              identity,
              reason,
              reference_document_uri,
              list.append(changes, mirrored_changes),
            )
          Ok(#(
            next_store,
            next_identity,
            group,
            inventory_adjustment_staged_ids(group),
          ))
        }
      }
    }
  }
}

fn validate_inventory_adjust_inputs(
  name: String,
  changes: List(InventoryAdjustmentChangeInput),
) -> List(ProductUserError) {
  let name_errors = case valid_inventory_adjust_quantity_name(name) {
    True -> []
    False -> [invalid_inventory_adjust_quantity_name_error(["input", "name"])]
  }
  let ledger_errors = case name {
    "available" -> []
    _ ->
      changes
      |> enumerate_items()
      |> list.filter_map(fn(pair) {
        let #(change, index) = pair
        case change.ledger_document_uri {
          Some(_) -> Error(Nil)
          None ->
            Ok(ProductUserError(
              ["input", "changes", int.to_string(index), "ledgerDocumentUri"],
              "A ledger document URI is required except when adjusting available.",
              None,
            ))
        }
      })
  }
  let quantity_errors =
    changes
    |> enumerate_items()
    |> list.flat_map(fn(pair) {
      let #(change, index) = pair
      case change.delta {
        Some(delta) ->
          inventory_quantity_bounds_errors(delta, [
            "input",
            "changes",
            int.to_string(index),
            "delta",
          ])
        None -> []
      }
    })
  list.append(name_errors, list.append(ledger_errors, quantity_errors))
}

fn stage_inventory_quantity_adjust(
  store: Store,
  name: String,
  change: InventoryAdjustmentChangeInput,
  index: Int,
  require_change_from_quantity: Bool,
) -> Result(
  #(Store, InventoryAdjustmentChange, List(InventoryAdjustmentChange)),
  ProductUserError,
) {
  let path = ["input", "changes", int.to_string(index)]
  case change.inventory_item_id, change.location_id, change.delta {
    None, _, _ ->
      Error(ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ))
    _, None, _ ->
      Error(ProductUserError(
        list.append(path, ["locationId"]),
        "Inventory location id is required",
        None,
      ))
    _, _, None ->
      Error(ProductUserError(
        list.append(path, ["delta"]),
        "Inventory delta is required",
        None,
      ))
    Some(inventory_item_id), Some(location_id), Some(delta) -> {
      case
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      {
        None ->
          Error(ProductUserError(
            list.append(path, ["inventoryItemId"]),
            "The specified inventory item could not be found.",
            None,
          ))
        Some(variant) -> {
          let current_levels = variant_inventory_levels(variant)
          case find_inventory_level(current_levels, location_id) {
            None ->
              Error(ProductUserError(
                list.append(path, ["locationId"]),
                "The specified location could not be found.",
                None,
              ))
            Some(level) -> {
              let previous = inventory_quantity_amount(level.quantities, name)
              case
                require_change_from_quantity
                && change.change_from_quantity != Some(previous)
              {
                True ->
                  Error(ProductUserError(
                    list.append(path, ["changeFromQuantity"]),
                    "The specified compare quantity does not match the current quantity.",
                    None,
                  ))
                False -> {
                  let quantities =
                    level.quantities
                    |> add_inventory_quantity_amount(name, delta)
                    |> maybe_add_on_hand_component_delta(name, delta)
                  let next_store =
                    stage_variant_inventory_levels(
                      store,
                      variant,
                      replace_inventory_level(
                        current_levels,
                        location_id,
                        InventoryLevelRecord(..level, quantities: quantities),
                      ),
                    )
                  let adjusted =
                    InventoryAdjustmentChange(
                      inventory_item_id: inventory_item_id,
                      location_id: location_id,
                      name: name,
                      delta: delta,
                      quantity_after_change: None,
                      ledger_document_uri: change.ledger_document_uri,
                    )
                  let mirrored = case is_on_hand_component_quantity_name(name) {
                    True -> [
                      InventoryAdjustmentChange(
                        inventory_item_id: inventory_item_id,
                        location_id: location_id,
                        name: "on_hand",
                        delta: delta,
                        quantity_after_change: None,
                        ledger_document_uri: None,
                      ),
                    ]
                    False -> []
                  }
                  Ok(#(next_store, adjusted, mirrored))
                }
              }
            }
          }
        }
      }
    }
  }
}

fn validate_inventory_set_quantity(
  quantity: InventorySetQuantityInput,
  index: Int,
) -> List(ProductUserError) {
  let path = ["input", "quantities", int.to_string(index)]
  let required_errors = case
    quantity.inventory_item_id,
    quantity.location_id,
    quantity.quantity
  {
    None, _, _ -> [
      ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ),
    ]
    _, None, _ -> [
      ProductUserError(
        list.append(path, ["locationId"]),
        "Inventory location id is required",
        None,
      ),
    ]
    _, _, None -> [
      ProductUserError(
        list.append(path, ["quantity"]),
        "Inventory quantity is required",
        None,
      ),
    ]
    _, _, _ -> []
  }
  let quantity_errors = case quantity.quantity {
    Some(quantity) -> inventory_set_quantity_bounds_errors(quantity, path)
    None -> []
  }
  list.append(required_errors, quantity_errors)
}

fn inventory_set_quantity_changes(
  inventory_item_id: String,
  location_id: String,
  name: String,
  delta: Int,
  change: InventoryAdjustmentChange,
) -> #(List(InventoryAdjustmentChange), List(InventoryAdjustmentChange)) {
  case name {
    "on_hand" -> #(
      [
        InventoryAdjustmentChange(
          inventory_item_id: inventory_item_id,
          location_id: location_id,
          name: "available",
          delta: delta,
          quantity_after_change: None,
          ledger_document_uri: None,
        ),
      ],
      [change],
    )
    _ -> {
      let mirrored = case is_on_hand_component_quantity_name(name) {
        True -> [
          InventoryAdjustmentChange(
            inventory_item_id: inventory_item_id,
            location_id: location_id,
            name: "on_hand",
            delta: delta,
            quantity_after_change: None,
            ledger_document_uri: None,
          ),
        ]
        False -> []
      }
      #([change], mirrored)
    }
  }
}

fn validate_inventory_set_quantity_inputs(
  quantities: List(InventorySetQuantityInput),
) -> List(ProductUserError) {
  let input_errors =
    quantities
    |> enumerate_items()
    |> list.flat_map(fn(pair) {
      let #(quantity, index) = pair
      validate_inventory_set_quantity(quantity, index)
    })
  list.append(input_errors, duplicate_inventory_set_quantity_errors(quantities))
}

fn inventory_set_quantity_bounds_errors(
  quantity: Int,
  path: List(String),
) -> List(ProductUserError) {
  case quantity {
    quantity if quantity > max_inventory_quantity -> [
      ProductUserError(
        list.append(path, ["quantity"]),
        "The quantity can't be higher than 1,000,000,000.",
        Some("INVALID_QUANTITY_TOO_HIGH"),
      ),
    ]
    quantity if quantity < min_inventory_quantity -> [
      ProductUserError(
        list.append(path, ["quantity"]),
        "The quantity can't be lower than -1,000,000,000.",
        Some("INVALID_QUANTITY_TOO_LOW"),
      ),
    ]
    quantity if quantity < 0 -> [
      ProductUserError(
        list.append(path, ["quantity"]),
        "The quantity can't be negative.",
        Some("INVALID_QUANTITY_NEGATIVE"),
      ),
    ]
    _ -> []
  }
}

fn inventory_quantity_bounds_errors(
  quantity: Int,
  path: List(String),
) -> List(ProductUserError) {
  case quantity {
    quantity if quantity > max_inventory_quantity -> [
      ProductUserError(
        path,
        "The quantity can't be higher than 1,000,000,000.",
        Some("INVALID_QUANTITY_TOO_HIGH"),
      ),
    ]
    quantity if quantity < min_inventory_quantity -> [
      ProductUserError(
        path,
        "The quantity can't be lower than -1,000,000,000.",
        Some("INVALID_QUANTITY_TOO_LOW"),
      ),
    ]
    _ -> []
  }
}

fn duplicate_inventory_set_quantity_errors(
  quantities: List(InventorySetQuantityInput),
) -> List(ProductUserError) {
  quantities
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(quantity, index) = pair
    case quantity.inventory_item_id, quantity.location_id {
      Some(inventory_item_id), Some(location_id) -> {
        case
          has_duplicate_inventory_item_location_pair(
            quantities,
            index,
            inventory_item_id,
            location_id,
          )
        {
          True -> [
            ProductUserError(
              ["input", "quantities", int.to_string(index), "locationId"],
              "The combination of inventoryItemId and locationId must be unique.",
              Some("NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR"),
            ),
          ]
          False -> []
        }
      }
      _, _ -> []
    }
  })
}

fn has_duplicate_inventory_item_location_pair(
  quantities: List(InventorySetQuantityInput),
  index: Int,
  inventory_item_id: String,
  location_id: String,
) -> Bool {
  quantities
  |> enumerate_items()
  |> list.any(fn(pair) {
    let #(quantity, other_index) = pair
    other_index != index
    && quantity.inventory_item_id == Some(inventory_item_id)
    && quantity.location_id == Some(location_id)
  })
}

fn stage_inventory_quantity_set(
  store: Store,
  inventory_item_id: String,
  location_id: String,
  name: String,
  next_quantity: Int,
  ignore_compare_quantity: Bool,
  compare_quantity: Option(Int),
  use_change_from_quantity: Bool,
  index: Int,
) -> Result(#(Store, Int), ProductUserError) {
  case
    store.find_effective_variant_by_inventory_item_id(store, inventory_item_id)
  {
    None ->
      Error(ProductUserError(
        ["input", "quantities", int.to_string(index), "inventoryItemId"],
        "The specified inventory item could not be found.",
        None,
      ))
    Some(variant) -> {
      let current_levels = variant_inventory_levels(variant)
      case find_inventory_level(current_levels, location_id) {
        None ->
          Error(ProductUserError(
            ["input", "quantities", int.to_string(index), "locationId"],
            "The specified location could not be found.",
            None,
          ))
        Some(level) -> {
          let previous = inventory_quantity_amount(level.quantities, name)
          case !ignore_compare_quantity && compare_quantity != Some(previous) {
            True ->
              Error(ProductUserError(
                [
                  "input",
                  "quantities",
                  int.to_string(index),
                  inventory_compare_field_name(use_change_from_quantity),
                ],
                "The specified compare quantity does not match the current quantity.",
                None,
              ))
            False -> {
              let delta = next_quantity - previous
              let quantities =
                write_inventory_quantity_amount(
                  level.quantities,
                  name,
                  next_quantity,
                )
                |> maybe_add_on_hand_component_delta(name, delta)
                |> maybe_add_available_for_on_hand_delta(name, delta)
              let next_store =
                stage_variant_inventory_levels(
                  store,
                  variant,
                  replace_inventory_level(
                    current_levels,
                    location_id,
                    InventoryLevelRecord(..level, quantities: quantities),
                  ),
                )
              Ok(#(next_store, delta))
            }
          }
        }
      }
    }
  }
}

fn quantity_compare_quantity(
  quantity: InventorySetQuantityInput,
  use_change_from_quantity: Bool,
) -> Option(Int) {
  case use_change_from_quantity {
    True -> quantity.change_from_quantity
    False -> quantity.compare_quantity
  }
}

fn inventory_compare_field_name(use_change_from_quantity: Bool) -> String {
  case use_change_from_quantity {
    True -> "changeFromQuantity"
    False -> "compareQuantity"
  }
}

fn apply_inventory_move_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  reason: String,
  changes: List(InventoryMoveQuantityInput),
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  let result =
    changes
    |> enumerate_items()
    |> list.try_fold(#([], store), fn(acc, pair) {
      let #(change, index) = pair
      let #(adjustment_changes, current_store) = acc
      case stage_inventory_quantity_move(current_store, change, index) {
        Error(error) -> Error([error])
        Ok(applied) -> {
          let #(next_store, from_change, to_change) = applied
          Ok(#(
            list.append(adjustment_changes, [from_change, to_change]),
            next_store,
          ))
        }
      }
    })
  case result {
    Error(errors) -> Error(errors)
    Ok(done) -> {
      let #(adjustment_changes, next_store) = done
      let #(group, next_identity) =
        make_inventory_adjustment_group(
          identity,
          reason,
          reference_document_uri,
          adjustment_changes,
        )
      Ok(#(
        next_store,
        next_identity,
        group,
        inventory_adjustment_staged_ids(group),
      ))
    }
  }
}

fn validate_inventory_move_inputs(
  changes: List(InventoryMoveQuantityInput),
) -> List(ProductUserError) {
  changes
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(change, index) = pair
    validate_inventory_move_input(change, index)
  })
}

fn validate_inventory_move_input(
  change: InventoryMoveQuantityInput,
  index: Int,
) -> List(ProductUserError) {
  let path = ["input", "changes", int.to_string(index)]
  let name_errors =
    list.filter_map(
      [
        #(change.from.name, list.append(path, ["from", "name"])),
        #(change.to.name, list.append(path, ["to", "name"])),
      ],
      fn(candidate) {
        let #(name, field_path) = candidate
        case name {
          Some(name) ->
            case valid_staged_inventory_quantity_name(name) {
              True -> Error(Nil)
              False -> Ok(invalid_inventory_quantity_name_error(field_path))
            }
          None -> Error(Nil)
        }
      },
    )
  let location_error = case change.from.location_id, change.to.location_id {
    Some(from), Some(to) if from != to -> [
      ProductUserError(
        path,
        "The quantities can't be moved between different locations.",
        None,
      ),
    ]
    _, _ -> []
  }
  let same_name_error = case change.from.name, change.to.name {
    Some(from), Some(to) if from == to -> [
      ProductUserError(
        path,
        "The quantity names for each change can't be the same.",
        None,
      ),
    ]
    _, _ -> []
  }
  let ledger_errors =
    list.append(
      validate_inventory_move_ledger_document_uri(
        change.from.name,
        change.from.ledger_document_uri,
        list.append(path, ["from", "ledgerDocumentUri"]),
      ),
      validate_inventory_move_ledger_document_uri(
        change.to.name,
        change.to.ledger_document_uri,
        list.append(path, ["to", "ledgerDocumentUri"]),
      ),
    )
  list.append(
    name_errors,
    list.append(location_error, list.append(same_name_error, ledger_errors)),
  )
}

fn validate_inventory_move_ledger_document_uri(
  quantity_name: Option(String),
  ledger_document_uri: Option(String),
  path: List(String),
) -> List(ProductUserError) {
  case quantity_name, ledger_document_uri {
    Some("available"), Some(_) -> [
      ProductUserError(
        path,
        "A ledger document URI is not allowed when adjusting available.",
        None,
      ),
    ]
    Some(name), None if name != "available" -> [
      ProductUserError(
        path,
        "A ledger document URI is required except when adjusting available.",
        None,
      ),
    ]
    _, _ -> []
  }
}

fn stage_inventory_quantity_move(
  store: Store,
  change: InventoryMoveQuantityInput,
  index: Int,
) -> Result(
  #(Store, InventoryAdjustmentChange, InventoryAdjustmentChange),
  ProductUserError,
) {
  let path = ["input", "changes", int.to_string(index)]
  case
    change.inventory_item_id,
    change.quantity,
    change.from.location_id,
    change.from.name,
    change.to.location_id,
    change.to.name
  {
    None, _, _, _, _, _ ->
      Error(ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ))
    _, None, _, _, _, _ ->
      Error(ProductUserError(
        list.append(path, ["quantity"]),
        "Inventory move quantity is required",
        None,
      ))
    _, _, None, _, _, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, None, _, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, _, None, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, _, _, None ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    Some(inventory_item_id),
      Some(quantity),
      Some(location_id),
      Some(from_name),
      _,
      Some(to_name)
    -> {
      case
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      {
        None ->
          Error(ProductUserError(
            list.append(path, ["inventoryItemId"]),
            "The specified inventory item could not be found.",
            None,
          ))
        Some(variant) -> {
          let current_levels = variant_inventory_levels(variant)
          case find_inventory_level(current_levels, location_id) {
            None ->
              Error(ProductUserError(
                list.append(path, ["from", "locationId"]),
                "The specified inventory item is not stocked at the location.",
                None,
              ))
            Some(level) -> {
              let quantities =
                level.quantities
                |> add_inventory_quantity_amount(from_name, 0 - quantity)
                |> add_inventory_quantity_amount(to_name, quantity)
                |> add_on_hand_move_delta(from_name, to_name, quantity)
              let next_store =
                stage_variant_inventory_levels(
                  store,
                  variant,
                  replace_inventory_level(
                    current_levels,
                    location_id,
                    InventoryLevelRecord(..level, quantities: quantities),
                  ),
                )
              Ok(#(
                next_store,
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: from_name,
                  delta: 0 - quantity,
                  quantity_after_change: None,
                  ledger_document_uri: change.from.ledger_document_uri,
                ),
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: to_name,
                  delta: quantity,
                  quantity_after_change: None,
                  ledger_document_uri: change.to.ledger_document_uri,
                ),
              ))
            }
          }
        }
      }
    }
  }
}

fn make_inventory_adjustment_group(
  identity: SyntheticIdentityRegistry,
  reason: String,
  reference_document_uri: Option(String),
  changes: List(InventoryAdjustmentChange),
) -> #(InventoryAdjustmentGroup, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "InventoryAdjustmentGroup")
  let #(created_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  #(
    InventoryAdjustmentGroup(
      id: id,
      created_at: created_at,
      reason: reason,
      reference_document_uri: reference_document_uri,
      changes: changes,
    ),
    next_identity,
  )
}

fn inventory_adjustment_staged_ids(
  group: InventoryAdjustmentGroup,
) -> List(String) {
  [
    group.id,
    ..dedupe_preserving_order(
      list.map(group.changes, fn(change) { change.inventory_item_id }),
    )
  ]
}

fn inventory_activate_staged_ids(
  resolved: Option(#(ProductVariantRecord, InventoryLevelRecord)),
) -> List(String) {
  case resolved {
    Some(#(variant, level)) ->
      case variant.inventory_item {
        Some(item) -> [level.id, item.id]
        None -> [level.id]
      }
    None -> []
  }
}

fn variant_inventory_levels(
  variant: ProductVariantRecord,
) -> List(InventoryLevelRecord) {
  case variant.inventory_item {
    Some(item) -> item.inventory_levels
    None -> []
  }
}

fn inventory_level_is_active(level: InventoryLevelRecord) -> Bool {
  case level.is_active {
    Some(False) -> False
    _ -> True
  }
}

fn active_inventory_levels(
  levels: List(InventoryLevelRecord),
) -> List(InventoryLevelRecord) {
  list.filter(levels, inventory_level_is_active)
}

fn filter_inventory_levels_by_include_inactive(
  levels: List(InventoryLevelRecord),
  include_inactive: Bool,
) -> List(InventoryLevelRecord) {
  case include_inactive {
    True -> levels
    False -> active_inventory_levels(levels)
  }
}

fn find_inventory_level(
  levels: List(InventoryLevelRecord),
  location_id: String,
) -> Option(InventoryLevelRecord) {
  levels
  |> list.find(fn(level) { level.location.id == location_id })
  |> option.from_result
}

fn find_inventory_level_target(
  store: Store,
  inventory_level_id: String,
) -> Option(#(ProductVariantRecord, InventoryLevelRecord)) {
  store.list_effective_product_variants(store)
  |> list.filter_map(fn(variant) {
    case
      list.find(variant_inventory_levels(variant), fn(level) {
        level.id == inventory_level_id
      })
    {
      Ok(level) -> Ok(#(variant, level))
      Error(_) -> Error(Nil)
    }
  })
  |> list.first
  |> option.from_result
}

fn replace_inventory_level(
  levels: List(InventoryLevelRecord),
  location_id: String,
  next_level: InventoryLevelRecord,
) -> List(InventoryLevelRecord) {
  list.map(levels, fn(level) {
    case level.location.id == location_id {
      True -> next_level
      False -> level
    }
  })
}

fn stage_variant_inventory_levels(
  store: Store,
  variant: ProductVariantRecord,
  next_levels: List(InventoryLevelRecord),
) -> Store {
  let next_variant = variant_with_inventory_levels(variant, next_levels)
  let next_variants =
    store.get_effective_variants_by_product_id(store, variant.product_id)
    |> list.map(fn(candidate) {
      case candidate.id == variant.id {
        True -> next_variant
        False -> candidate
      }
    })
  store.replace_staged_variants_for_product(
    store,
    variant.product_id,
    next_variants,
  )
}

fn variant_with_inventory_levels(
  variant: ProductVariantRecord,
  next_levels: List(InventoryLevelRecord),
) -> ProductVariantRecord {
  ProductVariantRecord(
    ..variant,
    inventory_quantity: sum_inventory_level_available(next_levels),
    inventory_item: option.map(variant.inventory_item, fn(item) {
      InventoryItemRecord(..item, inventory_levels: next_levels)
    }),
  )
}

fn sum_inventory_level_available(
  levels: List(InventoryLevelRecord),
) -> Option(Int) {
  Some(
    levels
    |> active_inventory_levels
    |> list.fold(0, fn(total, level) {
      total + inventory_quantity_amount(level.quantities, "available")
    }),
  )
}

fn inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
) -> Int {
  case list.find(quantities, fn(quantity) { quantity.name == name }) {
    Ok(quantity) -> quantity.quantity
    Error(_) -> 0
  }
}

fn write_inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True -> InventoryQuantityRecord(..quantity, quantity: amount)
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(name: name, quantity: amount, updated_at: None),
      ])
  }
}

fn add_inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
  delta: Int,
) -> List(InventoryQuantityRecord) {
  write_inventory_quantity_amount(
    quantities,
    name,
    inventory_quantity_amount(quantities, name) + delta,
  )
}

fn maybe_add_on_hand_component_delta(
  quantities: List(InventoryQuantityRecord),
  name: String,
  delta: Int,
) -> List(InventoryQuantityRecord) {
  case is_on_hand_component_quantity_name(name) {
    True -> add_inventory_quantity_amount(quantities, "on_hand", delta)
    False -> quantities
  }
}

fn maybe_add_available_for_on_hand_delta(
  quantities: List(InventoryQuantityRecord),
  name: String,
  delta: Int,
) -> List(InventoryQuantityRecord) {
  case name {
    "on_hand" -> add_inventory_quantity_amount(quantities, "available", delta)
    _ -> quantities
  }
}

fn add_on_hand_move_delta(
  quantities: List(InventoryQuantityRecord),
  from_name: String,
  to_name: String,
  quantity: Int,
) -> List(InventoryQuantityRecord) {
  let delta =
    on_hand_component_delta(from_name, 0 - quantity)
    + on_hand_component_delta(to_name, quantity)
  case delta == 0 {
    True -> quantities
    False -> add_inventory_quantity_amount(quantities, "on_hand", delta)
  }
}

fn on_hand_component_delta(name: String, delta: Int) -> Int {
  case is_on_hand_component_quantity_name(name) {
    True -> delta
    False -> 0
  }
}

fn is_on_hand_component_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "committed"
    | "damaged"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

fn valid_inventory_set_quantity_name(name: String) -> Bool {
  case name {
    "available" | "on_hand" -> True
    _ -> False
  }
}

fn valid_staged_inventory_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "committed"
    | "damaged"
    | "incoming"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

fn valid_inventory_adjust_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "on_hand"
    | "committed"
    | "damaged"
    | "incoming"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

fn invalid_inventory_set_quantity_name_error() -> ProductUserError {
  ProductUserError(
    ["input", "name"],
    "The quantity name must be either 'available' or 'on_hand'.",
    Some("INVALID_NAME"),
  )
}

fn invalid_inventory_adjust_quantity_name_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The specified quantity name is invalid. Valid values are: available, on_hand, committed, damaged, incoming, quality_control, reserved, safety_stock.",
    Some("INVALID_QUANTITY_NAME"),
  )
}

fn invalid_inventory_quantity_name_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.",
    None,
  )
}

fn inventory_change_location(
  store: Store,
  change: InventoryAdjustmentChange,
) -> InventoryLocationRecord {
  case
    store.find_effective_variant_by_inventory_item_id(
      store,
      change.inventory_item_id,
    )
  {
    Some(variant) ->
      case
        find_inventory_level(
          variant_inventory_levels(variant),
          change.location_id,
        )
      {
        Some(level) -> level.location
        None -> InventoryLocationRecord(id: change.location_id, name: "")
      }
    None -> InventoryLocationRecord(id: change.location_id, name: "")
  }
}

fn read_object_field(
  fields: Dict(String, ResolvedValue),
  name: String,
) -> Option(Dict(String, ResolvedValue)) {
  case dict.get(fields, name) {
    Ok(ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_bool_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(input, name) {
    Ok(BoolVal(value)) -> Some(value)
    _ -> None
  }
}

fn variant_title_with_fallback(
  selected_options: List(ProductVariantSelectedOptionRecord),
  fallback: String,
) -> String {
  case selected_options {
    [] -> fallback
    _ -> variant_title(selected_options)
  }
}

fn variant_staged_ids(variant: ProductVariantRecord) -> List(String) {
  case variant.inventory_item {
    Some(item) -> [variant.id, item.id]
    None -> [variant.id]
  }
}

fn sync_product_inventory_summary(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
) -> #(Option(ProductRecord), Store, SyntheticIdentityRegistry) {
  case store.get_effective_product_by_id(store, product_id) {
    None -> #(None, store, identity)
    Some(product) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let variants =
        store.get_effective_variants_by_product_id(store, product_id)
      let next_product =
        ProductRecord(
          ..product,
          total_inventory: sum_variant_inventory(variants),
          tracks_inventory: derive_tracks_inventory(variants),
          updated_at: Some(updated_at),
        )
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      #(Some(next_product), next_store, next_identity)
    }
  }
}

fn sum_variant_inventory(variants: List(ProductVariantRecord)) -> Option(Int) {
  let quantities =
    list.filter_map(variants, fn(variant) {
      case variant.inventory_quantity {
        Some(quantity) -> Ok(quantity)
        None -> Error(Nil)
      }
    })
  case quantities {
    [] -> None
    _ ->
      Some(list.fold(quantities, 0, fn(total, quantity) { total + quantity }))
  }
}

fn derive_tracks_inventory(
  variants: List(ProductVariantRecord),
) -> Option(Bool) {
  let tracked_values =
    list.filter_map(variants, fn(variant) {
      case variant.inventory_item {
        Some(item) ->
          case item.tracked {
            Some(tracked) -> Ok(tracked)
            None -> Error(Nil)
          }
        None -> Error(Nil)
      }
    })
  case tracked_values {
    [] ->
      case
        list.any(variants, fn(variant) { variant.inventory_quantity != None })
      {
        True -> Some(True)
        False -> None
      }
    _ -> Some(list.any(tracked_values, fn(tracked) { tracked }))
  }
}

fn read_non_empty_string_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case read_string_field(input, name) {
    Some(value) ->
      case string.length(string.trim(value)) > 0 {
        True -> Some(value)
        False -> None
      }
    None -> None
  }
}

fn read_product_status_field(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_string_field(input, "status") {
    Some("ACTIVE") -> Some("ACTIVE")
    Some("ARCHIVED") -> Some("ARCHIVED")
    Some("DRAFT") -> Some("DRAFT")
    _ -> None
  }
}

fn read_string_list_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(List(String)) {
  case dict.get(input, name) {
    Ok(ListVal(values)) ->
      Some(
        list.filter_map(values, fn(value) {
          case value {
            StringVal(item) -> Ok(item)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

fn read_object_list_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> List(Dict(String, ResolvedValue)) {
  case dict.get(input, name) {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn updated_product_seo(
  current: ProductSeoRecord,
  input: Dict(String, ResolvedValue),
) -> ProductSeoRecord {
  case dict.get(input, "seo") {
    Ok(ObjectVal(seo)) ->
      ProductSeoRecord(
        title: read_string_field(seo, "title") |> option.or(current.title),
        description: read_string_field(seo, "description")
          |> option.or(current.description),
      )
    _ -> current
  }
}

fn read_option_create_inputs(
  args: Dict(String, ResolvedValue),
) -> List(Dict(String, ResolvedValue)) {
  case dict.get(args, "options") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(input) -> Ok(input)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn find_product_option(
  options: List(ProductOptionRecord),
  product_id: String,
  option_id: String,
) -> Option(ProductOptionRecord) {
  options
  |> list.find(fn(option) {
    option.id == option_id && option.product_id == product_id
  })
  |> option.from_result
}

type RenamedOptionValue =
  #(String, String)

fn update_product_option_record(
  identity: SyntheticIdentityRegistry,
  option: ProductOptionRecord,
  input: Dict(String, ResolvedValue),
  values_to_add: List(Dict(String, ResolvedValue)),
  values_to_update: List(Dict(String, ResolvedValue)),
  value_ids_to_delete: List(String),
) -> #(
  ProductOptionRecord,
  List(RenamedOptionValue),
  SyntheticIdentityRegistry,
  List(String),
) {
  let next_name =
    read_string_field(input, "name")
    |> option.unwrap(option.name)
  let #(updated_values, renamed_values) =
    option.option_values
    |> list.filter(fn(value) { !list.contains(value_ids_to_delete, value.id) })
    |> update_option_values(values_to_update)
  let #(created_values, final_identity) =
    make_created_option_value_records(
      identity,
      read_option_value_create_names(values_to_add),
    )
  let next_option =
    ProductOptionRecord(
      ..option,
      name: next_name,
      option_values: list.append(updated_values, created_values),
    )
  #(
    next_option,
    renamed_values,
    final_identity,
    list.map(created_values, fn(value) { value.id }),
  )
}

fn update_option_values(
  values: List(ProductOptionValueRecord),
  updates: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionValueRecord), List(RenamedOptionValue)) {
  let #(reversed_values, reversed_renames) =
    list.fold(values, #([], []), fn(acc, value) {
      let #(next_values, renames) = acc
      case find_option_value_update(updates, value.id) {
        Some(name) -> #(
          [ProductOptionValueRecord(..value, name: name), ..next_values],
          [#(value.name, name), ..renames],
        )
        None -> #([value, ..next_values], renames)
      }
    })
  #(list.reverse(reversed_values), list.reverse(reversed_renames))
}

fn find_option_value_update(
  updates: List(Dict(String, ResolvedValue)),
  value_id: String,
) -> Option(String) {
  updates
  |> list.find_map(fn(update) {
    case read_string_field(update, "id"), read_string_field(update, "name") {
      Some(id), Some(name) if id == value_id -> Ok(name)
      _, _ -> Error(Nil)
    }
  })
  |> option.from_result
}

fn read_option_value_create_names(
  inputs: List(Dict(String, ResolvedValue)),
) -> List(String) {
  inputs
  |> list.filter_map(fn(input) {
    case read_string_field(input, "name") {
      Some(name) -> Ok(name)
      None -> Error(Nil)
    }
  })
}

fn insert_option_at_position(
  options: List(ProductOptionRecord),
  option: ProductOptionRecord,
  position: Option(Int),
) -> List(ProductOptionRecord) {
  let insertion_index = case position {
    Some(position) if position > 0 ->
      int.min(position, list.length(options) + 1) - 1
    _ -> list.length(options)
  }
  let before = list.take(options, insertion_index)
  let after = list.drop(options, insertion_index)
  list.append(before, [option, ..after])
  |> position_options(1, [])
}

fn remap_variant_selections_for_option_update(
  variants: List(ProductVariantRecord),
  previous_option_name: String,
  next_option_name: String,
  renamed_values: List(RenamedOptionValue),
) -> List(ProductVariantRecord) {
  list.map(variants, fn(variant) {
    let selected_options =
      list.map(variant.selected_options, fn(selected) {
        case selected.name == previous_option_name {
          True ->
            ProductVariantSelectedOptionRecord(
              name: next_option_name,
              value: renamed_value_name(renamed_values, selected.value),
            )
          False -> selected
        }
      })
    ProductVariantRecord(
      ..variant,
      title: variant_title(selected_options),
      selected_options: selected_options,
    )
  })
}

fn renamed_value_name(
  renamed_values: List(RenamedOptionValue),
  current_name: String,
) -> String {
  case renamed_values {
    [] -> current_name
    [#(from, to), ..rest] ->
      case from == current_name {
        True -> to
        False -> renamed_value_name(rest, current_name)
      }
  }
}

fn reorder_variant_selections_for_options(
  variants: List(ProductVariantRecord),
  options: List(ProductOptionRecord),
) -> List(ProductVariantRecord) {
  list.map(variants, fn(variant) {
    let selected_options =
      options
      |> list.filter_map(fn(option) {
        find_selected_option(variant.selected_options, option.name)
      })
    ProductVariantRecord(
      ..variant,
      title: variant_title(selected_options),
      selected_options: selected_options,
    )
  })
}

fn find_selected_option(
  selected_options: List(ProductVariantSelectedOptionRecord),
  name: String,
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  selected_options
  |> list.find(fn(selected) { selected.name == name })
}

fn unknown_option_errors(
  option_ids: List(String),
  existing_ids: List(String),
) -> List(ProductUserError) {
  option_ids
  |> list.index_map(fn(option_id, index) {
    case list.contains(existing_ids, option_id) {
      True -> None
      False ->
        Some(ProductUserError(
          ["options", int.to_string(index)],
          "Option does not exist",
          None,
        ))
    }
  })
  |> list.filter_map(fn(error) {
    case error {
      Some(error) -> Ok(error)
      None -> Error(Nil)
    }
  })
}

fn reorder_product_options(
  options: List(ProductOptionRecord),
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionRecord), List(ProductUserError)) {
  let #(remaining, reversed_reordered, reversed_errors, _) =
    list.fold(inputs, #(options, [], [], 0), fn(acc, input) {
      let #(current_remaining, reordered, errors, index) = acc
      let #(matched, next_remaining) =
        take_matching_option(current_remaining, input)
      case matched {
        Some(option) -> #(
          next_remaining,
          [option, ..reordered],
          errors,
          index + 1,
        )
        None -> #(
          current_remaining,
          reordered,
          [
            ProductUserError(
              ["options", int.to_string(index)],
              "Option does not exist",
              None,
            ),
            ..errors
          ],
          index + 1,
        )
      }
    })
  let next_options =
    list.append(list.reverse(reversed_reordered), remaining)
    |> position_options(1, [])
  #(next_options, list.reverse(reversed_errors))
}

fn take_matching_option(
  options: List(ProductOptionRecord),
  input: Dict(String, ResolvedValue),
) -> #(Option(ProductOptionRecord), List(ProductOptionRecord)) {
  let option_id = read_string_field(input, "id")
  let option_name = read_string_field(input, "name")
  take_matching_option_loop(options, option_id, option_name, [])
}

fn take_matching_option_loop(
  options: List(ProductOptionRecord),
  option_id: Option(String),
  option_name: Option(String),
  reversed_before: List(ProductOptionRecord),
) -> #(Option(ProductOptionRecord), List(ProductOptionRecord)) {
  case options {
    [] -> #(None, list.reverse(reversed_before))
    [option, ..rest] -> {
      let matches_id = case option_id {
        Some(id) -> option.id == id
        None -> False
      }
      let matches_name = case option_name {
        Some(name) -> option.name == name
        None -> False
      }
      case matches_id || matches_name {
        True -> #(
          Some(option),
          list.append(list.reverse(reversed_before), rest),
        )
        False ->
          take_matching_option_loop(rest, option_id, option_name, [
            option,
            ..reversed_before
          ])
      }
    }
  }
}

fn restore_default_option_state(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  variants: List(ProductVariantRecord),
) -> #(
  List(ProductOptionRecord),
  List(ProductVariantRecord),
  SyntheticIdentityRegistry,
  List(String),
) {
  let #(option_id, identity_after_option) =
    synthetic_identity.make_synthetic_gid(identity, "ProductOption")
  let #(value_id, final_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_option,
      "ProductOptionValue",
    )
  let default_option =
    ProductOptionRecord(
      id: option_id,
      product_id: product.id,
      name: "Title",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: value_id,
          name: "Default Title",
          has_variants: True,
        ),
      ],
    )
  let next_variants = case variants {
    [variant, ..] -> [
      ProductVariantRecord(
        ..variant,
        product_id: product.id,
        title: "Default Title",
        selected_options: [
          ProductVariantSelectedOptionRecord(
            name: "Title",
            value: "Default Title",
          ),
        ],
      ),
    ]
    [] -> []
  }
  #([default_option], next_variants, final_identity, [option_id, value_id])
}

fn make_created_option_records(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(records, current_identity) = acc
      let #(option, next_identity) =
        make_created_option_record(current_identity, product_id, input)
      #([option, ..records], next_identity)
    })
  #(list.reverse(reversed), final_identity)
}

fn make_created_option_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
) -> #(ProductOptionRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "ProductOption")
  let #(values, final_identity) =
    make_created_option_value_records(
      identity_after_id,
      read_option_value_names(input),
    )
  #(
    ProductOptionRecord(
      id: id,
      product_id: product_id,
      name: read_string_field(input, "name") |> option.unwrap(""),
      position: read_int_field(input, "position") |> option.unwrap(9999),
      option_values: values,
    ),
    final_identity,
  )
}

fn make_created_option_value_records(
  identity: SyntheticIdentityRegistry,
  names: List(String),
) -> #(List(ProductOptionValueRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(names, #([], identity), fn(acc, name) {
      let #(records, current_identity) = acc
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "ProductOptionValue",
        )
      #(
        [
          ProductOptionValueRecord(id: id, name: name, has_variants: False),
          ..records
        ],
        next_identity,
      )
    })
  #(list.reverse(reversed), final_identity)
}

fn read_option_value_names(input: Dict(String, ResolvedValue)) -> List(String) {
  case dict.get(input, "values") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            case read_string_field(fields, "name") {
              Some(name) -> Ok(name)
              None -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_int_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(Int) {
  case dict.get(input, name) {
    Ok(IntVal(value)) -> Some(value)
    _ -> None
  }
}

fn product_uses_only_default_option_state(
  options: List(ProductOptionRecord),
  variants: List(ProductVariantRecord),
) -> Bool {
  case options, variants {
    [option], [variant] ->
      option.name == "Title"
      && option_values_are_default(option.option_values)
      && variant.selected_options
      == [
        ProductVariantSelectedOptionRecord(
          name: "Title",
          value: "Default Title",
        ),
      ]
    _, _ -> False
  }
}

fn product_has_standalone_default_variant(
  options: List(ProductOptionRecord),
  variants: List(ProductVariantRecord),
) -> Bool {
  case variants {
    [variant] ->
      product_uses_only_default_option_state(options, variants)
      && variant.title == "Default Title"
    _ -> False
  }
}

fn option_values_are_default(values: List(ProductOptionValueRecord)) -> Bool {
  case values {
    [value] -> value.name == "Default Title"
    _ -> False
  }
}

fn sort_and_position_options(
  options: List(ProductOptionRecord),
) -> List(ProductOptionRecord) {
  options
  |> list.sort(fn(left, right) {
    case int.compare(left.position, right.position) {
      order.Eq -> string.compare(left.id, right.id)
      other -> other
    }
  })
  |> position_options(1, [])
}

fn position_options(
  options: List(ProductOptionRecord),
  position: Int,
  acc: List(ProductOptionRecord),
) -> List(ProductOptionRecord) {
  case options {
    [] -> list.reverse(acc)
    [option, ..rest] ->
      position_options(rest, position + 1, [
        ProductOptionRecord(..option, position: position),
        ..acc
      ])
  }
}

fn remap_variant_to_first_option_values(
  variant: ProductVariantRecord,
  options: List(ProductOptionRecord),
) -> ProductVariantRecord {
  let selected_options =
    list.map(options, fn(option) {
      ProductVariantSelectedOptionRecord(
        name: option.name,
        value: first_option_value_name(option),
      )
    })
  ProductVariantRecord(
    ..variant,
    title: variant_title(selected_options),
    selected_options: selected_options,
  )
}

fn map_variants_to_first_new_option_values(
  variants: List(ProductVariantRecord),
  new_options: List(ProductOptionRecord),
) -> List(ProductVariantRecord) {
  list.map(variants, fn(variant) {
    let additions =
      list.map(new_options, fn(option) {
        ProductVariantSelectedOptionRecord(
          name: option.name,
          value: first_option_value_name(option),
        )
      })
    let selected_options = list.append(variant.selected_options, additions)
    ProductVariantRecord(
      ..variant,
      title: variant_title(selected_options),
      selected_options: selected_options,
    )
  })
}

fn first_option_value_name(option: ProductOptionRecord) -> String {
  case option.option_values {
    [value, ..] -> value.name
    [] -> "Default Title"
  }
}

fn variant_title(
  selected_options: List(ProductVariantSelectedOptionRecord),
) -> String {
  selected_options
  |> list.map(fn(selected) { selected.value })
  |> string.join(" / ")
}

fn sync_product_options_with_variants(
  options: List(ProductOptionRecord),
  variants: List(ProductVariantRecord),
) -> List(ProductOptionRecord) {
  list.map(options, fn(option) {
    ProductOptionRecord(
      ..option,
      option_values: list.map(option.option_values, fn(value) {
        ProductOptionValueRecord(
          ..value,
          has_variants: variants_use_option_value(
            variants,
            option.name,
            value.name,
          ),
        )
      }),
    )
  })
}

fn make_options_from_variant_selections(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  variants: List(ProductVariantRecord),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  let #(options, next_identity) =
    upsert_variant_selections_into_options(identity, product_id, [], variants)
  #(sync_product_options_with_variants(options, variants), next_identity)
}

fn upsert_variant_selections_into_options(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  options: List(ProductOptionRecord),
  variants: List(ProductVariantRecord),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  variants
  |> list.flat_map(fn(variant) { variant.selected_options })
  |> list.fold(#(options, identity), fn(acc, selected) {
    let #(current_options, current_identity) = acc
    upsert_option_selection(
      current_options,
      current_identity,
      product_id,
      selected,
    )
  })
}

fn upsert_option_selection(
  options: List(ProductOptionRecord),
  identity: SyntheticIdentityRegistry,
  product_id: String,
  selected: ProductVariantSelectedOptionRecord,
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  let #(updated, found, next_identity) =
    upsert_option_selection_loop(options, identity, product_id, selected, [])
  case found {
    True -> #(updated, next_identity)
    False -> {
      let #(option_id, identity_after_option) =
        synthetic_identity.make_synthetic_gid(identity, "ProductOption")
      let #(value_id, identity_after_value) =
        synthetic_identity.make_synthetic_gid(
          identity_after_option,
          "ProductOptionValue",
        )
      #(
        list.append(options, [
          ProductOptionRecord(
            id: option_id,
            product_id: product_id,
            name: selected.name,
            position: list.length(options) + 1,
            option_values: [
              ProductOptionValueRecord(
                id: value_id,
                name: selected.value,
                has_variants: True,
              ),
            ],
          ),
        ]),
        identity_after_value,
      )
    }
  }
}

fn upsert_option_selection_loop(
  options: List(ProductOptionRecord),
  identity: SyntheticIdentityRegistry,
  product_id: String,
  selected: ProductVariantSelectedOptionRecord,
  reversed_before: List(ProductOptionRecord),
) -> #(List(ProductOptionRecord), Bool, SyntheticIdentityRegistry) {
  case options {
    [] -> #(list.reverse(reversed_before), False, identity)
    [option, ..rest] ->
      case option.name == selected.name {
        True -> {
          let #(option_values, next_identity) =
            upsert_option_value(option.option_values, identity, selected.value)
          #(
            list.append(list.reverse(reversed_before), [
              ProductOptionRecord(
                ..option,
                product_id: product_id,
                option_values: option_values,
              ),
              ..rest
            ]),
            True,
            next_identity,
          )
        }
        False ->
          upsert_option_selection_loop(rest, identity, product_id, selected, [
            option,
            ..reversed_before
          ])
      }
  }
}

fn upsert_option_value(
  option_values: List(ProductOptionValueRecord),
  identity: SyntheticIdentityRegistry,
  value_name: String,
) -> #(List(ProductOptionValueRecord), SyntheticIdentityRegistry) {
  case
    list.any(option_values, fn(option_value) { option_value.name == value_name })
  {
    True -> #(option_values, identity)
    False -> {
      let #(value_id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "ProductOptionValue")
      #(
        list.append(option_values, [
          ProductOptionValueRecord(
            id: value_id,
            name: value_name,
            has_variants: True,
          ),
        ]),
        next_identity,
      )
    }
  }
}

fn variants_use_option_value(
  variants: List(ProductVariantRecord),
  option_name: String,
  value_name: String,
) -> Bool {
  list.any(variants, fn(variant) {
    list.any(variant.selected_options, fn(selected) {
      selected.name == option_name && selected.value == value_name
    })
  })
}

type VariantCombination =
  List(ProductVariantSelectedOptionRecord)

fn create_variants_for_option_value_combinations(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  options: List(ProductOptionRecord),
  created_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  case created_options, existing_variants {
    [created_option], [_, ..] ->
      create_variants_for_single_new_option(
        identity,
        product_id,
        created_option,
        existing_variants,
      )
    _, _ ->
      create_variants_for_all_combinations(
        identity,
        product_id,
        options,
        existing_variants,
      )
  }
}

fn create_variants_for_single_new_option(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  created_option: ProductOptionRecord,
  existing_variants: List(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  let remaining_values = remaining_option_values(created_option)
  let #(new_variants, final_identity) =
    list.fold(existing_variants, #([], identity), fn(acc, existing_variant) {
      let #(records, current_identity) = acc
      let #(created, next_identity) =
        list.fold(
          remaining_values,
          #([], current_identity),
          fn(value_acc, value) {
            let #(value_records, value_identity) = value_acc
            let combination =
              variant_selected_options_with_value(
                existing_variant.selected_options,
                created_option.name,
                value.name,
              )
            let #(variant, next_value_identity) =
              make_variant_for_combination(
                value_identity,
                product_id,
                combination,
                Some(existing_variant),
              )
            #(list.append(value_records, [variant]), next_value_identity)
          },
        )
      #(list.append(records, created), next_identity)
    })
  #(list.append(existing_variants, new_variants), final_identity)
}

fn remaining_option_values(
  option: ProductOptionRecord,
) -> List(ProductOptionValueRecord) {
  case option.option_values {
    [] -> []
    [_, ..rest] -> rest
  }
}

fn variant_selected_options_with_value(
  selected_options: List(ProductVariantSelectedOptionRecord),
  option_name: String,
  value_name: String,
) -> List(ProductVariantSelectedOptionRecord) {
  list.map(selected_options, fn(selected) {
    case selected.name == option_name {
      True -> ProductVariantSelectedOptionRecord(..selected, value: value_name)
      False -> selected
    }
  })
}

fn create_variants_for_all_combinations(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  let combinations = option_value_combinations(options)
  let #(reversed, final_identity) =
    list.fold(combinations, #([], identity), fn(acc, combination) {
      let #(records, current_identity) = acc
      case find_variant_for_combination(existing_variants, combination) {
        Some(variant) -> #(
          [variant_for_combination(variant, combination), ..records],
          current_identity,
        )
        None -> {
          let template = case existing_variants {
            [first, ..] -> Some(first)
            [] -> None
          }
          let #(variant, next_identity) =
            make_variant_for_combination(
              current_identity,
              product_id,
              combination,
              template,
            )
          #([variant, ..records], next_identity)
        }
      }
    })
  #(list.reverse(reversed), final_identity)
}

fn option_value_combinations(
  options: List(ProductOptionRecord),
) -> List(VariantCombination) {
  case options {
    [] -> [[]]
    [option, ..rest] -> {
      let tail_combinations = option_value_combinations(rest)
      list.flat_map(tail_combinations, fn(tail) {
        option.option_values
        |> list.map(fn(value) {
          [
            ProductVariantSelectedOptionRecord(
              name: option.name,
              value: value.name,
            ),
            ..tail
          ]
        })
      })
    }
  }
}

fn find_variant_for_combination(
  variants: List(ProductVariantRecord),
  combination: VariantCombination,
) -> Option(ProductVariantRecord) {
  variants
  |> list.find(fn(variant) {
    selected_options_equal(variant.selected_options, combination)
  })
  |> option.from_result
}

fn selected_options_equal(
  left: List(ProductVariantSelectedOptionRecord),
  right: List(ProductVariantSelectedOptionRecord),
) -> Bool {
  list.length(left) == list.length(right)
  && list.all(left, fn(selected) {
    list.any(right, fn(other) {
      selected.name == other.name && selected.value == other.value
    })
  })
}

fn variant_for_combination(
  variant: ProductVariantRecord,
  combination: VariantCombination,
) -> ProductVariantRecord {
  ProductVariantRecord(
    ..variant,
    title: variant_title(combination),
    selected_options: combination,
  )
}

fn make_variant_for_combination(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  combination: VariantCombination,
  template: Option(ProductVariantRecord),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item, final_identity) =
    make_inventory_item_for_variant(identity_after_variant, template)
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: variant_title(combination),
      sku: option.then(template, fn(variant) { variant.sku }),
      barcode: option.then(template, fn(variant) { variant.barcode }),
      price: option.then(template, fn(variant) { variant.price }),
      compare_at_price: option.then(template, fn(variant) {
        variant.compare_at_price
      }),
      taxable: option.then(template, fn(variant) { variant.taxable }),
      inventory_policy: option.then(template, fn(variant) {
        variant.inventory_policy
      }),
      inventory_quantity: option.then(template, fn(variant) {
        variant.inventory_quantity
      }),
      selected_options: combination,
      media_ids: [],
      inventory_item: inventory_item,
      contextual_pricing: option.then(template, fn(variant) {
        variant.contextual_pricing
      }),
      cursor: None,
    ),
    final_identity,
  )
}

fn make_inventory_item_for_variant(
  identity: SyntheticIdentityRegistry,
  template: Option(ProductVariantRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  let template_item =
    option.then(template, fn(variant) { variant.inventory_item })
  case template_item {
    Some(item) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..item, id: id, inventory_levels: [])),
        next_identity,
      )
    }
    None -> #(None, identity)
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, ProductsError) {
  use data <- result.try(handle_products_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}
