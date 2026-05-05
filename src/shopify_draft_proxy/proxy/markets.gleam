//// Markets domain port.
////
//// Supports captured/snapshot read projection for core Markets catalog
//// resources plus the locally-staged MarketWebPresence lifecycle covered by
//// the checked-in parity captures.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionPageInfoOptions, type FragmentMap,
  type SerializeConnectionConfig, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, serialize_empty_connection,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, DraftProxy, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CatalogRecord, type MarketRecord,
  type PriceListRecord, type ProductMetafieldRecord, type ProductRecord,
  type ProductVariantRecord, type ShopDomainRecord, type WebPresenceRecord,
  CapturedArray, CapturedBool, CapturedFloat, CapturedInt, CapturedNull,
  CapturedObject, CapturedString, CatalogRecord, MarketRecord, PriceListRecord,
  ProductMetafieldRecord, ProductRecord, ProductSeoRecord, ProductVariantRecord,
  ProductVariantSelectedOptionRecord, WebPresenceRecord,
}

pub type MarketsError {
  ParseFailed(root_field.RootFieldError)
}

type MarketConnectionItem {
  MarketConnectionItem(
    source: SourceValue,
    pagination_cursor: String,
    output_cursor: String,
  )
}

type MarketRegionInput {
  MarketRegionInput(field: List(String), country_code: String)
}

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

pub fn is_markets_mutation_root(name: String) -> Bool {
  case name {
    "marketCreate"
    | "marketUpdate"
    | "marketDelete"
    | "catalogCreate"
    | "catalogUpdate"
    | "catalogContextUpdate"
    | "catalogDelete"
    | "priceListCreate"
    | "priceListUpdate"
    | "priceListDelete"
    | "priceListFixedPricesByProductUpdate"
    | "quantityPricingByVariantUpdate"
    | "quantityRulesAdd"
    | "quantityRulesDelete"
    | "webPresenceCreate"
    | "webPresenceUpdate"
    | "webPresenceDelete"
    | "marketLocalizationsRegister"
    | "marketLocalizationsRemove" -> True
    _ -> False
  }
}

