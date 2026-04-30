//// Pure-Gleam parity runner.
////
//// Replaces the legacy vitest harness in
//// `tests/unit/conformance-parity-scenarios.test.ts`. Reads a parity
//// spec, loads the capture and GraphQL document referenced by the
//// spec, drives them through `draft_proxy.process_request`, and
//// compares each target's `capturePath` slice of the capture against
//// the same `proxyPath` slice of the proxy response — applying the
//// spec's `expectedDifferences` matchers.
////
//// Per-target `proxyRequest` overrides are supported. State (store,
//// synthetic identity) is threaded forward across requests, so a
//// target can read back records the primary mutation created.
////
//// File-system paths in the spec are repo-root relative. Tests run
//// from the `gleam/` subdirectory; the runner resolves paths via `..`
//// (configurable via `RunnerConfig.repo_root`).

import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import parity/diff.{type Mismatch}
import parity/json_value.{
  type JsonValue, JArray, JBool, JFloat, JInt, JObject, JString,
}
import parity/jsonpath
import parity/spec.{
  type Spec, type Target, NoVariables, OverrideRequest, ReusePrimary,
  VariablesFromCapture, VariablesFromFile, VariablesInline,
}
import shopify_draft_proxy/proxy/draft_proxy.{
  type DraftProxy, type Response, Request,
}
import shopify_draft_proxy/state/store as store_mod
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CollectionImageRecord, type CollectionRecord, type CollectionRuleRecord,
  type CollectionRuleSetRecord, type GiftCardConfigurationRecord,
  type GiftCardRecipientAttributesRecord, type GiftCardRecord,
  type GiftCardTransactionRecord, type InventoryItemRecord,
  type InventoryLevelRecord, type InventoryLocationRecord,
  type InventoryMeasurementRecord, type InventoryQuantityRecord,
  type InventoryWeightRecord, type Money, type PaymentSettingsRecord,
  type ProductCategoryRecord, type ProductCollectionRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductSeoRecord, type ProductVariantRecord,
  type ProductVariantSelectedOptionRecord, type ShopAddressRecord,
  type ShopDomainRecord, type ShopFeaturesRecord, type ShopPlanRecord,
  type ShopPolicyRecord, type ShopRecord, type ShopResourceLimitsRecord,
  type ShopifyFunctionAppRecord, type ShopifyFunctionRecord,
  CollectionImageRecord, CollectionRecord, CollectionRuleRecord,
  CollectionRuleSetRecord, GiftCardConfigurationRecord,
  GiftCardRecipientAttributesRecord, GiftCardRecord, GiftCardTransactionRecord,
  InventoryItemRecord, InventoryLevelRecord, InventoryLocationRecord,
  InventoryMeasurementRecord, InventoryQuantityRecord, InventoryWeightFloat,
  InventoryWeightInt, InventoryWeightRecord, Money, PaymentSettingsRecord,
  ProductCategoryRecord, ProductCollectionRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ProductVariantSelectedOptionRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopFeaturesRecord,
  ShopPlanRecord, ShopPolicyRecord, ShopRecord, ShopResourceLimitsRecord,
  ShopifyFunctionAppRecord, ShopifyFunctionRecord,
}
import simplifile

pub type RunError {
  /// File could not be read off disk.
  FileError(path: String, reason: String)
  /// File contents could not be parsed as JSON.
  JsonError(path: String, reason: String)
  /// Spec was malformed.
  SpecError(reason: String)
  /// Variables JSONPath did not resolve.
  VariablesUnresolved(path: String)
  /// `fromPrimaryProxyPath` substitution path didn't resolve.
  PrimaryRefUnresolved(path: String)
  /// `fromCapturePath` substitution path didn't resolve.
  CaptureRefUnresolved(path: String)
  /// Capture JSONPath did not resolve for a target.
  CaptureUnresolved(target: String, path: String)
  /// Proxy response JSONPath did not resolve for a target.
  ProxyUnresolved(target: String, path: String)
  /// Proxy returned a non-200 status.
  ProxyStatus(target: String, status: Int, body: String)
}

pub type TargetReport {
  TargetReport(
    name: String,
    capture_path: String,
    proxy_path: String,
    mismatches: List(Mismatch),
  )
}

pub type Report {
  Report(scenario_id: String, targets: List(TargetReport))
}

pub type RunnerConfig {
  RunnerConfig(repo_root: String)
}

pub fn default_config() -> RunnerConfig {
  RunnerConfig(repo_root: "..")
}

pub fn run(spec_path: String) -> Result(Report, RunError) {
  run_with_config(default_config(), spec_path)
}

pub fn run_with_config(
  config: RunnerConfig,
  spec_path: String,
) -> Result(Report, RunError) {
  use spec_source <- result.try(read_file(resolve(config, spec_path)))
  use parsed <- result.try(parse_spec(spec_source))
  use capture <- result.try(load_capture(config, parsed))
  use primary_doc <- result.try(
    read_file(resolve(config, parsed.proxy_request.document_path)),
  )
  use primary_vars <- result.try(resolve_variables(
    config,
    parsed.proxy_request.variables,
    capture,
    None,
    "<primary>",
  ))
  let proxy = draft_proxy.new()
  let proxy = seed_capture_preconditions(parsed, capture, proxy)
  use #(primary_response, proxy) <- result.try(execute(
    proxy,
    primary_doc,
    primary_vars,
    "<primary>",
  ))
  use primary_value <- result.try(parse_response_body(primary_response))
  use #(_proxy, target_reports) <- result.try(run_targets(
    config,
    parsed,
    capture,
    primary_value,
    proxy,
  ))
  Ok(Report(scenario_id: parsed.scenario_id, targets: target_reports))
}

fn seed_capture_preconditions(
  parsed: Spec,
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let proxy = seed_captured_products_preconditions(capture, proxy)
  case parsed.scenario_id {
    "gift-card-search-filters" ->
      seed_gift_card_lifecycle_preconditions(capture, proxy)
    "functions-owner-metadata-local-staging" ->
      seed_shopify_function_preconditions(capture, proxy)
    "shop-baseline-read"
    | "shop-policy-update-parity"
    | "admin-platform-store-property-node-reads" ->
      seed_shop_preconditions(capture, proxy)
    "product-detail-read" -> seed_product_preconditions(capture, proxy)
    "collection-detail-read" | "collection-identifier-read" ->
      seed_collection_detail_preconditions(capture, proxy)
    "collections-catalog-read" ->
      seed_collections_catalog_preconditions(capture, proxy)
    "collection-add-products-live-parity" ->
      seed_collection_add_products_preconditions(capture, proxy)
    "collection-remove-products-live-parity" ->
      seed_collection_remove_products_preconditions(capture, proxy)
    "collection-reorder-products-live-parity" ->
      seed_collection_reorder_products_preconditions(capture, proxy)
    "collection-update-live-parity" ->
      seed_collection_update_preconditions(capture, proxy)
    "collection-delete-live-parity" ->
      seed_collection_delete_preconditions(capture, proxy)
    "collection-create-initial-products-live-parity" ->
      seed_collection_create_initial_products_preconditions(capture, proxy)
    "products-catalog-read" ->
      seed_products_catalog_preconditions(capture, proxy)
    "products-search-read" ->
      seed_products_search_read_preconditions(capture, proxy)
    "product-variants-read" | "inventory-level-read" ->
      seed_product_variants_read_preconditions(capture, proxy)
    "product-options-create-variant-strategy-create"
    | "product-options-create-variant-strategy-create-over-default-limit"
    | "product-options-create-variant-strategy-leave-as-is"
    | "product-options-create-variant-strategy-null"
    | "product-options-create-live-parity"
    | "product-option-update-live-parity"
    | "product-options-delete-live-parity"
    | "product-variants-bulk-create-strategy-default-default-standalone"
    | "product-variants-bulk-create-strategy-remove-default-standalone"
    | "product-variants-bulk-create-strategy-default-custom-standalone"
    | "product-variants-bulk-create-strategy-remove-custom-standalone"
    | "admin-platform-product-option-node-reads" ->
      seed_pre_mutation_product_preconditions(capture, proxy)
    "product-delete-live-parity" ->
      seed_product_delete_preconditions(capture, proxy)
    "product-update-live-parity" | "productUpdate-blank-title-parity" ->
      seed_product_update_preconditions(capture, proxy)
    "tags-remove-live-parity" -> seed_tags_remove_preconditions(capture, proxy)
    "product-variant-create-compatibility-evidence" ->
      seed_product_variant_create_preconditions(capture, proxy)
    "product-variants-bulk-create-live-parity"
    | "product-variants-bulk-create-inventory-read-live-parity" ->
      seed_product_variants_bulk_create_preconditions(capture, proxy)
    "product-variant-update-compatibility-evidence" ->
      seed_product_variant_update_preconditions(capture, proxy)
    "product-variants-bulk-update-live-parity" ->
      seed_product_variants_bulk_update_preconditions(capture, proxy)
    "product-variant-delete-compatibility-evidence" ->
      seed_product_variant_delete_preconditions(capture, proxy)
    "product-variants-bulk-delete-live-parity" ->
      seed_product_variant_delete_preconditions(capture, proxy)
    "inventory-quantity-roots-parity" ->
      seed_inventory_quantity_roots_preconditions(capture, proxy)
    "inventory-adjust-quantities-live-parity" ->
      seed_inventory_adjust_quantities_preconditions(capture, proxy)
    "inventory-activate-live-parity" ->
      seed_inventory_activate_preconditions(capture, proxy)
    "inventory-deactivate-live-parity" ->
      seed_inventory_activate_preconditions(capture, proxy)
    "inventory-bulk-toggle-activation-live-parity" ->
      seed_inventory_activate_preconditions(capture, proxy)
    "inventory-item-update-live-parity" ->
      seed_inventory_item_update_preconditions(capture, proxy)
    "product-variants-bulk-reorder-live-parity" ->
      seed_product_variants_bulk_reorder_preconditions(capture, proxy)
    _ -> proxy
  }
}

fn seed_captured_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let single_product_nodes = case read_object_field(capture, "seedProduct") {
    Some(node) -> [node]
    None -> []
  }
  let product_nodes = case read_array_field(capture, "seedProducts") {
    Some(nodes) -> nodes
    None -> []
  }
  let product_nodes = list.append(single_product_nodes, product_nodes)
  let products = case product_nodes {
    [] -> []
    nodes -> list.filter_map(nodes, make_seed_product)
  }
  let variants = case product_nodes {
    [] -> []
    nodes ->
      list.flat_map(nodes, fn(product_json) {
        seed_variants_for_product(product_json)
      })
  }
  let store =
    proxy.store
    |> store_mod.upsert_base_products(products)
    |> store_mod.upsert_base_product_variants(variants)
  let store = list.fold(product_nodes, store, seed_options_for_product)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_options_for_product(
  store: store_mod.Store,
  product_json: JsonValue,
) -> store_mod.Store {
  case required_string_field(product_json, "id") {
    Ok(product_id) ->
      case read_array_field(product_json, "options") {
        Some(option_nodes) -> {
          let options =
            list.filter_map(option_nodes, fn(option_json) {
              make_seed_product_option(product_id, option_json)
            })
          store_mod.replace_base_options_for_product(store, product_id, options)
        }
        None -> store
      }
    Error(_) -> store
  }
}

