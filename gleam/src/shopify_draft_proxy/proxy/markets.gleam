//// Markets domain port.
////
//// Supports captured/snapshot read projection for core Markets catalog
//// resources plus the locally-staged MarketWebPresence lifecycle covered by
//// the checked-in parity captures.

import gleam/dict.{type Dict}
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
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, serialize_empty_connection,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, single_root_log_draft,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CatalogRecord, type MarketRecord,
  type PriceListRecord, type WebPresenceRecord, CapturedArray, CapturedBool,
  CapturedFloat, CapturedInt, CapturedNull, CapturedObject, CapturedString,
  WebPresenceRecord,
}

pub type MarketsError {
  ParseFailed(root_field.RootFieldError)
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

type MarketConnectionItem {
  MarketConnectionItem(
    source: SourceValue,
    pagination_cursor: String,
    output_cursor: String,
  )
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
    "webPresenceCreate" | "webPresenceUpdate" | "webPresenceDelete" -> True
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

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MarketsError) {
  use data <- result.try(handle_markets_query(store, document, variables))
  Ok(wrap_data(data))
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, MarketsError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  Ok(handle_mutation_fields(store, identity, fields, fragments, variables))
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
            handle_web_presence_mutation(
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

fn handle_web_presence_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(MutationFieldResult) {
  case name {
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
    _ -> None
  }
}

fn handle_web_presence_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input") |> option.unwrap(dict.new())
  let errors = web_presence_create_errors(input)
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
  let args = field_args(field, variables)
  let id = read_arg_string(args, "id")
  let input = read_arg_object(args, "input") |> option.unwrap(dict.new())
  case id {
    Some(id_value) ->
      case store.get_effective_web_presence_by_id(store, id_value) {
        Some(_) -> {
          let data = web_presence_data(store, id_value, input)
          let record = WebPresenceRecord(id: id_value, cursor: None, data: data)
          let #(_, next_store) = store.upsert_staged_web_presence(store, record)
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
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
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
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  let domain_errors = case read_arg_string(input, "domainId") {
    Some(_) -> [
      user_error(
        ["input", "domainId"],
        "Domain does not exist",
        "DOMAIN_NOT_FOUND",
      ),
    ]
    None -> []
  }
  let locale_errors = case read_arg_string(input, "defaultLocale") {
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
    read_arg_string(input, "defaultLocale") |> option.unwrap("en")
  let alternate_locales =
    read_arg_string_array(input, "alternateLocales") |> option.unwrap([])
  let suffix = read_arg_string(input, "subfolderSuffix")
  CapturedObject([
    #("__typename", CapturedString("MarketWebPresence")),
    #("id", CapturedString(id)),
    #("subfolderSuffix", optional_captured_string(suffix)),
    #("domain", CapturedNull),
    #(
      "rootUrls",
      CapturedArray([
        CapturedObject([
          #("locale", CapturedString(default_locale)),
          #(
            "url",
            CapturedString(web_presence_root_url(store, default_locale, suffix)),
          ),
        ]),
      ]),
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
  suffix: Option(String),
) -> String {
  let base_url = store_web_presence_base_url(store)
  case suffix {
    Some(s) -> base_url <> "/" <> locale <> "-" <> s <> "/"
    None -> base_url <> "/"
  }
}

fn store_web_presence_base_url(store: Store) -> String {
  case list.first(store.list_effective_web_presences(store)) {
    Ok(record) ->
      case captured_field(record.data, "domain") {
        Some(domain) ->
          case captured_string_field(domain, "url") {
            Some(url) -> url
            None ->
              case captured_string_field(domain, "host") {
                Some(host) -> "https://" <> host
                None -> "https://harry-test-heelo.myshopify.com"
              }
          }
        None -> "https://harry-test-heelo.myshopify.com"
      }
    Error(_) -> "https://harry-test-heelo.myshopify.com"
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
        "marketLocalizableResource" -> json.null()
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
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
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

fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
}

fn read_arg_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(s)) ->
      case s {
        "" -> None
        _ -> Some(s)
      }
    _ -> None
  }
}

fn read_arg_object(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, name) {
    Ok(root_field.ObjectVal(fields)) -> Some(fields)
    _ -> None
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