pub fn handle_markets_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketsError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  Ok(serialize_root_fields(store, fields, fragments, variables))
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketsError) {
  use data <- result.try(handle_markets_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// Pattern 2 for cold Markets LiveHybrid reads: fetch the captured upstream
/// response once, hydrate the local Markets/Product slices from it, then keep
/// later read-after-write requests local so staged changes are not bypassed.
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
fn hydrate_mutation_preconditions(
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
  Ok(ProductRecord(
    id: id,
    legacy_resource_id: None,
    title: title,
    handle: json_get_string(value, "handle") |> option.unwrap("product"),
    status: json_get_string(value, "status") |> option.unwrap("ACTIVE"),
    vendor: None,
    product_type: None,
    tags: [],
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
  ))
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
  ))
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

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let hydrated_store =
        hydrate_mutation_preconditions(store, fields, variables, upstream)
      handle_mutation_fields(
        hydrated_store,
        identity,
        fields,
        fragments,
        variables,
      )
    }
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  json.object(
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      #(key, root_payload_for_field(store, field, fragments, variables))
    }),
  )
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let #(entries, final_store, final_identity, staged_ids, drafts) =
    list.fold(fields, #([], store, identity, [], []), fn(acc, field) {
      let #(current_entries, current_store, current_identity, ids, log_drafts) =
        acc
      case field {
        Field(name: name, ..) ->
          case
            handle_market_mutation(
              current_store,
              current_identity,
              field,
              name.value,
              fragments,
              variables,
            )
          {
            Some(result) -> #(
              list.append(current_entries, [#(result.key, result.payload)]),
              result.store,
              result.identity,
              list.append(ids, result.staged_resource_ids),
              list.append(log_drafts, result.log_drafts),
            )
            None -> acc
          }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: drafts,
  )
}

fn handle_market_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(MutationFieldResult) {
  case name {
    "marketCreate" ->
      Some(handle_market_create(store, identity, field, fragments, variables))
    "marketUpdate" ->
      Some(handle_market_update(store, identity, field, fragments, variables))
    "marketDelete" ->
      Some(handle_market_delete(store, identity, field, fragments, variables))
    "catalogCreate" ->
      Some(handle_catalog_create(store, identity, field, fragments, variables))
    "catalogUpdate" ->
      Some(handle_catalog_update(store, identity, field, fragments, variables))
    "catalogContextUpdate" ->
      Some(handle_catalog_context_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "catalogDelete" ->
      Some(handle_catalog_delete(store, identity, field, fragments, variables))
    "priceListCreate" ->
      Some(handle_price_list_create(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListUpdate" ->
      Some(handle_price_list_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListDelete" ->
      Some(handle_price_list_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "priceListFixedPricesByProductUpdate" ->
      Some(handle_price_list_fixed_prices_by_product_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "quantityPricingByVariantUpdate" ->
      Some(handle_quantity_pricing_by_variant_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "quantityRulesAdd" ->
      Some(handle_quantity_rules_add(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "quantityRulesDelete" ->
      Some(handle_quantity_rules_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "webPresenceCreate" ->
      Some(handle_web_presence_create(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "webPresenceUpdate" ->
      Some(handle_web_presence_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "webPresenceDelete" ->
      Some(handle_web_presence_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "marketLocalizationsRegister" ->
      Some(handle_market_localizations_register(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "marketLocalizationsRemove" ->
      Some(handle_market_localizations_remove(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    _ -> None
  }
}

fn handle_market_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let name =
    graphql_helpers.read_arg_string_nonempty(input, "name") |> option.unwrap("")
  let errors = market_create_input_errors(store, input, name)
  case errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "Market")
      let data = market_data(id, input, None)
      let #(_, next_store) =
        store.upsert_staged_market(store, MarketRecord(id, Some(id), data))
      mutation_result(
        key,
        field,
        fragments,
        "marketCreate",
        "market",
        data,
        [],
        next_store,
        next_identity,
        [id],
      )
    }
    _ ->
      mutation_result(
        key,
        field,
        fragments,
        "marketCreate",
        "market",
        CapturedNull,
        errors,
        store,
        identity,
        [],
      )
  }
}

fn market_create_input_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(CapturedJsonValue) {
  market_create_name_errors(name)
  |> list.append(market_create_status_enabled_errors(input))
  |> list.append(market_create_plan_limit_errors(store))
  |> list.append(market_create_currency_errors(store, input))
  |> list.append(market_create_region_errors(store, input))
}

fn market_create_name_errors(name: String) -> List(CapturedJsonValue) {
  case string.trim(name) {
    "" -> [
      user_error(["input", "name"], "Name can't be blank", "BLANK"),
      user_error(
        ["input", "name"],
        "Name is too short (minimum is 2 characters)",
        "TOO_SHORT",
      ),
    ]
    trimmed ->
      case string.length(trimmed) < 2 {
        True -> [
          user_error(
            ["input", "name"],
            "Name is too short (minimum is 2 characters)",
            "TOO_SHORT",
          ),
        ]
        False -> []
      }
  }
}

fn market_create_status_enabled_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  let status =
    graphql_helpers.read_arg_string_nonempty(input, "status")
    |> option.unwrap("ACTIVE")
  let enabled =
    graphql_helpers.read_arg_bool(input, "enabled")
    |> option.unwrap(status == "ACTIVE")
  case enabled == { status == "ACTIVE" } {
    True -> []
    False -> [
      user_error(
        ["input"],
        "Invalid status and enabled combination.",
        "INVALID_STATUS_AND_ENABLED_COMBINATION",
      ),
    ]
  }
}

fn market_create_plan_limit_errors(store: Store) -> List(CapturedJsonValue) {
  let market_count = store.list_effective_markets(store) |> list.length
  case market_count >= default_market_plan_limit() {
    True -> [
      user_error(
        ["input"],
        "Shop has reached the maximum number of markets for the current plan.",
        "SHOP_REACHED_PLAN_MARKETS_LIMIT",
      ),
    ]
    False -> []
  }
}

fn default_market_plan_limit() -> Int {
  3
}

fn market_create_currency_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_object(input, "currencySettings") {
    Some(currency_settings) ->
      case
        graphql_helpers.read_arg_string_nonempty(
          currency_settings,
          "baseCurrency",
        )
      {
        Some(currency) ->
          case valid_market_base_currency(store, currency) {
            True -> []
            False -> [
              user_error(
                ["input", "currencySettings", "baseCurrency"],
                "Base currency is invalid",
                "INVALID",
              ),
            ]
          }
        None -> []
      }
    None -> []
  }
}

fn valid_market_base_currency(store: Store, currency: String) -> Bool {
  valid_currency(currency) && market_base_currency_supported(store, currency)
}

fn market_base_currency_supported(store: Store, currency: String) -> Bool {
  let known =
    store.list_effective_markets(store)
    |> list.filter_map(fn(record) {
      captured_field(record.data, "currencySettings")
      |> option.then(captured_field(_, "baseCurrency"))
      |> option.then(captured_string_field(_, "currencyCode"))
      |> option_to_result
    })
  list.contains(known, currency)
  || list.contains(default_supported_market_base_currencies(), currency)
}

fn default_supported_market_base_currencies() -> List(String) {
  ["CAD", "DKK", "MXN", "USD"]
}

fn market_create_region_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  let existing_codes = assigned_market_country_codes(store)
  read_market_region_inputs(input)
  |> list.filter_map(fn(region) {
    case list.contains(existing_codes, region.country_code) {
      True ->
        Ok(user_error(region.field, "Code has already been taken", "TAKEN"))
      False -> Error(Nil)
    }
  })
}

fn handle_market_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_market_by_id(store, id) {
        Some(existing) -> {
          let data = market_data(id, input, Some(existing.data))
          let #(_, next_store) =
            store.upsert_staged_market(
              store,
              MarketRecord(id, existing.cursor, data),
            )
          mutation_result(
            key,
            field,
            fragments,
            "marketUpdate",
            "market",
            data,
            [],
            next_store,
            identity,
            [id],
          )
        }
        None ->
          not_found_mutation_result(
            key,
            field,
            fragments,
            "marketUpdate",
            "market",
            ["id"],
            "Market does not exist",
            "DEFINITION_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      not_found_mutation_result(
        key,
        field,
        fragments,
        "marketUpdate",
        "market",
        ["id"],
        "Market does not exist",
        "DEFINITION_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_market_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_market_by_id(store, id) {
        Some(_) ->
          delete_result(
            key,
            field,
            fragments,
            "marketDelete",
            id,
            store.delete_staged_market(store, id),
            identity,
          )
        None ->
          delete_error_result(
            key,
            field,
            fragments,
            "marketDelete",
            ["id"],
            "Market does not exist",
            "DEFINITION_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      delete_error_result(
        key,
        field,
        fragments,
        "marketDelete",
        ["id"],
        "Market does not exist",
        "DEFINITION_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_catalog_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let title = read_arg_string_allow_empty(input, "title") |> option.unwrap("")
  let errors = catalog_create_input_errors(store, input, title)
  case errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "MarketCatalog")
      let data = catalog_data(store, id, input, None)
      let #(_, next_store) =
        store.upsert_staged_catalog(store, CatalogRecord(id, Some(id), data))
      mutation_result(
        key,
        field,
        fragments,
        "catalogCreate",
        "catalog",
        data,
        [],
        next_store,
        next_identity,
        [id],
      )
    }
    _ ->
      mutation_result(
        key,
        field,
        fragments,
        "catalogCreate",
        "catalog",
        CapturedNull,
        errors,
        store,
        identity,
        [],
      )
  }
}

fn handle_catalog_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_catalog_by_id(store, id) {
        Some(existing) -> {
          let data = catalog_data(store, id, input, Some(existing.data))
          let #(_, next_store) =
            store.upsert_staged_catalog(
              store,
              CatalogRecord(id, existing.cursor, data),
            )
          mutation_result(
            key,
            field,
            fragments,
            "catalogUpdate",
            "catalog",
            data,
            [],
            next_store,
            identity,
            [id],
          )
        }
        None ->
          not_found_mutation_result(
            key,
            field,
            fragments,
            "catalogUpdate",
            "catalog",
            ["id"],
            "Catalog does not exist",
            "CATALOG_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      not_found_mutation_result(
        key,
        field,
        fragments,
        "catalogUpdate",
        "catalog",
        ["id"],
        "Catalog does not exist",
        "CATALOG_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_catalog_context_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "catalogId") {
    Some(id) ->
      case store.get_effective_catalog_by_id(store, id) {
        Some(existing) -> {
          let market_ids =
            graphql_helpers.read_arg_object(args, "contextsToAdd")
            |> option.then(read_arg_string_array(_, "marketIds"))
            |> option.unwrap([])
          let data =
            captured_object_upsert(existing.data, [
              #("markets", market_connection_from_ids(store, market_ids)),
            ])
          let #(_, next_store) =
            store.upsert_staged_catalog(
              store,
              CatalogRecord(id, existing.cursor, data),
            )
          mutation_result(
            key,
            field,
            fragments,
            "catalogContextUpdate",
            "catalog",
            data,
            [],
            next_store,
            identity,
            [id],
          )
        }
        None ->
          not_found_mutation_result(
            key,
            field,
            fragments,
            "catalogContextUpdate",
            "catalog",
            ["catalogId"],
            "Catalog does not exist",
            "CATALOG_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      not_found_mutation_result(
        key,
        field,
        fragments,
        "catalogContextUpdate",
        "catalog",
        ["catalogId"],
        "Catalog does not exist",
        "CATALOG_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_catalog_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_catalog_by_id(store, id) {
        Some(_) ->
          delete_result(
            key,
            field,
            fragments,
            "catalogDelete",
            id,
            store.delete_staged_catalog(store, id),
            identity,
          )
        None ->
          delete_error_result(
            key,
            field,
            fragments,
            "catalogDelete",
            ["id"],
            "Catalog does not exist",
            "CATALOG_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      delete_error_result(
        key,
        field,
        fragments,
        "catalogDelete",
        ["id"],
        "Catalog does not exist",
        "CATALOG_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_price_list_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let errors = price_list_input_errors(input, None)
  case errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "PriceList")
      let data = price_list_data(id, input, None)
      let #(_, next_store) =
        store.upsert_staged_price_list(
          store,
          PriceListRecord(id, Some(id), data),
        )
      mutation_result(
        key,
        field,
        fragments,
        "priceListCreate",
        "priceList",
        data,
        [],
        next_store,
        next_identity,
        [id],
      )
    }
    _ ->
      mutation_result(
        key,
        field,
        fragments,
        "priceListCreate",
        "priceList",
        CapturedNull,
        errors,
        store,
        identity,
        [],
      )
  }
}

fn handle_price_list_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_price_list_by_id(store, id) {
        Some(existing) -> {
          let errors = price_list_input_errors(input, Some(existing.data))
          case errors {
            [] -> {
              let data = price_list_data(id, input, Some(existing.data))
              let #(_, next_store) =
                store.upsert_staged_price_list(
                  store,
                  PriceListRecord(id, existing.cursor, data),
                )
              mutation_result(
                key,
                field,
                fragments,
                "priceListUpdate",
                "priceList",
                data,
                [],
                next_store,
                identity,
                [id],
              )
            }
            _ ->
              mutation_result(
                key,
                field,
                fragments,
                "priceListUpdate",
                "priceList",
                CapturedNull,
                errors,
                store,
                identity,
                [],
              )
          }
        }
        None ->
          not_found_mutation_result(
            key,
            field,
            fragments,
            "priceListUpdate",
            "priceList",
            ["id"],
            "Price list does not exist",
            "PRICE_LIST_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      not_found_mutation_result(
        key,
        field,
        fragments,
        "priceListUpdate",
        "priceList",
        ["id"],
        "Price list does not exist",
        "PRICE_LIST_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_price_list_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_price_list_by_id(store, id) {
        Some(existing) -> {
          let next_store = store.delete_staged_price_list(store, id)
          let payload =
            CapturedObject([
              #("deletedId", CapturedString(id)),
              #("priceList", existing.data),
              #("userErrors", CapturedArray([])),
            ])
          MutationFieldResult(
            key: key,
            payload: project_record(
              field,
              fragments,
              captured_json_source(payload),
            ),
            store: next_store,
            identity: identity,
            staged_resource_ids: [],
            log_drafts: [markets_log_draft("priceListDelete", [id])],
          )
        }
        None ->
          delete_error_result(
            key,
            field,
            fragments,
            "priceListDelete",
            ["id"],
            "Price list does not exist",
            "PRICE_LIST_NOT_FOUND",
            store,
            identity,
          )
      }
    None ->
      delete_error_result(
        key,
        field,
        fragments,
        "priceListDelete",
        ["id"],
        "Price list does not exist",
        "PRICE_LIST_NOT_FOUND",
        store,
        identity,
      )
  }
}

fn handle_price_list_fixed_prices_by_product_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list_id = read_price_list_id(args)
  let price_list =
    option.then(price_list_id, store.get_effective_price_list_by_id(store, _))
  let price_inputs = read_arg_object_array(args, "pricesToAdd")
  let delete_product_ids =
    read_arg_string_array(args, "pricesToDeleteByProductIds")
    |> option.unwrap([])
  let errors =
    case price_list_id, price_list {
      Some(_), Some(_) -> []
      _, _ -> [
        user_error(
          ["priceListId"],
          "Price list does not exist.",
          "PRICE_LIST_DOES_NOT_EXIST",
        ),
      ]
    }
    |> list.append(product_level_fixed_price_errors(
      store,
      price_inputs,
      delete_product_ids,
    ))

  case price_list, errors {
    Some(existing), [] -> {
      let added_product_ids =
        list.filter_map(price_inputs, fn(input) {
          graphql_helpers.read_arg_string_nonempty(input, "productId")
          |> option_to_result
        })
      let fixed_inputs =
        list.flat_map(price_inputs, fn(input) {
          case graphql_helpers.read_arg_string_nonempty(input, "productId") {
            Some(product_id) ->
              store.get_effective_variants_by_product_id(store, product_id)
              |> list.map(fn(variant) {
                dict.insert(
                  input,
                  "variantId",
                  root_field.StringVal(variant.id),
                )
              })
            None -> []
          }
        })
      let delete_variant_ids =
        delete_product_ids
        |> list.flat_map(fn(product_id) {
          store.get_effective_variants_by_product_id(store, product_id)
          |> list.map(fn(variant) { variant.id })
        })
      let updated =
        existing
        |> upsert_fixed_price_nodes(store, fixed_inputs)
        |> delete_fixed_price_nodes(delete_variant_ids)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      let payload =
        CapturedObject([
          #("priceList", updated.data),
          #("pricesToAddProducts", product_payloads(store, added_product_ids)),
          #(
            "pricesToDeleteProducts",
            product_payloads(store, delete_product_ids),
          ),
          #("fixedPriceVariantIds", CapturedArray([])),
          #("deletedFixedPriceVariantIds", CapturedArray([])),
          #("userErrors", CapturedArray([])),
        ])
      MutationFieldResult(
        key: key,
        payload: project_record(field, fragments, captured_json_source(payload)),
        store: next_store,
        identity: identity,
        staged_resource_ids: [existing.id],
        log_drafts: [
          markets_log_draft("priceListFixedPricesByProductUpdate", [existing.id]),
        ],
      )
    }
    _, _ ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "priceListFixedPricesByProductUpdate",
        CapturedObject([
          #("priceList", CapturedNull),
          #("pricesToAddProducts", CapturedNull),
          #("pricesToDeleteProducts", CapturedNull),
          #("userErrors", CapturedArray(errors)),
        ]),
        store,
        identity,
        [],
      )
  }
}

fn handle_quantity_pricing_by_variant_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let price_list_id = read_price_list_id(args)
  let price_list =
    option.then(price_list_id, store.get_effective_price_list_by_id(store, _))
  let errors = case price_list {
    Some(_) -> quantity_pricing_input_errors(store, input)
    None -> [
      user_error(
        ["priceListId"],
        "Price list not found.",
        "PRICE_LIST_NOT_FOUND",
      ),
    ]
  }
  case price_list, errors {
    Some(existing), [] -> {
      let fixed_inputs = read_arg_object_array(input, "pricesToAdd")
      let delete_variant_ids =
        read_arg_string_array(input, "pricesToDeleteByVariantId")
        |> option.unwrap([])
      let rule_inputs = read_arg_object_array(input, "quantityRulesToAdd")
      let rule_delete_ids =
        read_arg_string_array(input, "quantityRulesToDeleteByVariantId")
        |> option.unwrap([])
      let price_break_inputs =
        read_arg_object_array(input, "quantityPriceBreaksToAdd")
      let updated =
        existing
        |> upsert_fixed_price_nodes(store, fixed_inputs)
        |> delete_fixed_price_nodes(delete_variant_ids)
        |> upsert_quantity_rule_nodes(store, rule_inputs)
        |> delete_quantity_rule_nodes(rule_delete_ids)
        |> upsert_quantity_price_break_nodes(
          store,
          identity,
          price_break_inputs,
        )
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      let changed_variant_ids =
        mutation_variant_ids(fixed_inputs)
        |> append_unique_strings(mutation_variant_ids(rule_inputs))
        |> append_unique_strings(mutation_variant_ids(price_break_inputs))
        |> append_unique_strings(delete_variant_ids)
        |> append_unique_strings(rule_delete_ids)
      let payload =
        CapturedObject([
          #("productVariants", variant_payloads(store, changed_variant_ids)),
          #("userErrors", CapturedArray([])),
        ])
      MutationFieldResult(
        key: key,
        payload: project_record(field, fragments, captured_json_source(payload)),
        store: next_store,
        identity: identity,
        staged_resource_ids: [existing.id],
        log_drafts: [
          markets_log_draft("quantityPricingByVariantUpdate", [existing.id]),
        ],
      )
    }
    _, _ ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityPricingByVariantUpdate",
        CapturedObject([
          #("productVariants", CapturedNull),
          #("userErrors", CapturedArray(errors)),
        ]),
        store,
        identity,
        [],
      )
  }
}

fn handle_quantity_rules_add(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list =
    option.then(read_price_list_id(args), store.get_effective_price_list_by_id(
      store,
      _,
    ))
  let inputs = read_arg_object_array(args, "quantityRules")
  let errors = case price_list {
    Some(_) -> quantity_rules_input_errors(store, inputs)
    None -> [
      user_error(
        ["priceListId"],
        "Price list does not exist.",
        "PRICE_LIST_DOES_NOT_EXIST",
      ),
    ]
  }
  case price_list, errors {
    Some(existing), [] -> {
      let updated = upsert_quantity_rule_nodes(existing, store, inputs)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      let payload =
        CapturedObject([
          #("quantityRules", quantity_rule_payloads(store, inputs)),
          #("userErrors", CapturedArray([])),
        ])
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityRulesAdd",
        payload,
        next_store,
        identity,
        [existing.id],
      )
    }
    _, _ ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityRulesAdd",
        CapturedObject([
          #("quantityRules", CapturedArray([])),
          #("userErrors", CapturedArray(errors)),
        ]),
        store,
        identity,
        [],
      )
  }
}

fn handle_quantity_rules_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let price_list =
    option.then(read_price_list_id(args), store.get_effective_price_list_by_id(
      store,
      _,
    ))
  let variant_ids =
    read_arg_string_array(args, "variantIds") |> option.unwrap([])
  let errors = case price_list {
    Some(_) -> quantity_rule_delete_errors(store, variant_ids)
    None -> [
      user_error(
        ["priceListId"],
        "Price list does not exist.",
        "PRICE_LIST_DOES_NOT_EXIST",
      ),
    ]
  }
  case price_list, errors {
    Some(existing), [] -> {
      let updated = delete_quantity_rule_nodes(existing, variant_ids)
      let #(_, next_store) = store.upsert_staged_price_list(store, updated)
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityRulesDelete",
        CapturedObject([
          #("deletedQuantityRulesVariantIds", string_array(variant_ids)),
          #("userErrors", CapturedArray([])),
        ]),
        next_store,
        identity,
        [existing.id],
      )
    }
    _, _ ->
      mutation_payload_result(
        key,
        field,
        fragments,
        "quantityRulesDelete",
        CapturedObject([
          #("deletedQuantityRulesVariantIds", CapturedArray([])),
          #("userErrors", CapturedArray(errors)),
        ]),
        store,
        identity,
        [],
      )
  }
}

fn handle_market_localizations_register(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let resource_id = graphql_helpers.read_arg_string_nonempty(args, "resourceId")
  let errors = case resource_id {
    Some(id) ->
      case store.find_effective_metafield_by_id(store, id) {
        Some(_) -> [
          user_error(
            ["marketLocalizations", "0", "key"],
            "Key value is not a valid market localizable field",
            "INVALID_KEY_FOR_MODEL",
          ),
        ]
        None -> [resource_not_found_error(id)]
      }
    None -> [resource_not_found_error("")]
  }
  mutation_payload_result(
    key,
    field,
    fragments,
    "marketLocalizationsRegister",
    CapturedObject([
      #("marketLocalizations", CapturedNull),
      #("userErrors", CapturedArray(errors)),
    ]),
    store,
    identity,
    [],
  )
}

fn handle_market_localizations_remove(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let resource_id = graphql_helpers.read_arg_string_nonempty(args, "resourceId")
  let errors = case resource_id {
    Some(id) ->
      case store.find_effective_metafield_by_id(store, id) {
        Some(_) -> []
        None -> [resource_not_found_error(id)]
      }
    None -> [resource_not_found_error("")]
  }
  mutation_payload_result(
    key,
    field,
    fragments,
    "marketLocalizationsRemove",
    CapturedObject([
      #("marketLocalizations", CapturedNull),
      #("userErrors", CapturedArray(errors)),
    ]),
    store,
    identity,
    [],
  )
}

fn resource_not_found_error(resource_id: String) -> CapturedJsonValue {
  user_error(
    ["resourceId"],
    "Resource " <> resource_id <> " does not exist",
    "RESOURCE_NOT_FOUND",
  )
}

fn handle_web_presence_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let errors = web_presence_create_errors(store, input)
  case errors {
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "MarketWebPresence")
      let data = web_presence_data(store, id, input)
      let record = WebPresenceRecord(id: id, cursor: None, data: data)
      let #(_, next_store) = store.upsert_staged_web_presence(store, record)
      mutation_result(
        key,
        field,
        fragments,
        "webPresenceCreate",
        "webPresence",
        data,
        [],
        next_store,
        next_identity,
        [id],
      )
    }
    _ ->
      mutation_result(
        key,
        field,
        fragments,
        "webPresenceCreate",
        "webPresence",
        CapturedNull,
        errors,
        store,
        identity,
        [],
      )
  }
}

