//// Mirrors the localization slice of `src/proxy/localization.ts`.
////
//// Pass 23 ships:
//// - `availableLocales` and `shopLocales` reads, fully fed by the store
////   plus a hardcoded default catalog when the store hasn't been
////   hydrated.
//// - `translatableResource(s)` / `translatableResourcesByIds` reads.
////   Without a Products domain in the Gleam port the only resources
////   that can be discovered are translation/source-content entries
////   already staged in the store — the resource is reconstructed from
////   those records so captured register-then-read lifecycles can run
////   end-to-end.
//// - `shopLocaleEnable/Update/Disable` mutations, including the
////   "disabling clears translations for that locale" cleanup.
//// - `translationsRegister/Remove` mutations with the same validation
////   structure as the TS handler. With no Products in the store, unknown
////   gids still return `RESOURCE_NOT_FOUND`; captured resources can be
////   seeded with source-content markers until the Products domain ports.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, ConnectionWindow,
  SelectedFieldOptions, SerializeConnectionConfig,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  paginate_connection_items, serialize_connection, serialize_empty_connection,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome,
  read_optional_string_array, single_root_log_draft,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, DraftProxy, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type LocaleRecord, type ProductMetafieldRecord, type ProductRecord,
  type ShopLocaleRecord, type TranslationRecord, LocaleRecord, ShopLocaleRecord,
  TranslationRecord,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub type LocalizationError {
  ParseFailed(root_field.RootFieldError)
}

/// Validation user-error variant. Translation register/remove emits
/// errors with `code`; shopLocale lifecycle emits errors without one.
type AnyUserError {
  TranslationError(field: List(String), message: String, code: String)
  ShopLocaleError(field: List(String), message: String)
}

/// One translatable content slot on a translatable resource. `digest`
/// is `None` when no captured source digest is available.
@internal
pub type TranslatableContent {
  TranslatableContent(
    key: String,
    value: Option(String),
    digest: Option(String),
    locale: String,
    type_: String,
  )
}

@internal
pub type TranslatableResource {
  TranslatableResource(
    resource_id: String,
    resource_type: String,
    content: List(TranslatableContent),
  )
}

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

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

pub fn is_localization_mutation_root(name: String) -> Bool {
  case name {
    "shopLocaleEnable" -> True
    "shopLocaleUpdate" -> True
    "shopLocaleDisable" -> True
    "translationsRegister" -> True
    "translationsRemove" -> True
    _ -> False
  }
}

pub fn handle_localization_query(
  store_in: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, LocalizationError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store_in, fields, fragments, variables))
    }
  }
}

pub fn process(
  store_in: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, LocalizationError) {
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

pub fn process_mutation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store_in, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
        store_in,
        identity,
        fields,
        fragments,
        variables,
      )
    }
  }
}

// ---------------------------------------------------------------------------
// Default catalog
// ---------------------------------------------------------------------------

fn default_available_locales() -> List(LocaleRecord) {
  [
    LocaleRecord(iso_code: "en", name: "English"),
    LocaleRecord(iso_code: "fr", name: "French"),
    LocaleRecord(iso_code: "de", name: "German"),
    LocaleRecord(iso_code: "es", name: "Spanish"),
    LocaleRecord(iso_code: "it", name: "Italian"),
    LocaleRecord(iso_code: "pt-BR", name: "Portuguese (Brazil)"),
    LocaleRecord(iso_code: "ja", name: "Japanese"),
    LocaleRecord(iso_code: "zh-CN", name: "Chinese (Simplified)"),
  ]
}

fn available_locales(store_in: Store) -> List(LocaleRecord) {
  case store.list_effective_available_locales(store_in) {
    [] -> default_available_locales()
    stored -> stored
  }
}

fn locale_name(store_in: Store, locale: String) -> Option(String) {
  available_locales(store_in)
  |> list.find(fn(candidate) { candidate.iso_code == locale })
  |> option.from_result
  |> option.map(fn(record) { record.name })
}

fn default_shop_locales(store_in: Store) -> List(ShopLocaleRecord) {
  let name = case locale_name(store_in, "en") {
    Some(value) -> value
    None -> "English"
  }
  [
    ShopLocaleRecord(
      locale: "en",
      name: name,
      primary: True,
      published: True,
      market_web_presence_ids: [],
    ),
  ]
}

fn list_shop_locales(
  store_in: Store,
  published: Option(Bool),
) -> List(ShopLocaleRecord) {
  case store.list_effective_shop_locales(store_in, published) {
    [] ->
      default_shop_locales(store_in)
      |> list.filter(fn(locale) {
        case published {
          Some(target) -> locale.published == target
          None -> True
        }
      })
    stored -> stored
  }
}

fn get_shop_locale(
  store_in: Store,
  locale: String,
) -> Option(ShopLocaleRecord) {
  case store.get_effective_shop_locale(store_in, locale) {
    Some(record) -> Some(record)
    None ->
      default_shop_locales(store_in)
      |> list.find(fn(candidate) { candidate.locale == locale })
      |> option.from_result
  }
}

// ---------------------------------------------------------------------------
// Resource reconstruction
// ---------------------------------------------------------------------------

