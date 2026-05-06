//// Markets payload builders, projection helpers, and shared scalar readers.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionPageInfoOptions, type FragmentMap,
  type SerializeConnectionConfig, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, serialize_empty_connection,
}
import shopify_draft_proxy/proxy/markets/types as market_types
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, single_root_log_draft,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CatalogRecord,
  type MarketLocalizableContentRecord, type MarketLocalizationRecord,
  type MarketRecord, type PriceListRecord, type ProductRecord,
  type ProductVariantRecord, type PublicationRecord, type WebPresenceRecord,
  CapturedArray, CapturedBool, CapturedFloat, CapturedInt, CapturedNull,
  CapturedObject, CapturedString, PriceListRecord,
}

@internal
pub fn serialize_root_fields(
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

@internal
pub fn user_error(
  field: List(String),
  message: String,
  code: String,
) -> CapturedJsonValue {
  user_error_with_typename(field, message, code, None)
}

@internal
pub fn translation_user_error(
  field: List(String),
  message: String,
  code: String,
) -> CapturedJsonValue {
  user_error_with_typename(field, message, code, Some("TranslationUserError"))
}

@internal
pub fn quantity_rule_user_error(
  field: List(String),
  message: String,
  code: String,
) -> CapturedJsonValue {
  user_error_with_typename(field, message, code, Some("QuantityRuleUserError"))
}

@internal
pub fn user_error_with_typename(
  field: List(String),
  message: String,
  code: String,
  typename: Option(String),
) -> CapturedJsonValue {
  let typename_fields = case typename {
    Some(value) -> [#("__typename", CapturedString(value))]
    None -> []
  }
  CapturedObject(
    list.append(typename_fields, [
      #("field", CapturedArray(list.map(field, CapturedString))),
      #("message", CapturedString(message)),
      #("code", CapturedString(code)),
    ]),
  )
}

const price_list_fixed_prices_by_product_user_error_typename = "PriceListFixedPricesByProductBulkUpdateUserError"

const price_list_fixed_prices_by_product_fixed_price_limit = 10_000

@internal
pub fn price_list_fixed_prices_by_product_user_error(
  field: List(String),
  message: String,
  code: String,
) -> CapturedJsonValue {
  user_error_with_typename(
    field,
    message,
    code,
    Some(price_list_fixed_prices_by_product_user_error_typename),
  )
}

@internal
pub fn price_list_fixed_prices_by_product_user_error_null_field(
  message: String,
  code: String,
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "__typename",
      CapturedString(price_list_fixed_prices_by_product_user_error_typename),
    ),
    #("field", CapturedNull),
    #("message", CapturedString(message)),
    #("code", CapturedString(code)),
  ])
}