fn handle_web_presence_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string_nonempty(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case id {
    Some(id_value) ->
      case store.get_effective_web_presence_by_id(store, id_value) {
        Some(_) -> {
          let errors = web_presence_create_errors(store, input)
          case errors {
            [] -> {
              let data = web_presence_data(store, id_value, input)
              let record =
                WebPresenceRecord(id: id_value, cursor: None, data: data)
              let #(_, next_store) =
                store.upsert_staged_web_presence(store, record)
              mutation_result(
                key,
                field,
                fragments,
                "webPresenceUpdate",
                "webPresence",
                data,
                [],
                next_store,
                identity,
                [id_value],
              )
            }
            _ ->
              mutation_result(
                key,
                field,
                fragments,
                "webPresenceUpdate",
                "webPresence",
                CapturedNull,
                errors,
                store,
                identity,
                [],
              )
          }
        }
        None ->
          web_presence_not_found_result(
            key,
            field,
            fragments,
            "webPresenceUpdate",
            "webPresence",
            store,
            identity,
          )
      }
    None ->
      web_presence_not_found_result(
        key,
        field,
        fragments,
        "webPresenceUpdate",
        "webPresence",
        store,
        identity,
      )
  }
}

fn handle_web_presence_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_web_presence_by_id(store, id) {
        Some(_) -> {
          let next_store = store.delete_staged_web_presence(store, id)
          let payload =
            CapturedObject([
              #("deletedId", CapturedString(id)),
              #("userErrors", CapturedArray([])),
            ])
          let staged_ids: List(String) = []
          MutationFieldResult(
            key: key,
            payload: project_record(
              field,
              fragments,
              captured_json_source(payload),
            ),
            store: next_store,
            identity: identity,
            staged_resource_ids: staged_ids,
            log_drafts: [markets_log_draft("webPresenceDelete", staged_ids)],
          )
        }
        None ->
          web_presence_delete_not_found_result(
            key,
            field,
            fragments,
            store,
            identity,
          )
      }
    None ->
      web_presence_delete_not_found_result(
        key,
        field,
        fragments,
        store,
        identity,
      )
  }
}

fn mutation_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  resource_key: String,
  resource: CapturedJsonValue,
  user_errors: List(CapturedJsonValue),
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_ids: List(String),
) -> MutationFieldResult {
  let payload =
    CapturedObject([
      #(resource_key, resource),
      #("userErrors", CapturedArray(user_errors)),
    ])
  MutationFieldResult(
    key: key,
    payload: project_record(field, fragments, captured_json_source(payload)),
    store: store,
    identity: identity,
    staged_resource_ids: staged_ids,
    log_drafts: [markets_log_draft(root_name, staged_ids)],
  )
}

fn mutation_payload_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  payload: CapturedJsonValue,
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_ids: List(String),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: project_record(field, fragments, captured_json_source(payload)),
    store: store,
    identity: identity,
    staged_resource_ids: staged_ids,
    log_drafts: [markets_log_draft(root_name, staged_ids)],
  )
}

fn not_found_mutation_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  resource_key: String,
  error_field: List(String),
  message: String,
  code: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_result(
    key,
    field,
    fragments,
    root_name,
    resource_key,
    CapturedNull,
    [user_error(error_field, message, code)],
    store,
    identity,
    [],
  )
}

fn delete_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  id: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_payload_result(
    key,
    field,
    fragments,
    root_name,
    CapturedObject([
      #("deletedId", CapturedString(id)),
      #("userErrors", CapturedArray([])),
    ]),
    store,
    identity,
    [id],
  )
}

fn delete_error_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  error_field: List(String),
  message: String,
  code: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_payload_result(
    key,
    field,
    fragments,
    root_name,
    CapturedObject([
      #("deletedId", CapturedNull),
      #("userErrors", CapturedArray([user_error(error_field, message, code)])),
    ]),
    store,
    identity,
    [],
  )
}

fn web_presence_not_found_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  resource_key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  mutation_result(
    key,
    field,
    fragments,
    root_name,
    resource_key,
    CapturedNull,
    [
      user_error(
        ["id"],
        "The market web presence wasn't found.",
        "WEB_PRESENCE_NOT_FOUND",
      ),
    ],
    store,
    identity,
    [],
  )
}

fn web_presence_delete_not_found_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  let staged_ids: List(String) = []
  let payload =
    CapturedObject([
      #("deletedId", CapturedNull),
      #(
        "userErrors",
        CapturedArray([
          user_error(
            ["id"],
            "The market web presence wasn't found.",
            "WEB_PRESENCE_NOT_FOUND",
          ),
        ]),
      ),
    ])
  MutationFieldResult(
    key: key,
    payload: project_record(field, fragments, captured_json_source(payload)),
    store: store,
    identity: identity,
    staged_resource_ids: staged_ids,
    log_drafts: [markets_log_draft("webPresenceDelete", staged_ids)],
  )
}

fn web_presence_create_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  let domain_errors = case
    graphql_helpers.read_arg_string_nonempty(input, "domainId")
  {
    Some(domain_id) ->
      case web_presence_domain_for_id(store, domain_id) {
        Some(_) -> []
        None -> [
          user_error(
            ["input", "domainId"],
            "Domain does not exist",
            "DOMAIN_NOT_FOUND",
          ),
        ]
      }
    None -> []
  }
  let locale_errors = case
    graphql_helpers.read_arg_string_nonempty(input, "defaultLocale")
  {
    Some(locale) ->
      case locale == "en" {
        True -> []
        False -> [
          user_error(
            ["input", "defaultLocale"],
            "Invalid locale codes: " <> locale,
            "INVALID",
          ),
        ]
      }
    None -> []
  }
  list.append(domain_errors, locale_errors)
}

fn web_presence_data(
  store: Store,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let default_locale =
    graphql_helpers.read_arg_string_nonempty(input, "defaultLocale")
    |> option.unwrap("en")
  let alternate_locales =
    read_arg_string_array(input, "alternateLocales") |> option.unwrap([])
  let suffix =
    graphql_helpers.read_arg_string_nonempty(input, "subfolderSuffix")
  let domain =
    web_presence_domain_for_input(
      store,
      graphql_helpers.read_arg_string_nonempty(input, "domainId"),
    )
  let has_alternate_locales = !list.is_empty(alternate_locales)
  let locales = [default_locale, ..alternate_locales]
  CapturedObject([
    #("__typename", CapturedString("MarketWebPresence")),
    #("id", CapturedString(id)),
    #("subfolderSuffix", optional_captured_string(suffix)),
    #("domain", optional_web_presence_domain(domain)),
    #(
      "rootUrls",
      CapturedArray(
        list.map(locales, fn(locale) {
          CapturedObject([
            #("locale", CapturedString(locale)),
            #(
              "url",
              CapturedString(web_presence_root_url(
                store,
                locale,
                default_locale,
                suffix,
                domain,
                has_alternate_locales,
              )),
            ),
          ])
        }),
      ),
    ),
    #("defaultLocale", locale_payload(default_locale, True)),
    #(
      "alternateLocales",
      CapturedArray(
        list.map(alternate_locales, fn(locale) { locale_payload(locale, False) }),
      ),
    ),
    #(
      "markets",
      CapturedObject([
        #("nodes", CapturedArray([])),
        #("edges", CapturedArray([])),
      ]),
    ),
  ])
}