/// Find a translatable resource by id from effective Product and
/// product Metafield state, mirroring the TypeScript localization
/// runtime.
fn find_resource(
  store_in: Store,
  resource_id: String,
) -> Option(TranslatableResource) {
  case store.get_effective_product_by_id(store_in, resource_id) {
    Some(product) ->
      Some(TranslatableResource(
        resource_id: product.id,
        resource_type: "PRODUCT",
        content: product_content(product),
      ))
    None ->
      case
        list.find(list_product_metafields(store_in), fn(metafield) {
          metafield.id == resource_id
        })
      {
        Ok(metafield) ->
          Some(TranslatableResource(
            resource_id: metafield.id,
            resource_type: "METAFIELD",
            content: metafield_content(metafield),
          ))
        Error(_) -> None
      }
  }
}

/// Enumerate every translatable resource of a given type from effective
/// Product/Product Metafield state plus capture-backed source markers.
fn list_resources(
  store_in: Store,
  resource_type: Option(String),
) -> List(TranslatableResource) {
  let product_resources = case resource_matches(resource_type, "PRODUCT") {
    True ->
      store.list_effective_products(store_in)
      |> list.map(fn(product) {
        TranslatableResource(
          resource_id: product.id,
          resource_type: "PRODUCT",
          content: product_content(product),
        )
      })
    False -> []
  }
  let metafield_resources = case resource_matches(resource_type, "METAFIELD") {
    True ->
      list_product_metafields(store_in)
      |> list.map(fn(metafield) {
        TranslatableResource(
          resource_id: metafield.id,
          resource_type: "METAFIELD",
          content: metafield_content(metafield),
        )
      })
    False -> []
  }
  let seeded_resources =
    source_marker_resources(store_in)
    |> list.filter(fn(resource) {
      resource_matches(resource_type, resource.resource_type)
    })
  list.append(product_resources, metafield_resources)
  |> list.append(seeded_resources)
  |> dedupe_resources([])
  |> list.sort(fn(left, right) {
    string.compare(left.resource_id, right.resource_id)
  })
}

fn resource_matches(filter: Option(String), resource_type: String) -> Bool {
  case filter {
    Some(target) -> target == resource_type
    None -> False
  }
}

fn list_product_metafields(store_in: Store) -> List(ProductMetafieldRecord) {
  store.list_effective_products(store_in)
  |> list.flat_map(fn(product) {
    store.get_effective_metafields_by_owner_id(store_in, product.id)
  })
  |> dedupe_metafields([])
  |> list.sort(fn(left, right) { string.compare(left.id, right.id) })
}

fn dedupe_metafields(
  metafields: List(ProductMetafieldRecord),
  seen: List(String),
) -> List(ProductMetafieldRecord) {
  case metafields {
    [] -> []
    [metafield, ..rest] ->
      case list.contains(seen, metafield.id) {
        True -> dedupe_metafields(rest, seen)
        False -> [metafield, ..dedupe_metafields(rest, [metafield.id, ..seen])]
      }
  }
}

fn product_content(product: ProductRecord) -> List(TranslatableContent) {
  let base = [
    TranslatableContent(
      key: "title",
      value: Some(product.title),
      digest: Some(crypto.sha256_hex(product.title)),
      locale: "en",
      type_: "SINGLE_LINE_TEXT_FIELD",
    ),
    TranslatableContent(
      key: "handle",
      value: Some(product.handle),
      digest: Some(crypto.sha256_hex(product.handle)),
      locale: "en",
      type_: "URI",
    ),
  ]
  let with_body = case product.description_html {
    "" -> base
    value ->
      list.append(base, [
        TranslatableContent(
          key: "body_html",
          value: Some(value),
          digest: Some(crypto.sha256_hex(value)),
          locale: "en",
          type_: "HTML",
        ),
      ])
  }
  let with_type = case product.product_type {
    Some(value) ->
      list.append(with_body, [
        TranslatableContent(
          key: "product_type",
          value: Some(value),
          digest: Some(crypto.sha256_hex(value)),
          locale: "en",
          type_: "SINGLE_LINE_TEXT_FIELD",
        ),
      ])
    None -> with_body
  }
  let with_seo_title = case product.seo.title {
    Some(value) ->
      list.append(with_type, [
        TranslatableContent(
          key: "meta_title",
          value: Some(value),
          digest: Some(crypto.sha256_hex(value)),
          locale: "en",
          type_: "SINGLE_LINE_TEXT_FIELD",
        ),
      ])
    None -> with_type
  }
  case product.seo.description {
    Some(value) ->
      list.append(with_seo_title, [
        TranslatableContent(
          key: "meta_description",
          value: Some(value),
          digest: Some(crypto.sha256_hex(value)),
          locale: "en",
          type_: "MULTI_LINE_TEXT_FIELD",
        ),
      ])
    None -> with_seo_title
  }
}

