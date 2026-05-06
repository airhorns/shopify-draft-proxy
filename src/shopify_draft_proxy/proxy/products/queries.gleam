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
import shopify_draft_proxy/proxy/products/collections_serializers.{
  serialize_collection_by_handle_root, serialize_collection_by_identifier_root,
  serialize_collection_root, serialize_collections_connection,
}
import shopify_draft_proxy/proxy/products/inventory_core.{
  serialize_locations_connection,
}
import shopify_draft_proxy/proxy/products/inventory_handlers.{
  serialize_inventory_item_root, serialize_inventory_items_connection,
  serialize_inventory_level_root,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_handlers.{
  serialize_inventory_shipment_root,
}
import shopify_draft_proxy/proxy/products/inventory_transfers.{
  serialize_inventory_transfer_root, serialize_inventory_transfers_connection,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  serialize_inventory_properties,
}
import shopify_draft_proxy/proxy/products/products_core.{
  has_effective_product_metafield_owner, product_tags, product_types,
  product_vendors, published_products_count_for_field,
  serialize_product_duplicate_job,
}
import shopify_draft_proxy/proxy/products/products_handlers.{
  serialize_product_by_identifier_root, serialize_product_operation_root,
  serialize_product_root, serialize_products_connection,
}
import shopify_draft_proxy/proxy/products/products_records.{
  product_count_for_field,
}
import shopify_draft_proxy/proxy/products/publications_core.{
  serialize_product_feed_root, serialize_product_feeds_connection,
}
import shopify_draft_proxy/proxy/products/publications_feeds.{
  serialize_channel_root, serialize_channels_connection,
  serialize_product_resource_feedback_root,
}
import shopify_draft_proxy/proxy/products/publications_publishable.{
  serialize_publication_root, serialize_publications_connection,
}
import shopify_draft_proxy/proxy/products/selling_plans_handlers.{
  serialize_selling_plan_group_root, serialize_selling_plan_groups_connection,
}
import shopify_draft_proxy/proxy/products/shared.{
  serialize_exact_count, serialize_string_connection,
}
import shopify_draft_proxy/proxy/products/types.{type ProductsError, ParseFailed} as product_types
import shopify_draft_proxy/proxy/products/variants_sources.{
  serialize_product_variant_by_identifier_root, serialize_product_variant_root,
  serialize_product_variants_connection,
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

@internal
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

@internal
pub fn resolved_value_matches_local_product(
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

@internal
pub fn local_has_product_state(proxy: DraftProxy) -> Bool {
  !list.is_empty(store.list_effective_products(proxy.store))
  || dict.size(proxy.store.staged_state.deleted_product_ids) > 0
}

/// Pattern 1: cold product detail and catalog reads in LiveHybrid can
/// forward the captured Shopify document verbatim. Once local product
/// state exists, stay in the local serializer so staged lifecycle
/// effects and staged deletes are observable without runtime writes.
@internal
pub fn should_passthrough_in_live_hybrid(
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

@internal
pub fn local_has_product_domain_state(proxy: DraftProxy) -> Bool {
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

@internal
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

@internal
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

@internal
pub fn serialize_root_fields(
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

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, ProductsError) {
  use data <- result.try(handle_products_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}