fn web_presence_root_url(
  store: Store,
  locale: String,
  default_locale: String,
  suffix: Option(String),
  domain: Option(ShopDomainRecord),
  has_alternate_locales: Bool,
) -> String {
  let base_url = case domain {
    Some(domain) -> web_presence_domain_base_url(domain)
    None -> shop_primary_web_presence_base_url(store) |> option.unwrap("")
  }
  case domain {
    Some(_) -> localized_root_url(base_url, locale, default_locale)
    None ->
      case shop_primary_web_presence_base_url(store) {
        Some(base_url) ->
          documented_subfolder_root_url(
            base_url,
            locale,
            default_locale,
            suffix,
          )
        None ->
          case has_alternate_locales {
            True ->
              documented_subfolder_root_url(
                captured_web_presence_base_url(store) |> option.unwrap(""),
                locale,
                default_locale,
                suffix,
              )
            False ->
              legacy_captured_subfolder_root_url(
                captured_web_presence_base_url(store) |> option.unwrap(""),
                locale,
                suffix,
              )
          }
      }
  }
}

fn web_presence_domain_for_input(
  store: Store,
  domain_id: Option(String),
) -> Option(ShopDomainRecord) {
  case domain_id {
    Some(id) -> web_presence_domain_for_id(store, id)
    None -> None
  }
}

fn web_presence_domain_for_id(
  store: Store,
  domain_id: String,
) -> Option(ShopDomainRecord) {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.primary_domain.id == domain_id {
        True -> Some(shop.primary_domain)
        False -> None
      }
    None -> None
  }
}

fn optional_web_presence_domain(
  domain: Option(ShopDomainRecord),
) -> CapturedJsonValue {
  case domain {
    Some(domain) ->
      CapturedObject([
        #("__typename", CapturedString("Domain")),
        #("id", CapturedString(domain.id)),
        #("host", CapturedString(domain.host)),
        #("url", CapturedString(domain.url)),
        #("sslEnabled", CapturedBool(domain.ssl_enabled)),
      ])
    None -> CapturedNull
  }
}

fn localized_root_url(
  base_url: String,
  locale: String,
  default_locale: String,
) -> String {
  case locale == default_locale {
    True -> base_url <> "/"
    False -> base_url <> "/" <> locale <> "/"
  }
}

fn documented_subfolder_root_url(
  base_url: String,
  locale: String,
  default_locale: String,
  suffix: Option(String),
) -> String {
  case suffix {
    Some(s) ->
      case locale == default_locale {
        True -> base_url <> "/" <> s <> "/"
        False -> base_url <> "/" <> s <> "/" <> locale <> "/"
      }
    None -> localized_root_url(base_url, locale, default_locale)
  }
}

fn legacy_captured_subfolder_root_url(
  base_url: String,
  locale: String,
  suffix: Option(String),
) -> String {
  case suffix {
    Some(s) -> base_url <> "/" <> locale <> "-" <> s <> "/"
    None -> base_url <> "/"
  }
}

fn shop_primary_web_presence_base_url(store: Store) -> Option(String) {
  case store.get_effective_shop(store) {
    Some(shop) -> Some(web_presence_domain_base_url(shop.primary_domain))
    None -> None
  }
}

fn captured_web_presence_base_url(store: Store) -> Option(String) {
  case list.first(store.list_effective_web_presences(store)) {
    Ok(record) ->
      case captured_field(record.data, "domain") {
        Some(domain) -> captured_domain_base_url(domain)
        None -> None
      }
    Error(_) -> None
  }
}

fn captured_domain_base_url(domain: CapturedJsonValue) -> Option(String) {
  case captured_string_field(domain, "url") {
    Some(url) -> Some(trim_trailing_slash(url))
    None ->
      case captured_string_field(domain, "host") {
        Some(host) -> Some("https://" <> host)
        None -> None
      }
  }
}

fn web_presence_domain_base_url(domain: ShopDomainRecord) -> String {
  let raw = case domain.url == "" {
    True -> "https://" <> domain.host
    False -> domain.url
  }
  trim_trailing_slash(raw)
}

fn trim_trailing_slash(value: String) -> String {
  case string.ends_with(value, "/") {
    True -> string.drop_end(value, 1)
    False -> value
  }
}

fn locale_payload(locale: String, primary: Bool) -> CapturedJsonValue {
  CapturedObject([
    #("locale", CapturedString(locale)),
    #("name", CapturedString(locale_name(locale))),
    #("primary", CapturedBool(primary)),
    #("published", CapturedBool(True)),
  ])
}

fn locale_name(locale: String) -> String {
  case locale {
    "en" -> "English"
    "fr" -> "French"
    "de" -> "German"
    "es" -> "Spanish"
    _ -> string.uppercase(locale)
  }
}

fn user_error(
  field: List(String),
  message: String,
  code: String,
) -> CapturedJsonValue {
  CapturedObject([
    #("field", CapturedArray(list.map(field, CapturedString))),
    #("message", CapturedString(message)),
    #("code", CapturedString(code)),
  ])
}

fn markets_log_draft(root_name: String, staged_ids: List(String)) -> LogDraft {
  let status = case staged_ids {
    [] -> store.Failed
    [_, ..] -> store.Staged
  }
  single_root_log_draft(
    root_name,
    staged_ids,
    status,
    "markets",
    "stage-locally",
    Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
  )
}

fn market_data(
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  let existing_value = existing |> option.unwrap(CapturedObject([]))
  let region_inputs = read_market_region_inputs(input)
  let name =
    read_arg_string_allow_empty(input, "name")
    |> option.or(captured_string_field(existing_value, "name"))
    |> option.unwrap("")
  let status =
    graphql_helpers.read_arg_string_nonempty(input, "status")
    |> option.or(captured_string_field(existing_value, "status"))
    |> option.unwrap("ACTIVE")
  let enabled =
    graphql_helpers.read_arg_bool(input, "enabled")
    |> option.unwrap(status == "ACTIVE")
  let market_type = case region_inputs {
    [] ->
      captured_string_field(existing_value, "type")
      |> option.unwrap("NONE")
    [_, ..] -> "REGION"
  }
  captured_object_upsert(existing_value, [
    #("__typename", CapturedString("Market")),
    #("id", CapturedString(id)),
    #("name", CapturedString(name)),
    #("handle", CapturedString(market_handle(name))),
    #("status", CapturedString(status)),
    #("enabled", CapturedBool(enabled)),
    #("type", CapturedString(market_type)),
    #("conditions", market_conditions_data(region_inputs, existing_value)),
    #(
      "currencySettings",
      market_currency_settings_data(input, region_inputs, existing_value),
    ),
    #(
      "priceInclusions",
      captured_field(existing_value, "priceInclusions")
        |> option.unwrap(default_market_price_inclusions()),
    ),
    #(
      "catalogs",
      captured_field(existing_value, "catalogs")
        |> option.unwrap(empty_connection()),
    ),
    #(
      "webPresences",
      captured_field(existing_value, "webPresences")
        |> option.unwrap(empty_connection()),
    ),
  ])
}

fn market_handle(name: String) -> String {
  string.trim(name)
  |> string.lowercase
  |> string.replace(" ", "-")
}

fn read_market_region_inputs(
  input: Dict(String, root_field.ResolvedValue),
) -> List(MarketRegionInput) {
  list.append(
    read_legacy_market_region_inputs(input),
    read_conditions_market_region_inputs(input),
  )
}

fn read_legacy_market_region_inputs(
  input: Dict(String, root_field.ResolvedValue),
) -> List(MarketRegionInput) {
  read_arg_object_array(input, "regions")
  |> list.index_map(fn(region, index) {
    case graphql_helpers.read_arg_string_nonempty(region, "countryCode") {
      Some(code) ->
        Ok(MarketRegionInput(
          field: ["input", "regions", int.to_string(index), "countryCode"],
          country_code: code,
        ))
      None -> Error(Nil)
    }
  })
  |> result.values
}

fn read_conditions_market_region_inputs(
  input: Dict(String, root_field.ResolvedValue),
) -> List(MarketRegionInput) {
  let regions_condition =
    graphql_helpers.read_arg_object(input, "conditions")
    |> option.then(graphql_helpers.read_arg_object(_, "regionsCondition"))
    |> option.unwrap(dict.new())
  read_arg_object_array(regions_condition, "regions")
  |> list.index_map(fn(region, index) {
    case graphql_helpers.read_arg_string_nonempty(region, "countryCode") {
      Some(code) ->
        Ok(MarketRegionInput(
          field: [
            "input",
            "conditions",
            "regionsCondition",
            "regions",
            int.to_string(index),
            "countryCode",
          ],
          country_code: code,
        ))
      None -> Error(Nil)
    }
  })
  |> result.values
}

fn assigned_market_country_codes(store: Store) -> List(String) {
  store.list_effective_markets(store)
  |> list.fold([], fn(codes, record) {
    list.append(codes, market_country_codes(record.data))
  })
}

fn market_country_codes(data: CapturedJsonValue) -> List(String) {
  captured_field(data, "conditions")
  |> option.then(captured_field(_, "regionsCondition"))
  |> option.then(captured_field(_, "regions"))
  |> option.map(region_codes_from_connection)
  |> option.unwrap([])
}

fn region_codes_from_connection(connection: CapturedJsonValue) -> List(String) {
  let node_codes =
    captured_field(connection, "nodes")
    |> option.map(fn(nodes) {
      case nodes {
        CapturedArray(items) ->
          list.filter_map(items, fn(item) {
            region_code_from_node(item) |> option_to_result
          })
        _ -> []
      }
    })
    |> option.unwrap([])
  let edge_codes =
    captured_field(connection, "edges")
    |> option.map(fn(edges) {
      case edges {
        CapturedArray(items) ->
          list.filter_map(items, fn(edge) {
            captured_field(edge, "node")
            |> option.then(region_code_from_node)
            |> option_to_result
          })
        _ -> []
      }
    })
    |> option.unwrap([])
  list.append(node_codes, edge_codes)
}

fn region_code_from_node(node: CapturedJsonValue) -> Option(String) {
  captured_string_field(node, "code")
  |> option.or(captured_string_field(node, "countryCode"))
}

fn market_conditions_data(
  regions: List(MarketRegionInput),
  existing_value: CapturedJsonValue,
) -> CapturedJsonValue {
  case regions {
    [] ->
      captured_field(existing_value, "conditions")
      |> option.unwrap(empty_market_conditions())
    [_, ..] ->
      CapturedObject([
        #("conditionTypes", CapturedArray([CapturedString("REGION")])),
        #(
          "regionsCondition",
          CapturedObject([
            #("applicationLevel", CapturedString("SPECIFIED")),
            #("regions", market_regions_connection(regions)),
          ]),
        ),
      ])
  }
}

fn empty_market_conditions() -> CapturedJsonValue {
  CapturedObject([
    #("conditionTypes", CapturedArray([])),
    #(
      "regionsCondition",
      CapturedObject([
        #("applicationLevel", CapturedString("SPECIFIED")),
        #(
          "regions",
          CapturedObject([
            #("edges", CapturedArray([])),
            #("nodes", CapturedArray([])),
            #("pageInfo", page_info_for_cursors([])),
          ]),
        ),
      ]),
    ),
  ])
}