fn metafield_content(
  metafield: ProductMetafieldRecord,
) -> List(TranslatableContent) {
  [
    TranslatableContent(
      key: "value",
      value: metafield.value,
      digest: case metafield.compare_digest, metafield.value {
        Some(digest), _ -> Some(digest)
        None, Some(value) -> Some(crypto.sha256_hex(value))
        None, None -> None
      },
      locale: "en",
      type_: localizable_content_type_for_metafield(metafield.type_),
    ),
  ]
}

fn localizable_content_type_for_metafield(type_: Option(String)) -> String {
  case type_ {
    Some("multi_line_text_field") -> "MULTI_LINE_TEXT_FIELD"
    Some("rich_text_field") -> "RICH_TEXT_FIELD"
    Some("url") -> "URL"
    Some("json") -> "JSON"
    _ -> "SINGLE_LINE_TEXT_FIELD"
  }
}

fn source_marker_resources(store_in: Store) -> List(TranslatableResource) {
  let source_markers =
    list.append(
      dict.values(store_in.base_state.translations),
      dict.values(store_in.staged_state.translations),
    )
    |> list.filter(fn(t) { t.locale == "__source" })
  let resource_ids =
    source_markers
    |> list.map(fn(t) { t.resource_id })
    |> dedupe_strings([])
  list.filter_map(resource_ids, fn(resource_id) {
    let content =
      source_markers
      |> list.filter(fn(t) { t.resource_id == resource_id })
      |> list.map(fn(t) {
        content_from_translation(t, t.translatable_content_digest)
      })
      |> dedupe_content_by_key([])
      |> list.sort(compare_content)
    case content {
      [] -> Error(Nil)
      _ ->
        Ok(TranslatableResource(
          resource_id: resource_id,
          resource_type: synthetic_resource_type(resource_id),
          content: content,
        ))
    }
  })
}

fn dedupe_strings(values: List(String), seen: List(String)) -> List(String) {
  case values {
    [] -> []
    [value, ..rest] ->
      case list.contains(seen, value) {
        True -> dedupe_strings(rest, seen)
        False -> [value, ..dedupe_strings(rest, [value, ..seen])]
      }
  }
}

fn dedupe_resources(
  resources: List(TranslatableResource),
  seen: List(String),
) -> List(TranslatableResource) {
  case resources {
    [] -> []
    [resource, ..rest] ->
      case list.contains(seen, resource.resource_id) {
        True -> dedupe_resources(rest, seen)
        False -> [
          resource,
          ..dedupe_resources(rest, [resource.resource_id, ..seen])
        ]
      }
  }
}

// ---------------------------------------------------------------------------
// Read-path serialization
// ---------------------------------------------------------------------------

fn serialize_root_fields(
  store_in: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let payload =
        root_payload_for_field(store_in, field, fragments, variables)
      #(key, payload)
    })
  json.object(entries)
}

fn root_payload_for_field(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "availableLocales" -> serialize_available_locales(store_in, field)
        "shopLocales" ->
          serialize_shop_locales_root(store_in, field, fragments, variables)
        "translatableResource" ->
          serialize_translatable_resource_root(
            store_in,
            field,
            fragments,
            variables,
          )
        "translatableResources" ->
          serialize_translatable_resources_root(
            store_in,
            field,
            fragments,
            variables,
          )
        "translatableResourcesByIds" ->
          serialize_translatable_resources_by_ids_root(
            store_in,
            field,
            fragments,
            variables,
          )
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn selections_of(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

// availableLocales
fn serialize_available_locales(store_in: Store, field: Selection) -> Json {
  let locales = available_locales(store_in)
  json.array(locales, fn(locale) { serialize_locale(locale, field) })
}

fn serialize_locale(locale: LocaleRecord, field: Selection) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "isoCode" -> #(key, json.string(locale.iso_code))
            "name" -> #(key, json.string(locale.name))
            "__typename" -> #(key, json.string("Locale"))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

// shopLocales
fn serialize_shop_locales_root(
  store_in: Store,
  field: Selection,
  _fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let published = graphql_helpers.read_arg_bool(args, "published")
  let locales = list_shop_locales(store_in, published)
  json.array(locales, fn(locale) { serialize_shop_locale(locale, field) })
}

fn serialize_shop_locale(locale: ShopLocaleRecord, field: Selection) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "locale" -> #(key, json.string(locale.locale))
            "name" -> #(key, json.string(locale.name))
            "primary" -> #(key, json.bool(locale.primary))
            "published" -> #(key, json.bool(locale.published))
            "__typename" -> #(key, json.string("ShopLocale"))
            "marketWebPresences" -> #(key, json.preprocessed_array([]))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

// translatableResource
fn serialize_translatable_resource_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "resourceId") {
    Some(resource_id) -> {
      let resource = find_resource_or_synthesize(store_in, resource_id)
      serialize_resource(store_in, resource, field, fragments, variables)
    }
    None -> json.null()
  }
}

/// Synthesize a translatable resource from in-store translation/source
/// markers even when the underlying Product/Metafield record isn't
/// available. Parity seeding can provide a captured source digest as a
/// non-target-locale marker; ordinary unknown ids still return `None`.
fn find_resource_or_synthesize(
  store_in: Store,
  resource_id: String,
) -> Option(TranslatableResource) {
  case find_resource(store_in, resource_id) {
    Some(record) -> Some(record)
    None -> {
      let content = synthesized_content_from_translations(store_in, resource_id)
      case list.is_empty(content) {
        False ->
          Some(TranslatableResource(
            resource_id: resource_id,
            resource_type: synthetic_resource_type(resource_id),
            content: content,
          ))
        True -> None
      }
    }
  }
}