fn make_seed_product_option(
  product_id: String,
  source: JsonValue,
) -> Result(ProductOptionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  let position = read_int_field(source, "position") |> option.unwrap(0)
  let option_values = case read_array_field(source, "optionValues") {
    Some(nodes) -> list.filter_map(nodes, make_seed_product_option_value)
    None -> []
  }
  Ok(ProductOptionRecord(
    id: id,
    product_id: product_id,
    name: name,
    position: position,
    option_values: option_values,
  ))
}

fn make_seed_product_option_value(
  source: JsonValue,
) -> Result(ProductOptionValueRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  Ok(ProductOptionValueRecord(
    id: id,
    name: name,
    has_variants: read_bool_field(source, "hasVariants") |> option.unwrap(False),
  ))
}

fn seed_product_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.data.product") {
    Some(product_json) ->
      case make_seed_product(product_json) {
        Ok(product) -> {
          let store =
            proxy.store
            |> store_mod.upsert_base_products([product])
            |> seed_options_for_product(product_json)
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn seed_collection_detail_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let collection_sources =
    ["$.data.customCollection", "$.data.smartCollection"]
    |> list.filter_map(fn(path) {
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(value)
        None -> Error(Nil)
      }
    })
  let has_product_seed_id =
    jsonpath.lookup(
      capture,
      "$.data.customCollection.products.edges[0].node.id",
    )
    |> json_string_option
  let collections = list.filter_map(collection_sources, make_seed_collection)
  let store =
    proxy.store
    |> store_mod.upsert_base_collections(collections)
  let proxy = draft_proxy.DraftProxy(..proxy, store: store)
  list.fold(collection_sources, proxy, fn(acc, collection_json) {
    seed_collection_products(collection_json, has_product_seed_id, acc)
  })
}

fn seed_collection_products(
  collection_json: JsonValue,
  has_product_seed_id: Option(String),
  proxy: DraftProxy,
) -> DraftProxy {
  case make_seed_collection(collection_json) {
    Ok(collection) -> {
      let edges = case jsonpath.lookup(collection_json, "$.products.edges") {
        Some(JArray(edges)) -> edges
        _ -> []
      }
      let products = list.filter_map(edges, make_seed_product_relaxed_from_edge)
      let memberships =
        edges
        |> enumerate_json_values()
        |> list.filter_map(fn(pair) {
          let #(edge, position) = pair
          make_seed_product_collection_from_edge(collection.id, edge, position)
        })
      let memberships = case
        read_bool_field(collection_json, "hasProduct"),
        has_product_seed_id
      {
        Some(True), Some(product_id) ->
          case
            list.any(memberships, fn(record) { record.product_id == product_id })
          {
            True -> memberships
            False ->
              list.append(memberships, [
                ProductCollectionRecord(
                  collection_id: collection.id,
                  product_id: product_id,
                  position: list.length(memberships),
                  cursor: None,
                ),
              ])
          }
        _, _ -> memberships
      }
      let store =
        proxy.store
        |> store_mod.upsert_base_products(products)
        |> store_mod.replace_base_products_for_collection(
          collection.id,
          memberships,
        )
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    Error(_) -> proxy
  }
}

fn seed_collections_catalog_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let edges = case jsonpath.lookup(capture, "$.data.collections.edges") {
    Some(JArray(edges)) -> edges
    _ -> []
  }
  let collections =
    edges
    |> list.filter_map(make_seed_collection_from_edge)
    |> merge_collection_cursors_from_path(
      capture,
      "$.data.titleWildcard.edges",
      "title",
    )
    |> merge_collection_cursors_from_path(
      capture,
      "$.data.smartCollections.edges",
      "title",
    )
    |> merge_collection_cursors_from_path(
      capture,
      "$.data.updatedNewest.edges",
      "updated_at",
    )
  let store =
    proxy.store
    |> store_mod.upsert_base_collections(collections)
  let proxy = draft_proxy.DraftProxy(..proxy, store: store)
  list.fold(edges, proxy, fn(acc, edge) {
    case read_object_field(edge, "node") {
      Some(collection_json) ->
        seed_collection_products(collection_json, None, acc)
      None -> acc
    }
  })
}