fn market_regions_connection(
  regions: List(MarketRegionInput),
) -> CapturedJsonValue {
  let nodes = list.map(regions, market_region_node)
  let cursors = list.map(regions, fn(region) { region.country_code })
  CapturedObject([
    #(
      "edges",
      CapturedArray(
        list.map(nodes, fn(node) {
          let cursor =
            captured_string_field(node, "code") |> option.unwrap("region")
          CapturedObject([
            #("cursor", CapturedString(cursor)),
            #("node", node),
          ])
        }),
      ),
    ),
    #("nodes", CapturedArray(nodes)),
    #("pageInfo", page_info_for_cursors(cursors)),
  ])
}

fn market_region_node(region: MarketRegionInput) -> CapturedJsonValue {
  let currency = country_currency(region.country_code)
  CapturedObject([
    #("__typename", CapturedString("MarketRegionCountry")),
    #(
      "id",
      CapturedString(
        "gid://shopify/MarketRegionCountry/" <> region.country_code,
      ),
    ),
    #("name", CapturedString(country_name(region.country_code))),
    #("code", CapturedString(region.country_code)),
    #("currency", currency_payload(currency)),
  ])
}

fn market_currency_settings_data(
  input: Dict(String, root_field.ResolvedValue),
  regions: List(MarketRegionInput),
  existing_value: CapturedJsonValue,
) -> CapturedJsonValue {
  let settings =
    graphql_helpers.read_arg_object(input, "currencySettings")
    |> option.unwrap(dict.new())
  let base_currency =
    graphql_helpers.read_arg_string_nonempty(settings, "baseCurrency")
    |> option.or(
      captured_field(existing_value, "currencySettings")
      |> option.then(captured_field(_, "baseCurrency"))
      |> option.then(captured_string_field(_, "currencyCode")),
    )
    |> option.or(first_region_currency(regions))
    |> option.unwrap("CAD")
  let existing_settings =
    captured_field(existing_value, "currencySettings")
    |> option.unwrap(CapturedObject([]))
  CapturedObject([
    #("baseCurrency", currency_payload(base_currency)),
    #(
      "localCurrencies",
      graphql_helpers.read_arg_bool(settings, "localCurrencies")
        |> option.map(CapturedBool)
        |> option.or(captured_field(existing_settings, "localCurrencies"))
        |> option.unwrap(CapturedBool(False)),
    ),
    #(
      "roundingEnabled",
      graphql_helpers.read_arg_bool(settings, "roundingEnabled")
        |> option.map(CapturedBool)
        |> option.or(captured_field(existing_settings, "roundingEnabled"))
        |> option.unwrap(CapturedBool(True)),
    ),
  ])
}

fn first_region_currency(regions: List(MarketRegionInput)) -> Option(String) {
  case regions {
    [first, ..] -> Some(country_currency(first.country_code))
    [] -> None
  }
}

fn currency_payload(currency: String) -> CapturedJsonValue {
  CapturedObject([
    #("currencyCode", CapturedString(currency)),
    #("currencyName", CapturedString(currency_name(currency))),
    #("enabled", CapturedBool(True)),
  ])
}

fn default_market_price_inclusions() -> CapturedJsonValue {
  CapturedObject([
    #(
      "inclusiveDutiesPricingStrategy",
      CapturedString("ADD_DUTIES_AT_CHECKOUT"),
    ),
    #(
      "inclusiveTaxPricingStrategy",
      CapturedString("INCLUDES_TAXES_IN_PRICE_BASED_ON_COUNTRY"),
    ),
  ])
}

fn country_currency(country_code: String) -> String {
  case country_code {
    "CA" -> "CAD"
    "CO" -> "COP"
    "BR" -> "BRL"
    "CL" -> "CLP"
    "DK" -> "DKK"
    "MX" -> "MXN"
    "PE" -> "PEN"
    "US" -> "USD"
    _ -> "CAD"
  }
}

fn country_name(country_code: String) -> String {
  case country_code {
    "BR" -> "Brazil"
    "CA" -> "Canada"
    "CL" -> "Chile"
    "CO" -> "Colombia"
    "DK" -> "Denmark"
    "MX" -> "Mexico"
    "PE" -> "Peru"
    "US" -> "United States"
    _ -> country_code
  }
}

fn currency_name(currency: String) -> String {
  case currency {
    "BRL" -> "Brazilian Real"
    "CAD" -> "Canadian Dollar"
    "CLP" -> "Chilean Peso"
    "COP" -> "Colombian Peso"
    "DKK" -> "Danish Krone"
    "MXN" -> "Mexican Peso"
    "PEN" -> "Peruvian Sol"
    "USD" -> "United States Dollar"
    _ -> currency
  }
}

fn catalog_create_input_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  title: String,
) -> List(CapturedJsonValue) {
  let title_errors = catalog_title_errors(title)
  case title_errors {
    [] -> {
      let status_errors = catalog_status_errors(input)
      case status_errors {
        [] -> catalog_context_errors(store, input)
        _ -> status_errors
      }
    }
    _ -> title_errors
  }
}

fn catalog_title_errors(title: String) -> List(CapturedJsonValue) {
  case string.trim(title) {
    "" -> [user_error(["input", "title"], "Title can't be blank", "BLANK")]
    trimmed ->
      case string.length(trimmed) < 2 {
        True -> [
          user_error(
            ["input", "title"],
            "Title is too short (minimum is 2 characters)",
            "TOO_SHORT",
          ),
        ]
        False -> []
      }
  }
}

fn catalog_status_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case read_arg_string_allow_empty(input, "status") {
    None -> [
      user_error(["input", "status"], "Status is required", "REQUIRED"),
    ]
    Some(status) ->
      case list.contains(["ACTIVE", "ARCHIVED", "DRAFT"], status) {
        True -> []
        False -> [
          user_error(["input", "status"], "Status is invalid", "INVALID"),
        ]
      }
  }
}

fn catalog_context_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_object(input, "context") {
    None -> [
      user_error(["input", "context"], "Context is required", "INVALID"),
    ]
    Some(context) -> catalog_context_object_errors(store, context)
  }
}

fn catalog_context_object_errors(
  store: Store,
  context: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_string_nonempty(context, "driverType") {
    None -> [
      user_error(
        ["input", "context", "driverType"],
        "Driver type is required",
        "INVALID",
      ),
    ]
    Some(driver_type) ->
      case driver_type {
        "MARKET" -> {
          case
            require_catalog_context_ids(
              context,
              "marketIds",
              "Market ids can't be blank",
            )
          {
            Ok(ids) -> missing_market_context_errors(store, ids)
            Error(errors) -> errors
          }
        }
        "COMPANY_LOCATION" -> {
          case
            require_catalog_context_ids(
              context,
              "companyLocationIds",
              "Company location ids can't be blank",
            )
          {
            Ok(ids) -> {
              case missing_company_location_context_errors(store, ids) {
                [] -> unsupported_catalog_context_errors("COMPANY_LOCATION")
                errors -> errors
              }
            }
            Error(errors) -> errors
          }
        }
        "COUNTRY" -> {
          case
            require_catalog_context_ids(
              context,
              "countryCodes",
              "Country codes can't be blank",
            )
          {
            Ok(_) -> unsupported_catalog_context_errors("COUNTRY")
            Error(errors) -> errors
          }
        }
        _ -> [
          user_error(
            ["input", "context", "driverType"],
            "Driver type is invalid",
            "INVALID",
          ),
        ]
      }
  }
}

fn unsupported_catalog_context_errors(
  driver_type: String,
) -> List(CapturedJsonValue) {
  [
    user_error(
      ["input", "context", "driverType"],
      "Catalog context driverType "
        <> driver_type
        <> " is not supported by the local MarketCatalog model",
      "INVALID",
    ),
  ]
}

fn require_catalog_context_ids(
  context: Dict(String, root_field.ResolvedValue),
  field_name: String,
  message: String,
) -> Result(List(String), List(CapturedJsonValue)) {
  case read_arg_string_array(context, field_name) {
    Some(ids) ->
      case ids {
        [] ->
          Error([
            user_error(["input", "context", field_name], message, "INVALID"),
          ])
        [_, ..] -> Ok(ids)
      }
    None ->
      Error([
        user_error(["input", "context", field_name], message, "INVALID"),
      ])
  }
}

fn missing_market_context_errors(
  store: Store,
  market_ids: List(String),
) -> List(CapturedJsonValue) {
  market_ids
  |> list.index_map(fn(id, index) { #(id, index) })
  |> list.filter_map(fn(entry) {
    let #(id, index) = entry
    case store.get_effective_market_by_id(store, id) {
      Some(_) -> Error(Nil)
      None ->
        Ok(user_error(
          ["input", "context", "marketIds", int.to_string(index)],
          "Market does not exist",
          "INVALID",
        ))
    }
  })
}

fn missing_company_location_context_errors(
  store: Store,
  location_ids: List(String),
) -> List(CapturedJsonValue) {
  location_ids
  |> list.index_map(fn(id, index) { #(id, index) })
  |> list.filter_map(fn(entry) {
    let #(id, index) = entry
    case store.get_effective_b2b_company_location_by_id(store, id) {
      Some(_) -> Error(Nil)
      None ->
        Ok(user_error(
          ["input", "context", "companyLocationIds", int.to_string(index)],
          "Company location does not exist",
          "INVALID",
        ))
    }
  })
}

fn catalog_data(
  store: Store,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  let existing_value = existing |> option.unwrap(CapturedObject([]))
  let title =
    read_arg_string_allow_empty(input, "title")
    |> option.or(captured_string_field(existing_value, "title"))
    |> option.unwrap("")
    |> string.trim
  let status =
    graphql_helpers.read_arg_string_nonempty(input, "status")
    |> option.or(captured_string_field(existing_value, "status"))
    |> option.unwrap("ACTIVE")
  let market_ids =
    graphql_helpers.read_arg_object(input, "context")
    |> option.then(read_arg_string_array(_, "marketIds"))
    |> option.unwrap([])
  captured_object_upsert(existing_value, [
    #("__typename", CapturedString("MarketCatalog")),
    #("id", CapturedString(id)),
    #("title", CapturedString(title)),
    #("status", CapturedString(status)),
    #("markets", market_connection_from_ids(store, market_ids)),
    #(
      "operations",
      captured_field(existing_value, "operations")
        |> option.unwrap(CapturedArray([])),
    ),
    #(
      "priceList",
      captured_field(existing_value, "priceList") |> option.unwrap(CapturedNull),
    ),
    #(
      "publication",
      captured_field(existing_value, "publication")
        |> option.unwrap(CapturedNull),
    ),
  ])
}

fn market_connection_from_ids(
  store: Store,
  market_ids: List(String),
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "edges",
      CapturedArray(
        list.map(market_ids, fn(id) {
          CapturedObject([
            #("cursor", CapturedString(id)),
            #("node", market_node_for_id(store, id)),
          ])
        }),
      ),
    ),
    #(
      "nodes",
      CapturedArray(list.map(market_ids, market_node_for_id(store, _))),
    ),
    #("pageInfo", page_info_for_cursors(market_ids)),
  ])
}

fn market_node_for_id(store: Store, id: String) -> CapturedJsonValue {
  case store.get_effective_market_by_id(store, id) {
    Some(record) -> record.data
    None ->
      CapturedObject([
        #("__typename", CapturedString("Market")),
        #("id", CapturedString(id)),
      ])
  }
}