fn synthesized_content_from_translations(
  store_in: Store,
  resource_id: String,
) -> List(TranslatableContent) {
  let translations =
    list.append(
      dict.values(store_in.base_state.translations),
      dict.values(store_in.staged_state.translations),
    )
    |> list.filter(fn(t) { t.resource_id == resource_id })
  translations
  |> list.map(fn(translation) {
    content_from_translation(
      translation,
      translation.translatable_content_digest,
    )
  })
  |> dedupe_content_by_key([])
  |> list.sort(compare_content)
}

fn compare_content(
  left: TranslatableContent,
  right: TranslatableContent,
) -> order.Order {
  case int.compare(content_order(left.key), content_order(right.key)) {
    order.Eq -> string.compare(left.key, right.key)
    other -> other
  }
}

fn content_order(key: String) -> Int {
  case key {
    "title" -> 0
    "handle" -> 1
    "body_html" -> 2
    "product_type" -> 3
    "meta_title" -> 4
    "meta_description" -> 5
    "value" -> 6
    _ -> 100
  }
}

fn dedupe_content_by_key(
  content: List(TranslatableContent),
  seen: List(String),
) -> List(TranslatableContent) {
  case content {
    [] -> []
    [entry, ..rest] ->
      case list.contains(seen, entry.key) {
        True -> dedupe_content_by_key(rest, seen)
        False -> [entry, ..dedupe_content_by_key(rest, [entry.key, ..seen])]
      }
  }
}

fn content_from_translation(
  translation: TranslationRecord,
  digest: String,
) -> TranslatableContent {
  let source_value = case translation.locale, translation.value {
    "__source", "" -> None
    "__source", value -> Some(value)
    _, _ -> None
  }
  TranslatableContent(
    key: translation.key,
    value: source_value,
    digest: case digest {
      "" -> None
      value -> Some(value)
    },
    locale: "en",
    type_: content_type_for_key(translation.key),
  )
}

fn content_type_for_key(key: String) -> String {
  case key {
    "body_html" -> "HTML"
    "handle" -> "URI"
    "meta_description" -> "MULTI_LINE_TEXT_FIELD"
    _ -> "SINGLE_LINE_TEXT_FIELD"
  }
}

fn resource_exists_for_validation(
  store_in: Store,
  resource_id: String,
) -> Option(TranslatableResource) {
  case find_resource(store_in, resource_id) {
    Some(resource) -> Some(resource)
    None -> find_resource_or_synthesize(store_in, resource_id)
  }
}

fn synthetic_resource_type(resource_id: String) -> String {
  case string.starts_with(resource_id, "gid://shopify/Metafield/") {
    True -> "METAFIELD"
    False -> "PRODUCT"
  }
}

fn serialize_resource(
  store_in: Store,
  resource: Option(TranslatableResource),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case resource {
    None -> json.null()
    Some(record) -> {
      let entries =
        list.map(selections_of(field), fn(selection) {
          let key = get_field_response_key(selection)
          case selection {
            Field(name: name, ..) ->
              case name.value {
                "resourceId" -> #(key, json.string(record.resource_id))
                "__typename" -> #(key, json.string("TranslatableResource"))
                "translatableContent" -> #(
                  key,
                  serialize_content(record.content, selection),
                )
                "translations" -> #(
                  key,
                  serialize_translations_for_resource(
                    store_in,
                    record,
                    selection,
                    fragments,
                    variables,
                  ),
                )
                "nestedTranslatableResources" -> #(
                  key,
                  serialize_empty_connection(
                    selection,
                    default_selected_field_options(),
                  ),
                )
                _ -> #(key, json.null())
              }
            _ -> #(key, json.null())
          }
        })
      json.object(entries)
    }
  }
}