fn seed_collection_add_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.collectionAddProducts.collection",
    )
  {
    Some(collection_json) -> {
      let target_collection_id =
        read_string_field(collection_json, "id") |> option.unwrap("")
      let target_collections = case make_seed_collection(collection_json) {
        Ok(collection) -> [collection]
        Error(_) -> []
      }
      let product_nodes = case
        jsonpath.lookup(collection_json, "$.products.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let products = list.filter_map(product_nodes, make_seed_product_relaxed)
      let existing =
        list.append(
          seed_existing_product_collections(
            capture,
            "$.downstreamRead.data.first",
            target_collection_id,
          ),
          seed_existing_product_collections(
            capture,
            "$.downstreamRead.data.second",
            target_collection_id,
          ),
        )
      let existing_collections =
        list.filter_map(existing, fn(entry) {
          let #(collection, _) = entry
          Ok(collection)
        })
      let existing_memberships =
        list.map(existing, fn(entry) {
          let #(_, membership) = entry
          membership
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(list.append(
          target_collections,
          existing_collections,
        ))
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_collections(existing_memberships)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_collection_remove_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamRead.data.collection") {
    Some(collection_json) -> {
      let target_collection_id =
        read_string_field(collection_json, "id") |> option.unwrap("")
      let target_collections = case make_seed_collection(collection_json) {
        Ok(collection) -> [collection]
        Error(_) -> []
      }
      let collection_product_nodes = case
        jsonpath.lookup(collection_json, "$.products.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let target_memberships =
        collection_product_nodes
        |> enumerate_json_values()
        |> list.filter_map(fn(entry) {
          let #(product_json, position) = entry
          case read_string_field(product_json, "id") {
            Some(product_id) ->
              Ok(ProductCollectionRecord(
                collection_id: target_collection_id,
                product_id: product_id,
                position: position + 1,
                cursor: None,
              ))
            None -> Error(Nil)
          }
        })
      let removed_memberships =
        collection_remove_product_ids(capture)
        |> enumerate_strings()
        |> list.map(fn(entry) {
          let #(product_id, position) = entry
          ProductCollectionRecord(
            collection_id: target_collection_id,
            product_id: product_id,
            position: position,
            cursor: None,
          )
        })
      let existing =
        seed_existing_product_collections(
          capture,
          "$.downstreamRead.data.untouched",
          target_collection_id,
        )
      let existing_collections =
        list.filter_map(existing, fn(entry) {
          let #(collection, _) = entry
          Ok(collection)
        })
      let existing_memberships =
        list.map(existing, fn(entry) {
          let #(_, membership) = entry
          membership
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(list.append(
          target_collections,
          existing_collections,
        ))
        |> store_mod.upsert_base_product_collections(list.append(
          list.append(removed_memberships, target_memberships),
          existing_memberships,
        ))
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn collection_remove_product_ids(capture: JsonValue) -> List(String) {
  case jsonpath.lookup(capture, "$.mutation.variables.productIds") {
    Some(JArray(ids)) ->
      list.filter_map(ids, fn(id) {
        case id {
          JString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn seed_collection_reorder_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.initialCollectionRead.data.collection") {
    Some(collection_json) -> {
      let target_collection_id =
        read_string_field(collection_json, "id") |> option.unwrap("")
      let target_collections = case make_seed_collection(collection_json) {
        Ok(collection) -> [collection]
        Error(_) -> []
      }
      let product_nodes = case
        jsonpath.lookup(collection_json, "$.products.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let target_memberships =
        product_nodes
        |> enumerate_json_values()
        |> list.filter_map(fn(entry) {
          let #(product_json, position) = entry
          case read_string_field(product_json, "id") {
            Some(product_id) ->
              Ok(ProductCollectionRecord(
                collection_id: target_collection_id,
                product_id: product_id,
                position: position,
                cursor: None,
              ))
            None -> Error(Nil)
          }
        })
      let existing =
        list.append(
          seed_existing_product_collections(
            capture,
            "$.initialCollectionRead.data.first",
            target_collection_id,
          ),
          seed_existing_product_collections(
            capture,
            "$.initialCollectionRead.data.second",
            target_collection_id,
          ),
        )
      let existing_collections =
        list.filter_map(existing, fn(entry) {
          let #(collection, _) = entry
          Ok(collection)
        })
      let existing_memberships =
        list.map(existing, fn(entry) {
          let #(_, membership) = entry
          membership
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(list.append(
          target_collections,
          existing_collections,
        ))
        |> store_mod.upsert_base_product_collections(list.append(
          target_memberships,
          existing_memberships,
        ))
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_collection_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.collectionUpdate.collection",
    )
  {
    Some(collection_json) -> {
      let target_collection_id =
        read_string_field(collection_json, "id") |> option.unwrap("")
      let target_collections = case make_seed_collection(collection_json) {
        Ok(collection) -> [collection]
        Error(_) -> []
      }
      let product_nodes = case
        jsonpath.lookup(collection_json, "$.products.nodes")
      {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      let products = list.filter_map(product_nodes, make_seed_product_relaxed)
      let memberships =
        product_nodes
        |> enumerate_json_values()
        |> list.filter_map(fn(entry) {
          let #(product_json, position) = entry
          case read_string_field(product_json, "id") {
            Some(product_id) ->
              Ok(ProductCollectionRecord(
                collection_id: target_collection_id,
                product_id: product_id,
                position: position,
                cursor: None,
              ))
            None -> Error(Nil)
          }
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(target_collections)
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_collections(memberships)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_collection_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.mutation.variables.input.id") {
    Some(JString(collection_id)) -> {
      let collection =
        CollectionRecord(
          id: collection_id,
          legacy_resource_id: None,
          title: "Delete parity collection",
          handle: "delete-parity-collection",
          publication_ids: [],
          updated_at: None,
          description: None,
          description_html: None,
          image: None,
          sort_order: Some("MANUAL"),
          template_suffix: None,
          seo: ProductSeoRecord(title: None, description: None),
          rule_set: None,
          products_count: Some(0),
          is_smart: False,
          cursor: None,
          title_cursor: None,
          updated_at_cursor: None,
        )
      let store = store_mod.upsert_base_collections(proxy.store, [collection])
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    _ -> proxy
  }
}

fn seed_collection_create_initial_products_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamReadVariables.collectionId") {
    Some(JString(target_collection_id)) -> {
      let existing =
        list.append(
          seed_existing_product_collections(
            capture,
            "$.downstreamRead.data.first",
            target_collection_id,
          ),
          seed_existing_product_collections(
            capture,
            "$.downstreamRead.data.second",
            target_collection_id,
          ),
        )
      let collections =
        list.map(existing, fn(entry) {
          let #(collection, _) = entry
          collection
        })
      let memberships =
        list.map(existing, fn(entry) {
          let #(_, membership) = entry
          membership
        })
      let store =
        proxy.store
        |> store_mod.upsert_base_collections(collections)
        |> store_mod.upsert_base_product_collections(memberships)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    _ -> proxy
  }
}

fn seed_existing_product_collections(
  capture: JsonValue,
  product_path: String,
  target_collection_id: String,
) -> List(#(CollectionRecord, ProductCollectionRecord)) {
  case jsonpath.lookup(capture, product_path) {
    Some(product_json) -> {
      let product_id = read_string_field(product_json, "id")
      let nodes = case jsonpath.lookup(product_json, "$.collections.nodes") {
        Some(JArray(nodes)) -> nodes
        _ -> []
      }
      nodes
      |> enumerate_json_values()
      |> list.filter_map(fn(entry) {
        let #(collection_json, position) = entry
        case
          make_seed_collection(collection_json),
          product_id,
          read_string_field(collection_json, "id")
        {
          Ok(collection), Some(product_id), Some(collection_id) ->
            case collection_id == target_collection_id {
              True -> Error(Nil)
              False ->
                Ok(#(
                  collection,
                  ProductCollectionRecord(
                    collection_id: collection.id,
                    product_id: product_id,
                    position: position,
                    cursor: None,
                  ),
                ))
            }
          _, _, _ -> Error(Nil)
        }
      })
    }
    None -> []
  }
}

fn make_seed_collection_from_edge(
  edge: JsonValue,
) -> Result(CollectionRecord, Nil) {
  use node <- result.try(required_object_field(edge, "node"))
  use collection <- result.try(make_seed_collection(node))
  Ok(CollectionRecord(..collection, cursor: read_string_field(edge, "cursor")))
}

fn merge_collection_cursors_from_path(
  collections: List(CollectionRecord),
  capture: JsonValue,
  path: String,
  cursor_kind: String,
) -> List(CollectionRecord) {
  let edges = case jsonpath.lookup(capture, path) {
    Some(JArray(edges)) -> edges
    _ -> []
  }
  list.fold(edges, collections, fn(acc, edge) {
    case
      read_object_field(edge, "node")
      |> option.then(read_string_field(_, "id")),
      read_string_field(edge, "cursor")
    {
      Some(collection_id), Some(cursor) ->
        list.map(acc, fn(collection) {
          case collection.id == collection_id, cursor_kind {
            True, "title" ->
              CollectionRecord(..collection, title_cursor: Some(cursor))
            True, "updated_at" ->
              CollectionRecord(..collection, updated_at_cursor: Some(cursor))
            _, _ -> collection
          }
        })
      _, _ -> acc
    }
  })
}

fn make_seed_collection(source: JsonValue) -> Result(CollectionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use title <- result.try(required_string_field(source, "title"))
  use handle <- result.try(required_string_field(source, "handle"))
  let rule_set =
    make_seed_collection_rule_set(read_object_field(source, "ruleSet"))
  Ok(CollectionRecord(
    id: id,
    legacy_resource_id: read_string_field(source, "legacyResourceId"),
    title: title,
    handle: handle,
    publication_ids: [],
    updated_at: read_string_field(source, "updatedAt"),
    description: read_string_field(source, "description"),
    description_html: read_string_field(source, "descriptionHtml"),
    image: make_seed_collection_image(read_object_field(source, "image")),
    sort_order: read_string_field(source, "sortOrder"),
    template_suffix: read_string_field(source, "templateSuffix"),
    seo: make_seed_product_seo(read_object_field(source, "seo")),
    rule_set: rule_set,
    products_count: read_object_field(source, "productsCount")
      |> option.then(read_int_field(_, "count")),
    is_smart: case rule_set {
      Some(_) -> True
      None -> False
    },
    cursor: None,
    title_cursor: None,
    updated_at_cursor: None,
  ))
}

fn make_seed_collection_image(
  source: Option(JsonValue),
) -> Option(CollectionImageRecord) {
  case source {
    None -> None
    Some(value) ->
      Some(CollectionImageRecord(
        id: read_string_field(value, "id"),
        alt_text: read_string_field(value, "altText"),
        url: read_string_field(value, "url"),
        width: read_int_field(value, "width"),
        height: read_int_field(value, "height"),
      ))
  }
}

fn make_seed_collection_rule_set(
  source: Option(JsonValue),
) -> Option(CollectionRuleSetRecord) {
  case source {
    None -> None
    Some(value) ->
      Some(
        CollectionRuleSetRecord(
          applied_disjunctively: read_bool_field(value, "appliedDisjunctively")
            |> option.unwrap(False),
          rules: case read_array_field(value, "rules") {
            Some(rules) -> list.filter_map(rules, make_seed_collection_rule)
            None -> []
          },
        ),
      )
  }
}

fn make_seed_collection_rule(
  source: JsonValue,
) -> Result(CollectionRuleRecord, Nil) {
  use column <- result.try(required_string_field(source, "column"))
  use relation <- result.try(required_string_field(source, "relation"))
  use condition <- result.try(required_string_field(source, "condition"))
  Ok(CollectionRuleRecord(
    column: column,
    relation: relation,
    condition: condition,
  ))
}

fn make_seed_product_collection_from_edge(
  collection_id: String,
  edge: JsonValue,
  position: Int,
) -> Result(ProductCollectionRecord, Nil) {
  use node <- result.try(required_object_field(edge, "node"))
  use product_id <- result.try(required_string_field(node, "id"))
  Ok(ProductCollectionRecord(
    collection_id: collection_id,
    product_id: product_id,
    position: position,
    cursor: read_string_field(edge, "cursor"),
  ))
}

fn make_seed_product(source: JsonValue) -> Result(ProductRecord, Nil) {
  make_seed_product_with_cursor(source, None)
}

fn seed_products_catalog_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let products = case jsonpath.lookup(capture, "$.data.products.edges") {
    Some(JArray(edges)) ->
      list.filter_map(edges, fn(edge) { make_seed_product_from_edge(edge) })
    _ -> []
  }
  let store = store_mod.upsert_base_products(proxy.store, products)
  let store = case jsonpath.lookup(capture, "$.data.productsCount.count") {
    Some(JInt(count)) -> store_mod.set_base_product_count(store, count)
    _ -> store
  }
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_products_search_read_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let products =
    list.append(
      seed_products_from_connection_path(capture, "$.data.nike.edges"),
      seed_products_from_connection_path(capture, "$.data.lowInventory.edges"),
    )
  let products = append_search_has_next_page_sentinel(capture, products)
  let store = store_mod.upsert_base_products(proxy.store, products)
  let store = case jsonpath.lookup(capture, "$.data.total.count") {
    Some(JInt(count)) -> store_mod.set_base_product_count(store, count)
    _ -> store
  }
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_products_from_connection_path(
  capture: JsonValue,
  path: String,
) -> List(ProductRecord) {
  case jsonpath.lookup(capture, path) {
    Some(JArray(edges)) ->
      list.filter_map(edges, fn(edge) {
        make_seed_product_relaxed_from_edge(edge)
      })
    _ -> []
  }
}

fn make_seed_product_relaxed_from_edge(
  edge: JsonValue,
) -> Result(ProductRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) -> make_seed_product_relaxed(node)
    None -> Error(Nil)
  }
}

fn append_search_has_next_page_sentinel(
  capture: JsonValue,
  products: List(ProductRecord),
) -> List(ProductRecord) {
  case jsonpath.lookup(capture, "$.data.nike.pageInfo.hasNextPage") {
    Some(JBool(True)) ->
      case list.find(products, fn(product) { product.vendor == Some("NIKE") }) {
        Ok(product) ->
          list.append(products, [
            ProductRecord(
              ..product,
              id: "gid://shopify/Product/999999999999999",
              legacy_resource_id: Some("999999999999999"),
              title: product.title <> " (pagination sentinel)",
            ),
          ])
        Error(_) -> products
      }
    _ -> products
  }
}

fn seed_product_variants_read_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.data.product") {
    Some(product_json) -> {
      let products = case make_seed_product_relaxed(product_json) {
        Ok(product) -> [product]
        Error(_) -> []
      }
      let variants = seed_variants_for_product(product_json)
      let store =
        proxy.store
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_variants(variants)
        |> seed_options_for_product(product_json)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_pre_mutation_product_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.preMutationRead.data.product") {
    Some(product_json) -> seed_product_json(product_json, proxy)
    None -> proxy
  }
}

fn seed_product_json(product_json: JsonValue, proxy: DraftProxy) -> DraftProxy {
  let products = case make_seed_product_relaxed(product_json) {
    Ok(product) -> [product]
    Error(_) -> []
  }
  let variants = seed_variants_for_product(product_json)
  let store =
    proxy.store
    |> store_mod.upsert_base_products(products)
    |> store_mod.upsert_base_product_variants(variants)
    |> seed_options_for_product(product_json)
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_product_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.mutation.variables.input.id") {
    Some(JString(product_id)) -> {
      let product_json =
        JObject([
          #("id", JString(product_id)),
          #("title", JString("Product delete conformance seed")),
        ])
      seed_product_json(product_json, proxy)
    }
    _ -> proxy
  }
}

fn seed_product_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(capture, "$.mutation.response.data.productUpdate.product")
  {
    Some(product_json) -> seed_product_json(product_json, proxy)
    None -> proxy
  }
}

fn seed_tags_remove_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.mutation.response.data.tagsRemove.node") {
    Some(product_json) -> {
      case make_seed_product_relaxed(product_json) {
        Ok(post_product) -> {
          let removed_tags = case
            jsonpath.lookup(capture, "$.mutation.variables")
          {
            Some(variables) -> read_string_array_field(variables, "tags")
            None -> []
          }
          let search_lagged_tags = read_tags_remove_search_lagged_tags(capture)
          let queried_tags = read_tags_remove_queried_tags(capture)
          let base_tags =
            post_product.tags
            |> list.filter(fn(tag) {
              !list.contains(queried_tags, tag)
              || list.contains(search_lagged_tags, tag)
            })
            |> list.append(
              list.filter(removed_tags, fn(tag) {
                list.contains(search_lagged_tags, tag)
              }),
            )
            |> dedupe_strings_preserving_order
          let pre_mutation_tags =
            list.append(post_product.tags, removed_tags)
            |> dedupe_strings_preserving_order
          let base_product = ProductRecord(..post_product, tags: base_tags)
          let pre_mutation_product =
            ProductRecord(..post_product, tags: pre_mutation_tags)
          let base_store =
            store_mod.upsert_base_products(proxy.store, [base_product])
          let #(_, seeded_store) =
            store_mod.upsert_staged_product(base_store, pre_mutation_product)
          draft_proxy.DraftProxy(..proxy, store: seeded_store)
        }
        Error(_) -> proxy
      }
    }
    None -> proxy
  }
}

fn read_tags_remove_search_lagged_tags(capture: JsonValue) -> List(String) {
  let variables =
    jsonpath.lookup(capture, "$.downstreamReadVariables")
    |> option.unwrap(JObject([]))
  let data =
    jsonpath.lookup(capture, "$.downstreamRead.data")
    |> option.unwrap(JObject([]))
  ["remainingQuery", "removedQuery"]
  |> list.filter_map(fn(key) {
    let tag = read_tag_query_value(read_string_field(variables, key))
    let response_key = case key {
      "remainingQuery" -> "remaining"
      _ -> "removed"
    }
    let has_nodes = case read_object_field(data, response_key) {
      Some(connection) ->
        case read_array_field(connection, "nodes") {
          Some([_, ..]) -> True
          _ -> False
        }
      None -> False
    }
    case tag, has_nodes {
      Some(tag), True -> Ok(tag)
      _, _ -> Error(Nil)
    }
  })
}

fn read_tags_remove_queried_tags(capture: JsonValue) -> List(String) {
  let variables =
    jsonpath.lookup(capture, "$.downstreamReadVariables")
    |> option.unwrap(JObject([]))
  ["remainingQuery", "removedQuery"]
  |> list.filter_map(fn(key) {
    case read_tag_query_value(read_string_field(variables, key)) {
      Some(tag) -> Ok(tag)
      None -> Error(Nil)
    }
  })
}

fn read_tag_query_value(query: Option(String)) -> Option(String) {
  case query {
    Some(raw) ->
      case string.split_once(raw, "tag:") {
        Ok(#(_, tail)) -> {
          let token = case string.split_once(string.trim(tail), " ") {
            Ok(#(head, _)) -> head
            Error(_) -> string.trim(tail)
          }
          Some(strip_query_quotes(token))
        }
        Error(_) -> None
      }
    None -> None
  }
}

fn strip_query_quotes(value: String) -> String {
  let trimmed = string.trim(value)
  let trimmed = case string.ends_with(trimmed, ")") {
    True -> string.drop_end(trimmed, 1)
    False -> trimmed
  }
  case
    string.length(trimmed) >= 2
    && {
      let first = string.slice(trimmed, 0, 1)
      let last = string.slice(trimmed, string.length(trimmed) - 1, 1)
      first == last && { first == "\"" || first == "'" }
    }
  {
    True -> string.slice(trimmed, 1, string.length(trimmed) - 2)
    False -> trimmed
  }
}

fn dedupe_strings_preserving_order(values: List(String)) -> List(String) {
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

fn seed_product_variant_create_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.productVariantsBulkCreate.product",
    )
  {
    Some(product_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let variants = seed_variants_for_product(product_json) |> take_first(1)
      seed_product_and_base_variants(proxy, product, variants)
    }
    None -> proxy
  }
}

fn seed_product_variant_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.productVariantsBulkUpdate.product",
    ),
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.productVariantsBulkUpdate.productVariants[0]",
    )
  {
    Some(product_json), Some(variant_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let variants = case
        make_seed_product_variant_from_product_json(product_json, variant_json)
      {
        Ok(variant) -> [
          ProductVariantRecord(..variant, sku: None, selected_options: []),
        ]
        Error(_) -> []
      }
      seed_product_and_base_variants(proxy, product, variants)
    }
    _, _ -> proxy
  }
}

fn seed_product_variants_bulk_create_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamRead.data.product") {
    Some(product_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let variants = seed_variants_for_product(product_json) |> take_first(1)
      seed_product_and_base_variants(proxy, product, variants)
    }
    None -> proxy
  }
}

fn seed_product_variants_bulk_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.productVariantsBulkUpdate.product",
    ),
    jsonpath.lookup(capture, "$.downstreamRead.data.product.variants.nodes[0]")
  {
    Some(product_json), Some(variant_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let variants = case
        make_seed_product_variant_from_product_json(product_json, variant_json)
      {
        Ok(variant) -> [ProductVariantRecord(..variant, sku: None)]
        Error(_) -> []
      }
      seed_product_and_base_variants(proxy, product, variants)
    }
    _, _ -> proxy
  }
}

fn seed_product_variant_delete_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.downstreamRead.data.product") {
    Some(product_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let base_variants = seed_variants_for_product(product_json)
      let delete_variant = case
        jsonpath.lookup(capture, "$.mutation.variables.variantsIds[0]"),
        jsonpath.lookup(capture, "$.mutation.variables.productId")
      {
        Some(JString(variant_id)), Some(JString(product_id)) -> [
          minimal_seed_variant(product_id, variant_id),
        ]
        _, _ -> []
      }
      let proxy = seed_product_and_base_variants(proxy, product, base_variants)
      let staged_variants = list.append(delete_variant, base_variants)
      case product {
        Ok(product) -> {
          let store =
            store_mod.replace_staged_variants_for_product(
              proxy.store,
              product.id,
              staged_variants,
            )
          draft_proxy.DraftProxy(..proxy, store: store)
        }
        Error(_) -> proxy
      }
    }
    None -> proxy
  }
}

fn seed_product_variants_bulk_reorder_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.setup.productVariantsBulkCreate.data.productVariantsBulkCreate.product",
    )
  {
    Some(product_json) -> {
      let product = make_seed_product_relaxed(product_json)
      let variants = seed_variants_for_product(product_json)
      seed_product_and_base_variants(proxy, product, variants)
    }
    None -> proxy
  }
}

fn seed_inventory_quantity_roots_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let product_id =
    jsonpath.lookup(capture, "$.mutationEvidence.setup.productId")
    |> json_string_or("gid://shopify/Product/10171266400562")
  let variant_id =
    jsonpath.lookup(capture, "$.mutationEvidence.setup.variantId")
    |> json_string_or("gid://shopify/ProductVariant/51101855646002")
  let inventory_item_id =
    jsonpath.lookup(capture, "$.mutationEvidence.setup.inventoryItemId")
    |> json_string_or("gid://shopify/InventoryItem/53204673823026")
  let location_0_id =
    jsonpath.lookup(
      capture,
      "$.mutationEvidence.inventorySetQuantitiesAvailable.variables.input.quantities[0].locationId",
    )
    |> json_string_or("gid://shopify/Location/106318430514")
  let location_1_id =
    jsonpath.lookup(
      capture,
      "$.mutationEvidence.inventorySetQuantitiesAvailable.variables.input.quantities[1].locationId",
    )
    |> json_string_or("gid://shopify/Location/106318463282")
  let product =
    ProductRecord(
      id: product_id,
      legacy_resource_id: None,
      title: "Inventory quantity parity seed",
      handle: "inventory-quantity-parity-seed",
      status: "ACTIVE",
      vendor: None,
      product_type: None,
      tags: [],
      total_inventory: Some(0),
      tracks_inventory: Some(True),
      created_at: None,
      updated_at: None,
      description_html: "",
      online_store_preview_url: None,
      template_suffix: None,
      seo: ProductSeoRecord(title: None, description: None),
      category: None,
      cursor: None,
    )
  let variant =
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: "Default Title",
      sku: None,
      barcode: None,
      price: None,
      compare_at_price: None,
      taxable: None,
      inventory_policy: None,
      inventory_quantity: Some(0),
      selected_options: [],
      inventory_item: Some(
        InventoryItemRecord(
          id: inventory_item_id,
          tracked: Some(True),
          requires_shipping: None,
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [
            inventory_quantity_seed_level(
              inventory_item_id,
              location_0_id,
              "Shop location",
            ),
            inventory_quantity_seed_level(
              inventory_item_id,
              location_1_id,
              "My Custom Location",
            ),
          ],
        ),
      ),
      cursor: None,
    )
  let store =
    proxy.store
    |> store_mod.upsert_base_products([product])
    |> store_mod.upsert_base_product_variants([variant])
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn seed_inventory_adjust_quantities_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let location_id =
    jsonpath.lookup(capture, "$.setup.locationId")
    |> json_string_or("gid://shopify/Location/68509171945")
  let location_name =
    jsonpath.lookup(
      capture,
      "$.mutation.response.data.inventoryAdjustQuantities.inventoryAdjustmentGroup.changes[0].location.name",
    )
    |> json_string_or("103 ossington")
  let first_product_id =
    jsonpath.lookup(capture, "$.setup.products[0].productId")
    |> json_string_or("gid://shopify/Product/9257220145385")
  let first_variant_id =
    jsonpath.lookup(capture, "$.setup.products[0].variantId")
    |> json_string_or("gid://shopify/ProductVariant/50897202381033")
  let first_inventory_item_id =
    jsonpath.lookup(capture, "$.setup.products[0].inventoryItemId")
    |> json_string_or("gid://shopify/InventoryItem/53044947747049")
  let second_product_id =
    jsonpath.lookup(capture, "$.setup.products[1].productId")
    |> json_string_or("gid://shopify/Product/9257220178153")
  let second_variant_id =
    jsonpath.lookup(capture, "$.setup.products[1].variantId")
    |> json_string_or("gid://shopify/ProductVariant/50897202413801")
  let second_inventory_item_id =
    jsonpath.lookup(capture, "$.setup.products[1].inventoryItemId")
    |> json_string_or("gid://shopify/InventoryItem/53044947779817")
  let first_available =
    jsonpath.lookup(
      capture,
      "$.setup.seedAdjustment.data.inventoryAdjustQuantities.inventoryAdjustmentGroup.changes[0].delta",
    )
    |> json_int_or(3)
  let second_available =
    jsonpath.lookup(
      capture,
      "$.setup.seedAdjustment.data.inventoryAdjustQuantities.inventoryAdjustmentGroup.changes[1].delta",
    )
    |> json_int_or(7)
  let quantity_updated_at =
    jsonpath.lookup(
      capture,
      "$.nonAvailableMutation.downstreamRead.data.firstInventoryItem.inventoryLevels.nodes[0].quantities[0].updatedAt",
    )
    |> json_string_or("2026-04-18T22:21:57Z")
  let products = [
    inventory_adjust_seed_product(
      first_product_id,
      "inventory-adjust-quantities-first",
    ),
    inventory_adjust_seed_product(
      second_product_id,
      "inventory-adjust-quantities-second",
    ),
  ]
  let variants = [
    inventory_adjust_seed_variant(
      first_product_id,
      first_variant_id,
      first_inventory_item_id,
      location_id,
      location_name,
      first_available,
      quantity_updated_at,
    ),
    inventory_adjust_seed_variant(
      second_product_id,
      second_variant_id,
      second_inventory_item_id,
      location_id,
      location_name,
      second_available,
      quantity_updated_at,
    ),
  ]
  let matching_nodes = case
    jsonpath.lookup(capture, "$.downstreamRead.data.matching.nodes")
  {
    Some(JArray(nodes)) -> nodes
    _ -> []
  }
  let matching_products =
    list.filter_map(matching_nodes, make_seed_product_relaxed)
  let matching_variants =
    list.flat_map(matching_nodes, seed_variants_for_product)
  let store =
    proxy.store
    |> store_mod.upsert_base_products(list.append(products, matching_products))
    |> store_mod.upsert_base_product_variants(list.append(
      variants,
      matching_variants,
    ))
  draft_proxy.DraftProxy(..proxy, store: store)
}