fn price_list_input_errors(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> List(CapturedJsonValue) {
  let name =
    read_arg_string_allow_empty(input, "name")
    |> option.or(option.then(existing, captured_string_field(_, "name")))
    |> option.unwrap("")
    |> string.trim
  let name_errors = case name {
    "" -> [user_error(["input", "name"], "Name can't be blank", "BLANK")]
    _ -> []
  }
  let currency_errors = price_list_currency_errors(input, existing)
  let parent_errors = case currency_errors {
    [] -> price_list_parent_errors(input, existing)
    _ -> []
  }
  name_errors
  |> list.append(currency_errors)
  |> list.append(parent_errors)
}

fn price_list_currency_errors(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_string_nonempty(input, "currency") {
    Some(currency) ->
      case valid_currency(currency) {
        True -> []
        False -> [
          user_error(
            ["input", "currency"],
            "Currency isn't included in the list",
            "INCLUSION",
          ),
        ]
      }
    None ->
      case existing {
        Some(_) -> []
        None -> [
          user_error(["input", "currency"], "Currency can't be blank", "BLANK"),
        ]
      }
  }
}

fn price_list_parent_errors(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_object(input, "parent") {
    Some(parent) -> price_list_parent_adjustment_errors(parent)
    None ->
      case existing {
        Some(_) -> []
        None -> [
          user_error(["input", "parent"], "Parent must exist", "REQUIRED"),
        ]
      }
  }
}

fn price_list_parent_adjustment_errors(
  parent: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_object(parent, "adjustment") {
    Some(adjustment) ->
      case graphql_helpers.read_arg_string_nonempty(adjustment, "type") {
        Some("PERCENTAGE_DECREASE") | Some("PERCENTAGE_INCREASE") -> []
        _ -> [
          user_error(
            ["input", "parent", "adjustment", "type"],
            "Type is invalid",
            "INVALID",
          ),
        ]
      }
    None -> [
      user_error(
        ["input", "parent", "adjustment"],
        "Adjustment must exist",
        "REQUIRED",
      ),
    ]
  }
}

fn valid_currency(currency: String) -> Bool {
  list.contains(iso_currency_codes(), currency)
}

fn iso_currency_codes() -> List(String) {
  [
    "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM",
    "BBD", "BDT", "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BRL", "BSD", "BTN",
    "BWP", "BYN", "BYR", "BZD", "CAD", "CDF", "CHF", "CLF", "CLP", "CNY", "COP",
    "CRC", "CUC", "CUP", "CVE", "CZK", "DJF", "DKK", "DOP", "DZD", "EGP", "ERN",
    "ETB", "EUR", "FJD", "FKP", "GBP", "GEL", "GHS", "GIP", "GMD", "GNF", "GTQ",
    "GYD", "HKD", "HNL", "HTG", "HUF", "IDR", "ILS", "INR", "IQD", "IRR", "ISK",
    "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRW", "KWD", "KYD",
    "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD",
    "MMK", "MNT", "MOP", "MRU", "MUR", "MVR", "MWK", "MXN", "MYR", "MZN", "NAD",
    "NGN", "NIO", "NOK", "NPR", "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR",
    "PLN", "PYG", "QAR", "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SDG",
    "SEK", "SGD", "SHP", "SKK", "SLE", "SLL", "SOS", "SRD", "SSP", "STD", "STN",
    "SVC", "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD",
    "TZS", "UAH", "UGX", "USD", "UYU", "UZS", "VES", "VND", "VUV", "WST", "XAF",
    "XAG", "XAU", "XBA", "XBB", "XBC", "XBD", "XCD", "XCG", "XDR", "XOF", "XPD",
    "XPF", "XPT", "XTS", "YER", "ZAR", "ZMK", "ZMW", "ZWG",
  ]
}

fn price_list_data(
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  let existing_value = existing |> option.unwrap(CapturedObject([]))
  let name =
    read_arg_string_allow_empty(input, "name")
    |> option.or(captured_string_field(existing_value, "name"))
    |> option.unwrap("")
    |> string.trim
  let currency =
    graphql_helpers.read_arg_string_nonempty(input, "currency")
    |> option.or(captured_string_field(existing_value, "currency"))
    |> option.unwrap("")
  captured_object_upsert(existing_value, [
    #("__typename", CapturedString("PriceList")),
    #("id", CapturedString(id)),
    #("name", CapturedString(name)),
    #("currency", CapturedString(currency)),
    #(
      "fixedPricesCount",
      captured_field(existing_value, "fixedPricesCount")
        |> option.unwrap(CapturedInt(0)),
    ),
    #("parent", price_list_parent_data(input, existing_value)),
    #(
      "catalog",
      captured_field(existing_value, "catalog") |> option.unwrap(CapturedNull),
    ),
    #(
      "prices",
      captured_field(existing_value, "prices")
        |> option.unwrap(empty_price_connection()),
    ),
    #(
      "quantityRules",
      captured_field(existing_value, "quantityRules")
        |> option.unwrap(empty_connection()),
    ),
  ])
}

fn price_list_parent_data(
  input: Dict(String, root_field.ResolvedValue),
  existing_value: CapturedJsonValue,
) -> CapturedJsonValue {
  case graphql_helpers.read_arg_object(input, "parent") {
    Some(parent) -> {
      let adjustment =
        graphql_helpers.read_arg_object(parent, "adjustment")
        |> option.unwrap(dict.new())
      let adjustment_type =
        graphql_helpers.read_arg_string_nonempty(adjustment, "type")
        |> option.unwrap("")
      let adjustment_value =
        read_price_list_adjustment_value(adjustment)
        |> option.unwrap(CapturedInt(0))
      CapturedObject([
        #(
          "adjustment",
          CapturedObject([
            #("type", CapturedString(adjustment_type)),
            #("value", adjustment_value),
          ]),
        ),
      ])
    }
    None ->
      captured_field(existing_value, "parent") |> option.unwrap(CapturedNull)
  }
}

fn read_price_list_adjustment_value(
  adjustment: Dict(String, root_field.ResolvedValue),
) -> Option(CapturedJsonValue) {
  case dict.get(adjustment, "value") {
    Ok(root_field.IntVal(value)) -> Some(CapturedInt(value))
    Ok(root_field.FloatVal(value)) -> Some(CapturedFloat(value))
    _ -> None
  }
}

fn product_level_fixed_price_errors(
  store: Store,
  price_inputs: List(Dict(String, root_field.ResolvedValue)),
  delete_product_ids: List(String),
) -> List(CapturedJsonValue) {
  let add_errors =
    price_inputs
    |> enumerate_dicts
    |> list.filter_map(fn(entry) {
      let #(input, index) = entry
      let product_id =
        graphql_helpers.read_arg_string_nonempty(input, "productId")
        |> option.unwrap("")
      case store.get_effective_product_by_id(store, product_id) {
        Some(_) -> Error(Nil)
        None ->
          Ok(user_error(
            ["pricesToAdd", int.to_string(index), "productId"],
            "Product " <> product_id <> " in `pricesToAdd` does not exist.",
            "PRODUCT_DOES_NOT_EXIST",
          ))
      }
    })
  let delete_errors =
    delete_product_ids
    |> enumerate_strings
    |> list.filter_map(fn(entry) {
      let #(product_id, index) = entry
      case store.get_effective_product_by_id(store, product_id) {
        Some(_) -> Error(Nil)
        None ->
          Ok(user_error(
            ["pricesToDeleteByProductIds", int.to_string(index)],
            "Product "
              <> product_id
              <> " in `pricesToDeleteByProductIds` does not exist.",
            "PRODUCT_DOES_NOT_EXIST",
          ))
      }
    })
  list.append(add_errors, delete_errors)
}

fn upsert_fixed_price_nodes(
  price_list: PriceListRecord,
  store: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> PriceListRecord {
  let existing_edges = price_edges(price_list.data)
  let input_variant_ids = mutation_variant_ids(inputs)
  let retained =
    existing_edges
    |> list.filter(fn(edge) {
      case fixed_price_edge_variant_id(edge) {
        Some(id) -> !list.contains(input_variant_ids, id)
        None -> True
      }
    })
  let new_edges =
    inputs
    |> list.filter_map(fn(input) {
      use variant_id <- result.try(
        graphql_helpers.read_arg_string_nonempty(input, "variantId")
        |> option_to_result,
      )
      use variant <- result.try(
        store.get_effective_variant_by_id(store, variant_id) |> option_to_result,
      )
      Ok(price_edge_for_variant(
        store,
        variant,
        input,
        price_list_currency(price_list),
      ))
    })
  rebuild_price_list_prices(price_list, list.append(retained, new_edges))
}

fn delete_fixed_price_nodes(
  price_list: PriceListRecord,
  variant_ids: List(String),
) -> PriceListRecord {
  let retained =
    price_edges(price_list.data)
    |> list.filter(fn(edge) {
      case fixed_price_edge_variant_id(edge) {
        Some(id) -> !list.contains(variant_ids, id)
        None -> True
      }
    })
  rebuild_price_list_prices(price_list, retained)
}

fn rebuild_price_list_prices(
  price_list: PriceListRecord,
  edges: List(CapturedJsonValue),
) -> PriceListRecord {
  let fixed_count =
    edges
    |> list.filter(fn(edge) {
      case captured_edge_node(edge) {
        Some(node) -> captured_string_field(node, "originType") == Some("FIXED")
        None -> False
      }
    })
    |> list.length
  PriceListRecord(
    ..price_list,
    data: captured_object_upsert(price_list.data, [
      #("fixedPricesCount", CapturedInt(fixed_count)),
      #("prices", price_connection_from_edges(edges)),
    ]),
  )
}

fn price_edge_for_variant(
  store: Store,
  variant: ProductVariantRecord,
  input: Dict(String, root_field.ResolvedValue),
  currency: String,
) -> CapturedJsonValue {
  let product = store.get_effective_product_by_id(store, variant.product_id)
  CapturedObject([
    #("cursor", CapturedString(variant.id)),
    #(
      "node",
      CapturedObject([
        #("__typename", CapturedString("PriceListPrice")),
        #(
          "price",
          money_payload(
            graphql_helpers.read_arg_object(input, "price"),
            currency,
          ),
        ),
        #(
          "compareAtPrice",
          optional_money_payload(
            graphql_helpers.read_arg_object(input, "compareAtPrice"),
            currency,
          ),
        ),
        #("originType", CapturedString("FIXED")),
        #("variant", variant_payload(store, variant, product)),
        #("quantityPriceBreaks", empty_connection()),
      ]),
    ),
  ])
}

fn price_list_currency(price_list: PriceListRecord) -> String {
  captured_string_field(price_list.data, "currency") |> option.unwrap("USD")
}

fn product_payloads(
  store: Store,
  product_ids: List(String),
) -> CapturedJsonValue {
  CapturedArray(
    product_ids
    |> list.filter_map(fn(id) {
      store.get_effective_product_by_id(store, id) |> option_to_result
    })
    |> list.map(product_payload),
  )
}

fn product_payload(product: ProductRecord) -> CapturedJsonValue {
  CapturedObject([
    #("__typename", CapturedString("Product")),
    #("id", CapturedString(product.id)),
    #("title", CapturedString(product.title)),
    #("handle", CapturedString(product.handle)),
    #("status", CapturedString(product.status)),
  ])
}

