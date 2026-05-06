//// Markets query handling, live-hybrid hydration, and snapshot projection.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{get_document_fragments}
import shopify_draft_proxy/proxy/markets/serializers.{
  option_to_result, serialize_root_fields,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, DraftProxy, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CatalogRecord,
  type MarketLocalizableContentRecord, type MarketRecord, type PriceListRecord,
  type ProductMetafieldRecord, type ProductRecord, type ProductVariantRecord,
  type WebPresenceRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, CatalogRecord,
  MarketLocalizableContentRecord, MarketRecord, PriceListRecord,
  ProductMetafieldRecord, ProductRecord, ProductSeoRecord, ProductVariantRecord,
  ProductVariantSelectedOptionRecord, WebPresenceRecord,
}

@internal
pub fn is_markets_query_root(name: String) -> Bool {
  case name {
    "market"
    | "markets"
    | "catalog"
    | "catalogs"
    | "catalogsCount"
    | "priceList"
    | "priceLists"
    | "webPresences"
    | "marketsResolvedValues"
    | "marketLocalizableResource"
    | "marketLocalizableResources"
    | "marketLocalizableResourcesByIds" -> True
    _ -> False
  }
}

@internal
pub fn handle_markets_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use fields <- result.try(root_field.get_root_fields(document))
  let fragments = get_document_fragments(document)
  Ok(serialize_root_fields(store, fields, fragments, variables))
}

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use data <- result.try(handle_markets_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// Pattern 2 for cold Markets LiveHybrid reads: fetch the captured upstream
/// response once, hydrate the local Markets/Product slices from it, then keep
/// later read-after-write requests local so staged changes are not bypassed.
@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_upstream = case proxy.config.read_mode {
    LiveHybrid ->
      should_fetch_upstream_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_upstream {
    True ->
      fetch_and_hydrate_live_hybrid_query(
        proxy,
        request,
        parsed,
        document,
        variables,
      )
    False -> local_query_response(proxy, document, variables)
  }
}

fn should_fetch_upstream_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "market" ->
      !local_has_market_id(proxy, variables)
    parse_operation.QueryOperation, "catalog" ->
      !local_has_catalog_id(proxy, variables)
    parse_operation.QueryOperation, "priceList" ->
      !local_has_price_list_id(proxy, variables)
    parse_operation.QueryOperation, "markets"
    | parse_operation.QueryOperation, "catalogs"
    | parse_operation.QueryOperation, "catalogsCount"
    | parse_operation.QueryOperation, "priceLists"
    | parse_operation.QueryOperation, "webPresences"
    | parse_operation.QueryOperation, "marketsResolvedValues"
    | parse_operation.QueryOperation, "marketLocalizableResources"
    | parse_operation.QueryOperation, "marketLocalizableResourcesByIds"
    -> !has_local_markets_query_state(proxy)
    parse_operation.QueryOperation, "marketLocalizableResource" ->
      !has_local_markets_query_state(proxy)
    _, _ -> False
  }
}

