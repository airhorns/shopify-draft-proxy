//// Markets domain port.
////
//// This first slice supports captured/snapshot read projection for the core
//// Markets catalog resources. Mutation staging lands separately so the
//// dispatcher only claims the query roots implemented here.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
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
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CatalogRecord, type MarketRecord,
  type PriceListRecord, type WebPresenceRecord, CapturedArray, CapturedBool,
  CapturedFloat, CapturedInt, CapturedNull, CapturedObject, CapturedString,
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