fn variant_payloads(
  store: Store,
  variant_ids: List(String),
) -> CapturedJsonValue {
  CapturedArray(
    variant_ids
    |> list.filter_map(fn(id) {
      store.get_effective_variant_by_id(store, id) |> option_to_result
    })
    |> list.map(fn(variant) {
      variant_payload(
        store,
        variant,
        store.get_effective_product_by_id(store, variant.product_id),
      )
    }),
  )
}

fn variant_payload(
  _store: Store,
  variant: ProductVariantRecord,
  product: Option(ProductRecord),
) -> CapturedJsonValue {
  CapturedObject([
    #("__typename", CapturedString("ProductVariant")),
    #("id", CapturedString(variant.id)),
    #("title", CapturedString(variant.title)),
    #("sku", optional_captured_string(variant.sku)),
    #("product", case product {
      Some(p) -> product_payload(p)
      None -> CapturedNull
    }),
  ])
}

fn quantity_pricing_input_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  let price_break_errors =
    variant_not_found_errors(
      store,
      read_arg_object_array(input, "quantityPriceBreaksToAdd"),
      ["input", "quantityPriceBreaksToAdd"],
      "QUANTITY_PRICE_BREAK_ADD_VARIANT_NOT_FOUND",
      "Variant not found.",
    )
  let rule_errors =
    variant_not_found_errors(
      store,
      read_arg_object_array(input, "quantityRulesToAdd"),
      ["input", "quantityRulesToAdd"],
      "QUANTITY_RULE_ADD_VARIANT_NOT_FOUND",
      "Variant not found.",
    )
  let price_errors =
    variant_not_found_errors(
      store,
      read_arg_object_array(input, "pricesToAdd"),
      ["input", "pricesToAdd"],
      "PRICE_ADD_VARIANT_NOT_FOUND",
      "Variant not found.",
    )
  list.append(price_break_errors, list.append(rule_errors, price_errors))
}

fn quantity_rules_input_errors(
  store: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(CapturedJsonValue) {
  inputs
  |> enumerate_dicts
  |> list.filter_map(fn(entry) {
    let #(input, index) = entry
    let variant_id =
      graphql_helpers.read_arg_string_nonempty(input, "variantId")
      |> option.unwrap("")
    case store.get_effective_variant_by_id(store, variant_id) {
      Some(_) -> Error(Nil)
      None ->
        Ok(user_error(
          ["quantityRules", int.to_string(index), "variantId"],
          "Product variant ID does not exist.",
          "PRODUCT_VARIANT_DOES_NOT_EXIST",
        ))
    }
  })
}

fn quantity_rule_delete_errors(
  store: Store,
  variant_ids: List(String),
) -> List(CapturedJsonValue) {
  variant_ids
  |> enumerate_strings
  |> list.filter_map(fn(entry) {
    let #(variant_id, index) = entry
    case store.get_effective_variant_by_id(store, variant_id) {
      Some(_) -> Error(Nil)
      None ->
        Ok(user_error(
          ["variantIds", int.to_string(index)],
          "Product variant ID does not exist.",
          "PRODUCT_VARIANT_DOES_NOT_EXIST",
        ))
    }
  })
}

fn variant_not_found_errors(
  store: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  field_prefix: List(String),
  code: String,
  message: String,
) -> List(CapturedJsonValue) {
  inputs
  |> enumerate_dicts
  |> list.filter_map(fn(entry) {
    let #(input, index) = entry
    let variant_id =
      graphql_helpers.read_arg_string_nonempty(input, "variantId")
      |> option.unwrap("")
    case store.get_effective_variant_by_id(store, variant_id) {
      Some(_) -> Error(Nil)
      None ->
        Ok(user_error(
          list.append(field_prefix, [int.to_string(index)]),
          message,
          code,
        ))
    }
  })
}

fn upsert_quantity_rule_nodes(
  price_list: PriceListRecord,
  store: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> PriceListRecord {
  let existing_edges = quantity_rule_edges(price_list.data)
  let input_variant_ids = mutation_variant_ids(inputs)
  let retained =
    existing_edges
    |> list.filter(fn(edge) {
      case quantity_rule_edge_variant_id(edge) {
        Some(id) -> !list.contains(input_variant_ids, id)
        None -> True
      }
    })
  let new_edges =
    inputs
    |> list.filter_map(fn(input) {
      use variant_id <- result.try(
        graphql_helpers.read_arg_string_nonempty(input, "variantId")
        |> option_to_result,
      )
      use variant <- result.try(
        store.get_effective_variant_by_id(store, variant_id) |> option_to_result,
      )
      Ok(
        CapturedObject([
          #("cursor", CapturedString(variant_id)),
          #("node", quantity_rule_node(store, variant, input)),
        ]),
      )
    })
  PriceListRecord(
    ..price_list,
    data: captured_object_upsert(price_list.data, [
      #(
        "quantityRules",
        price_connection_from_edges(list.append(retained, new_edges)),
      ),
    ]),
  )
}

fn delete_quantity_rule_nodes(
  price_list: PriceListRecord,
  variant_ids: List(String),
) -> PriceListRecord {
  let retained =
    quantity_rule_edges(price_list.data)
    |> list.filter(fn(edge) {
      case quantity_rule_edge_variant_id(edge) {
        Some(id) -> !list.contains(variant_ids, id)
        None -> True
      }
    })
  PriceListRecord(
    ..price_list,
    data: captured_object_upsert(price_list.data, [
      #("quantityRules", price_connection_from_edges(retained)),
    ]),
  )
}

fn quantity_rule_payloads(
  store: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> CapturedJsonValue {
  CapturedArray(
    inputs
    |> list.filter_map(fn(input) {
      use variant_id <- result.try(
        graphql_helpers.read_arg_string_nonempty(input, "variantId")
        |> option_to_result,
      )
      use variant <- result.try(
        store.get_effective_variant_by_id(store, variant_id) |> option_to_result,
      )
      Ok(quantity_rule_node(store, variant, input))
    }),
  )
}

fn quantity_rule_node(
  store: Store,
  variant: ProductVariantRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("__typename", CapturedString("QuantityRule")),
    #(
      "minimum",
      CapturedInt(
        graphql_helpers.read_arg_int(input, "minimum") |> option.unwrap(1),
      ),
    ),
    #(
      "maximum",
      optional_captured_int(graphql_helpers.read_arg_int(input, "maximum")),
    ),
    #(
      "increment",
      CapturedInt(
        graphql_helpers.read_arg_int(input, "increment") |> option.unwrap(1),
      ),
    ),
    #("isDefault", CapturedBool(False)),
    #("originType", CapturedString("FIXED")),
    #(
      "productVariant",
      variant_payload(
        store,
        variant,
        store.get_effective_product_by_id(store, variant.product_id),
      ),
    ),
  ])
}

fn upsert_quantity_price_break_nodes(
  price_list: PriceListRecord,
  store: Store,
  _identity: SyntheticIdentityRegistry,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> PriceListRecord {
  let input_variant_ids = mutation_variant_ids(inputs)
  let next_edges =
    price_edges(price_list.data)
    |> list.map(fn(edge) {
      case fixed_price_edge_variant_id(edge) {
        Some(variant_id) ->
          case list.contains(input_variant_ids, variant_id) {
            True ->
              rebuild_price_edge_with_breaks(
                edge,
                list.filter(inputs, fn(input) {
                  graphql_helpers.read_arg_string_nonempty(input, "variantId")
                  == Some(variant_id)
                })
                  |> list.filter_map(fn(input) {
                    use variant <- result.try(
                      store.get_effective_variant_by_id(store, variant_id)
                      |> option_to_result,
                    )
                    Ok(
                      CapturedObject([
                        #("cursor", CapturedString(variant_id <> ":break")),
                        #(
                          "node",
                          quantity_price_break_node(
                            store,
                            price_list,
                            variant,
                            input,
                          ),
                        ),
                      ]),
                    )
                  }),
              )
            False -> edge
          }
        None -> edge
      }
    })
  rebuild_price_list_prices(price_list, next_edges)
}

fn quantity_price_break_node(
  store: Store,
  price_list: PriceListRecord,
  variant: ProductVariantRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("__typename", CapturedString("QuantityPriceBreak")),
    #("id", CapturedString(variant.id <> ":quantity-price-break")),
    #(
      "minimumQuantity",
      CapturedInt(
        graphql_helpers.read_arg_int(input, "minimumQuantity")
        |> option.unwrap(1),
      ),
    ),
    #(
      "price",
      money_payload(
        graphql_helpers.read_arg_object(input, "price"),
        price_list_currency(price_list),
      ),
    ),
    #(
      "variant",
      variant_payload(
        store,
        variant,
        store.get_effective_product_by_id(store, variant.product_id),
      ),
    ),
  ])
}

fn rebuild_price_edge_with_breaks(
  edge: CapturedJsonValue,
  quantity_break_edges: List(CapturedJsonValue),
) -> CapturedJsonValue {
  case edge {
    CapturedObject(fields) ->
      case captured_field(edge, "node") {
        Some(node) ->
          CapturedObject(replace_field(
            fields,
            "node",
            captured_object_upsert(node, [
              #(
                "quantityPriceBreaks",
                price_connection_from_edges(quantity_break_edges),
              ),
            ]),
          ))
        None -> edge
      }
    _ -> edge
  }
}

fn money_payload(
  raw: Option(Dict(String, root_field.ResolvedValue)),
  currency: String,
) -> CapturedJsonValue {
  case raw {
    Some(value) ->
      CapturedObject([
        #(
          "amount",
          CapturedString(
            graphql_helpers.read_arg_string_nonempty(value, "amount")
            |> option.or(
              graphql_helpers.read_arg_int(value, "amount")
              |> option.map(int.to_string),
            )
            |> option.unwrap("0")
            |> format_money_amount,
          ),
        ),
        #(
          "currencyCode",
          CapturedString(
            graphql_helpers.read_arg_string_nonempty(value, "currencyCode")
            |> option.unwrap(currency),
          ),
        ),
      ])
    None -> CapturedNull
  }
}

fn optional_money_payload(
  raw: Option(Dict(String, root_field.ResolvedValue)),
  currency: String,
) -> CapturedJsonValue {
  case raw {
    Some(_) -> money_payload(raw, currency)
    None -> CapturedNull
  }
}

fn format_money_amount(raw: String) -> String {
  case string.split(raw, ".") {
    [whole, fraction] -> whole <> "." <> trim_money_fraction(fraction)
    _ -> raw
  }
}

fn trim_money_fraction(fraction: String) -> String {
  case string.ends_with(fraction, "0") {
    True -> trim_money_fraction(string.drop_end(fraction, 1))
    False ->
      case fraction {
        "" -> "0"
        _ -> fraction
      }
  }
}

fn price_edges(value: CapturedJsonValue) -> List(CapturedJsonValue) {
  captured_connection_edges(captured_field(value, "prices"))
}

fn quantity_rule_edges(value: CapturedJsonValue) -> List(CapturedJsonValue) {
  captured_connection_edges(captured_field(value, "quantityRules"))
}

fn captured_connection_edges(
  value: Option(CapturedJsonValue),
) -> List(CapturedJsonValue) {
  case value {
    Some(CapturedObject(_)) ->
      case captured_field(value |> option.unwrap(CapturedNull), "edges") {
        Some(CapturedArray(edges)) -> edges
        _ -> []
      }
    _ -> []
  }
}

fn fixed_price_edge_variant_id(edge: CapturedJsonValue) -> Option(String) {
  case captured_edge_node(edge) {
    Some(node) ->
      case captured_field(node, "variant") {
        Some(variant) -> captured_string_field(variant, "id")
        None -> None
      }
    None -> None
  }
}

