//// Localization query handling, live-hybrid hydration, and snapshot projection.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{get_document_fragments}
import shopify_draft_proxy/proxy/localization/serializers
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, DraftProxy, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type LocaleRecord, type ShopLocaleRecord,
  type WebPresenceRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, LocaleRecord,
  ShopLocaleRecord, TranslationRecord, WebPresenceRecord,
}

@internal
pub fn is_localization_query_root(name: String) -> Bool {
  case name {
    "availableLocales" -> True
    "shopLocales" -> True
    "translatableResource" -> True
    "translatableResources" -> True
    "translatableResourcesByIds" -> True
    _ -> False
  }
}

@internal
pub fn handle_localization_query(
  store_in: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use fields <- result.try(root_field.get_root_fields(document))
  let fragments = get_document_fragments(document)
  Ok(serializers.serialize_root_fields(store_in, fields, fragments, variables))
}

@internal
pub fn process(
  store_in: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use data <- result.try(handle_localization_query(
    store_in,
    document,
    variables,
  ))
  Ok(graphql_helpers.wrap_data(data))
}

/// Pattern 2: cold LiveHybrid localization reads need the captured
/// upstream product/source-content slice before local translation
/// mutations can validate digests and stage read-after-write effects.
/// Once any localization/product state exists, stay local so staged
/// locale and translation changes are not bypassed by passthrough.
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
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "availableLocales" ->
      !has_local_localization_query_state(proxy)
    parse_operation.QueryOperation, "shopLocales" ->
      !has_local_localization_query_state(proxy)
    parse_operation.QueryOperation, "translatableResource" ->
      !has_local_localization_query_state(proxy)
    parse_operation.QueryOperation, "translatableResources" ->
      !has_local_localization_query_state(proxy)
    parse_operation.QueryOperation, "translatableResourcesByIds" ->
      !has_local_localization_query_state(proxy)
    _, _ -> False
  }
}

fn has_local_localization_query_state(proxy: DraftProxy) -> Bool {
  store.has_localization_state(proxy.store)
  || !list.is_empty(store.list_effective_products(proxy.store))
}

fn fetch_and_hydrate_live_hybrid_query(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let operation_name =
    parsed.name
    |> option.unwrap("LocalizationLiveHybridRead")
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
                  #("message", json.string(fetch_error_message(err))),
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
                  #(
                    "message",
                    json.string("Failed to handle localization query"),
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

fn hydrate_from_upstream_response(
  store_in: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) ->
      store_in
      |> hydrate_available_locales(data)
      |> hydrate_shop_locale_web_presences(data)
      |> hydrate_shop_locales(data)
      |> hydrate_translatable_resources(data)
    None -> store_in
  }
}

fn hydrate_available_locales(store_in: Store, data: commit.JsonValue) -> Store {
  let locales = case locale_records_from_field(data, "availableLocales") {
    [] -> locale_records_from_field(data, "availableLocalesExcerpt")
    records -> records
  }
  case locales {
    [] -> store_in
    _ -> store.replace_base_available_locales(store_in, locales)
  }
}

fn hydrate_shop_locales(store_in: Store, data: commit.JsonValue) -> Store {
  let locales = case shop_locale_records_from_field(data, "allShopLocales") {
    [] -> shop_locale_records_from_field(data, "shopLocales")
    records -> records
  }
  case locales {
    [] -> store_in
    _ -> store.upsert_base_shop_locales(store_in, locales)
  }
}

fn hydrate_shop_locale_web_presences(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let records =
    list.append(
      web_presence_records_from_shop_locale_field(data, "allShopLocales"),
      web_presence_records_from_shop_locale_field(data, "shopLocales"),
    )
  case records {
    [] -> store_in
    _ -> store.upsert_base_web_presences(store_in, records)
  }
}

fn hydrate_translatable_resources(
  store_in: Store,
  data: commit.JsonValue,
) -> Store {
  let resources =
    list.append(
      resources_from_connection_field(data, "resources"),
      resources_from_connection_field(data, "byIds"),
    )
  let resources = case json_get(data, "translatableResource") {
    Some(commit.JsonNull) | None -> resources
    Some(node) -> [node, ..resources]
  }
  list.fold(resources, store_in, hydrate_resource_source_markers)
}

fn hydrate_resource_source_markers(
  store_in: Store,
  resource: commit.JsonValue,
) -> Store {
  case json_get_string(resource, "resourceId") {
    None -> store_in
    Some(resource_id) -> {
      let content = case json_get(resource, "translatableContent") {
        Some(commit.JsonArray(items)) -> items
        _ -> []
      }
      list.fold(content, store_in, fn(acc, item) {
        case json_get_string(item, "key") {
          None -> acc
          Some(key) -> {
            let record =
              TranslationRecord(
                resource_id: resource_id,
                key: key,
                locale: "__source",
                value: option.unwrap(json_get_string(item, "value"), ""),
                translatable_content_digest: option.unwrap(
                  json_get_string(item, "digest"),
                  "",
                ),
                market_id: None,
                updated_at: "1970-01-01T00:00:00.000Z",
                outdated: False,
              )
            store.upsert_base_translation(acc, record)
          }
        }
      })
    }
  }
}