fn inventory_adjust_seed_product(id: String, handle: String) -> ProductRecord {
  ProductRecord(
    id: id,
    legacy_resource_id: None,
    title: handle,
    handle: handle,
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: [],
    total_inventory: Some(0),
    tracks_inventory: Some(True),
    created_at: None,
    updated_at: None,
    description_html: "",
    online_store_preview_url: None,
    template_suffix: None,
    seo: ProductSeoRecord(title: None, description: None),
    category: None,
    cursor: None,
  )
}

fn inventory_adjust_seed_variant(
  product_id: String,
  variant_id: String,
  inventory_item_id: String,
  location_id: String,
  location_name: String,
  available: Int,
  quantity_updated_at: String,
) -> ProductVariantRecord {
  ProductVariantRecord(
    id: variant_id,
    product_id: product_id,
    title: "Default Title",
    sku: None,
    barcode: None,
    price: None,
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(available),
    selected_options: [],
    inventory_item: Some(
      InventoryItemRecord(
        id: inventory_item_id,
        tracked: Some(True),
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [
          InventoryLevelRecord(
            id: inventory_quantity_seed_level_id(inventory_item_id, location_id),
            location: InventoryLocationRecord(
              id: location_id,
              name: location_name,
            ),
            quantities: inventory_adjust_seed_quantities(
              available,
              quantity_updated_at,
            ),
            cursor: None,
          ),
        ],
      ),
    ),
    cursor: None,
  )
}