@internal
pub fn markets_log_draft(
  root_name: String,
  staged_ids: List(String),
) -> LogDraft {
  let status = case staged_ids {
    [] -> store_types.Failed
    [_, ..] -> store_types.Staged
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

@internal
pub fn market_data(
  store: Store,
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
    |> option.unwrap("DRAFT")
  let enabled =
    graphql_helpers.read_arg_bool(input, "enabled")
    |> option.unwrap(status == "ACTIVE")
  let handle = market_data_handle(store, id, input, existing_value, name)
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
    #("handle", CapturedString(handle)),
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

@internal
pub fn market_data_handle(
  store: Store,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  existing_value: CapturedJsonValue,
  name: String,
) -> String {
  case read_explicit_market_handle(input) {
    Some(handle) -> handle
    None -> {
      let base =
        captured_string_field(existing_value, "handle")
        |> option.unwrap(market_handle(name))
      ensure_unique_market_handle(store, base, Some(id), 0)
    }
  }
}

@internal
pub fn read_explicit_market_handle(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case read_arg_string_allow_empty(input, "handle") {
    Some(handle) -> Some(market_handle(handle))
    None -> None
  }
}

@internal
pub fn market_handle(name: String) -> String {
  string.trim(name)
  |> string.lowercase
  |> string.to_graphemes
  |> list.fold(#([], ""), fn(acc, grapheme) {
    let #(parts, current) = acc
    case is_market_handle_grapheme(grapheme) {
      True -> #(parts, current <> grapheme)
      False ->
        case current {
          "" -> #(parts, "")
          _ -> #([current, ..parts], "")
        }
    }
  })
  |> finish_market_handle_parts
}

@internal
pub fn finish_market_handle_parts(
  parts_state: #(List(String), String),
) -> String {
  let #(parts, current) = parts_state
  let parts = case current {
    "" -> parts
    _ -> [current, ..parts]
  }
  parts
  |> list.reverse
  |> string.join("-")
}

@internal
pub fn is_market_handle_grapheme(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    _ -> False
  }
}

@internal
pub fn ensure_unique_market_handle(
  store: Store,
  handle: String,
  current_id: Option(String),
  suffix: Int,
) -> String {
  let candidate = case suffix {
    0 -> handle
    _ -> handle <> "-" <> int.to_string(suffix)
  }
  case market_handle_in_use(store, candidate, current_id) {
    True -> ensure_unique_market_handle(store, handle, current_id, suffix + 1)
    False -> candidate
  }
}

@internal
pub fn market_handle_in_use(
  store: Store,
  handle: String,
  current_id: Option(String),
) -> Bool {
  store.list_effective_markets(store)
  |> list.any(fn(record) {
    case current_id == Some(record.id) {
      True -> False
      False -> captured_string_field(record.data, "handle") == Some(handle)
    }
  })
}

@internal
pub fn market_name_in_use(store: Store, name: String) -> Bool {
  store.list_effective_markets(store)
  |> list.any(fn(record) {
    captured_string_field(record.data, "name") == Some(name)
  })
}

@internal
pub fn read_market_region_inputs(
  input: Dict(String, root_field.ResolvedValue),
) -> List(market_types.MarketRegionInput) {
  list.append(
    read_legacy_market_region_inputs(input),
    read_conditions_market_region_inputs(input),
  )
}

@internal
pub fn read_legacy_market_region_inputs(
  input: Dict(String, root_field.ResolvedValue),
) -> List(market_types.MarketRegionInput) {
  read_arg_object_array(input, "regions")
  |> list.index_map(fn(region, index) {
    case graphql_helpers.read_arg_string_nonempty(region, "countryCode") {
      Some(code) ->
        Ok(market_types.MarketRegionInput(
          field: ["input", "regions", int.to_string(index), "countryCode"],
          country_code: code,
        ))
      None -> Error(Nil)
    }
  })
  |> result.values
}

@internal
pub fn read_conditions_market_region_inputs(
  input: Dict(String, root_field.ResolvedValue),
) -> List(market_types.MarketRegionInput) {
  let regions_condition =
    graphql_helpers.read_arg_object(input, "conditions")
    |> option.then(graphql_helpers.read_arg_object(_, "regionsCondition"))
    |> option.unwrap(dict.new())
  read_arg_object_array(regions_condition, "regions")
  |> list.index_map(fn(region, index) {
    case graphql_helpers.read_arg_string_nonempty(region, "countryCode") {
      Some(code) ->
        Ok(market_types.MarketRegionInput(
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

@internal
pub fn assigned_market_country_codes(store: Store) -> List(String) {
  store.list_effective_markets(store)
  |> list.fold([], fn(codes, record) {
    list.append(codes, market_country_codes(record.data))
  })
}

@internal
pub fn market_country_codes(data: CapturedJsonValue) -> List(String) {
  captured_field(data, "conditions")
  |> option.then(captured_field(_, "regionsCondition"))
  |> option.then(captured_field(_, "regions"))
  |> option.map(region_codes_from_connection)
  |> option.unwrap([])
}

@internal
pub fn region_codes_from_connection(
  connection: CapturedJsonValue,
) -> List(String) {
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

@internal
pub fn region_code_from_node(node: CapturedJsonValue) -> Option(String) {
  captured_string_field(node, "code")
  |> option.or(captured_string_field(node, "countryCode"))
}

@internal
pub fn market_conditions_data(
  regions: List(market_types.MarketRegionInput),
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

@internal
pub fn empty_market_conditions() -> CapturedJsonValue {
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

@internal
pub fn market_regions_connection(
  regions: List(market_types.MarketRegionInput),
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

@internal
pub fn market_region_node(
  region: market_types.MarketRegionInput,
) -> CapturedJsonValue {
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

@internal
pub fn market_currency_settings_data(
  input: Dict(String, root_field.ResolvedValue),
  regions: List(market_types.MarketRegionInput),
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

@internal
pub fn first_region_currency(
  regions: List(market_types.MarketRegionInput),
) -> Option(String) {
  case regions {
    [first, ..] -> Some(country_currency(first.country_code))
    [] -> None
  }
}

@internal
pub fn currency_payload(currency: String) -> CapturedJsonValue {
  CapturedObject([
    #("currencyCode", CapturedString(currency)),
    #("currencyName", CapturedString(currency_name(currency))),
    #("enabled", CapturedBool(True)),
  ])
}

@internal
pub fn default_market_price_inclusions() -> CapturedJsonValue {
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

@internal
pub fn country_currency(country_code: String) -> String {
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

@internal
pub fn country_name(country_code: String) -> String {
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

@internal
pub fn currency_name(currency: String) -> String {
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

@internal
pub fn catalog_create_input_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  title: String,
) -> List(CapturedJsonValue) {
  let title_errors = catalog_title_errors(title)
  case title_errors {
    [] -> {
      let status_errors = catalog_status_errors(input)
      case status_errors {
        [] -> {
          let context_errors = catalog_context_errors(store, input)
          case context_errors {
            [] -> catalog_relation_errors(store, input, None)
            _ -> context_errors
          }
        }
        _ -> status_errors
      }
    }
    _ -> title_errors
  }
}

@internal
pub fn catalog_update_input_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  catalog_id: String,
) -> List(CapturedJsonValue) {
  catalog_relation_errors(store, input, Some(catalog_id))
}

fn catalog_relation_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  current_catalog_id: Option(String),
) -> List(CapturedJsonValue) {
  let price_list_errors = case
    graphql_helpers.read_arg_string_nonempty(input, "priceListId")
  {
    Some(id) -> catalog_price_list_errors(store, id, current_catalog_id)
    None -> []
  }
  let publication_errors = case
    graphql_helpers.read_arg_string_nonempty(input, "publicationId")
  {
    Some(id) -> catalog_publication_errors(store, id, current_catalog_id)
    None -> []
  }
  list.append(price_list_errors, publication_errors)
}

fn catalog_price_list_errors(
  store: Store,
  price_list_id: String,
  current_catalog_id: Option(String),
) -> List(CapturedJsonValue) {
  case store.get_effective_price_list_by_id(store, price_list_id) {
    None -> [
      user_error(
        ["input", "priceListId"],
        "Price list not found.",
        "PRICE_LIST_NOT_FOUND",
      ),
    ]
    Some(_) ->
      case
        catalog_referencing_price_list(store, price_list_id, current_catalog_id)
      {
        Some(_) -> [
          user_error(
            ["input", "priceListId"],
            "Price list has already been taken",
            "TAKEN",
          ),
        ]
        None -> []
      }
  }
}

fn catalog_publication_errors(
  store: Store,
  publication_id: String,
  current_catalog_id: Option(String),
) -> List(CapturedJsonValue) {
  case store.get_effective_publication_by_id(store, publication_id) {
    None -> [
      user_error(
        ["input", "publicationId"],
        "Publication not found.",
        "PUBLICATION_NOT_FOUND",
      ),
    ]
    Some(publication) ->
      case publication.catalog_id {
        Some(catalog_id) ->
          case same_optional_catalog(current_catalog_id, catalog_id) {
            True -> []
            False -> [
              user_error(
                ["input", "publicationId"],
                "Publication is already attached to another catalog",
                "PUBLICATION_TAKEN",
              ),
            ]
          }
        None ->
          case
            catalog_referencing_publication(
              store,
              publication_id,
              current_catalog_id,
            )
          {
            Some(_) -> [
              user_error(
                ["input", "publicationId"],
                "Publication is already attached to another catalog",
                "PUBLICATION_TAKEN",
              ),
            ]
            None -> []
          }
      }
  }
}

fn same_optional_catalog(
  current_catalog_id: Option(String),
  catalog_id: String,
) {
  case current_catalog_id {
    Some(current) -> current == catalog_id
    None -> False
  }
}

fn catalog_referencing_price_list(
  store: Store,
  price_list_id: String,
  current_catalog_id: Option(String),
) -> Option(CatalogRecord) {
  store.list_effective_catalogs(store)
  |> list.find(fn(catalog) {
    !is_current_catalog(catalog.id, current_catalog_id)
    && catalog_price_list_id(catalog.data) == Some(price_list_id)
  })
  |> result_to_option
}

fn catalog_referencing_publication(
  store: Store,
  publication_id: String,
  current_catalog_id: Option(String),
) -> Option(CatalogRecord) {
  store.list_effective_catalogs(store)
  |> list.find(fn(catalog) {
    !is_current_catalog(catalog.id, current_catalog_id)
    && catalog_publication_id(catalog.data) == Some(publication_id)
  })
  |> result_to_option
}

fn is_current_catalog(id: String, current_catalog_id: Option(String)) -> Bool {
  case current_catalog_id {
    Some(current) -> current == id
    None -> False
  }
}

fn catalog_price_list_id(data: CapturedJsonValue) -> Option(String) {
  captured_field(data, "priceList")
  |> option.then(captured_string_field(_, "id"))
}

fn catalog_publication_id(data: CapturedJsonValue) -> Option(String) {
  captured_field(data, "publication")
  |> option.then(captured_string_field(_, "id"))
}

@internal
pub fn catalog_title_errors(title: String) -> List(CapturedJsonValue) {
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

@internal
pub fn catalog_status_errors(
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

@internal
pub fn catalog_context_errors(
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

@internal
pub fn catalog_context_object_errors(
  store: Store,
  context: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  case graphql_helpers.read_arg_string_nonempty(context, "driverType") {
    None -> catalog_context_object_errors_without_driver_type(store, context)
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

fn catalog_context_object_errors_without_driver_type(
  store: Store,
  context: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
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

@internal
pub fn unsupported_catalog_context_errors(
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

@internal
pub fn require_catalog_context_ids(
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

@internal
pub fn missing_market_context_errors(
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

@internal
pub fn missing_company_location_context_errors(
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

@internal
pub fn catalog_data(
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
  let markets = case graphql_helpers.read_arg_object(input, "context") {
    Some(context) ->
      read_arg_string_array(context, "marketIds")
      |> option.unwrap([])
      |> market_connection_from_ids(store, _)
    None ->
      captured_field(existing_value, "markets")
      |> option.unwrap(market_connection_from_ids(store, []))
  }
  captured_object_upsert(existing_value, [
    #("__typename", CapturedString("MarketCatalog")),
    #("id", CapturedString(id)),
    #("title", CapturedString(title)),
    #("status", CapturedString(status)),
    #("markets", markets),
    #(
      "operations",
      captured_field(existing_value, "operations")
        |> option.unwrap(CapturedArray([])),
    ),
    #("priceList", catalog_price_list_payload(store, input, existing_value)),
    #("publication", catalog_publication_payload(store, input, existing_value)),
  ])
}

fn catalog_price_list_payload(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  existing_value: CapturedJsonValue,
) -> CapturedJsonValue {
  case graphql_helpers.read_arg_string_nonempty(input, "priceListId") {
    Some(id) ->
      case store.get_effective_price_list_by_id(store, id) {
        Some(record) -> record.data
        None -> CapturedObject([#("id", CapturedString(id))])
      }
    None ->
      captured_field(existing_value, "priceList") |> option.unwrap(CapturedNull)
  }
}

fn catalog_publication_payload(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  existing_value: CapturedJsonValue,
) -> CapturedJsonValue {
  case graphql_helpers.read_arg_string_nonempty(input, "publicationId") {
    Some(id) ->
      case store.get_effective_publication_by_id(store, id) {
        Some(record) -> publication_record_payload(record)
        None -> CapturedObject([#("id", CapturedString(id))])
      }
    None ->
      captured_field(existing_value, "publication")
      |> option.unwrap(CapturedNull)
  }
}

fn publication_record_payload(record: PublicationRecord) -> CapturedJsonValue {
  CapturedObject([
    #("__typename", CapturedString("Publication")),
    #("id", CapturedString(record.id)),
    #("name", optional_captured_string(record.name)),
    #("autoPublish", optional_captured_bool(record.auto_publish)),
    #(
      "supportsFuturePublishing",
      optional_captured_bool(record.supports_future_publishing),
    ),
  ])
}

@internal
pub fn market_connection_from_ids(
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

@internal
pub fn market_node_for_id(store: Store, id: String) -> CapturedJsonValue {
  case store.get_effective_market_by_id(store, id) {
    Some(record) -> record.data
    None ->
      CapturedObject([
        #("__typename", CapturedString("Market")),
        #("id", CapturedString(id)),
      ])
  }
}

@internal
pub fn price_list_input_errors(
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

@internal
pub fn price_list_currency_errors(
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

@internal
pub fn price_list_parent_errors(
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

@internal
pub fn price_list_parent_adjustment_errors(
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

@internal
pub fn valid_currency(currency: String) -> Bool {
  list.contains(iso_currency_codes(), currency)
}

@internal
pub fn iso_currency_codes() -> List(String) {
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

@internal
pub fn price_list_data(
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

@internal
pub fn price_list_parent_data(
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

@internal
pub fn read_price_list_adjustment_value(
  adjustment: Dict(String, root_field.ResolvedValue),
) -> Option(CapturedJsonValue) {
  case dict.get(adjustment, "value") {
    Ok(root_field.IntVal(value)) -> Some(CapturedInt(value))
    Ok(root_field.FloatVal(value)) -> Some(CapturedFloat(value))
    _ -> None
  }
}

@internal
pub fn product_level_fixed_price_errors(
  store: Store,
  price_list: Option(PriceListRecord),
  price_inputs: List(Dict(String, root_field.ResolvedValue)),
  delete_product_ids: List(String),
) -> List(CapturedJsonValue) {
  let no_op_errors = case price_inputs, delete_product_ids {
    [], [] -> [
      price_list_fixed_prices_by_product_user_error_null_field(
        "No update operations are specified. `pricesToAdd` and `pricesToDeleteByProductIds` are empty.",
        "NO_UPDATE_OPERATIONS_SPECIFIED",
      ),
    ]
    _, _ -> []
  }
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
          Ok(price_list_fixed_prices_by_product_user_error(
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
          Ok(price_list_fixed_prices_by_product_user_error(
            ["pricesToDeleteByProductIds", int.to_string(index)],
            "Product "
              <> product_id
              <> " in `pricesToDeleteByProductIds` does not exist.",
            "PRODUCT_DOES_NOT_EXIST",
          ))
      }
    })
  no_op_errors
  |> list.append(add_errors)
  |> list.append(delete_errors)
  |> list.append(price_currency_mismatch_errors(price_list, price_inputs))
  |> list.append(duplicate_price_input_errors(price_inputs))
  |> list.append(duplicate_delete_product_errors(delete_product_ids))
  |> list.append(mutually_exclusive_product_errors(
    price_inputs,
    delete_product_ids,
  ))
  |> list.append(price_list_fixed_price_limit_errors(
    store,
    price_list,
    price_inputs,
    delete_product_ids,
  ))
}

fn price_currency_mismatch_errors(
  price_list: Option(PriceListRecord),
  price_inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(CapturedJsonValue) {
  case price_list {
    Some(existing) -> {
      let currency = price_list_currency(existing)
      price_inputs
      |> enumerate_dicts
      |> list.flat_map(fn(entry) {
        let #(input, index) = entry
        money_currency_mismatch_error(input, index, "price", currency)
        |> list.append(money_currency_mismatch_error(
          input,
          index,
          "compareAtPrice",
          currency,
        ))
      })
    }
    None -> []
  }
}

fn money_currency_mismatch_error(
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
  field_name: String,
  currency: String,
) -> List(CapturedJsonValue) {
  case
    graphql_helpers.read_arg_object(input, field_name)
    |> option.then(graphql_helpers.read_arg_string_nonempty(_, "currencyCode"))
  {
    Some(input_currency) ->
      case input_currency == currency {
        True -> []
        False -> [
          price_list_fixed_prices_by_product_user_error(
            ["pricesToAdd", int.to_string(index), field_name, "currencyCode"],
            "The currency specified in `pricesToAdd` for product ID "
              <> product_id_for_price_input(input)
              <> " does not match the price list's currency of "
              <> currency
              <> ".",
            "PRICES_TO_ADD_CURRENCY_MISMATCH",
          ),
        ]
      }
    None -> []
  }
}

fn duplicate_price_input_errors(
  price_inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(CapturedJsonValue) {
  let #(_, errors) =
    price_inputs
    |> list.fold(#([], []), fn(acc, input) {
      let #(seen, errors) = acc
      case graphql_helpers.read_arg_string_nonempty(input, "productId") {
        Some(product_id) ->
          case list.contains(seen, product_id) {
            True -> #(seen, [
              price_list_fixed_prices_by_product_user_error(
                ["pricesToAdd"],
                "Duplicate ID exists in `pricesToAdd`.",
                "DUPLICATE_ID_IN_INPUT",
              ),
              ..errors
            ])
            False -> #([product_id, ..seen], errors)
          }
        None -> #(seen, errors)
      }
    })
  errors |> list.reverse
}

fn duplicate_delete_product_errors(
  delete_product_ids: List(String),
) -> List(CapturedJsonValue) {
  let #(_, errors) =
    delete_product_ids
    |> list.fold(#([], []), fn(acc, product_id) {
      let #(seen, errors) = acc
      case list.contains(seen, product_id) {
        True -> #(seen, [
          price_list_fixed_prices_by_product_user_error(
            ["pricesToDeleteByProductIds"],
            "Duplicate ID exists in `pricesToDeleteByProductIds`.",
            "DUPLICATE_ID_IN_INPUT",
          ),
          ..errors
        ])
        False -> #([product_id, ..seen], errors)
      }
    })
  errors |> list.reverse
}

fn mutually_exclusive_product_errors(
  price_inputs: List(Dict(String, root_field.ResolvedValue)),
  delete_product_ids: List(String),
) -> List(CapturedJsonValue) {
  price_inputs
  |> enumerate_dicts
  |> list.filter_map(fn(entry) {
    let #(input, _) = entry
    case graphql_helpers.read_arg_string_nonempty(input, "productId") {
      Some(product_id) ->
        case list.contains(delete_product_ids, product_id) {
          True ->
            Ok(price_list_fixed_prices_by_product_user_error_null_field(
              "IDs specified in `pricesToAdd` and `pricesToDeleteByProductIds` must be mutually exclusive.",
              "ID_MUST_BE_MUTUALLY_EXCLUSIVE",
            ))
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

fn product_id_for_price_input(
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  graphql_helpers.read_arg_string_nonempty(input, "productId")
  |> option.unwrap("")
}

fn price_list_fixed_price_limit_errors(
  store: Store,
  price_list: Option(PriceListRecord),
  price_inputs: List(Dict(String, root_field.ResolvedValue)),
  delete_product_ids: List(String),
) -> List(CapturedJsonValue) {
  case price_list {
    Some(existing) ->
      case
        list.length(resulting_fixed_price_variant_ids(
          store,
          existing,
          price_inputs,
          delete_product_ids,
        ))
        >= price_list_fixed_prices_by_product_fixed_price_limit
      {
        True -> [
          price_list_fixed_prices_by_product_user_error(
            ["pricesToAdd"],
            "The maximum number of fixed prices allowed for the price list has been exceeded.",
            "PRICE_LIMIT_EXCEEDED",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn resulting_fixed_price_variant_ids(
  store: Store,
  price_list: PriceListRecord,
  price_inputs: List(Dict(String, root_field.ResolvedValue)),
  delete_product_ids: List(String),
) -> List(String) {
  let delete_variant_ids =
    delete_product_ids
    |> list.flat_map(fn(product_id) {
      store.get_effective_variants_by_product_id(store, product_id)
      |> list.map(fn(variant) { variant.id })
    })
  let retained =
    price_edges(price_list.data)
    |> list.filter_map(fixed_price_edge_variant_id_for_limit)
    |> list.filter(fn(variant_id) {
      !list.contains(delete_variant_ids, variant_id)
    })
  let add_variant_ids =
    price_inputs
    |> list.flat_map(fn(input) {
      case graphql_helpers.read_arg_string_nonempty(input, "productId") {
        Some(product_id) ->
          store.get_effective_variants_by_product_id(store, product_id)
          |> list.map(fn(variant) { variant.id })
        None -> []
      }
    })
  list.fold(add_variant_ids, retained, append_unique_string)
}

fn fixed_price_edge_variant_id_for_limit(
  edge: CapturedJsonValue,
) -> Result(String, Nil) {
  case captured_edge_node(edge) {
    Some(node) ->
      case captured_string_field(node, "originType") {
        Some("FIXED") -> fixed_price_edge_variant_id(edge) |> option_to_result
        _ -> Error(Nil)
      }
    None -> Error(Nil)
  }
}

fn append_unique_string(values: List(String), value: String) -> List(String) {
  case list.contains(values, value) {
    True -> values
    False -> list.append(values, [value])
  }
}

@internal
pub fn upsert_fixed_price_nodes(
  price_list: PriceListRecord,
  store: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> PriceListRecord {
  let existing_edges = price_edges(price_list.data)
  let input_variant_ids = mutation_variant_ids(inputs)
  let retained =
    existing_edges
    |> list.filter(fn(edge) {
      case price_edge_variant_id(edge) {
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
  rebuild_price_list_prices(price_list, list.append(new_edges, retained))
}

@internal
pub fn delete_fixed_price_nodes(
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

@internal
pub fn rebuild_price_list_prices(
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

@internal
pub fn price_edge_for_variant(
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

@internal
pub fn price_list_currency(price_list: PriceListRecord) -> String {
  captured_string_field(price_list.data, "currency") |> option.unwrap("USD")
}

@internal
pub fn product_payloads(
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

@internal
pub fn product_payload(product: ProductRecord) -> CapturedJsonValue {
  CapturedObject([
    #("__typename", CapturedString("Product")),
    #("id", CapturedString(product.id)),
    #("title", CapturedString(product.title)),
    #("handle", CapturedString(product.handle)),
    #("status", CapturedString(product.status)),
  ])
}

@internal
pub fn variant_payloads(
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

@internal
pub fn variant_payload(
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

@internal
pub fn quantity_pricing_input_errors(
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

@internal
pub fn quantity_rules_input_errors(
  store: Store,
  price_list: PriceListRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(CapturedJsonValue) {
  quantity_rules_input_errors_loop(
    store,
    price_list,
    enumerate_dicts(inputs),
    duplicate_quantity_rule_variant_ids(inputs),
  )
}

fn quantity_rules_input_errors_loop(
  store: Store,
  price_list: PriceListRecord,
  entries: List(#(Dict(String, root_field.ResolvedValue), Int)),
  duplicate_variant_ids: List(String),
) -> List(CapturedJsonValue) {
  case entries {
    [] -> []
    [entry, ..rest] -> {
      let #(input, index) = entry
      let variant_id =
        graphql_helpers.read_arg_string_nonempty(input, "variantId")
      let current_errors =
        list.append(
          quantity_rule_variant_errors(store, input, index),
          list.append(
            quantity_rule_numeric_errors(input, index),
            list.append(
              quantity_rule_price_break_errors(price_list, input, index),
              quantity_rule_duplicate_variant_errors(
                variant_id,
                index,
                duplicate_variant_ids,
              ),
            ),
          ),
        )
      list.append(
        current_errors,
        quantity_rules_input_errors_loop(
          store,
          price_list,
          rest,
          duplicate_variant_ids,
        ),
      )
    }
  }
}

fn quantity_rule_variant_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(CapturedJsonValue) {
  let variant_id =
    graphql_helpers.read_arg_string_nonempty(input, "variantId")
    |> option.unwrap("")
  case store.get_effective_variant_by_id(store, variant_id) {
    Some(_) -> []
    None -> [
      quantity_rule_user_error(
        ["quantityRules", int.to_string(index), "variantId"],
        "Product variant ID does not exist.",
        "PRODUCT_VARIANT_DOES_NOT_EXIST",
      ),
    ]
  }
}

fn quantity_rule_numeric_errors(
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(CapturedJsonValue) {
  let minimum = graphql_helpers.read_arg_int(input, "minimum")
  let maximum = graphql_helpers.read_arg_int(input, "maximum")
  let increment = graphql_helpers.read_arg_int(input, "increment")
  list.flatten([
    quantity_rule_minimum_bound_errors(minimum, index),
    quantity_rule_increment_bound_errors(increment, index),
    quantity_rule_increment_ceiling_errors(minimum, increment, index),
    quantity_rule_range_errors(minimum, maximum, index),
    quantity_rule_minimum_divisibility_errors(minimum, increment, index),
    quantity_rule_maximum_divisibility_errors(maximum, increment, index),
  ])
}

fn quantity_rule_minimum_bound_errors(
  minimum: Option(Int),
  index: Int,
) -> List(CapturedJsonValue) {
  case minimum {
    Some(value) ->
      case value < 1 {
        True -> [
          quantity_rule_user_error(
            ["quantityRules", int.to_string(index), "minimum"],
            "Minimum must be greater than or equal to one.",
            "GREATER_THAN_OR_EQUAL_TO",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn quantity_rule_increment_bound_errors(
  increment: Option(Int),
  index: Int,
) -> List(CapturedJsonValue) {
  case increment {
    Some(value) ->
      case value < 1 {
        True -> [
          quantity_rule_user_error(
            ["quantityRules", int.to_string(index), "increment"],
            "Increment must be greater than or equal to one.",
            "GREATER_THAN_OR_EQUAL_TO",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn quantity_rule_increment_ceiling_errors(
  minimum: Option(Int),
  increment: Option(Int),
  index: Int,
) -> List(CapturedJsonValue) {
  case minimum, increment {
    Some(minimum_value), Some(increment_value) ->
      case increment_value > minimum_value {
        True -> [
          quantity_rule_user_error(
            ["quantityRules", int.to_string(index), "increment"],
            "Increment must be lower than or equal to the minimum.",
            "INCREMENT_IS_GREATER_THAN_MINIMUM",
          ),
        ]
        False -> []
      }
    _, _ -> []
  }
}

fn quantity_rule_range_errors(
  minimum: Option(Int),
  maximum: Option(Int),
  index: Int,
) -> List(CapturedJsonValue) {
  case minimum, maximum {
    Some(minimum_value), Some(maximum_value) ->
      case minimum_value > maximum_value {
        True -> [
          quantity_rule_user_error(
            ["quantityRules", int.to_string(index), "minimum"],
            "Minimum must be lower than or equal to the maximum.",
            "MINIMUM_IS_GREATER_THAN_MAXIMUM",
          ),
        ]
        False -> []
      }
    _, _ -> []
  }
}

fn quantity_rule_minimum_divisibility_errors(
  minimum: Option(Int),
  increment: Option(Int),
  index: Int,
) -> List(CapturedJsonValue) {
  case minimum, increment {
    Some(minimum_value), Some(increment_value) ->
      case
        minimum_value >= 1
        && increment_value >= 1
        && minimum_value % increment_value != 0
      {
        True -> [
          quantity_rule_user_error(
            ["quantityRules", int.to_string(index), "minimum"],
            "Minimum must be a multiple of the increment.",
            "MINIMUM_NOT_MULTIPLE_OF_INCREMENT",
          ),
        ]
        False -> []
      }
    _, _ -> []
  }
}

fn quantity_rule_maximum_divisibility_errors(
  maximum: Option(Int),
  increment: Option(Int),
  index: Int,
) -> List(CapturedJsonValue) {
  case maximum, increment {
    Some(maximum_value), Some(increment_value) ->
      case increment_value >= 1 && maximum_value % increment_value != 0 {
        True -> [
          quantity_rule_user_error(
            ["quantityRules", int.to_string(index), "maximum"],
            "Maximum must be a multiple of the increment.",
            "MAXIMUM_NOT_MULTIPLE_OF_INCREMENT",
          ),
        ]
        False -> []
      }
    _, _ -> []
  }
}

fn quantity_rule_price_break_errors(
  price_list: PriceListRecord,
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(CapturedJsonValue) {
  let variant_id = graphql_helpers.read_arg_string_nonempty(input, "variantId")
  let maximum = graphql_helpers.read_arg_int(input, "maximum")
  case variant_id, maximum {
    Some(id), Some(maximum_value) -> {
      let break_minimums = quantity_price_break_minimums(price_list, id)
      case list.any(break_minimums, fn(value) { value > maximum_value }) {
        True -> [
          quantity_rule_user_error(
            ["quantityRules", int.to_string(index), "maximum"],
            "Maximum must be greater than or equal to all quantity price break minimums associated with this variant in the specified price list.",
            "MAXIMUM_IS_LOWER_THAN_QUANTITY_PRICE_BREAK_MINIMUM",
          ),
        ]
        False -> []
      }
    }
    _, _ -> []
  }
}

fn quantity_rule_duplicate_variant_errors(
  variant_id: Option(String),
  index: Int,
  duplicate_variant_ids: List(String),
) -> List(CapturedJsonValue) {
  case variant_id {
    Some(id) ->
      case list.contains(duplicate_variant_ids, id) {
        True -> [
          quantity_rule_user_error(
            ["quantityRules", int.to_string(index), "variantId"],
            "Quantity rule inputs must be unique by variant id.",
            "DUPLICATE_INPUT_FOR_VARIANT",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn duplicate_quantity_rule_variant_ids(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(String) {
  duplicate_quantity_rule_variant_ids_loop(inputs, [], [])
}

fn duplicate_quantity_rule_variant_ids_loop(
  inputs: List(Dict(String, root_field.ResolvedValue)),
  seen_variant_ids: List(String),
  duplicate_variant_ids: List(String),
) -> List(String) {
  case inputs {
    [] -> duplicate_variant_ids
    [input, ..rest] -> {
      let variant_id =
        graphql_helpers.read_arg_string_nonempty(input, "variantId")
      case variant_id {
        Some(id) ->
          case list.contains(seen_variant_ids, id) {
            True ->
              duplicate_quantity_rule_variant_ids_loop(
                rest,
                seen_variant_ids,
                append_unique_strings(duplicate_variant_ids, [id]),
              )
            False ->
              duplicate_quantity_rule_variant_ids_loop(
                rest,
                [id, ..seen_variant_ids],
                duplicate_variant_ids,
              )
          }
        None ->
          duplicate_quantity_rule_variant_ids_loop(
            rest,
            seen_variant_ids,
            duplicate_variant_ids,
          )
      }
    }
  }
}

@internal
pub fn quantity_rule_delete_errors(
  store: Store,
  price_list: PriceListRecord,
  variant_ids: List(String),
) -> List(CapturedJsonValue) {
  variant_ids
  |> enumerate_strings
  |> list.filter_map(fn(entry) {
    let #(variant_id, index) = entry
    case store.get_effective_variant_by_id(store, variant_id) {
      Some(_) ->
        case price_list_has_fixed_quantity_rule(price_list, variant_id) {
          True -> Error(Nil)
          False ->
            Ok(user_error(
              ["variantIds", int.to_string(index)],
              "Quantity rule for variant associated with the price list provided does not exist.",
              "VARIANT_QUANTITY_RULE_DOES_NOT_EXIST",
            ))
        }
      None ->
        Ok(user_error(
          ["variantIds", int.to_string(index)],
          "Product variant ID does not exist.",
          "PRODUCT_VARIANT_DOES_NOT_EXIST",
        ))
    }
  })
}

fn price_list_has_fixed_quantity_rule(
  price_list: PriceListRecord,
  variant_id: String,
) -> Bool {
  quantity_rule_edges(price_list.data)
  |> list.any(fn(edge) {
    quantity_rule_edge_variant_id(edge) == Some(variant_id)
    && case captured_edge_node(edge) {
      Some(node) -> captured_string_field(node, "originType") == Some("FIXED")
      None -> False
    }
  })
}

@internal
pub fn variant_not_found_errors(
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

@internal
pub fn upsert_quantity_rule_nodes(
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
        price_connection_from_edges(list.append(new_edges, retained)),
      ),
    ]),
  )
}

@internal
pub fn delete_quantity_rule_nodes(
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

@internal
pub fn quantity_rule_payloads(
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

@internal
pub fn quantity_rule_node(
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

@internal
pub fn upsert_quantity_price_break_nodes(
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

@internal
pub fn quantity_price_break_node(
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

@internal
pub fn rebuild_price_edge_with_breaks(
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

@internal
pub fn money_payload(
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

@internal
pub fn optional_money_payload(
  raw: Option(Dict(String, root_field.ResolvedValue)),
  currency: String,
) -> CapturedJsonValue {
  case raw {
    Some(_) -> money_payload(raw, currency)
    None -> CapturedNull
  }
}

@internal
pub fn format_money_amount(raw: String) -> String {
  case string.split(raw, ".") {
    [whole, fraction] -> whole <> "." <> trim_money_fraction(fraction)
    _ -> raw
  }
}

@internal
pub fn trim_money_fraction(fraction: String) -> String {
  case string.ends_with(fraction, "0") {
    True -> trim_money_fraction(string.drop_end(fraction, 1))
    False ->
      case fraction {
        "" -> "0"
        _ -> fraction
      }
  }
}

@internal
pub fn price_edges(value: CapturedJsonValue) -> List(CapturedJsonValue) {
  captured_connection_edges(captured_field(value, "prices"))
}

@internal
pub fn quantity_rule_edges(
  value: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  captured_connection_edges(captured_field(value, "quantityRules"))
}

@internal
pub fn captured_connection_edges(
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

@internal
pub fn fixed_price_edge_variant_id(edge: CapturedJsonValue) -> Option(String) {
  case captured_edge_node(edge) {
    Some(node) ->
      case captured_field(node, "variant") {
        Some(variant) -> captured_string_field(variant, "id")
        None -> None
      }
    None -> None
  }
}

fn price_edge_variant_id(edge: CapturedJsonValue) -> Option(String) {
  case captured_edge_node(edge) {
    Some(node) ->
      case captured_field(node, "variant") {
        Some(variant) -> captured_string_field(variant, "id")
        None -> None
      }
    None -> None
  }
}

@internal
pub fn quantity_rule_edge_variant_id(
  edge: CapturedJsonValue,
) -> Option(String) {
  case captured_edge_node(edge) {
    Some(node) ->
      case captured_field(node, "productVariant") {
        Some(variant) -> captured_string_field(variant, "id")
        None -> None
      }
    None -> None
  }
}

@internal
pub fn captured_edge_node(
  edge: CapturedJsonValue,
) -> Option(CapturedJsonValue) {
  captured_field(edge, "node")
}

fn quantity_price_break_minimums(
  price_list: PriceListRecord,
  variant_id: String,
) -> List(Int) {
  price_edges(price_list.data)
  |> list.filter(fn(edge) {
    fixed_price_edge_variant_id(edge) == Some(variant_id)
  })
  |> list.flat_map(fn(edge) {
    case captured_edge_node(edge) {
      Some(node) ->
        captured_field(node, "quantityPriceBreaks")
        |> captured_connection_edges
        |> list.filter_map(fn(break_edge) {
          use break_node <- result.try(
            captured_edge_node(break_edge) |> option_to_result,
          )
          captured_int_field(break_node, "minimumQuantity")
          |> option_to_result
        })
      None -> []
    }
  })
}

@internal
pub fn price_connection_from_edges(
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

@internal
pub fn page_info_for_cursors(cursors: List(String)) -> CapturedJsonValue {
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

@internal
pub fn empty_connection() -> CapturedJsonValue {
  price_connection_from_edges([])
}

@internal
pub fn empty_price_connection() -> CapturedJsonValue {
  price_connection_from_edges([])
}

@internal
pub fn market_localizable_resource_payload(
  store: Store,
  resource_id: String,
) -> CapturedJsonValue {
  case store.find_effective_metafield_by_id(store, resource_id) {
    Some(metafield) ->
      CapturedObject([
        #("resourceId", CapturedString(resource_id)),
        #(
          "marketLocalizableContent",
          CapturedArray(list.map(
            metafield.market_localizable_content,
            market_localizable_content_payload,
          )),
        ),
        #(
          "marketLocalizations",
          CapturedArray(
            store.list_effective_market_localizations(store, resource_id)
            |> list.map(fn(record) {
              market_localization_payload(store, record)
            }),
          ),
        ),
      ])
    None -> CapturedNull
  }
}

@internal
pub fn market_localizable_content_payload(
  content: MarketLocalizableContentRecord,
) -> CapturedJsonValue {
  CapturedObject([
    #("key", CapturedString(content.key)),
    #("value", CapturedString(content.value)),
    #("digest", CapturedString(content.digest)),
  ])
}

@internal
pub fn market_localization_payload(
  store: Store,
  record: MarketLocalizationRecord,
) -> CapturedJsonValue {
  CapturedObject([
    #("key", CapturedString(record.key)),
    #("value", CapturedString(record.value)),
    #("updatedAt", CapturedString(record.updated_at)),
    #("outdated", CapturedBool(record.outdated)),
    #("market", market_localization_market_payload(store, record.market_id)),
  ])
}

@internal
pub fn market_localization_market_payload(
  store: Store,
  market_id: String,
) -> CapturedJsonValue {
  case store.get_effective_market_by_id(store, market_id) {
    Some(record) -> record.data
    None -> CapturedNull
  }
}

@internal
pub fn captured_object_upsert(
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

@internal
pub fn replace_field(
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

@internal
pub fn string_array(values: List(String)) -> CapturedJsonValue {
  CapturedArray(list.map(values, CapturedString))
}

@internal
pub fn optional_captured_int(value: Option(Int)) -> CapturedJsonValue {
  case value {
    Some(i) -> CapturedInt(i)
    None -> CapturedNull
  }
}

@internal
pub fn optional_captured_bool(value: Option(Bool)) -> CapturedJsonValue {
  case value {
    Some(b) -> CapturedBool(b)
    None -> CapturedNull
  }
}

@internal
pub fn mutation_variant_ids(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(String) {
  inputs
  |> list.filter_map(fn(input) {
    graphql_helpers.read_arg_string_nonempty(input, "variantId")
    |> option_to_result
  })
}

@internal
pub fn append_unique_strings(
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

@internal
pub fn enumerate_dicts(
  items: List(Dict(String, root_field.ResolvedValue)),
) -> List(#(Dict(String, root_field.ResolvedValue), Int)) {
  enumerate_dicts_loop(items, 0)
}

@internal
pub fn enumerate_dicts_loop(
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

@internal
pub fn enumerate_strings(items: List(String)) -> List(#(String, Int)) {
  enumerate_strings_loop(items, 0)
}

@internal
pub fn enumerate_strings_loop(
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

@internal
pub fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(item) -> Ok(item)
    None -> Error(Nil)
  }
}

@internal
pub fn result_to_option(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(item) -> Some(item)
    Error(_) -> None
  }
}

@internal
pub fn root_payload_for_field(
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

@internal
pub fn serialize_record_by_id(
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

@internal
pub fn connection_item(
  cursor: Option(String),
  source: SourceValue,
) -> market_types.MarketConnectionItem {
  let fallback = case source_string_field(source, "id") {
    Some(id) -> id
    None -> "market-cursor"
  }
  let output = cursor |> option.unwrap(fallback)
  market_types.MarketConnectionItem(
    source: source,
    pagination_cursor: output,
    output_cursor: output,
  )
}

@internal
pub fn connection_config_for_field(
  field: Selection,
  items: List(market_types.MarketConnectionItem),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> SerializeConnectionConfig(market_types.MarketConnectionItem) {
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

@internal
pub fn market_page_info_options() -> ConnectionPageInfoOptions {
  ConnectionPageInfoOptions(
    include_inline_fragments: False,
    prefix_cursors: False,
    include_cursors: True,
    fallback_start_cursor: None,
    fallback_end_cursor: None,
  )
}

@internal
pub fn project_record(
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

@internal
pub fn market_record_source(record: MarketRecord) -> SourceValue {
  captured_json_source(record.data)
}

@internal
pub fn catalog_record_source(record: CatalogRecord) -> SourceValue {
  captured_json_source(record.data)
}

@internal
pub fn price_list_record_source(record: PriceListRecord) -> SourceValue {
  captured_json_source(record.data)
}

@internal
pub fn web_presence_record_source(record: WebPresenceRecord) -> SourceValue {
  captured_json_source(record.data)
}

@internal
pub fn captured_json_source(value: CapturedJsonValue) -> SourceValue {
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

@internal
pub fn source_string_field(
  source: SourceValue,
  name: String,
) -> Option(String) {
  case source {
    SrcObject(fields) ->
      case dict.get(fields, name) {
        Ok(SrcString(value)) -> Some(value)
        _ -> None
      }
    _ -> None
  }
}

@internal
pub fn serialize_exact_count(field: Selection, count: Int) -> Json {
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

@internal
pub fn read_arg_string_allow_empty(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
}

@internal
pub fn read_arg_object_array(
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

@internal
pub fn read_price_list_id(
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

@internal
pub fn read_arg_string_array(
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

@internal
pub fn optional_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(s) -> CapturedString(s)
    None -> CapturedNull
  }
}

@internal
pub fn captured_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) -> captured_field_from_pairs(fields, key)
    _ -> None
  }
}

@internal
pub fn captured_field_from_pairs(
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

@internal
pub fn captured_string_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(String) {
  case captured_field(value, key) {
    Some(CapturedString(s)) -> Some(s)
    _ -> None
  }
}

fn captured_int_field(value: CapturedJsonValue, key: String) -> Option(Int) {
  case captured_field(value, key) {
    Some(CapturedInt(i)) -> Some(i)
    _ -> None
  }
}