fn resources_from_connection_field(
  data: commit.JsonValue,
  key: String,
) -> List(commit.JsonValue) {
  case json_get(data, key) {
    Some(connection) ->
      case json_get(connection, "nodes") {
        Some(commit.JsonArray(items)) -> non_null_json_values(items)
        _ -> []
      }
    None -> []
  }
}

fn locale_records_from_field(
  data: commit.JsonValue,
  key: String,
) -> List(LocaleRecord) {
  case json_get(data, key) {
    Some(commit.JsonArray(items)) ->
      list.filter_map(items, locale_record_from_json)
    _ -> []
  }
}

fn locale_record_from_json(
  value: commit.JsonValue,
) -> Result(LocaleRecord, Nil) {
  case json_get_string(value, "isoCode"), json_get_string(value, "name") {
    Some(iso_code), Some(name) ->
      Ok(LocaleRecord(iso_code: iso_code, name: name))
    _, _ -> Error(Nil)
  }
}

fn shop_locale_records_from_field(
  data: commit.JsonValue,
  key: String,
) -> List(ShopLocaleRecord) {
  case json_get(data, key) {
    Some(commit.JsonArray(items)) ->
      list.filter_map(items, shop_locale_record_from_json)
    _ -> []
  }
}

fn shop_locale_record_from_json(
  value: commit.JsonValue,
) -> Result(ShopLocaleRecord, Nil) {
  case json_get_string(value, "locale") {
    Some(locale) ->
      Ok(ShopLocaleRecord(
        locale: locale,
        name: option.unwrap(json_get_string(value, "name"), locale),
        primary: option.unwrap(json_get_bool(value, "primary"), False),
        published: option.unwrap(json_get_bool(value, "published"), False),
        market_web_presence_ids: market_web_presence_ids_from_json(value),
      ))
    None -> Error(Nil)
  }
}

fn market_web_presence_ids_from_json(value: commit.JsonValue) -> List(String) {
  case json_get(value, "marketWebPresences") {
    Some(commit.JsonArray(items)) ->
      list.filter_map(items, fn(item) {
        case json_get_string(item, "id") {
          Some(id) -> Ok(id)
          None -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn web_presence_records_from_shop_locale_field(
  data: commit.JsonValue,
  key: String,
) -> List(WebPresenceRecord) {
  case json_get(data, key) {
    Some(commit.JsonArray(locales)) -> {
      let default_locale = primary_locale_from_shop_locale_items(locales)
      list.flat_map(locales, fn(locale) {
        web_presence_records_from_shop_locale(locale, default_locale)
      })
    }
    _ -> []
  }
}

fn web_presence_records_from_shop_locale(
  value: commit.JsonValue,
  default_locale: String,
) -> List(WebPresenceRecord) {
  case json_get(value, "marketWebPresences") {
    Some(commit.JsonArray(items)) ->
      list.filter_map(items, fn(item) {
        web_presence_record_from_json(item, default_locale)
      })
    _ -> []
  }
}

fn web_presence_record_from_json(
  value: commit.JsonValue,
  default_locale: String,
) -> Result(WebPresenceRecord, Nil) {
  case json_get_string(value, "id") {
    Some(id) ->
      Ok(WebPresenceRecord(
        id: id,
        cursor: None,
        data: captured_from_json_value(web_presence_json_with_defaults(
          value,
          default_locale,
        )),
      ))
    None -> Error(Nil)
  }
}

fn primary_locale_from_shop_locale_items(
  locales: List(commit.JsonValue),
) -> String {
  case
    list.find(locales, fn(value) {
      option.unwrap(json_get_bool(value, "primary"), False)
    })
  {
    Ok(value) -> json_get_string(value, "locale") |> option.unwrap("en")
    Error(_) -> "en"
  }
}

fn web_presence_json_with_defaults(
  value: commit.JsonValue,
  default_locale: String,
) -> commit.JsonValue {
  case value {
    commit.JsonObject(fields) -> {
      let fields =
        ensure_json_object_field(
          fields,
          "__typename",
          commit.JsonString("MarketWebPresence"),
        )
      let fields =
        ensure_json_object_field(
          fields,
          "defaultLocale",
          commit.JsonObject([#("locale", commit.JsonString(default_locale))]),
        )
      commit.JsonObject(fields)
    }
    _ -> value
  }
}

fn ensure_json_object_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
  value: commit.JsonValue,
) -> List(#(String, commit.JsonValue)) {
  case list.any(fields, fn(field) { field.0 == key }) {
    True -> fields
    False -> list.append(fields, [#(key, value)])
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

fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(b)) -> Some(b)
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