fn local_has_market_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case store.get_effective_market_by_id(proxy.store, id) {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

fn local_has_catalog_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case store.get_effective_catalog_by_id(proxy.store, id) {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

fn local_has_price_list_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case store.get_effective_price_list_by_id(proxy.store, id) {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

fn has_local_markets_query_state(proxy: DraftProxy) -> Bool {
  !list.is_empty(store.list_effective_markets(proxy.store))
  || !list.is_empty(store.list_effective_catalogs(proxy.store))
  || !list.is_empty(store.list_effective_price_lists(proxy.store))
  || !list.is_empty(store.list_effective_web_presences(proxy.store))
  || !list.is_empty(store.list_effective_products(proxy.store))
}

fn fetch_and_hydrate_live_hybrid_query(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let operation_name = parsed.name |> option.unwrap("MarketsLiveHybridRead")
  case
    upstream_query.fetch_sync(
      proxy.config.shopify_admin_origin,
      proxy.upstream_transport,
      request.headers,
      operation_name,
      document,
      variables_to_json(variables),
    )
  {
    Ok(value) -> {
      let next_store = hydrate_from_upstream_response(proxy.store, value)
      #(
        Response(
          status: 200,
          body: commit.json_value_to_json(value),
          headers: [],
        ),
        DraftProxy(..proxy, store: next_store),
      )
    }
    Error(err) -> #(
      Response(
        status: 502,
        body: json.object([
          #(
            "errors",
            json.array(
              [
                json.object([
                  #(
                    "message",
                    json.string(
                      "Failed to fetch upstream Markets query: "
                      <> fetch_error_message(err),
                    ),
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

fn local_query_response(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case process(proxy.store, document, variables) {
    Ok(envelope) -> #(Response(status: 200, body: envelope, headers: []), proxy)
    Error(_) -> #(
      Response(
        status: 400,
        body: json.object([
          #(
            "errors",
            json.array(
              [
                json.object([
                  #("message", json.string("Failed to handle markets query")),
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

fn variables_to_json(
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  json.object(
    dict.to_list(variables)
    |> list.map(fn(pair) {
      #(pair.0, root_field.resolved_value_to_json(pair.1))
    }),
  )
}

fn fetch_error_message(error: upstream_query.FetchError) -> String {
  case error {
    upstream_query.TransportFailed(message) -> message
    upstream_query.HttpStatusError(status, body) ->
      "upstream returned HTTP " <> int.to_string(status) <> ": " <> body
    upstream_query.MalformedResponse(message) -> message
    upstream_query.NoTransportInstalled -> "no upstream transport installed"
  }
}

/// Pattern 2: locally supported Markets mutations still stage locally, but
/// some captured flows start from existing upstream price lists, products,
/// metafields, or baseline web presences. When a cassette transport is present,
/// hydrate that prior state before running normal local validation/staging.
@internal
pub fn hydrate_mutation_preconditions(
  store_in: Store,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  case upstream.transport, mutation_needs_preflight(fields) {
    Some(_), True ->
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "MarketsMutationPreflightHydrate",
          "query MarketsMutationPreflightHydrate { __typename }",
          variables_to_json(variables),
        )
      {
        Ok(value) -> hydrate_from_upstream_response(store_in, value)
        Error(_) -> store_in
      }
    _, _ -> store_in
  }
}

fn mutation_needs_preflight(fields: List(Selection)) -> Bool {
  fields
  |> list.any(fn(selection) {
    case selection {
      Field(name: name, ..) ->
        case name.value {
          "priceListFixedPricesByProductUpdate"
          | "quantityPricingByVariantUpdate"
          | "quantityRulesAdd"
          | "quantityRulesDelete"
          | "webPresenceCreate"
          | "marketLocalizationsRegister"
          | "marketLocalizationsRemove" -> True
          _ -> False
        }
      _ -> False
    }
  })
}

fn hydrate_from_upstream_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      store_in
      |> hydrate_market_records(data)
      |> hydrate_catalog_records(data)
      |> hydrate_price_list_records(data)
      |> hydrate_web_presence_records(data)
      |> hydrate_product_records(data)
      |> hydrate_market_localizable_metafields(data)
      |> hydrate_markets_root_payloads(data)
    None -> store_in
  }
}

fn hydrate_market_records(store_in: Store, data: commit.JsonValue) -> Store {
  let records =
    list.append(
      record_nodes_from_field(data, "markets", market_record_from_json),
      case json_get(data, "market") {
        Some(commit.JsonNull) | None -> []
        Some(value) ->
          case market_record_from_json(value) {
            Ok(record) -> [record]
            Error(_) -> []
          }
      },
    )
  case records {
    [] -> store_in
    _ -> store.upsert_base_markets(store_in, records)
  }
}

fn hydrate_catalog_records(store_in: Store, data: commit.JsonValue) -> Store {
  let records =
    list.append(
      record_nodes_from_field(data, "catalogs", catalog_record_from_json),
      list.append(nested_catalog_records(data), case json_get(data, "catalog") {
        Some(commit.JsonNull) | None -> []
        Some(value) ->
          case catalog_record_from_json(value) {
            Ok(record) -> [record]
            Error(_) -> []
          }
      }),
    )
  case records {
    [] -> store_in
    _ -> store.upsert_base_catalogs(store_in, records)
  }
}

fn hydrate_price_list_records(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let records =
    list.append(
      record_nodes_from_field(data, "priceLists", price_list_record_from_json),
      list.append(
        nested_price_list_records(data),
        case json_get(data, "priceList") {
          Some(commit.JsonNull) | None -> []
          Some(value) ->
            case price_list_record_from_json(value) {
              Ok(record) -> [record]
              Error(_) -> []
            }
        },
      ),
    )
  case records {
    [] -> store_in
    _ -> store.upsert_base_price_lists(store_in, records)
  }
}

fn hydrate_web_presence_records(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let records =
    list.append(
      record_nodes_from_field(
        data,
        "webPresences",
        web_presence_record_from_json,
      ),
      nested_web_presence_records(data),
    )
  case records {
    [] -> store_in
    _ -> store.upsert_base_web_presences(store_in, records)
  }
}

fn hydrate_product_records(store_in: Store, data: commit.JsonValue) -> Store {
  let product_nodes =
    list.append(
      record_json_nodes_from_field(data, "products"),
      case json_get(data, "product") {
        Some(commit.JsonNull) | None -> []
        Some(value) -> [value]
      },
    )
  let products = list.filter_map(product_nodes, product_record_from_json)
  let variants =
    product_nodes
    |> list.flat_map(fn(product) {
      case json_get_string(product, "id") {
        Some(product_id) ->
          record_json_nodes_from_field(product, "variants")
          |> list.filter_map(product_variant_record_from_json(_, product_id))
        None -> []
      }
    })
  let with_products = case products {
    [] -> store_in
    _ -> store.upsert_base_products(store_in, products)
  }
  case variants {
    [] -> with_products
    _ -> store.upsert_base_product_variants(with_products, variants)
  }
}

fn hydrate_market_localizable_metafields(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  case json_get(data, "product") {
    Some(product) -> hydrate_product_metafields_from_product(store_in, product)
    _ -> store_in
  }
}

fn hydrate_product_metafields_from_product(
  store_in: Store,
  product: commit.JsonValue,
) -> Store {
  case json_get_string(product, "id") {
    Some(product_id) -> {
      let metafields =
        record_json_nodes_from_field(product, "metafields")
        |> list.filter_map(product_metafield_record_from_json(_, product_id))
      case metafields {
        [] -> store_in
        _ ->
          store.replace_base_metafields_for_owner(
            store_in,
            product_id,
            metafields,
          )
      }
    }
    None -> store_in
  }
}

fn hydrate_markets_root_payloads(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  case json_get(data, "marketsResolvedValues") {
    Some(commit.JsonNull) | None -> store_in
    Some(value) ->
      store.upsert_base_markets_root_payload(
        store_in,
        "marketsResolvedValues",
        captured_from_json_value(value),
      )
  }
}

fn record_nodes_from_field(
  data: commit.JsonValue,
  key: String,
  decode: fn(commit.JsonValue) -> Result(a, Nil),
) -> List(a) {
  record_json_nodes_from_field(data, key) |> list.filter_map(decode)
}

fn record_json_nodes_from_field(
  data: commit.JsonValue,
  key: String,
) -> List(commit.JsonValue) {
  case json_get(data, key) {
    Some(connection) -> connection_nodes(connection)
    None -> []
  }
}

fn connection_nodes(connection: commit.JsonValue) -> List(commit.JsonValue) {
  case json_get(connection, "nodes") {
    Some(commit.JsonArray(items)) -> non_null_json_values(items)
    _ ->
      case json_get(connection, "edges") {
        Some(commit.JsonArray(edges)) ->
          list.filter_map(edges, fn(edge) {
            case json_get(edge, "node") {
              Some(commit.JsonNull) | None -> Error(Nil)
              Some(node) -> Ok(node)
            }
          })
        _ -> []
      }
  }
}

fn nested_catalog_records(data: commit.JsonValue) -> List(CatalogRecord) {
  collect_objects(data)
  |> list.filter_map(fn(value) {
    case json_get_string(value, "__typename") {
      Some("MarketCatalog") -> catalog_record_from_json(value)
      _ -> Error(Nil)
    }
  })
}

fn nested_price_list_records(data: commit.JsonValue) -> List(PriceListRecord) {
  collect_objects(data)
  |> list.filter_map(fn(value) {
    case json_get_string(value, "__typename") {
      Some("PriceList") -> price_list_record_from_json(value)
      _ -> Error(Nil)
    }
  })
}

fn nested_web_presence_records(
  data: commit.JsonValue,
) -> List(WebPresenceRecord) {
  collect_objects(data)
  |> list.filter_map(fn(value) {
    case json_get_string(value, "id") {
      Some(id) ->
        case string.contains(id, "gid://shopify/MarketWebPresence/") {
          True -> web_presence_record_from_json(value)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

fn market_record_from_json(
  value: commit.JsonValue,
) -> Result(MarketRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option_to_result)
  Ok(MarketRecord(id: id, cursor: None, data: captured_from_json_value(value)))
}

fn catalog_record_from_json(
  value: commit.JsonValue,
) -> Result(CatalogRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option_to_result)
  Ok(CatalogRecord(id: id, cursor: None, data: captured_from_json_value(value)))
}

fn price_list_record_from_json(
  value: commit.JsonValue,
) -> Result(PriceListRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option_to_result)
  Ok(PriceListRecord(
    id: id,
    cursor: None,
    data: captured_from_json_value(value),
  ))
}

fn web_presence_record_from_json(
  value: commit.JsonValue,
) -> Result(WebPresenceRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option_to_result)
  Ok(WebPresenceRecord(
    id: id,
    cursor: None,
    data: captured_from_json_value(value),
  ))
}

fn product_record_from_json(
  value: commit.JsonValue,
) -> Result(ProductRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option_to_result)
  let title = json_get_string(value, "title") |> option.unwrap("Product")
  Ok(
    ProductRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: json_get_string(value, "handle") |> option.unwrap("product"),
      status: json_get_string(value, "status") |> option.unwrap("ACTIVE"),
      vendor: None,
      product_type: None,
      tags: [],
      price_range_min: None,
      price_range_max: None,
      total_variants: None,
      has_only_default_variant: None,
      has_out_of_stock_variants: None,
      total_inventory: Some(0),
      tracks_inventory: Some(False),
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
      combined_listing_role: None,
      combined_listing_parent_id: None,
      combined_listing_child_ids: [],
    ),
  )
}

fn product_variant_record_from_json(
  value: commit.JsonValue,
  product_id: String,
) -> Result(ProductVariantRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option_to_result)
  Ok(ProductVariantRecord(
    id: id,
    product_id: product_id,
    title: json_get_string(value, "title") |> option.unwrap("Default Title"),
    sku: json_get_string(value, "sku"),
    barcode: None,
    price: json_get_string(value, "price"),
    compare_at_price: json_get_string(value, "compareAtPrice"),
    taxable: None,
    inventory_policy: None,
    inventory_quantity: None,
    selected_options: [
      ProductVariantSelectedOptionRecord("Title", "Default Title"),
    ],
    media_ids: [],
    inventory_item: None,
    contextual_pricing: None,
    cursor: None,
  ))
}

fn product_metafield_record_from_json(
  value: commit.JsonValue,
  owner_id: String,
) -> Result(ProductMetafieldRecord, Nil) {
  use id <- result.try(json_get_string(value, "id") |> option_to_result)
  use namespace <- result.try(
    json_get_string(value, "namespace") |> option_to_result,
  )
  use key <- result.try(json_get_string(value, "key") |> option_to_result)
  Ok(ProductMetafieldRecord(
    id: id,
    owner_id: owner_id,
    namespace: namespace,
    key: key,
    type_: json_get_string(value, "type"),
    value: json_get_string(value, "value"),
    compare_digest: json_get_string(value, "compareDigest"),
    json_value: None,
    created_at: json_get_string(value, "createdAt"),
    updated_at: json_get_string(value, "updatedAt"),
    owner_type: json_get_string(value, "ownerType"),
    market_localizable_content: market_localizable_content_from_json(value),
  ))
}

fn market_localizable_content_from_json(
  value: commit.JsonValue,
) -> List(MarketLocalizableContentRecord) {
  case json_get(value, "marketLocalizableContent") {
    Some(commit.JsonArray(items)) ->
      items |> list.filter_map(market_localizable_content_record_from_json)
    _ -> []
  }
}

fn market_localizable_content_record_from_json(
  value: commit.JsonValue,
) -> Result(MarketLocalizableContentRecord, Nil) {
  use key <- result.try(json_get_string(value, "key") |> option_to_result)
  use content_value <- result.try(
    json_get_string(value, "value") |> option_to_result,
  )
  use digest <- result.try(json_get_string(value, "digest") |> option_to_result)
  Ok(MarketLocalizableContentRecord(key, content_value, digest))
}

fn collect_objects(value: commit.JsonValue) -> List(commit.JsonValue) {
  do_collect_objects([value], []) |> list.reverse
}

fn do_collect_objects(
  stack: List(commit.JsonValue),
  acc: List(commit.JsonValue),
) -> List(commit.JsonValue) {
  case stack {
    [] -> acc
    [commit.JsonObject(entries) as obj, ..rest] -> {
      let next =
        list.fold(list.reverse(entries), rest, fn(s, pair) { [pair.1, ..s] })
      do_collect_objects(next, [obj, ..acc])
    }
    [commit.JsonArray(items), ..rest] -> {
      let next =
        list.fold(list.reverse(items), rest, fn(s, item) { [item, ..s] })
      do_collect_objects(next, acc)
    }
    [_, ..rest] -> do_collect_objects(rest, acc)
  }
}

fn non_null_json_values(
  values: List(commit.JsonValue),
) -> List(commit.JsonValue) {
  list.filter(values, fn(value) {
    case value {
      commit.JsonNull -> False
      _ -> True
    }
  })
}

fn captured_from_json_value(value: commit.JsonValue) -> CapturedJsonValue {
  case value {
    commit.JsonNull -> CapturedNull
    commit.JsonBool(value) -> CapturedBool(value)
    commit.JsonInt(value) -> CapturedInt(value)
    commit.JsonFloat(value) -> CapturedFloat(value)
    commit.JsonString(value) -> CapturedString(value)
    commit.JsonArray(items) ->
      CapturedArray(list.map(items, captured_from_json_value))
    commit.JsonObject(fields) ->
      CapturedObject(
        list.map(fields, fn(pair) {
          #(pair.0, captured_from_json_value(pair.1))
        }),
      )
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(k, v) if k == key -> Ok(v)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}