fn inventory_adjust_seed_quantities(
  available: Int,
  quantity_updated_at: String,
) -> List(InventoryQuantityRecord) {
  [
    InventoryQuantityRecord(
      name: "available",
      quantity: available,
      updated_at: Some(quantity_updated_at),
    ),
    InventoryQuantityRecord(
      name: "incoming",
      quantity: 0,
      updated_at: Some(quantity_updated_at),
    ),
    InventoryQuantityRecord(name: "reserved", quantity: 0, updated_at: None),
    InventoryQuantityRecord(name: "damaged", quantity: 0, updated_at: None),
    InventoryQuantityRecord(
      name: "quality_control",
      quantity: 0,
      updated_at: None,
    ),
    InventoryQuantityRecord(name: "safety_stock", quantity: 0, updated_at: None),
    InventoryQuantityRecord(name: "committed", quantity: 0, updated_at: None),
    InventoryQuantityRecord(
      name: "on_hand",
      quantity: available,
      updated_at: None,
    ),
  ]
}

fn seed_inventory_activate_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel",
    )
  {
    Some(level_json) -> {
      let product_id =
        jsonpath.lookup(
          capture,
          "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.product.id",
        )
        |> json_string_or("gid://shopify/Product/9257220047081")
      let variant_id =
        jsonpath.lookup(
          capture,
          "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.id",
        )
        |> json_string_or("gid://shopify/ProductVariant/50897202282729")
      let inventory_item_id =
        jsonpath.lookup(
          capture,
          "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.id",
        )
        |> json_string_or("gid://shopify/InventoryItem/53044947648745")
      let product =
        ProductRecord(
          id: product_id,
          legacy_resource_id: None,
          title: "inventory-activate-parity",
          handle: "inventory-activate-parity",
          status: "ACTIVE",
          vendor: None,
          product_type: None,
          tags: [],
          total_inventory: jsonpath.lookup(
            capture,
            "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.product.totalInventory",
          )
            |> json_int_or(0)
            |> Some,
          tracks_inventory: jsonpath.lookup(
            capture,
            "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.product.tracksInventory",
          )
            |> json_bool_or(False)
            |> Some,
          created_at: None,
          updated_at: None,
          description_html: "",
          online_store_preview_url: None,
          template_suffix: None,
          seo: ProductSeoRecord(title: None, description: None),
          category: None,
          cursor: None,
        )
      let variant =
        ProductVariantRecord(
          id: variant_id,
          product_id: product_id,
          title: "Default Title",
          sku: None,
          barcode: None,
          price: None,
          compare_at_price: None,
          taxable: None,
          inventory_policy: None,
          inventory_quantity: jsonpath.lookup(
            capture,
            "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.variant.inventoryQuantity",
          )
            |> json_int_or(0)
            |> Some,
          selected_options: [],
          inventory_item: Some(
            InventoryItemRecord(
              id: inventory_item_id,
              tracked: jsonpath.lookup(
                capture,
                "$.inventoryActivateNoOp.response.data.inventoryActivate.inventoryLevel.item.tracked",
              )
                |> json_bool_or(False)
                |> Some,
              requires_shipping: None,
              measurement: None,
              country_code_of_origin: None,
              province_code_of_origin: None,
              harmonized_system_code: None,
              inventory_levels: [
                make_seed_inventory_level(level_json, None)
                |> result.unwrap(inventory_quantity_seed_level(
                  inventory_item_id,
                  jsonpath.lookup(
                    capture,
                    "$.inventoryActivateNoOp.variables.locationId",
                  )
                    |> json_string_or("gid://shopify/Location/68509171945"),
                  "103 ossington",
                )),
              ],
            ),
          ),
          cursor: None,
        )
      let store =
        proxy.store
        |> store_mod.upsert_base_products([product])
        |> store_mod.upsert_base_product_variants([variant])
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn seed_inventory_item_update_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case
    jsonpath.lookup(
      capture,
      "$.mutation.create.response.data.productCreate.product",
    )
  {
    Some(product_json) -> {
      let products = case make_seed_product_relaxed(product_json) {
        Ok(product) -> [product]
        Error(_) -> []
      }
      let variants = seed_variants_for_product(product_json)
      let store =
        proxy.store
        |> store_mod.upsert_base_products(products)
        |> store_mod.upsert_base_product_variants(variants)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    None -> proxy
  }
}

fn inventory_quantity_seed_level(
  inventory_item_id: String,
  location_id: String,
  location_name: String,
) -> InventoryLevelRecord {
  InventoryLevelRecord(
    id: inventory_quantity_seed_level_id(inventory_item_id, location_id),
    location: InventoryLocationRecord(id: location_id, name: location_name),
    quantities: [
      InventoryQuantityRecord(name: "available", quantity: 0, updated_at: None),
      InventoryQuantityRecord(name: "on_hand", quantity: 0, updated_at: None),
      InventoryQuantityRecord(name: "damaged", quantity: 0, updated_at: None),
    ],
    cursor: None,
  )
}

fn inventory_quantity_seed_level_id(
  inventory_item_id: String,
  location_id: String,
) -> String {
  let inventory_item_tail =
    inventory_item_id |> string.split("/") |> list.last |> result.unwrap("0")
  let location_tail =
    location_id |> string.split("/") |> list.last |> result.unwrap("0")
  "gid://shopify/InventoryLevel/" <> inventory_item_tail <> "-" <> location_tail
}

fn seed_product_and_base_variants(
  proxy: DraftProxy,
  product: Result(ProductRecord, Nil),
  variants: List(ProductVariantRecord),
) -> DraftProxy {
  case product {
    Ok(product) -> {
      let store =
        proxy.store
        |> store_mod.upsert_base_products([product])
        |> store_mod.upsert_base_product_variants(variants)
      draft_proxy.DraftProxy(..proxy, store: store)
    }
    Error(_) -> proxy
  }
}

fn make_seed_product_variant_from_product_json(
  product_json: JsonValue,
  variant_json: JsonValue,
) -> Result(ProductVariantRecord, Nil) {
  use product_id <- result.try(required_string_field(product_json, "id"))
  make_seed_product_variant(product_id, variant_json, None)
}

fn minimal_seed_variant(
  product_id: String,
  variant_id: String,
) -> ProductVariantRecord {
  ProductVariantRecord(
    id: variant_id,
    product_id: product_id,
    title: "Deleted variant seed",
    sku: None,
    barcode: None,
    price: None,
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(0),
    selected_options: [],
    inventory_item: None,
    cursor: None,
  )
}

fn take_first(items: List(a), count: Int) -> List(a) {
  case items, count <= 0 {
    _, True -> []
    [], False -> []
    [first, ..rest], False -> [first, ..take_first(rest, count - 1)]
  }
}

fn make_seed_product_from_edge(edge: JsonValue) -> Result(ProductRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) -> {
      let cursor = read_string_field(edge, "cursor")
      make_seed_product_with_cursor(node, cursor)
    }
    None -> Error(Nil)
  }
}

fn make_seed_product_relaxed(source: JsonValue) -> Result(ProductRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  let title = read_string_field(source, "title") |> option.unwrap("")
  Ok(ProductRecord(
    id: id,
    legacy_resource_id: read_string_field(source, "legacyResourceId"),
    title: title,
    handle: read_string_field(source, "handle") |> option.unwrap(""),
    status: read_string_field(source, "status") |> option.unwrap("ACTIVE"),
    vendor: read_string_field(source, "vendor"),
    product_type: read_string_field(source, "productType"),
    tags: read_string_array_field(source, "tags"),
    total_inventory: read_int_field(source, "totalInventory"),
    tracks_inventory: read_bool_field(source, "tracksInventory"),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
    description_html: read_string_field(source, "descriptionHtml")
      |> option.unwrap(""),
    online_store_preview_url: read_string_field(source, "onlineStorePreviewUrl"),
    template_suffix: read_string_field(source, "templateSuffix"),
    seo: make_seed_product_seo(read_object_field(source, "seo")),
    category: read_object_field(source, "category")
      |> option.then(make_seed_product_category),
    cursor: None,
  ))
}

fn make_seed_product_with_cursor(
  source: JsonValue,
  cursor: Option(String),
) -> Result(ProductRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use title <- result.try(required_string_field(source, "title"))
  use handle <- result.try(required_string_field(source, "handle"))
  use status <- result.try(required_string_field(source, "status"))
  let seo = make_seed_product_seo(read_object_field(source, "seo"))
  let category =
    read_object_field(source, "category")
    |> option.then(make_seed_product_category)
  Ok(ProductRecord(
    id: id,
    legacy_resource_id: read_string_field(source, "legacyResourceId"),
    title: title,
    handle: handle,
    status: status,
    vendor: read_string_field(source, "vendor"),
    product_type: read_string_field(source, "productType"),
    tags: read_string_array_field(source, "tags"),
    total_inventory: read_int_field(source, "totalInventory"),
    tracks_inventory: read_bool_field(source, "tracksInventory"),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
    description_html: read_string_field(source, "descriptionHtml")
      |> option.unwrap(""),
    online_store_preview_url: read_string_field(source, "onlineStorePreviewUrl"),
    template_suffix: read_string_field(source, "templateSuffix"),
    seo: seo,
    category: category,
    cursor: cursor,
  ))
}

fn seed_variants_for_product(source: JsonValue) -> List(ProductVariantRecord) {
  case required_string_field(source, "id") {
    Ok(product_id) ->
      case read_object_field(source, "variants") {
        Some(connection) ->
          case read_array_field(connection, "edges") {
            Some(edges) ->
              list.filter_map(edges, fn(edge) {
                make_seed_product_variant_from_edge(product_id, edge)
              })
            None ->
              case read_array_field(connection, "nodes") {
                Some(nodes) ->
                  list.filter_map(nodes, fn(node) {
                    make_seed_product_variant(product_id, node, None)
                  })
                None -> []
              }
          }
        None -> []
      }
    Error(_) -> []
  }
}

fn make_seed_product_variant(
  product_id: String,
  source: JsonValue,
  cursor: Option(String),
) -> Result(ProductVariantRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  let selected_options = case read_array_field(source, "selectedOptions") {
    Some(nodes) -> list.filter_map(nodes, make_seed_selected_option)
    None -> []
  }
  Ok(ProductVariantRecord(
    id: id,
    product_id: product_id,
    title: read_string_field(source, "title") |> option.unwrap("Default Title"),
    sku: read_string_field(source, "sku"),
    barcode: read_string_field(source, "barcode"),
    price: read_string_field(source, "price"),
    compare_at_price: read_string_field(source, "compareAtPrice"),
    taxable: read_bool_field(source, "taxable"),
    inventory_policy: read_string_field(source, "inventoryPolicy"),
    inventory_quantity: read_int_field(source, "inventoryQuantity"),
    selected_options: selected_options,
    inventory_item: make_seed_inventory_item(read_object_field(
      source,
      "inventoryItem",
    )),
    cursor: cursor,
  ))
}

fn make_seed_product_variant_from_edge(
  product_id: String,
  edge: JsonValue,
) -> Result(ProductVariantRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) ->
      make_seed_product_variant(
        product_id,
        node,
        read_string_field(edge, "cursor"),
      )
    None -> Error(Nil)
  }
}

fn make_seed_inventory_item(
  source: Option(JsonValue),
) -> Option(InventoryItemRecord) {
  case source {
    Some(value) ->
      case required_string_field(value, "id") {
        Ok(id) ->
          Some(InventoryItemRecord(
            id: id,
            tracked: read_bool_field(value, "tracked"),
            requires_shipping: read_bool_field(value, "requiresShipping"),
            measurement: make_seed_inventory_measurement(read_object_field(
              value,
              "measurement",
            )),
            country_code_of_origin: read_string_field(
              value,
              "countryCodeOfOrigin",
            ),
            province_code_of_origin: read_string_field(
              value,
              "provinceCodeOfOrigin",
            ),
            harmonized_system_code: read_string_field(
              value,
              "harmonizedSystemCode",
            ),
            inventory_levels: read_seed_inventory_levels(value),
          ))
        Error(_) -> None
      }
    None -> None
  }
}

