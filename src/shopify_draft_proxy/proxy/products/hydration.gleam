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
import shopify_draft_proxy/proxy/products/collections_l00.{
  collection_rule_set_has_rules,
}
import shopify_draft_proxy/proxy/products/products_l00.{
  existing_group_description, existing_group_merchant_code, existing_group_name,
  existing_group_position, product_hydrate_nodes_query,
}
import shopify_draft_proxy/proxy/products/products_l01.{format_price_amount}
import shopify_draft_proxy/proxy/products/shared_l00.{
  dedupe_preserving_order, read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/variants_l00.{
  existing_group_options, has_only_default_variant, option_to_result,
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

@internal
pub fn collect_gid_strings(value: ResolvedValue) -> List(String) {
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

@internal
pub fn product_domain_hydratable_gid(id: String) -> Bool {
  string.starts_with(id, "gid://shopify/Product/")
  || string.starts_with(id, "gid://shopify/Collection/")
  || string.starts_with(id, "gid://shopify/ProductVariant/")
  || string.starts_with(id, "gid://shopify/InventoryItem/")
  || string.starts_with(id, "gid://shopify/InventoryLevel/")
}

@internal
pub fn collect_product_set_location_ids(
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

@internal
pub fn location_has_effective_id(store: Store, id: String) -> Bool {
  case store.get_effective_location_by_id(store, id) {
    Some(_) -> True
    None -> False
  }
}

@internal
pub fn dedupe_hydration_ids(ids: List(String)) -> List(String) {
  list.fold(ids, [], fn(acc, id) {
    case list.contains(acc, id) {
      True -> acc
      False -> list.append(acc, [id])
    }
  })
}

@internal
pub fn product_domain_has_effective_id(draft_store: Store, id: String) -> Bool {
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

@internal
pub fn upsert_hydrated_nodes(store: Store, body: commit.JsonValue) -> Store {
  case json_field(body, ["data", "nodes"]) {
    Some(commit.JsonArray(nodes)) ->
      list.fold(nodes, store, upsert_hydrated_node)
    _ -> store
  }
}

@internal
pub fn upsert_hydrated_node(store: Store, node: commit.JsonValue) -> Store {
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

@internal
pub fn upsert_hydrated_product(store: Store, node: commit.JsonValue) -> Store {
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

@internal
pub fn upsert_hydrated_collection(
  store: Store,
  node: commit.JsonValue,
) -> Store {
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

@internal
pub fn collection_product_entries_from_json(
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

@internal
pub fn upsert_hydrated_variant_without_product(
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
                  price_range_min: json_money_amount_field_at(node, [
                    "product",
                    "priceRangeV2",
                    "minVariantPrice",
                  ]),
                  price_range_max: json_money_amount_field_at(node, [
                    "product",
                    "priceRangeV2",
                    "maxVariantPrice",
                  ]),
                  total_variants: json_int_field_at(node, [
                    "product",
                    "totalVariants",
                  ]),
                  has_only_default_variant: json_bool_field_at(node, [
                    "product",
                    "hasOnlyDefaultVariant",
                  ]),
                  has_out_of_stock_variants: json_bool_field_at(node, [
                    "product",
                    "hasOutOfStockVariants",
                  ]),
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
                  combined_listing_role: json_string_field_at(node, [
                    "product",
                    "combinedListingRole",
                  ]),
                  combined_listing_parent_id: None,
                  combined_listing_child_ids: [],
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

@internal
pub fn upsert_hydrated_location(store: Store, node: commit.JsonValue) -> Store {
  case location_record_from_json(node) {
    Some(location) -> store.upsert_base_locations(store, [location])
    None -> store
  }
}

@internal
pub fn product_node_has_hydratable_fields(node: commit.JsonValue) -> Bool {
  option.is_some(json_string_field(node, "title"))
  || option.is_some(json_string_field(node, "handle"))
  || option.is_some(json_string_field(node, "status"))
  || option.is_some(json_field(node, ["variants"]))
}

@internal
pub fn replace_base_metafields_for_owners(
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

@internal
pub fn owner_ids_for_metafields(
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

@internal
pub fn upsert_hydrated_inventory_level(
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
      let item =
        InventoryItemRecord(
          ..base_item,
          inventory_levels: merge_hydrated_inventory_levels(
            level,
            base_item.inventory_levels,
          ),
        )
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
          price_range_min: json_money_amount_field_at(node, [
            "item",
            "variant",
            "product",
            "priceRangeV2",
            "minVariantPrice",
          ]),
          price_range_max: json_money_amount_field_at(node, [
            "item",
            "variant",
            "product",
            "priceRangeV2",
            "maxVariantPrice",
          ]),
          total_variants: json_int_field_at(node, [
            "item",
            "variant",
            "product",
            "totalVariants",
          ]),
          has_only_default_variant: json_bool_field_at(node, [
            "item",
            "variant",
            "product",
            "hasOnlyDefaultVariant",
          ]),
          has_out_of_stock_variants: json_bool_field_at(node, [
            "item",
            "variant",
            "product",
            "hasOutOfStockVariants",
          ]),
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
          combined_listing_role: json_string_field_at(node, [
            "item",
            "variant",
            "product",
            "combinedListingRole",
          ]),
          combined_listing_parent_id: None,
          combined_listing_child_ids: [],
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

@internal
pub fn merge_hydrated_inventory_levels(
  target: InventoryLevelRecord,
  levels: List(InventoryLevelRecord),
) -> List(InventoryLevelRecord) {
  case list.any(levels, fn(level) { level.id == target.id }) {
    True ->
      list.map(levels, fn(level) {
        case level.id == target.id {
          True -> target
          False -> level
        }
      })
    False -> [target, ..levels]
  }
}

@internal
pub fn upsert_hydrated_inventory_item_without_variant(
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
          price_range_min: json_money_amount_field_at(node, [
            "variant",
            "product",
            "priceRangeV2",
            "minVariantPrice",
          ]),
          price_range_max: json_money_amount_field_at(node, [
            "variant",
            "product",
            "priceRangeV2",
            "maxVariantPrice",
          ]),
          total_variants: json_int_field_at(node, [
            "variant",
            "product",
            "totalVariants",
          ]),
          has_only_default_variant: json_bool_field_at(node, [
            "variant",
            "product",
            "hasOnlyDefaultVariant",
          ]),
          has_out_of_stock_variants: json_bool_field_at(node, [
            "variant",
            "product",
            "hasOutOfStockVariants",
          ]),
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
          combined_listing_role: json_string_field_at(node, [
            "variant",
            "product",
            "combinedListingRole",
          ]),
          combined_listing_parent_id: None,
          combined_listing_child_ids: [],
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

@internal
pub fn product_record_from_json(
  node: commit.JsonValue,
) -> Option(ProductRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(
        ProductRecord(
          id: id,
          legacy_resource_id: json_string_field(node, "legacyResourceId"),
          title: json_string_field(node, "title") |> option.unwrap(""),
          handle: json_string_field(node, "handle") |> option.unwrap(id),
          status: json_string_field(node, "status") |> option.unwrap("ACTIVE"),
          vendor: json_string_field(node, "vendor"),
          product_type: json_string_field(node, "productType"),
          tags: json_string_array_field(node, ["tags"]),
          price_range_min: json_money_amount_field_at(node, [
            "priceRangeV2",
            "minVariantPrice",
          ]),
          price_range_max: json_money_amount_field_at(node, [
            "priceRangeV2",
            "maxVariantPrice",
          ]),
          total_variants: json_int_field(node, "totalVariants"),
          has_only_default_variant: json_bool_field(
            node,
            "hasOnlyDefaultVariant",
          ),
          has_out_of_stock_variants: json_bool_field(
            node,
            "hasOutOfStockVariants",
          ),
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
          combined_listing_role: json_string_field(node, "combinedListingRole"),
          combined_listing_parent_id: None,
          combined_listing_child_ids: [],
        ),
      )
  }
}

@internal
pub fn collection_record_from_json(
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
        rule_set: collection_rule_set_from_json(node),
        products_count: json_int_field_at(node, ["productsCount", "count"]),
        is_smart: collection_rule_set_from_json(node)
          |> option.map(collection_rule_set_has_rules)
          |> option.unwrap(False),
        cursor: None,
        title_cursor: None,
        updated_at_cursor: None,
      ))
  }
}

@internal
pub fn collection_rule_set_from_json(
  node: commit.JsonValue,
) -> Option(CollectionRuleSetRecord) {
  use rule_set <- option.then(json_field(node, ["ruleSet"]))
  let rules =
    json_array_field(rule_set, ["rules"])
    |> list.filter_map(fn(rule) {
      case
        json_string_field(rule, "column"),
        json_string_field(rule, "relation"),
        json_string_field(rule, "condition")
      {
        Some(column), Some(relation), Some(condition) ->
          Ok(CollectionRuleRecord(
            column: column,
            relation: relation,
            condition: condition,
          ))
        _, _, _ -> Error(Nil)
      }
    })
  Some(CollectionRuleSetRecord(
    applied_disjunctively: json_bool_field(rule_set, "appliedDisjunctively")
      |> option.unwrap(False),
    rules: rules,
  ))
}

@internal
pub fn product_option_from_json(
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

@internal
pub fn product_option_value_from_json(
  node: commit.JsonValue,
) -> ProductOptionValueRecord {
  ProductOptionValueRecord(
    id: json_string_field(node, "id")
      |> option.unwrap(json_string_field(node, "name") |> option.unwrap("")),
    name: json_string_field(node, "name") |> option.unwrap(""),
    has_variants: json_bool_field(node, "hasVariants") |> option.unwrap(False),
  )
}

@internal
pub fn product_variant_from_json(
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

@internal
pub fn product_media_from_json(
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

@internal
pub fn owner_metafields_from_json(
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

@internal
pub fn direct_metafield_nodes(
  node: commit.JsonValue,
) -> List(commit.JsonValue) {
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

@internal
pub fn dedupe_metafield_nodes(
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

@internal
pub fn product_metafield_from_json(
  owner_id: String,
  owner_type: String,
  node: commit.JsonValue,
) -> Option(ProductMetafieldRecord) {
  case json_string_field(node, "id") {
    None -> None
    Some(id) ->
      Some(
        ProductMetafieldRecord(
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
          market_localizable_content: [],
        ),
      )
  }
}

@internal
pub fn selected_option_from_json(
  node: commit.JsonValue,
) -> ProductVariantSelectedOptionRecord {
  ProductVariantSelectedOptionRecord(
    name: json_string_field(node, "name") |> option.unwrap(""),
    value: json_string_field(node, "value") |> option.unwrap(""),
  )
}

@internal
pub fn inventory_item_from_json(
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

@internal
pub fn inventory_measurement_from_json(
  value: Option(commit.JsonValue),
) -> Option(InventoryMeasurementRecord) {
  value
  |> option.map(fn(measurement) {
    InventoryMeasurementRecord(
      weight: inventory_weight_from_json(json_field(measurement, ["weight"])),
    )
  })
}

@internal
pub fn inventory_weight_from_json(
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

@internal
pub fn json_inventory_weight_value(
  value: commit.JsonValue,
  key: String,
) -> Option(InventoryWeightValue) {
  case json_field(value, [key]) {
    Some(commit.JsonInt(value)) -> Some(InventoryWeightInt(value))
    Some(commit.JsonFloat(value)) -> Some(InventoryWeightFloat(value))
    _ -> None
  }
}

@internal
pub fn location_record_from_json(
  node: commit.JsonValue,
) -> Option(LocationRecord) {
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

@internal
pub fn selling_plan_groups_from_json(
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

@internal
pub fn selling_plan_group_from_json(
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

@internal
pub fn inventory_level_from_json(
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

@internal
pub fn inventory_quantity_from_json(
  node: commit.JsonValue,
) -> InventoryQuantityRecord {
  InventoryQuantityRecord(
    name: json_string_field(node, "name") |> option.unwrap("available"),
    quantity: json_int_field(node, "quantity") |> option.unwrap(0),
    updated_at: json_string_field(node, "updatedAt"),
  )
}

@internal
pub fn json_field(
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

@internal
pub fn json_array_field(
  value: commit.JsonValue,
  path: List(String),
) -> List(commit.JsonValue) {
  case json_field(value, path) {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

@internal
pub fn json_string_field(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  json_string_field_at(value, [key])
}

@internal
pub fn json_string_field_at(
  value: commit.JsonValue,
  path: List(String),
) -> Option(String) {
  case json_field(value, path) {
    Some(commit.JsonString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn json_money_amount_field_at(
  value: commit.JsonValue,
  path: List(String),
) -> Option(String) {
  json_string_field_at(value, list.append(path, ["amount"]))
  |> option.map(format_price_amount)
}

@internal
pub fn json_string_or_number_field(
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

@internal
pub fn json_int_field(value: commit.JsonValue, key: String) -> Option(Int) {
  json_int_field_at(value, [key])
}

@internal
pub fn json_int_field_at(
  value: commit.JsonValue,
  path: List(String),
) -> Option(Int) {
  case json_field(value, path) {
    Some(commit.JsonInt(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn json_bool_field(value: commit.JsonValue, key: String) -> Option(Bool) {
  json_bool_field_at(value, [key])
}

@internal
pub fn json_bool_field_at(
  value: commit.JsonValue,
  path: List(String),
) -> Option(Bool) {
  case json_field(value, path) {
    Some(commit.JsonBool(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn json_string_array_field(
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

@internal
pub fn captured_json_from_commit(value: commit.JsonValue) -> CapturedJsonValue {
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