fn serialize_content(
  content: List(TranslatableContent),
  field: Selection,
) -> Json {
  let inner = selections_of(field)
  json.array(content, fn(entry) {
    let entries =
      list.map(inner, fn(selection) {
        let key = get_field_response_key(selection)
        case selection {
          Field(name: name, ..) ->
            case name.value {
              "key" -> #(key, json.string(entry.key))
              "value" -> #(key, optional_string_json(entry.value))
              "digest" -> #(key, optional_string_json(entry.digest))
              "locale" -> #(key, json.string(entry.locale))
              "type" -> #(key, json.string(entry.type_))
              "__typename" -> #(key, json.string("TranslatableContent"))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      })
    json.object(entries)
  })
}

fn serialize_translations_for_resource(
  store_in: Store,
  resource: TranslatableResource,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let locale = graphql_helpers.read_arg_string(args, "locale")
  let market_id = graphql_helpers.read_arg_string(args, "marketId")
  let outdated = graphql_helpers.read_arg_bool(args, "outdated")
  let translations = case locale {
    Some(loc) ->
      store.list_effective_translations(
        store_in,
        resource.resource_id,
        loc,
        market_id,
      )
    None -> []
  }
  let filtered = case outdated {
    Some(target) -> list.filter(translations, fn(t) { t.outdated == target })
    None -> translations
  }
  json.array(filtered, fn(t) {
    serialize_translation(store_in, t, field, fragments)
  })
}

fn serialize_translation(
  _store_in: Store,
  translation: TranslationRecord,
  field: Selection,
  _fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "key" -> #(key, json.string(translation.key))
            "value" -> #(key, json.string(translation.value))
            "locale" -> #(key, json.string(translation.locale))
            "outdated" -> #(key, json.bool(translation.outdated))
            "updatedAt" -> #(key, json.string(translation.updated_at))
            "__typename" -> #(key, json.string("Translation"))
            "market" -> #(key, json.null())
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_translatable_resources_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let resource_type = graphql_helpers.read_arg_string(args, "resourceType")
  let reverse = graphql_helpers.read_arg_bool(args, "reverse")
  let resources = list_resources(store_in, resource_type)
  let resources = case reverse {
    Some(True) -> list.reverse(resources)
    _ -> resources
  }
  serialize_resource_connection(
    store_in,
    resources,
    field,
    fragments,
    variables,
  )
}

fn serialize_translatable_resources_by_ids_root(
  store_in: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let ids = case read_optional_string_array(args, "resourceIds") {
    Some(ids) -> ids
    None -> []
  }
  let resources =
    list.filter_map(ids, fn(id) {
      case find_resource_or_synthesize(store_in, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let reverse = graphql_helpers.read_arg_bool(args, "reverse")
  let resources = case reverse {
    Some(True) -> list.reverse(resources)
    _ -> resources
  }
  serialize_resource_connection(
    store_in,
    resources,
    field,
    fragments,
    variables,
  )
}

fn serialize_resource_connection(
  store_in: Store,
  resources: List(TranslatableResource),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let cursor_value = fn(resource: TranslatableResource, _index: Int) {
    resource.resource_id
  }
  let window =
    paginate_connection_items(
      resources,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: paged,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let selected_field_options =
    SelectedFieldOptions(include_inline_fragments: True)
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: True,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: paged,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: cursor_value,
      serialize_node: fn(resource, node_field, _index) {
        serialize_resource(
          store_in,
          Some(resource),
          node_field,
          fragments,
          variables,
        )
      },
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
  )
}

// ---------------------------------------------------------------------------
// Mutation handlers
// ---------------------------------------------------------------------------

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

fn handle_mutation_fields(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store_in, identity, [], [])
  let #(data_entries, final_store, final_identity, all_staged, all_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, drafts) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "shopLocaleEnable" ->
              Some(handle_shop_locale_enable(
                current_store,
                current_identity,
                field,
                variables,
              ))
            "shopLocaleUpdate" ->
              Some(handle_shop_locale_update(
                current_store,
                current_identity,
                field,
                variables,
              ))
            "shopLocaleDisable" ->
              Some(handle_shop_locale_disable(
                current_store,
                current_identity,
                field,
                variables,
              ))
            "translationsRegister" ->
              Some(handle_translations_register(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "translationsRemove" ->
              Some(handle_translations_remove(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> {
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  localization_status_for(
                    name.value,
                    result.staged_resource_ids,
                  ),
                  "localization",
                  "stage-locally",
                  Some(localization_notes_for(name.value)),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                next_store,
                next_identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
          }
        }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: all_drafts,
  )
}

/// Per-root-field log status for localization mutations. Default
/// rule: an empty `staged_resource_ids` means the validation path
/// rejected the request, so the entry logs `Failed`; otherwise
/// `Staged`.
fn localization_status_for(
  _root_field_name: String,
  staged_resource_ids: List(String),
) -> store.EntryStatus {
  case staged_resource_ids {
    [] -> store.Failed
    [_, ..] -> store.Staged
  }
}

/// Notes string mirroring the `localization` dispatcher in
/// `routes.ts`.
fn localization_notes_for(_root_field_name: String) -> String {
  "Staged locally in the in-memory localization draft store."
}

// shopLocaleEnable
fn handle_shop_locale_enable(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let locale = case graphql_helpers.read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  case locale_name(store_in, locale) {
    None -> {
      let payload =
        project_shop_locale_payload(
          field,
          None,
          None,
          [invalid_locale_error()],
          "ShopLocaleEnablePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
    Some(name) -> {
      let market_web_presence_ids = case
        read_optional_string_array(args, "marketWebPresenceIds")
      {
        Some(ids) -> ids
        None -> []
      }
      let existing = get_shop_locale(store_in, locale)
      let existing_primary = case existing {
        Some(record) -> record.primary
        None -> False
      }
      let existing_published = case existing {
        Some(record) -> record.published
        None -> False
      }
      let record =
        ShopLocaleRecord(
          locale: locale,
          name: name,
          primary: existing_primary,
          published: existing_published,
          market_web_presence_ids: market_web_presence_ids,
        )
      let #(_, store_after) = store.stage_shop_locale(store_in, record)
      let payload =
        project_shop_locale_payload(
          field,
          Some(record),
          None,
          [],
          "ShopLocaleEnablePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          shop_locale_staged_id(record),
        ]),
        store_after,
        identity,
      )
    }
  }
}

// shopLocaleUpdate
fn handle_shop_locale_update(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let locale = case graphql_helpers.read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  let existing = get_shop_locale(store_in, locale)
  let has_locale_name = case locale_name(store_in, locale) {
    Some(_) -> True
    None -> False
  }
  case existing, has_locale_name {
    Some(current), True -> {
      let input = read_input_object(args, "shopLocale")
      let market_web_presence_ids = case
        read_optional_string_array(input, "marketWebPresenceIds")
      {
        Some(ids) -> ids
        None -> current.market_web_presence_ids
      }
      let published = case graphql_helpers.read_arg_bool(input, "published") {
        Some(b) -> b
        None -> current.published
      }
      let record =
        ShopLocaleRecord(
          locale: current.locale,
          name: current.name,
          primary: current.primary,
          published: published,
          market_web_presence_ids: market_web_presence_ids,
        )
      let #(_, store_after) = store.stage_shop_locale(store_in, record)
      let payload =
        project_shop_locale_payload(
          field,
          Some(record),
          None,
          [],
          "ShopLocaleUpdatePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          shop_locale_staged_id(record),
        ]),
        store_after,
        identity,
      )
    }
    _, _ -> {
      let payload =
        project_shop_locale_payload(
          field,
          None,
          None,
          [invalid_locale_error()],
          "ShopLocaleUpdatePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
  }
}

// shopLocaleDisable
fn handle_shop_locale_disable(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let locale = case graphql_helpers.read_arg_string(args, "locale") {
    Some(s) -> s
    None -> ""
  }
  let existing = get_shop_locale(store_in, locale)
  let has_locale_name = case locale_name(store_in, locale) {
    Some(_) -> True
    None -> False
  }
  let primary = case existing {
    Some(record) -> record.primary
    None -> False
  }
  case existing, has_locale_name, primary {
    Some(_), True, False -> {
      let #(_, store_after_disable) =
        store.disable_shop_locale(store_in, locale)
      let #(_, store_after) =
        store.remove_translations_for_locale(store_after_disable, locale)
      let payload =
        project_shop_locale_payload(
          field,
          None,
          Some(locale),
          [],
          "ShopLocaleDisablePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_after,
        identity,
      )
    }
    _, _, _ -> {
      let payload =
        project_shop_locale_payload(
          field,
          None,
          Some(locale),
          [invalid_locale_error()],
          "ShopLocaleDisablePayload",
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store_in,
        identity,
      )
    }
  }
}

fn invalid_locale_error() -> AnyUserError {
  ShopLocaleError(field: ["locale"], message: "Locale is invalid")
}

fn shop_locale_staged_id(record: ShopLocaleRecord) -> String {
  "ShopLocale/" <> record.locale
}

fn read_input_object(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(args, key) {
    Ok(root_field.ObjectVal(d)) -> d
    _ -> dict.new()
  }
}

fn project_shop_locale_payload(
  field: Selection,
  shop_locale: Option(ShopLocaleRecord),
  locale: Option(String),
  errors: List(AnyUserError),
  _typename: String,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "shopLocale" -> #(key, case shop_locale {
              Some(record) -> serialize_shop_locale(record, selection)
              None -> json.null()
            })
            "locale" -> #(key, case locale {
              Some(s) -> json.string(s)
              None -> json.null()
            })
            "userErrors" -> #(key, serialize_user_errors(errors, selection))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn serialize_user_errors(
  errors: List(AnyUserError),
  selection: Selection,
) -> Json {
  let inner = selections_of(selection)
  json.array(errors, fn(error) {
    let entries =
      list.map(inner, fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value, error {
              "field", TranslationError(field: parts, ..) -> #(
                key,
                json.array(parts, json.string),
              )
              "field", ShopLocaleError(field: parts, ..) -> #(
                key,
                json.array(parts, json.string),
              )
              "message", TranslationError(message: msg, ..) -> #(
                key,
                json.string(msg),
              )
              "message", ShopLocaleError(message: msg, ..) -> #(
                key,
                json.string(msg),
              )
              "code", TranslationError(code: code, ..) -> #(
                key,
                json.string(code),
              )
              "code", ShopLocaleError(..) -> #(key, json.null())
              _, _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      })
    json.object(entries)
  })
}