fn make_seed_inventory_measurement(
  source: Option(JsonValue),
) -> Option(InventoryMeasurementRecord) {
  case source {
    Some(value) ->
      Some(
        InventoryMeasurementRecord(
          weight: make_seed_inventory_weight(read_object_field(value, "weight")),
        ),
      )
    None -> None
  }
}

fn make_seed_inventory_weight(
  source: Option(JsonValue),
) -> Option(InventoryWeightRecord) {
  case source {
    Some(value) ->
      case
        read_string_field(value, "unit"),
        read_inventory_weight_value(value)
      {
        Some(unit), Some(weight_value) ->
          Some(InventoryWeightRecord(unit: unit, value: weight_value))
        _, _ -> None
      }
    None -> None
  }
}

fn read_inventory_weight_value(value: JsonValue) {
  case json_value.field(value, "value") {
    Some(JInt(i)) -> Some(InventoryWeightInt(i))
    Some(JFloat(f)) -> Some(InventoryWeightFloat(f))
    _ -> None
  }
}

fn read_seed_inventory_levels(source: JsonValue) -> List(InventoryLevelRecord) {
  case read_object_field(source, "inventoryLevels") {
    Some(connection) ->
      case read_array_field(connection, "edges") {
        Some(edges) ->
          list.filter_map(edges, fn(edge) {
            make_seed_inventory_level_from_edge(edge)
          })
        None ->
          case read_array_field(connection, "nodes") {
            Some(nodes) ->
              list.filter_map(nodes, fn(node) {
                make_seed_inventory_level(node, None)
              })
            None -> []
          }
      }
    None ->
      case read_array_field(source, "inventoryLevels") {
        Some(nodes) ->
          list.filter_map(nodes, fn(node) {
            make_seed_inventory_level(node, None)
          })
        None -> []
      }
  }
}

fn make_seed_inventory_level_from_edge(
  edge: JsonValue,
) -> Result(InventoryLevelRecord, Nil) {
  case read_object_field(edge, "node") {
    Some(node) ->
      make_seed_inventory_level(node, read_string_field(edge, "cursor"))
    None -> Error(Nil)
  }
}

fn make_seed_inventory_level(
  source: JsonValue,
  cursor: Option(String),
) -> Result(InventoryLevelRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use location <- result.try(
    make_seed_inventory_location(read_object_field(source, "location")),
  )
  Ok(InventoryLevelRecord(
    id: id,
    location: location,
    quantities: read_array_field(source, "quantities")
      |> option.unwrap([])
      |> list.filter_map(make_seed_inventory_quantity),
    cursor: cursor,
  ))
}

fn make_seed_inventory_location(
  source: Option(JsonValue),
) -> Result(InventoryLocationRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      use name <- result.try(required_string_field(value, "name"))
      Ok(InventoryLocationRecord(id: id, name: name))
    }
    None -> Error(Nil)
  }
}

fn make_seed_inventory_quantity(
  source: JsonValue,
) -> Result(InventoryQuantityRecord, Nil) {
  use name <- result.try(required_string_field(source, "name"))
  use quantity <- result.try(required_int_field(source, "quantity"))
  Ok(InventoryQuantityRecord(
    name: name,
    quantity: quantity,
    updated_at: read_string_field(source, "updatedAt"),
  ))
}

fn make_seed_selected_option(
  source: JsonValue,
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  use name <- result.try(required_string_field(source, "name"))
  use value <- result.try(required_string_field(source, "value"))
  Ok(ProductVariantSelectedOptionRecord(name: name, value: value))
}

fn make_seed_product_seo(source: Option(JsonValue)) -> ProductSeoRecord {
  ProductSeoRecord(
    title: read_string_field_from_option(source, "title"),
    description: read_string_field_from_option(source, "description"),
  )
}

fn make_seed_product_category(
  source: JsonValue,
) -> Option(ProductCategoryRecord) {
  case required_string_field(source, "id") {
    Ok(id) ->
      Some(ProductCategoryRecord(
        id: id,
        full_name: read_string_field(source, "fullName") |> option.unwrap(""),
      ))
    Error(_) -> None
  }
}

