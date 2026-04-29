//// Mirrors the read path of `src/proxy/metafields.ts`.
////
//// This is a substrate port: only the projection helpers
//// (`serializeMetafieldSelection*`) and the `compareDigest` builder
//// land here. The mutation paths (`upsertOwnerMetafields`,
//// `normalizeOwnerMetafield`, `mergeMetafieldRecords`,
//// `readMetafieldInputObjects`) are deferred — they depend on the
//// 360-LOC `products/metafield-values.ts` substrate which has its
//// own `parseMetafieldJsonValue` / `normalizeMetafieldValue`
//// machinery and a non-trivial value-shape pipeline. Read-path
//// consumers (every domain that exposes a `metafields(...)`
//// connection) only need projection, which is what this module
//// covers.
////
//// `MetafieldRecordCore` is the projection-shaped record. Mutation
//// passes will likely wrap it in an owner-scoped variant; the read
//// path here treats every metafield uniformly.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type SelectedFieldOptions, ConnectionPageInfoOptions,
  SerializeConnectionConfig, default_connection_page_info_options,
  default_connection_window_options, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, serialize_connection,
}

/// Mirrors `MetafieldRecordCore`. Optional fields are present when the
/// owning domain populates them (mutation-side); read paths from a
/// snapshot may omit any of them. `json_value` is carried as
/// `Option(Json)` so callers can pass through whatever shape the
/// upstream record holds without us having to reify it.
pub type MetafieldRecordCore {
  MetafieldRecordCore(
    id: String,
    namespace: String,
    key: String,
    type_: Option(String),
    value: Option(String),
    compare_digest: Option(String),
    json_value: Option(Json),
    created_at: Option(String),
    updated_at: Option(String),
    owner_type: Option(String),
  )
}

/// Build the `compareDigest` string for a metafield. Mirrors
/// `makeMetafieldCompareDigest`.
///
/// The TS encodes `JSON.stringify([namespace, key, type, value,
/// jsonValue ?? null, updatedAt ?? null])` as base64url. We mirror
/// that exactly: build a JSON array with the same six elements, render
/// it to a string, then base64url-encode the bytes (no padding —
/// `Buffer.toString('base64url')` strips it).
pub fn make_metafield_compare_digest(metafield: MetafieldRecordCore) -> String {
  let payload =
    json.array(
      [
        json.string(metafield.namespace),
        json.string(metafield.key),
        option_string_to_json(metafield.type_),
        option_string_to_json(metafield.value),
        option_json_to_json(metafield.json_value),
        option_string_to_json(metafield.updated_at),
      ],
      fn(x) { x },
    )
  let body =
    payload
    |> json.to_string
    |> bit_array.from_string
    |> bit_array.base64_url_encode(False)
  "draft:" <> body
}

fn option_string_to_json(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}

fn option_json_to_json(value: Option(Json)) -> Json {
  case value {
    Some(j) -> j
    None -> json.null()
  }
}

/// Project a metafield onto a list of selection nodes. Mirrors
/// `serializeMetafieldSelectionSet`. Unknown fields fall through to
/// `null`, matching the TS default branch. The `jsonValue` and
/// `definition` cases that depend on `parseMetafieldJsonValue` /
/// definition lookup return `null` here because those substrate
/// pieces haven't been ported yet.
pub fn serialize_metafield_selection_set(
  metafield: MetafieldRecordCore,
  selections: List(Selection),
) -> Json {
  let entries =
    list.filter_map(selections, fn(selection) {
      case selection {
        Field(name: name, ..) -> {
          let key = get_field_response_key(selection)
          let value = case name.value {
            "__typename" -> json.string("Metafield")
            "id" -> json.string(metafield.id)
            "namespace" -> json.string(metafield.namespace)
            "key" -> json.string(metafield.key)
            "type" -> option_string_to_json(metafield.type_)
            "value" -> option_string_to_json(metafield.value)
            "compareDigest" ->
              case metafield.compare_digest {
                Some(d) -> json.string(d)
                None -> json.string(make_metafield_compare_digest(metafield))
              }
            "jsonValue" -> option_json_to_json(metafield.json_value)
            "createdAt" -> option_string_to_json(metafield.created_at)
            "updatedAt" ->
              case metafield.updated_at, metafield.created_at {
                Some(u), _ -> json.string(u)
                None, Some(c) -> json.string(c)
                None, None -> json.null()
              }
            "ownerType" -> option_string_to_json(metafield.owner_type)
            "definition" -> json.null()
            _ -> json.null()
          }
          Ok(#(key, value))
        }
        _ -> Error(Nil)
      }
    })
  json.object(entries)
}

/// Project a metafield against the child fields of a `Field`.
/// Mirrors `serializeMetafieldSelection`.
pub fn serialize_metafield_selection(
  metafield: MetafieldRecordCore,
  field: Selection,
  options: SelectedFieldOptions,
) -> Json {
  serialize_metafield_selection_set(
    metafield,
    get_selected_child_fields(field, options),
  )
}

/// Serialize a metafields connection. Mirrors
/// `serializeMetafieldsConnection`. Pagination uses the metafield's
/// `id` as the cursor source — same as the TS
/// `(metafield) => metafield.id`.
pub fn serialize_metafields_connection(
  metafields: List(MetafieldRecordCore),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  options: SelectedFieldOptions,
) -> Json {
  let window =
    paginate_connection_items(
      metafields,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: options.include_inline_fragments,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: cursor_value,
      serialize_node: fn(record, node_field, _index) {
        serialize_metafield_selection(record, node_field, options)
      },
      selected_field_options: options,
      page_info_options: page_info_options,
    ),
  )
}

fn cursor_value(record: MetafieldRecordCore, _index: Int) -> String {
  record.id
}