// translationsRegister
fn handle_translations_register(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let resource_validation = validate_resource(store_in, args)
  let initial_errors = resource_validation.1
  let inputs = read_translation_inputs(args)
  let blank_errors = case inputs {
    [] -> [
      TranslationError(
        field: ["translations"],
        message: "At least one translation is required",
        code: "BLANK",
      ),
    ]
    _ -> []
  }
  let errors = list.append(initial_errors, blank_errors)

  let #(translations, errors, identity_after) = case resource_validation.0 {
    Some(resource) ->
      validate_and_build_translations(
        store_in,
        identity,
        resource,
        inputs,
        errors,
      )
    None -> #([], errors, identity)
  }

  let #(store_after, staged_ids) = case errors {
    [] -> {
      let store_after =
        list.fold(translations, store_in, fn(acc, t) {
          let #(_, next) = store.stage_translation(acc, t)
          next
        })
      let ids =
        list.map(translations, fn(t) {
          store.translation_storage_key(
            t.resource_id,
            t.locale,
            t.key,
            t.market_id,
          )
        })
      #(store_after, ids)
    }
    _ -> #(store_in, [])
  }

  let translations_for_payload = case errors {
    [] -> Some(translations)
    _ -> None
  }
  let payload =
    project_translations_payload(
      store_after,
      translations_for_payload,
      errors,
      field,
      fragments,
    )
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: staged_ids,
    ),
    store_after,
    identity_after,
  )
}