fn quantity_rule_edge_variant_id(edge: CapturedJsonValue) -> Option(String) {
  case captured_edge_node(edge) {
    Some(node) ->
      case captured_field(node, "productVariant") {
        Some(variant) -> captured_string_field(variant, "id")
        None -> None
      }
    None -> None
  }
}

fn captured_edge_node(edge: CapturedJsonValue) -> Option(CapturedJsonValue) {
  captured_field(edge, "node")
}

fn price_connection_from_edges(
  edges: List(CapturedJsonValue),
) -> CapturedJsonValue {
  let cursors =
    edges
    |> list.filter_map(fn(edge) {
      captured_string_field(edge, "cursor") |> option_to_result
    })
  CapturedObject([
    #("edges", CapturedArray(edges)),
    #(
      "nodes",
      CapturedArray(
        edges
        |> list.filter_map(fn(edge) {
          captured_edge_node(edge) |> option_to_result
        }),
      ),
    ),
    #("pageInfo", page_info_for_cursors(cursors)),
  ])
}

fn page_info_for_cursors(cursors: List(String)) -> CapturedJsonValue {
  CapturedObject([
    #("hasNextPage", CapturedBool(False)),
    #("hasPreviousPage", CapturedBool(False)),
    #(
      "startCursor",
      optional_captured_string(list.first(cursors) |> result_to_option),
    ),
    #(
      "endCursor",
      optional_captured_string(list.last(cursors) |> result_to_option),
    ),
  ])
}

fn empty_connection() -> CapturedJsonValue {
  price_connection_from_edges([])
}

fn empty_price_connection() -> CapturedJsonValue {
  price_connection_from_edges([])
}

fn market_localizable_resource_payload(
  store: Store,
  resource_id: String,
) -> CapturedJsonValue {
  case store.find_effective_metafield_by_id(store, resource_id) {
    Some(_) ->
      CapturedObject([
        #("resourceId", CapturedString(resource_id)),
        #("marketLocalizableContent", CapturedArray([])),
        #("marketLocalizations", CapturedArray([])),
      ])
    None -> CapturedNull
  }
}

fn captured_object_upsert(
  value: CapturedJsonValue,
  updates: List(#(String, CapturedJsonValue)),
) -> CapturedJsonValue {
  let base = case value {
    CapturedObject(fields) -> fields
    _ -> []
  }
  let retained =
    base
    |> list.filter(fn(pair) {
      let #(key, _) = pair
      !list.any(updates, fn(update) {
        let #(update_key, _) = update
        update_key == key
      })
    })
  CapturedObject(list.append(retained, updates))
}

fn replace_field(
  fields: List(#(String, CapturedJsonValue)),
  key: String,
  value: CapturedJsonValue,
) -> List(#(String, CapturedJsonValue)) {
  list.append(
    list.filter(fields, fn(pair) {
      let #(field_key, _) = pair
      field_key != key
    }),
    [#(key, value)],
  )
}

fn string_array(values: List(String)) -> CapturedJsonValue {
  CapturedArray(list.map(values, CapturedString))
}

fn optional_captured_int(value: Option(Int)) -> CapturedJsonValue {
  case value {
    Some(i) -> CapturedInt(i)
    None -> CapturedNull
  }
}

fn mutation_variant_ids(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(String) {
  inputs
  |> list.filter_map(fn(input) {
    graphql_helpers.read_arg_string_nonempty(input, "variantId")
    |> option_to_result
  })
}

fn append_unique_strings(
  base: List(String),
  extra: List(String),
) -> List(String) {
  list.fold(extra, base, fn(acc, item) {
    case list.contains(acc, item) {
      True -> acc
      False -> list.append(acc, [item])
    }
  })
}

fn enumerate_dicts(
  items: List(Dict(String, root_field.ResolvedValue)),
) -> List(#(Dict(String, root_field.ResolvedValue), Int)) {
  enumerate_dicts_loop(items, 0)
}

fn enumerate_dicts_loop(
  items: List(Dict(String, root_field.ResolvedValue)),
  index: Int,
) -> List(#(Dict(String, root_field.ResolvedValue), Int)) {
  case items {
    [] -> []
    [first, ..rest] -> [
      #(first, index),
      ..enumerate_dicts_loop(rest, index + 1)
    ]
  }
}

fn enumerate_strings(items: List(String)) -> List(#(String, Int)) {
  enumerate_strings_loop(items, 0)
}

fn enumerate_strings_loop(
  items: List(String),
  index: Int,
) -> List(#(String, Int)) {
  case items {
    [] -> []
    [first, ..rest] -> [
      #(first, index),
      ..enumerate_strings_loop(rest, index + 1)
    ]
  }
}

fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(item) -> Ok(item)
    None -> Error(Nil)
  }
}

fn result_to_option(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(item) -> Some(item)
    Error(_) -> None
  }
}

fn root_payload_for_field(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "market" ->
          serialize_record_by_id(
            field,
            fragments,
            variables,
            fn(id) { store.get_effective_market_by_id(store, id) },
            market_record_source,
          )
        "markets" ->
          serialize_connection(
            field,
            connection_config_for_field(
              field,
              store.list_effective_markets(store)
                |> list.map(fn(record) {
                  connection_item(record.cursor, market_record_source(record))
                }),
              fragments,
              variables,
            ),
          )
        "catalog" ->
          serialize_record_by_id(
            field,
            fragments,
            variables,
            fn(id) { store.get_effective_catalog_by_id(store, id) },
            catalog_record_source,
          )
        "catalogs" ->
          serialize_connection(
            field,
            connection_config_for_field(
              field,
              store.list_effective_catalogs(store)
                |> list.map(fn(record) {
                  connection_item(record.cursor, catalog_record_source(record))
                }),
              fragments,
              variables,
            ),
          )
        "catalogsCount" ->
          serialize_exact_count(
            field,
            list.length(store.list_effective_catalogs(store)),
          )
        "priceList" ->
          serialize_record_by_id(
            field,
            fragments,
            variables,
            fn(id) { store.get_effective_price_list_by_id(store, id) },
            price_list_record_source,
          )
        "priceLists" ->
          serialize_connection(
            field,
            connection_config_for_field(
              field,
              store.list_effective_price_lists(store)
                |> list.map(fn(record) {
                  connection_item(
                    record.cursor,
                    price_list_record_source(record),
                  )
                }),
              fragments,
              variables,
            ),
          )
        "webPresences" ->
          serialize_connection(
            field,
            connection_config_for_field(
              field,
              store.list_effective_web_presences(store)
                |> list.map(fn(record) {
                  connection_item(
                    record.cursor,
                    web_presence_record_source(record),
                  )
                }),
              fragments,
              variables,
            ),
          )
        "marketsResolvedValues" ->
          case
            store.get_effective_markets_root_payload(
              store,
              "marketsResolvedValues",
            )
          {
            Some(payload) ->
              project_record(field, fragments, captured_json_source(payload))
            None -> json.null()
          }
        "marketLocalizableResource" -> {
          let args = graphql_helpers.field_args(field, variables)
          case graphql_helpers.read_arg_string_nonempty(args, "resourceId") {
            Some(resource_id) ->
              project_record(
                field,
                fragments,
                captured_json_source(market_localizable_resource_payload(
                  store,
                  resource_id,
                )),
              )
            None -> json.null()
          }
        }
        "marketLocalizableResources" | "marketLocalizableResourcesByIds" ->
          serialize_empty_connection(field, default_selected_field_options())
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn serialize_record_by_id(
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  by_id: fn(String) -> Option(a),
  source: fn(a) -> SourceValue,
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case by_id(id) {
        Some(record) -> project_record(field, fragments, source(record))
        None -> json.null()
      }
    None -> json.null()
  }
}

fn connection_item(
  cursor: Option(String),
  source: SourceValue,
) -> MarketConnectionItem {
  let fallback = case source_string_field(source, "id") {
    Some(id) -> id
    None -> "market-cursor"
  }
  let output = cursor |> option.unwrap(fallback)
  MarketConnectionItem(
    source: source,
    pagination_cursor: output,
    output_cursor: output,
  )
}

fn connection_config_for_field(
  field: Selection,
  items: List(MarketConnectionItem),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> SerializeConnectionConfig(MarketConnectionItem) {
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      fn(item, _index) { item.pagination_cursor },
      default_connection_window_options(),
    )
  SerializeConnectionConfig(
    items: window.items,
    has_next_page: window.has_next_page,
    has_previous_page: window.has_previous_page,
    get_cursor_value: fn(item, _index) { item.output_cursor },
    serialize_node: fn(item, node_field, _index) {
      project_record(node_field, fragments, item.source)
    },
    selected_field_options: default_selected_field_options(),
    page_info_options: market_page_info_options(),
  )
}

fn market_page_info_options() -> ConnectionPageInfoOptions {
  ConnectionPageInfoOptions(
    include_inline_fragments: False,
    prefix_cursors: False,
    include_cursors: True,
    fallback_start_cursor: None,
    fallback_end_cursor: None,
  )
}

fn project_record(
  field: Selection,
  fragments: FragmentMap,
  source: SourceValue,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> json.null()
  }
}

fn market_record_source(record: MarketRecord) -> SourceValue {
  captured_json_source(record.data)
}

fn catalog_record_source(record: CatalogRecord) -> SourceValue {
  captured_json_source(record.data)
}

fn price_list_record_source(record: PriceListRecord) -> SourceValue {
  captured_json_source(record.data)
}

fn web_presence_record_source(record: WebPresenceRecord) -> SourceValue {
  captured_json_source(record.data)
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

fn source_string_field(source: SourceValue, name: String) -> Option(String) {
  case source {
    SrcObject(fields) ->
      case dict.get(fields, name) {
        Ok(SrcString(value)) -> Some(value)
        _ -> None
      }
    _ -> None
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

fn read_arg_string_allow_empty(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
}

fn read_arg_object_array(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, name) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(value) {
        case value {
          root_field.ObjectVal(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_price_list_id(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case graphql_helpers.read_arg_string_nonempty(args, "priceListId") {
    Some(id) -> Some(id)
    None ->
      case graphql_helpers.read_arg_string_nonempty(args, "id") {
        Some(id) -> Some(id)
        None ->
          graphql_helpers.read_arg_object(args, "input")
          |> option.then(graphql_helpers.read_arg_string_nonempty(
            _,
            "priceListId",
          ))
      }
  }
}

fn read_arg_string_array(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(List(String)) {
  case dict.get(args, name) {
    Ok(root_field.ListVal(items)) ->
      Some(
        list.filter_map(items, fn(value) {
          case value {
            root_field.StringVal(item) -> Ok(item)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

fn optional_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(s) -> CapturedString(s)
    None -> CapturedNull
  }
}

fn captured_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) -> captured_field_from_pairs(fields, key)
    _ -> None
  }
}

fn captured_field_from_pairs(
  fields: List(#(String, CapturedJsonValue)),
  key: String,
) -> Option(CapturedJsonValue) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let #(field_key, field_value) = first
      case field_key == key {
        True -> Some(field_value)
        False -> captured_field_from_pairs(rest, key)
      }
    }
  }
}

fn captured_string_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(String) {
  case captured_field(value, key) {
    Some(CapturedString(s)) -> Some(s)
    _ -> None
  }
}
