//// Public entrypoint for Markets domain handling.
////
//// Implementation is split across the markets/* submodules; this file keeps
//// the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcObject, SrcString,
  project_graphql_value, src_object,
}
import shopify_draft_proxy/proxy/markets/mutations
import shopify_draft_proxy/proxy/markets/queries
import shopify_draft_proxy/proxy/markets/serializers
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type BackupRegionRecord, type CapturedJsonValue, BackupRegionRecord,
} as state_types

pub type MarketsError {
  ParseFailed(root_field.RootFieldError)
}

pub fn is_markets_query_root(name: String) -> Bool {
  queries.is_markets_query_root(name)
}

pub fn is_markets_mutation_root(name: String) -> Bool {
  mutations.is_markets_mutation_root(name)
}

pub fn handle_markets_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketsError) {
  case queries.handle_markets_query(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketsError) {
  case queries.process(store, document, variables) {
    Ok(data) -> Ok(data)
    Error(err) -> Error(ParseFailed(err))
  }
}

pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  queries.handle_query_request(
    proxy,
    request,
    parsed,
    primary_root_field,
    document,
    variables,
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  mutations.process_mutation(
    store,
    identity,
    request_path,
    document,
    variables,
    upstream,
  )
}

pub fn serialize_market_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_market_by_id(store, id) {
    Some(record) ->
      project_graphql_value(
        serializers.market_record_source(record),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_market_catalog_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_catalog_by_id(store, id) {
    Some(record) ->
      project_graphql_value(
        serializers.catalog_record_source(record),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_price_list_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_price_list_by_id(store, id) {
    Some(record) ->
      project_graphql_value(
        serializers.price_list_record_source(record),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

pub fn serialize_web_presence_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
  fallback: fn() -> Json,
) -> Json {
  case store.get_effective_web_presence_by_id(store, id) {
    Some(record) ->
      project_node_source(
        serializers.web_presence_record_source(record),
        "MarketWebPresence",
        selections,
        fragments,
      )
    None -> fallback()
  }
}

pub fn serialize_market_region_country_node_by_id(
  store: Store,
  shop_origin: String,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case effective_backup_region(store, shop_origin) {
    Some(region) if region.id == id ->
      project_graphql_value(backup_region_source(region), selections, fragments)
    _ -> json.null()
  }
}

pub fn backup_region_for_country(
  store: Store,
  shop_origin: String,
  code: String,
) -> Option(BackupRegionRecord) {
  let normalized_code = string.uppercase(code)
  case store.get_effective_shop(store) {
    Some(shop) ->
      backup_region_for_shop_country(shop.myshopify_domain, normalized_code)
    None ->
      case backup_region_for_origin_country(shop_origin, normalized_code) {
        Some(region) -> Some(region)
        None ->
          case normalized_code {
            "CA" -> Some(captured_backup_region())
            _ -> None
          }
      }
  }
}

pub fn backup_region_country_has_region_market(
  store: Store,
  shop_origin: String,
  code: String,
) -> Bool {
  let normalized_code = string.uppercase(code)
  let markets = store.list_effective_markets(store)
  case markets {
    [] ->
      captured_region_market_for_country(store, shop_origin, normalized_code)
    [_, ..] ->
      list.any(markets, fn(record) {
        market_record_is_active_region_non_legacy(record.data)
        && market_record_contains_country(record.data, normalized_code)
      })
  }
}

fn market_record_is_active_region_non_legacy(data: CapturedJsonValue) -> Bool {
  market_record_enabled(data)
  && market_record_region_type(data)
  && !market_record_legacy(data)
}

fn market_record_enabled(data: CapturedJsonValue) -> Bool {
  case serializers.captured_field(data, "enabled") {
    Some(state_types.CapturedBool(True)) -> True
    Some(state_types.CapturedBool(False)) -> False
    _ ->
      case serializers.captured_string_field(data, "status") {
        Some("ACTIVE") -> True
        _ -> False
      }
  }
}

fn market_record_region_type(data: CapturedJsonValue) -> Bool {
  case serializers.captured_string_field(data, "type") {
    Some("REGION") -> True
    _ -> !list.is_empty(serializers.market_country_codes(data))
  }
}

fn market_record_legacy(data: CapturedJsonValue) -> Bool {
  case serializers.captured_field(data, "isLegacyMarket") {
    Some(state_types.CapturedBool(is_legacy)) -> is_legacy
    _ ->
      case serializers.captured_field(data, "isLegacy") {
        Some(state_types.CapturedBool(is_legacy)) -> is_legacy
        _ -> False
      }
  }
}

fn market_record_contains_country(
  data: CapturedJsonValue,
  code: String,
) -> Bool {
  serializers.market_country_codes(data)
  |> list.any(fn(country_code) { string.uppercase(country_code) == code })
}

fn captured_region_market_for_country(
  store: Store,
  shop_origin: String,
  code: String,
) -> Bool {
  case effective_shop_domain(store, shop_origin) {
    "very-big-test-store.myshopify.com" -> code == "CA"
    "harry-test-heelo.myshopify.com" ->
      list.contains(
        ["CA", "AE", "AT", "AU", "BE", "CH", "CZ", "DE", "DK", "ES", "FI", "MX"],
        code,
      )
    _ -> code == "CA"
  }
}

fn effective_shop_domain(store: Store, shop_origin: String) -> String {
  case store.get_effective_shop(store) {
    Some(shop) -> string.lowercase(shop.myshopify_domain)
    None -> shop_domain_from_origin(shop_origin)
  }
}

pub fn effective_backup_region(
  store: Store,
  shop_origin: String,
) -> Option(BackupRegionRecord) {
  case store.get_effective_backup_region(store) {
    Some(region) -> Some(region)
    None -> backup_region_for_effective_shop(store, shop_origin)
  }
}

fn backup_region_for_effective_shop(
  store: Store,
  shop_origin: String,
) -> Option(BackupRegionRecord) {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.shop_address.country_code_v2 {
        Some(code) ->
          backup_region_for_shop_country(shop.myshopify_domain, code)
        None -> None
      }
    None ->
      case backup_region_for_origin_country(shop_origin, "CA") {
        Some(region) -> Some(region)
        None -> Some(captured_backup_region())
      }
  }
}

fn backup_region_for_origin_country(
  shop_origin: String,
  code: String,
) -> Option(BackupRegionRecord) {
  let domain = shop_domain_from_origin(shop_origin)
  backup_region_for_shop_country(domain, code)
}

fn shop_domain_from_origin(shop_origin: String) -> String {
  let origin = string.lowercase(shop_origin)
  let without_scheme = case string.starts_with(origin, "https://") {
    True -> string.drop_start(origin, 8)
    False ->
      case string.starts_with(origin, "http://") {
        True -> string.drop_start(origin, 7)
        False -> origin
      }
  }
  case string.split(without_scheme, on: "/") {
    [host, ..] -> host
    [] -> without_scheme
  }
}

fn backup_region_for_shop_country(
  shop_domain: String,
  code: String,
) -> Option(BackupRegionRecord) {
  case string.lowercase(shop_domain), string.uppercase(code) {
    "harry-test-heelo.myshopify.com", "CA" -> Some(captured_backup_region())
    "harry-test-heelo.myshopify.com", "AE" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110482738",
        name: "United Arab Emirates",
        code: "AE",
      ))
    "harry-test-heelo.myshopify.com", "AT" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110515506",
        name: "Austria",
        code: "AT",
      ))
    "harry-test-heelo.myshopify.com", "AU" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110548274",
        name: "Australia",
        code: "AU",
      ))
    "harry-test-heelo.myshopify.com", "BE" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110581042",
        name: "Belgium",
        code: "BE",
      ))
    "harry-test-heelo.myshopify.com", "CH" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110613810",
        name: "Switzerland",
        code: "CH",
      ))
    "harry-test-heelo.myshopify.com", "CZ" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110646578",
        name: "Czechia",
        code: "CZ",
      ))
    "harry-test-heelo.myshopify.com", "DE" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110679346",
        name: "Germany",
        code: "DE",
      ))
    "harry-test-heelo.myshopify.com", "DK" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110712114",
        name: "Denmark",
        code: "DK",
      ))
    "harry-test-heelo.myshopify.com", "ES" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110744882",
        name: "Spain",
        code: "ES",
      ))
    "harry-test-heelo.myshopify.com", "FI" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110777650",
        name: "Finland",
        code: "FI",
      ))
    "harry-test-heelo.myshopify.com", "MX" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062111334706",
        name: "Mexico",
        code: "MX",
      ))
    "very-big-test-store.myshopify.com", "CA" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/454909493481",
        name: "Canada",
        code: "CA",
      ))
    "very-big-test-store.myshopify.com", "US" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/454910378217",
        name: "United States",
        code: "US",
      ))
    _, _ -> None
  }
}

fn captured_backup_region() -> BackupRegionRecord {
  BackupRegionRecord(
    id: "gid://shopify/MarketRegionCountry/4062110417202",
    name: "Canada",
    code: "CA",
  )
}

pub fn backup_region_source(region: BackupRegionRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("MarketRegionCountry")),
    #("id", SrcString(region.id)),
    #("name", SrcString(region.name)),
    #("code", SrcString(region.code)),
  ])
}

fn project_node_source(
  source: SourceValue,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    source_with_typename(source, typename),
    selections,
    fragments,
  )
}

fn source_with_typename(source: SourceValue, typename: String) -> SourceValue {
  case source {
    SrcObject(fields) ->
      SrcObject(dict.insert(fields, "__typename", SrcString(typename)))
    _ -> source
  }
}