// translationsRemove
fn handle_translations_remove(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let resource_validation = validate_resource(store_in, args)
  let resource = resource_validation.0
  let initial_errors = resource_validation.1

  let keys = case read_optional_string_array(args, "translationKeys") {
    Some(ks) -> ks
    None -> []
  }
  let locales = case read_optional_string_array(args, "locales") {
    Some(ls) -> ls
    None -> []
  }
  let market_ids = case read_optional_string_array(args, "marketIds") {
    Some(m) -> m
    None -> []
  }

  let key_errors = case keys {
    [] -> [
      TranslationError(
        field: ["translationKeys"],
        message: "At least one translation key is required",
        code: "BLANK",
      ),
    ]
    _ -> []
  }
  let locale_errors = case locales {
    [] -> [
      TranslationError(
        field: ["locales"],
        message: "At least one locale is required",
        code: "BLANK",
      ),
    ]
    _ -> []
  }
  let market_errors = case market_ids {
    [] -> []
    _ -> [
      TranslationError(
        field: ["marketIds"],
        message: "Market-specific translations are not supported for this local resource branch",
        code: "MARKET_CUSTOM_CONTENT_NOT_ALLOWED",
      ),
    ]
  }

  let resource_errors = case resource {
    Some(record) -> {
      let key_validation =
        list.flat_map(keys, fn(k) {
          case content_has_key(record.content, k) {
            True -> []
            False -> [
              TranslationError(
                field: ["translationKeys"],
                message: "Key " <> k <> " is not translatable for this resource",
                code: "INVALID_KEY_FOR_MODEL",
              ),
            ]
          }
        })
      let locale_validation =
        list.flat_map(locales, fn(loc) {
          validate_locale_errors(store_in, loc, ["locales"])
        })
      list.append(key_validation, locale_validation)
    }
    None -> []
  }

  let errors =
    list.append(
      initial_errors,
      list.append(
        key_errors,
        list.append(locale_errors, list.append(market_errors, resource_errors)),
      ),
    )

  let #(removed, store_after) = case errors, resource {
    [], Some(record) -> {
      let #(removed, store_acc) =
        list.fold(locales, #([], store_in), fn(outer_acc, loc) {
          list.fold(keys, outer_acc, fn(inner_acc, k) {
            let #(removed_acc, store_step) = inner_acc
            let #(removed_record, store_next) =
              store.remove_translation(
                store_step,
                record.resource_id,
                loc,
                k,
                None,
              )
            case removed_record {
              Some(t) -> #(list.append(removed_acc, [t]), store_next)
              None -> #(removed_acc, store_next)
            }
          })
        })
      #(removed, store_acc)
    }
    _, _ -> #([], store_in)
  }

  let translations_for_payload = case errors {
    [] -> Some(removed)
    _ -> None
  }
  let payload =
    project_translations_payload(
      store_after,
      translations_for_payload,
      errors,
      field,
      fragments,
    )
  #(
    MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
    store_after,
    identity,
  )
}

fn validate_resource(
  store_in: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> #(Option(TranslatableResource), List(AnyUserError)) {
  case graphql_helpers.read_arg_string(args, "resourceId") {
    None -> #(None, [
      TranslationError(
        field: ["resourceId"],
        message: "Resource does not exist",
        code: "RESOURCE_NOT_FOUND",
      ),
    ])
    Some("") -> #(None, [
      TranslationError(
        field: ["resourceId"],
        message: "Resource does not exist",
        code: "RESOURCE_NOT_FOUND",
      ),
    ])
    Some(resource_id) ->
      case resource_exists_for_validation(store_in, resource_id) {
        None -> #(None, [
          TranslationError(
            field: ["resourceId"],
            message: "Resource " <> resource_id <> " does not exist",
            code: "RESOURCE_NOT_FOUND",
          ),
        ])
        Some(record) -> #(Some(record), [])
      }
  }
}

fn validate_locale_errors(
  store_in: Store,
  locale: String,
  field_path: List(String),
) -> List(AnyUserError) {
  case get_shop_locale(store_in, locale) {
    Some(_) -> []
    None -> [
      TranslationError(
        field: field_path,
        message: "Locale is not enabled for this shop",
        code: "INVALID_LOCALE_FOR_SHOP",
      ),
    ]
  }
}