fn seed_shop_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.readOnlyBaselines.shop.data.shop") {
    Some(shop_json) ->
      case make_seed_shop(shop_json) {
        Ok(shop) ->
          draft_proxy.DraftProxy(
            ..proxy,
            store: store_mod.upsert_base_shop(proxy.store, shop),
          )
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn make_seed_shop(source: JsonValue) -> Result(ShopRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  use myshopify_domain <- result.try(required_string_field(
    source,
    "myshopifyDomain",
  ))
  use url <- result.try(required_string_field(source, "url"))
  use primary_domain <- result.try(
    make_seed_shop_domain(read_object_field(source, "primaryDomain")),
  )
  use shop_address <- result.try(
    make_seed_shop_address(read_object_field(source, "shopAddress")),
  )
  use plan <- result.try(make_seed_shop_plan(read_object_field(source, "plan")))
  use resource_limits <- result.try(
    make_seed_resource_limits(read_object_field(source, "resourceLimits")),
  )
  use features <- result.try(
    make_seed_shop_features(read_object_field(source, "features")),
  )
  let payment_settings =
    make_seed_payment_settings(read_object_field(source, "paymentSettings"))
  let policies =
    read_array_field(source, "shopPolicies")
    |> option.unwrap([])
    |> list.filter_map(make_seed_shop_policy)
  Ok(ShopRecord(
    id: id,
    name: name,
    myshopify_domain: myshopify_domain,
    url: url,
    primary_domain: primary_domain,
    contact_email: read_string_field(source, "contactEmail")
      |> option.unwrap(""),
    email: read_string_field(source, "email") |> option.unwrap(""),
    currency_code: read_string_field(source, "currencyCode")
      |> option.unwrap(""),
    enabled_presentment_currencies: read_string_array_field(
      source,
      "enabledPresentmentCurrencies",
    ),
    iana_timezone: read_string_field(source, "ianaTimezone")
      |> option.unwrap(""),
    timezone_abbreviation: read_string_field(source, "timezoneAbbreviation")
      |> option.unwrap(""),
    timezone_offset: read_string_field(source, "timezoneOffset")
      |> option.unwrap(""),
    timezone_offset_minutes: read_int_field(source, "timezoneOffsetMinutes")
      |> option.unwrap(0),
    taxes_included: read_bool_field(source, "taxesIncluded")
      |> option.unwrap(False),
    tax_shipping: read_bool_field(source, "taxShipping")
      |> option.unwrap(False),
    unit_system: read_string_field(source, "unitSystem") |> option.unwrap(""),
    weight_unit: read_string_field(source, "weightUnit") |> option.unwrap(""),
    shop_address: shop_address,
    plan: plan,
    resource_limits: resource_limits,
    features: features,
    payment_settings: payment_settings,
    shop_policies: policies,
  ))
}

fn make_seed_shop_domain(
  source: Option(JsonValue),
) -> Result(ShopDomainRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      use host <- result.try(required_string_field(value, "host"))
      use url <- result.try(required_string_field(value, "url"))
      Ok(ShopDomainRecord(
        id: id,
        host: host,
        url: url,
        ssl_enabled: read_bool_field(value, "sslEnabled")
          |> option.unwrap(False),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_shop_address(
  source: Option(JsonValue),
) -> Result(ShopAddressRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      Ok(ShopAddressRecord(
        id: id,
        address1: read_string_field(value, "address1"),
        address2: read_string_field(value, "address2"),
        city: read_string_field(value, "city"),
        company: read_string_field(value, "company"),
        coordinates_validated: read_bool_field(value, "coordinatesValidated")
          |> option.unwrap(False),
        country: read_string_field(value, "country"),
        country_code_v2: read_string_field(value, "countryCodeV2"),
        formatted: read_string_array_field(value, "formatted"),
        formatted_area: read_string_field(value, "formattedArea"),
        latitude: read_float_field(value, "latitude"),
        longitude: read_float_field(value, "longitude"),
        phone: read_string_field(value, "phone"),
        province: read_string_field(value, "province"),
        province_code: read_string_field(value, "provinceCode"),
        zip: read_string_field(value, "zip"),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_shop_plan(
  source: Option(JsonValue),
) -> Result(ShopPlanRecord, Nil) {
  case source {
    Some(value) ->
      Ok(ShopPlanRecord(
        partner_development: read_bool_field(value, "partnerDevelopment")
          |> option.unwrap(False),
        public_display_name: read_string_field(value, "publicDisplayName")
          |> option.unwrap(""),
        shopify_plus: read_bool_field(value, "shopifyPlus")
          |> option.unwrap(False),
      ))
    None -> Error(Nil)
  }
}

fn make_seed_resource_limits(
  source: Option(JsonValue),
) -> Result(ShopResourceLimitsRecord, Nil) {
  case source {
    Some(value) ->
      Ok(ShopResourceLimitsRecord(
        location_limit: read_int_field(value, "locationLimit")
          |> option.unwrap(0),
        max_product_options: read_int_field(value, "maxProductOptions")
          |> option.unwrap(0),
        max_product_variants: read_int_field(value, "maxProductVariants")
          |> option.unwrap(0),
        redirect_limit_reached: read_bool_field(value, "redirectLimitReached")
          |> option.unwrap(False),
      ))
    None -> Error(Nil)
  }
}

fn make_seed_shop_features(
  source: Option(JsonValue),
) -> Result(ShopFeaturesRecord, Nil) {
  case source {
    Some(value) -> {
      let bundles = case read_object_field(value, "bundles") {
        Some(b) ->
          ShopBundlesFeatureRecord(
            eligible_for_bundles: read_bool_field(b, "eligibleForBundles")
              |> option.unwrap(False),
            ineligibility_reason: read_string_field(b, "ineligibilityReason"),
            sells_bundles: read_bool_field(b, "sellsBundles")
              |> option.unwrap(False),
          )
        None ->
          ShopBundlesFeatureRecord(
            eligible_for_bundles: False,
            ineligibility_reason: None,
            sells_bundles: False,
          )
      }
      let operations = case
        read_object_field(value, "cartTransform")
        |> option.then(fn(cart) {
          read_object_field(cart, "eligibleOperations")
        })
      {
        Some(op) ->
          ShopCartTransformEligibleOperationsRecord(
            expand_operation: read_bool_field(op, "expandOperation")
              |> option.unwrap(False),
            merge_operation: read_bool_field(op, "mergeOperation")
              |> option.unwrap(False),
            update_operation: read_bool_field(op, "updateOperation")
              |> option.unwrap(False),
          )
        None ->
          ShopCartTransformEligibleOperationsRecord(
            expand_operation: False,
            merge_operation: False,
            update_operation: False,
          )
      }
      Ok(ShopFeaturesRecord(
        avalara_avatax: read_bool_field(value, "avalaraAvatax")
          |> option.unwrap(False),
        branding: read_string_field(value, "branding") |> option.unwrap(""),
        bundles: bundles,
        captcha: read_bool_field(value, "captcha") |> option.unwrap(False),
        cart_transform: ShopCartTransformFeatureRecord(
          eligible_operations: operations,
        ),
        dynamic_remarketing: read_bool_field(value, "dynamicRemarketing")
          |> option.unwrap(False),
        eligible_for_subscription_migration: read_bool_field(
          value,
          "eligibleForSubscriptionMigration",
        )
          |> option.unwrap(False),
        eligible_for_subscriptions: read_bool_field(
          value,
          "eligibleForSubscriptions",
        )
          |> option.unwrap(False),
        gift_cards: read_bool_field(value, "giftCards") |> option.unwrap(False),
        harmonized_system_code: read_bool_field(value, "harmonizedSystemCode")
          |> option.unwrap(False),
        legacy_subscription_gateway_enabled: read_bool_field(
          value,
          "legacySubscriptionGatewayEnabled",
        )
          |> option.unwrap(False),
        live_view: read_bool_field(value, "liveView") |> option.unwrap(False),
        paypal_express_subscription_gateway_status: read_string_field(
          value,
          "paypalExpressSubscriptionGatewayStatus",
        )
          |> option.unwrap(""),
        reports: read_bool_field(value, "reports") |> option.unwrap(False),
        sells_subscriptions: read_bool_field(value, "sellsSubscriptions")
          |> option.unwrap(False),
        show_metrics: read_bool_field(value, "showMetrics")
          |> option.unwrap(False),
        storefront: read_bool_field(value, "storefront") |> option.unwrap(False),
        unified_markets: read_bool_field(value, "unifiedMarkets")
          |> option.unwrap(False),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_payment_settings(
  source: Option(JsonValue),
) -> PaymentSettingsRecord {
  PaymentSettingsRecord(supported_digital_wallets: case source {
    Some(value) -> read_string_array_field(value, "supportedDigitalWallets")
    None -> []
  })
}

fn make_seed_shop_policy(source: JsonValue) -> Result(ShopPolicyRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use title <- result.try(required_string_field(source, "title"))
  use body <- result.try(required_string_field(source, "body"))
  use type_ <- result.try(required_string_field(source, "type"))
  use url <- result.try(required_string_field(source, "url"))
  use created_at <- result.try(required_string_field(source, "createdAt"))
  use updated_at <- result.try(required_string_field(source, "updatedAt"))
  Ok(ShopPolicyRecord(
    id: id,
    title: title,
    body: body,
    type_: type_,
    url: url,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

fn seed_shopify_function_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records = case jsonpath.lookup(capture, "$.seedShopifyFunctions") {
    Some(JArray(nodes)) -> list.filter_map(nodes, make_seed_shopify_function)
    _ -> []
  }

  let seeded_store =
    list.fold(records, proxy.store, fn(current_store, record) {
      let #(_, next_store) =
        store_mod.upsert_staged_shopify_function(current_store, record)
      next_store
    })

  // The local-runtime fixture was captured after the function metadata
  // seed step had advanced the synthetic counters once.
  let #(_, identity_after_id) =
    synthetic_identity.make_synthetic_gid(
      proxy.synthetic_identity,
      "MutationLogEntry",
    )
  let #(_, identity_after_seed) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)

  draft_proxy.DraftProxy(
    ..proxy,
    store: seeded_store,
    synthetic_identity: identity_after_seed,
  )
}

fn make_seed_shopify_function(
  source: JsonValue,
) -> Result(ShopifyFunctionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(
    ShopifyFunctionRecord(
      id: id,
      title: read_string_field(source, "title"),
      handle: read_string_field(source, "handle"),
      api_type: read_string_field(source, "apiType"),
      description: read_string_field(source, "description"),
      app_key: read_string_field(source, "appKey"),
      app: case read_object_field(source, "app") {
        Some(app) -> Some(make_seed_shopify_function_app(app))
        None -> None
      },
    ),
  )
}

fn make_seed_shopify_function_app(
  source: JsonValue,
) -> ShopifyFunctionAppRecord {
  ShopifyFunctionAppRecord(
    typename: read_string_field(source, "__typename"),
    id: read_string_field(source, "id"),
    title: read_string_field(source, "title"),
    handle: read_string_field(source, "handle"),
    api_key: read_string_field(source, "apiKey"),
  )
}

fn seed_gift_card_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records =
    [
      jsonpath.lookup(
        capture,
        "$.operations.create.response.payload.data.giftCardCreate.giftCard",
      ),
      jsonpath.lookup(
        capture,
        "$.create.response.payload.data.giftCardCreate.giftCard",
      ),
    ]
    |> list.filter_map(fn(candidate) {
      case candidate {
        Some(value) -> make_seed_gift_card(value, Some("api_client"))
        None -> Error(Nil)
      }
    })

  let empty_read_records = case
    jsonpath.lookup(
      capture,
      "$.operations.emptyRead.response.payload.data.giftCards.nodes",
    )
  {
    Some(JArray(nodes)) ->
      list.filter_map(nodes, fn(node) { make_seed_gift_card(node, None) })
    _ -> []
  }

  let records = list.append(records, empty_read_records)
  let seeded_store = case records {
    [] -> proxy.store
    _ -> store_mod.upsert_base_gift_cards(proxy.store, records)
  }
  let seeded_store = case seed_gift_card_configuration(capture) {
    Some(configuration) ->
      store_mod.upsert_base_gift_card_configuration(seeded_store, configuration)
    None -> seeded_store
  }
  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn make_seed_gift_card(
  source: JsonValue,
  source_override: Option(String),
) -> Result(GiftCardRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  case string.starts_with(id, "gid://shopify/GiftCard/") {
    False -> Error(Nil)
    True -> {
      let last_characters =
        read_string_field(source, "lastCharacters")
        |> option.unwrap(gift_card_tail(id))
      let initial_value =
        read_money_record(read_object_field(source, "initialValue"))
      let balance =
        read_money_record(
          read_object_field(source, "balance")
          |> option.or(read_object_field(source, "initialValue")),
        )
      let recipient_attributes_source =
        read_object_field(source, "recipientAttributes")
      let recipient_source =
        recipient_attributes_source
        |> option.then(read_object_field(_, "recipient"))
      let recipient_id =
        read_string_field_from_option(recipient_source, "id")
        |> option.or(read_string_field_from_option(
          read_object_field(source, "recipient"),
          "id",
        ))
      let transactions =
        read_transactions(read_object_field(source, "transactions"))
      Ok(GiftCardRecord(
        id: id,
        legacy_resource_id: read_string_field(source, "legacyResourceId")
          |> option.unwrap(gift_card_tail(id)),
        last_characters: last_characters,
        masked_code: read_string_field(source, "maskedCode")
          |> option.unwrap(masked_code(last_characters)),
        enabled: read_bool_field(source, "enabled") |> option.unwrap(True),
        deactivated_at: read_string_field(source, "deactivatedAt"),
        expires_on: read_string_field(source, "expiresOn"),
        note: read_string_field(source, "note"),
        template_suffix: read_string_field(source, "templateSuffix"),
        created_at: read_string_field(source, "createdAt")
          |> option.unwrap("2026-01-01T00:00:00Z"),
        updated_at: read_string_field(source, "updatedAt")
          |> option.unwrap("2026-01-01T00:00:00Z"),
        initial_value: initial_value,
        balance: balance,
        customer_id: read_string_field_from_option(
          read_object_field(source, "customer"),
          "id",
        ),
        recipient_id: recipient_id,
        source: case source_override {
          Some(_) -> source_override
          None -> read_string_field(source, "source")
        },
        recipient_attributes: make_seed_recipient_attributes(
          recipient_attributes_source,
          recipient_id,
        ),
        transactions: transactions,
      ))
    }
  }
}

fn seed_gift_card_configuration(
  capture: JsonValue,
) -> Option(GiftCardConfigurationRecord) {
  let primary =
    jsonpath.lookup(
      capture,
      "$.operations.configurationRead.response.payload.data.giftCardConfiguration",
    )
  let fallback =
    jsonpath.lookup(
      capture,
      "$.configurationRead.response.payload.data.giftCardConfiguration",
    )
  case primary |> option.or(fallback) {
    Some(value) ->
      Some(GiftCardConfigurationRecord(
        issue_limit: read_money_record(read_object_field(value, "issueLimit")),
        purchase_limit: read_money_record(read_object_field(
          value,
          "purchaseLimit",
        )),
      ))
    None -> None
  }
}

fn make_seed_recipient_attributes(
  source: Option(JsonValue),
  recipient_id: Option(String),
) -> Option(GiftCardRecipientAttributesRecord) {
  case source {
    None -> None
    Some(value) ->
      Some(GiftCardRecipientAttributesRecord(
        id: recipient_id,
        message: read_string_field(value, "message"),
        preferred_name: read_string_field(value, "preferredName"),
        send_notification_at: read_string_field(value, "sendNotificationAt"),
      ))
  }
}

fn read_transactions(
  source: Option(JsonValue),
) -> List(GiftCardTransactionRecord) {
  case source |> option.then(read_array_field(_, "nodes")) {
    Some(nodes) ->
      list.filter_map(nodes, fn(node) {
        let amount = read_money_record(read_object_field(node, "amount"))
        Ok(GiftCardTransactionRecord(
          id: read_string_field(node, "id")
            |> option.unwrap("gid://shopify/GiftCardTransaction/0"),
          kind: case string.starts_with(amount.amount, "-") {
            True -> "DEBIT"
            False -> "CREDIT"
          },
          amount: amount,
          processed_at: read_string_field(node, "processedAt")
            |> option.unwrap("2026-01-01T00:00:00Z"),
          note: read_string_field(node, "note"),
        ))
      })
    None -> []
  }
}

fn read_money_record(source: Option(JsonValue)) -> Money {
  case source {
    Some(value) ->
      Money(
        amount: read_string_field(value, "amount") |> option.unwrap("0.0"),
        currency_code: read_string_field(value, "currencyCode")
          |> option.unwrap("CAD"),
      )
    None -> Money(amount: "0.0", currency_code: "CAD")
  }
}

fn required_string_field(
  value: JsonValue,
  name: String,
) -> Result(String, Nil) {
  case read_string_field(value, name) {
    Some(s) -> Ok(s)
    None -> Error(Nil)
  }
}

fn required_object_field(
  value: JsonValue,
  name: String,
) -> Result(JsonValue, Nil) {
  case read_object_field(value, name) {
    Some(object) -> Ok(object)
    None -> Error(Nil)
  }
}

fn required_int_field(value: JsonValue, name: String) -> Result(Int, Nil) {
  case read_int_field(value, name) {
    Some(i) -> Ok(i)
    None -> Error(Nil)
  }
}

fn json_string_option(value: Option(JsonValue)) -> Option(String) {
  case value {
    Some(JString(value)) -> Some(value)
    _ -> None
  }
}

fn read_string_field(value: JsonValue, name: String) -> Option(String) {
  case json_value.field(value, name) {
    Some(JString(s)) -> Some(s)
    _ -> None
  }
}

fn read_string_field_from_option(
  value: Option(JsonValue),
  name: String,
) -> Option(String) {
  case value {
    Some(v) -> read_string_field(v, name)
    None -> None
  }
}

fn read_bool_field(value: JsonValue, name: String) -> Option(Bool) {
  case json_value.field(value, name) {
    Some(JBool(b)) -> Some(b)
    _ -> None
  }
}

fn read_int_field(value: JsonValue, name: String) -> Option(Int) {
  case json_value.field(value, name) {
    Some(JInt(i)) -> Some(i)
    _ -> None
  }
}

fn read_float_field(value: JsonValue, name: String) -> Option(Float) {
  case json_value.field(value, name) {
    Some(JFloat(f)) -> Some(f)
    Some(JInt(i)) -> Some(int.to_float(i))
    _ -> None
  }
}

fn read_string_array_field(value: JsonValue, name: String) -> List(String) {
  case read_array_field(value, name) {
    Some(items) ->
      list.filter_map(items, fn(item) {
        case item {
          JString(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    None -> []
  }
}

fn read_object_field(value: JsonValue, name: String) -> Option(JsonValue) {
  case json_value.field(value, name) {
    Some(JObject(_)) as object -> object
    _ -> None
  }
}

fn read_array_field(value: JsonValue, name: String) -> Option(List(JsonValue)) {
  case json_value.field(value, name) {
    Some(JArray(items)) -> Some(items)
    _ -> None
  }
}

fn json_string_or(value: Option(JsonValue), fallback: String) -> String {
  case value {
    Some(JString(value)) -> value
    _ -> fallback
  }
}

fn json_int_or(value: Option(JsonValue), fallback: Int) -> Int {
  case value {
    Some(JInt(value)) -> value
    _ -> fallback
  }
}

fn json_bool_or(value: Option(JsonValue), fallback: Bool) -> Bool {
  case value {
    Some(JBool(value)) -> value
    _ -> fallback
  }
}

fn enumerate_json_values(items: List(JsonValue)) -> List(#(JsonValue, Int)) {
  enumerate_json_values_loop(items, 0, [])
}

fn enumerate_strings(items: List(String)) -> List(#(String, Int)) {
  enumerate_strings_loop(items, 0, [])
}

fn enumerate_strings_loop(
  items: List(String),
  index: Int,
  acc: List(#(String, Int)),
) -> List(#(String, Int)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      enumerate_strings_loop(rest, index + 1, [#(first, index), ..acc])
  }
}

fn enumerate_json_values_loop(
  items: List(JsonValue),
  index: Int,
  acc: List(#(JsonValue, Int)),
) -> List(#(JsonValue, Int)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      enumerate_json_values_loop(rest, index + 1, [#(first, index), ..acc])
  }
}

fn gift_card_tail(id: String) -> String {
  case string.split(id, on: "/") |> list.last {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, on: "?") {
        [tail, ..] -> tail
        [] -> id
      }
    Error(_) -> id
  }
}

fn masked_code(last_characters: String) -> String {
  "•••• •••• •••• " <> last_characters
}

fn run_targets(
  config: RunnerConfig,
  parsed: Spec,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(DraftProxy, List(TargetReport)), RunError) {
  list.try_fold(parsed.targets, #(proxy, []), fn(state, target) {
    let #(current_proxy, acc_reports) = state
    use #(next_proxy, report) <- result.try(run_target(
      config,
      parsed,
      target,
      capture,
      primary_response,
      current_proxy,
    ))
    Ok(#(next_proxy, [report, ..acc_reports]))
  })
  |> result.map(fn(state) {
    let #(final_proxy, reports) = state
    #(final_proxy, list.reverse(reports))
  })
}

fn run_target(
  config: RunnerConfig,
  parsed: Spec,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(DraftProxy, TargetReport), RunError) {
  use #(actual_response, next_proxy) <- result.try(actual_response_for(
    config,
    target,
    capture,
    primary_response,
    proxy,
  ))
  let expected_opt = jsonpath.lookup(capture, target.capture_path)
  let actual_opt = jsonpath.lookup(actual_response, target.proxy_path)
  case expected_opt, actual_opt {
    None, _ ->
      Error(CaptureUnresolved(target: target.name, path: target.capture_path))
    _, None ->
      Error(ProxyUnresolved(target: target.name, path: target.proxy_path))
    Some(expected), Some(actual) -> {
      let rules = spec.rules_for(parsed, target)
      let mismatches = diff.diff_with_expected(expected, actual, rules)
      Ok(#(
        next_proxy,
        TargetReport(
          name: target.name,
          capture_path: target.capture_path,
          proxy_path: target.proxy_path,
          mismatches: mismatches,
        ),
      ))
    }
  }
}

/// Resolve which JsonValue tree to use as the proxy-side response for
/// a target. Targets without a per-target override reuse the primary
/// response (no extra HTTP call). Override targets execute their own
/// request, threading proxy state forward.
fn actual_response_for(
  config: RunnerConfig,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(JsonValue, DraftProxy), RunError) {
  case target.request {
    ReusePrimary -> Ok(#(primary_response, proxy))
    OverrideRequest(request: request) -> {
      case target.upstream_capture_path {
        Some(path) ->
          case jsonpath.lookup(capture, path) {
            Some(value) -> Ok(#(value, proxy))
            None -> Error(CaptureUnresolved(target: target.name, path: path))
          }
        None -> {
          use document <- result.try(
            read_file(resolve(config, request.document_path)),
          )
          use variables <- result.try(resolve_variables(
            config,
            request.variables,
            capture,
            Some(primary_response),
            target.name,
          ))
          use #(response, next_proxy) <- result.try(execute(
            proxy,
            document,
            variables,
            target.name,
          ))
          use value <- result.try(parse_response_body(response))
          Ok(#(value, next_proxy))
        }
      }
    }
  }
}

fn parse_spec(source: String) -> Result(Spec, RunError) {
  case spec.decode(source) {
    Ok(s) -> Ok(s)
    Error(_) -> Error(SpecError(reason: "could not decode parity spec"))
  }
}

fn load_capture(
  config: RunnerConfig,
  parsed: Spec,
) -> Result(JsonValue, RunError) {
  let path = resolve(config, parsed.capture_file)
  use source <- result.try(read_file(path))
  parse_json(path, source)
}

fn resolve_variables(
  config: RunnerConfig,
  variables: spec.ParityVariables,
  capture: JsonValue,
  primary_response: Option(JsonValue),
  context: String,
) -> Result(JsonValue, RunError) {
  case variables {
    NoVariables -> Ok(JObject([]))
    VariablesFromCapture(path: path) ->
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(value)
        None -> Error(VariablesUnresolved(path: path))
      }
    VariablesFromFile(path: path) -> {
      let resolved = resolve(config, path)
      use source <- result.try(read_file(resolved))
      parse_json(resolved, source)
    }
    VariablesInline(template: template) -> {
      let _ = context
      substitute(template, primary_response, capture)
    }
  }
}

/// Walk an inline variables template, substituting any
/// `{"fromPrimaryProxyPath": "$..."}` or `{"fromCapturePath": "$..."}`
/// markers with the corresponding value. Other nodes pass through.
fn substitute(
  template: JsonValue,
  primary: Option(JsonValue),
  capture: JsonValue,
) -> Result(JsonValue, RunError) {
  case as_primary_ref(template) {
    Some(path) ->
      case primary {
        None -> Error(PrimaryRefUnresolved(path: path))
        Some(root) ->
          case jsonpath.lookup(root, path) {
            Some(value) -> Ok(value)
            None -> Error(PrimaryRefUnresolved(path: path))
          }
      }
    None ->
      case as_capture_ref(template) {
        Some(path) ->
          case jsonpath.lookup(capture, path) {
            Some(value) -> Ok(value)
            None -> Error(CaptureRefUnresolved(path: path))
          }
        None ->
          case template {
            JObject(entries) ->
              entries
              |> list.try_map(fn(pair) {
                let #(k, v) = pair
                case substitute(v, primary, capture) {
                  Ok(v2) -> Ok(#(k, v2))
                  Error(e) -> Error(e)
                }
              })
              |> result.map(JObject)
            JArray(items) ->
              items
              |> list.try_map(fn(item) { substitute(item, primary, capture) })
              |> result.map(JArray)
            leaf -> Ok(leaf)
          }
      }
  }
}

/// If `value` is exactly `{"fromPrimaryProxyPath": "..."}` (one entry
/// with a string value), return the path. Otherwise None.
fn as_primary_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromPrimaryProxyPath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

/// If `value` is exactly `{"fromCapturePath": "..."}` (one entry with
/// a string value), return the path. Otherwise None.
fn as_capture_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromCapturePath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

fn execute(
  proxy: DraftProxy,
  document: String,
  variables: JsonValue,
  context: String,
) -> Result(#(Response, DraftProxy), RunError) {
  let body = build_graphql_body(document, variables)
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: dict.new(),
      body: body,
    )
  let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
  case response.status {
    200 -> Ok(#(response, next_proxy))
    status ->
      Error(ProxyStatus(
        target: context,
        status: status,
        body: json.to_string(response.body),
      ))
  }
}

fn build_graphql_body(document: String, variables: JsonValue) -> String {
  let query = json.to_string(json.string(document))
  let vars = json_value.to_string(variables)
  "{\"query\":" <> query <> ",\"variables\":" <> vars <> "}"
}

fn parse_response_body(response: Response) -> Result(JsonValue, RunError) {
  let serialized = json.to_string(response.body)
  parse_json("<proxy-response>", serialized)
}

fn read_file(path: String) -> Result(String, RunError) {
  case simplifile.read(path) {
    Ok(s) -> Ok(s)
    Error(reason) ->
      Error(FileError(path: path, reason: simplifile.describe_error(reason)))
  }
}

fn parse_json(path: String, source: String) -> Result(JsonValue, RunError) {
  case json_value.parse(source) {
    Ok(v) -> Ok(v)
    Error(e) -> Error(JsonError(path: path, reason: e.message))
  }
}

fn resolve(config: RunnerConfig, path: String) -> String {
  case string.starts_with(path, "/") {
    True -> path
    False -> config.repo_root <> "/" <> path
  }
}

pub fn has_mismatches(report: Report) -> Bool {
  list.any(report.targets, fn(t) { t.mismatches != [] })
}

pub fn render(report: Report) -> String {
  case has_mismatches(report) {
    False -> "OK: " <> report.scenario_id
    True ->
      report.scenario_id
      <> "\n"
      <> string.join(list.map(report.targets, render_target), "\n")
  }
}

fn render_target(target: TargetReport) -> String {
  case target.mismatches {
    [] -> "  [" <> target.name <> "] OK"
    mismatches ->
      "  ["
      <> target.name
      <> "] "
      <> int.to_string(list.length(mismatches))
      <> " mismatch(es):\n"
      <> diff.render_mismatches(mismatches)
  }
}

pub fn into_assert(report: Report) -> Result(Nil, String) {
  case has_mismatches(report) {
    False -> Ok(Nil)
    True -> Error(render(report))
  }
}

pub fn render_error(error: RunError) -> String {
  case error {
    FileError(path, reason) -> "file error at " <> path <> ": " <> reason
    JsonError(path, reason) -> "json error at " <> path <> ": " <> reason
    SpecError(reason) -> "spec error: " <> reason
    VariablesUnresolved(path) -> "variables jsonpath did not resolve: " <> path
    PrimaryRefUnresolved(path) ->
      "fromPrimaryProxyPath did not resolve in primary response: " <> path
    CaptureRefUnresolved(path) ->
      "fromCapturePath did not resolve in capture: " <> path
    CaptureUnresolved(target, path) ->
      "capture jsonpath did not resolve for target '" <> target <> "': " <> path
    ProxyUnresolved(target, path) ->
      "proxy response jsonpath did not resolve for target '"
      <> target
      <> "': "
      <> path
    ProxyStatus(target, status, body) ->
      "proxy returned status "
      <> int.to_string(status)
      <> " for target '"
      <> target
      <> "': "
      <> body
  }
}
