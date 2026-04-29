//// Mirrors the operation-time helpers from `src/proxy/graphql-helpers.ts`.
////
//// The TS module is a 500-line grab bag covering value resolution,
//// projection, pagination, and connection serialization. This Gleam
//// port starts with the subset the events endpoint actually exercises
//// (`get_field_response_key`, `get_selected_child_fields`,
//// `serialize_empty_connection`, plus the page-info building blocks).
//// Other helpers will be added as further endpoint groups are ported.

import gleam/json.{type Json}
import gleam/list
import gleam/option.{Some}
import shopify_draft_proxy/graphql/ast.{
  type Selection, Field, InlineFragment, SelectionSet,
}

/// Whether `get_selected_child_fields` should also flatten field
/// selections that appear inside inline fragments. Mirrors the TS
/// `SelectedFieldOptions.includeInlineFragments`.
pub type SelectedFieldOptions {
  SelectedFieldOptions(include_inline_fragments: Bool)
}

/// Default options matching the TS `{}` argument.
pub fn default_selected_field_options() -> SelectedFieldOptions {
  SelectedFieldOptions(include_inline_fragments: False)
}

/// Response key for a `Field` selection: the alias if present, otherwise
/// the underlying field name. Mirrors `getFieldResponseKey`.
///
/// Returns the field's `name.value` for non-Field selections, which lets
/// callers thread the helper through fragment-flattening flows without
/// needing to special-case shape mismatches. The TS helper isn't
/// generic, but Gleam's exhaustive pattern matching forces us to handle
/// every variant — and the TS code never calls it on non-Field nodes.
pub fn get_field_response_key(field: Selection) -> String {
  case field {
    Field(alias: Some(alias), ..) -> alias.value
    Field(name: name, ..) -> name.value
    ast.FragmentSpread(name: name, ..) -> name.value
    ast.InlineFragment(..) -> ""
  }
}

/// Direct-child `Field` selections of a parent `Field`. Optionally
/// flattens `Field` selections from inline fragments. Mirrors
/// `getSelectedChildFields`.
pub fn get_selected_child_fields(
  field: Selection,
  options: SelectedFieldOptions,
) -> List(Selection) {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  flatten_field_selections(selections, options)
}

fn flatten_field_selections(
  selections: List(Selection),
  options: SelectedFieldOptions,
) -> List(Selection) {
  list.flat_map(selections, fn(selection) {
    case selection {
      Field(..) -> [selection]
      InlineFragment(selection_set: ss, ..) ->
        case options.include_inline_fragments {
          True -> {
            let SelectionSet(selections: inner, ..) = ss
            list.filter(inner, fn(s) {
              case s {
                Field(..) -> True
                _ -> False
              }
            })
          }
          False -> []
        }
      _ -> []
    }
  })
}

/// Serialize a connection's `pageInfo` block when there are no items —
/// the only shape the events handler ever needs. Mirrors
/// `serializeEmptyConnectionPageInfo` (which delegates to
/// `serializeConnectionPageInfo` with empty items).
pub fn serialize_empty_connection_page_info(
  page_info_field: Selection,
  options: SelectedFieldOptions,
) -> Json {
  let entries =
    list.map(get_selected_child_fields(page_info_field, options), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "hasNextPage" -> #(key, json.bool(False))
            "hasPreviousPage" -> #(key, json.bool(False))
            "startCursor" -> #(key, json.null())
            "endCursor" -> #(key, json.null())
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

/// Serialize an empty connection: `nodes` and `edges` become empty
/// arrays, `pageInfo` reflects the empty page, anything else becomes
/// `null`. Equivalent to calling the TS `serializeConnection` with
/// `items: []`, but specialised for the empty case to avoid pulling
/// in the rest of the pagination machinery.
pub fn serialize_empty_connection(
  field: Selection,
  options: SelectedFieldOptions,
) -> Json {
  let entries =
    list.map(get_selected_child_fields(field, options), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "nodes" -> #(key, json.array([], fn(x) { x }))
            "edges" -> #(key, json.array([], fn(x) { x }))
            "pageInfo" -> #(
              key,
              serialize_empty_connection_page_info(child, options),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}