fn content_has_key(content: List(TranslatableContent), key: String) -> Bool {
  list.any(content, fn(entry) { entry.key == key })
}

fn read_translation_inputs(
  args: Dict(String, root_field.ResolvedValue),
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "translations") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.ObjectVal(d) -> Ok(d)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn validate_and_build_translations(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  resource: TranslatableResource,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  initial_errors: List(AnyUserError),
) -> #(List(TranslationRecord), List(AnyUserError), SyntheticIdentityRegistry) {
  let #(_, errors_after, translations_rev, identity_after) =
    list.fold(inputs, #(0, initial_errors, [], identity), fn(acc, input) {
      let #(index, errors_acc, translations_acc, identity_acc) = acc
      let prefix = ["translations", int.to_string(index)]
      let locale_validation = case
        graphql_helpers.read_arg_string(input, "locale")
      {
        Some(loc) ->
          case get_shop_locale(store_in, loc) {
            Some(_) -> #(Some(loc), [])
            None -> #(Some(loc), [
              TranslationError(
                field: list.append(prefix, ["locale"]),
                message: "Locale is not enabled for this shop",
                code: "INVALID_LOCALE_FOR_SHOP",
              ),
            ])
          }
        None -> #(None, [
          TranslationError(
            field: list.append(prefix, ["locale"]),
            message: "Locale is not enabled for this shop",
            code: "INVALID_LOCALE_FOR_SHOP",
          ),
        ])
      }
      let #(maybe_locale, locale_errs) = locale_validation
      let key = case graphql_helpers.read_arg_string(input, "key") {
        Some(k) -> k
        None -> ""
      }
      let content = list.find(resource.content, fn(c) { c.key == key })
      let key_errors = case content {
        Ok(_) -> []
        Error(_) -> [
          TranslationError(
            field: list.append(prefix, ["key"]),
            message: "Key " <> key <> " is not translatable for this resource",
            code: "INVALID_KEY_FOR_MODEL",
          ),
        ]
      }
      let value = graphql_helpers.read_arg_string(input, "value")
      let value_errors = case value {
        Some(v) ->
          case v {
            "" -> [
              TranslationError(
                field: list.append(prefix, ["value"]),
                message: "Value can't be blank",
                code: "BLANK",
              ),
            ]
            _ -> []
          }
        None -> [
          TranslationError(
            field: list.append(prefix, ["value"]),
            message: "Value can't be blank",
            code: "BLANK",
          ),
        ]
      }
      let supplied_digest =
        graphql_helpers.read_arg_string(input, "translatableContentDigest")
      let digest_errors = case content, supplied_digest {
        Ok(c), Some(supplied) ->
          case c.digest {
            Some(actual) ->
              case actual == supplied {
                True -> []
                False -> [
                  TranslationError(
                    field: list.append(prefix, ["translatableContentDigest"]),
                    message: "Translatable content digest does not match the resource content",
                    code: "INVALID_TRANSLATABLE_CONTENT",
                  ),
                ]
              }
            None -> []
          }
        _, _ -> []
      }
      let market_errors = case
        graphql_helpers.read_arg_string(input, "marketId")
      {
        Some(_) -> [
          TranslationError(
            field: list.append(prefix, ["marketId"]),
            message: "Market-specific translations are not supported for this local resource branch",
            code: "MARKET_CUSTOM_CONTENT_NOT_ALLOWED",
          ),
        ]
        None -> []
      }
      let row_errors =
        list.append(
          locale_errs,
          list.append(
            key_errors,
            list.append(value_errors, list.append(digest_errors, market_errors)),
          ),
        )
      let new_errors = list.append(errors_acc, row_errors)
      let can_record = case row_errors, maybe_locale, value, content {
        [], Some(_), Some(_), Ok(_) -> True
        _, _, _, _ -> False
      }
      case can_record, errors_acc, maybe_locale, value, content {
        True, [], Some(loc), Some(v), Ok(c) -> {
          let #(timestamp, identity_next) =
            synthetic_identity.make_synthetic_timestamp(identity_acc)
          let supplied_digest_value = case supplied_digest {
            Some(d) -> d
            None ->
              case c.digest {
                Some(d) -> d
                None -> ""
              }
          }
          let record =
            TranslationRecord(
              resource_id: resource.resource_id,
              key: key,
              locale: loc,
              value: v,
              translatable_content_digest: supplied_digest_value,
              market_id: None,
              updated_at: timestamp,
              outdated: False,
            )
          #(index + 1, new_errors, [record, ..translations_acc], identity_next)
        }
        _, _, _, _, _ -> #(
          index + 1,
          new_errors,
          translations_acc,
          identity_acc,
        )
      }
    })
  #(list.reverse(translations_rev), errors_after, identity_after)
}

fn project_translations_payload(
  store_in: Store,
  translations: Option(List(TranslationRecord)),
  errors: List(AnyUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selections_of(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "translations" -> #(key, case translations {
              Some(records) ->
                json.array(records, fn(t) {
                  serialize_translation(store_in, t, selection, fragments)
                })
              None -> json.null()
            })
            "userErrors" -> #(key, serialize_user_errors(errors, selection))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn optional_string_json(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}
