//// Products-domain submodule: variants_sources.
//// Combines layered files: variants_l08, variants_l09, variants_l10.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SerializeConnectionConfig, SrcList,
  SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, serialize_connection, src_object,
}

import shopify_draft_proxy/proxy/products/inventory_apply.{
  variant_inventory_item_source,
}
import shopify_draft_proxy/proxy/products/inventory_handlers.{
  product_variant_source_with_inventory,
}
import shopify_draft_proxy/proxy/products/products_core.{
  enumerate_items, serialize_product_metafield,
  serialize_product_metafields_connection,
}
import shopify_draft_proxy/proxy/products/shared.{
  connection_page_info_source, read_bool_argument, read_identifier_argument,
  read_string_argument, read_string_field,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  product_variant_cursor,
}

import shopify_draft_proxy/shopify/resource_ids

import shopify_draft_proxy/state/store.{type Store}

import shopify_draft_proxy/state/types.{
  type ProductRecord, type ProductVariantRecord,
}

// ===== from variants_l08 =====
@internal
pub fn product_variant_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  product_variant_source_with_inventory(
    store,
    variant,
    variant_inventory_item_source(variant),
  )
}

// ===== from variants_l09 =====
@internal
pub fn product_variants_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let variants = store.get_effective_variants_by_product_id(store, product.id)
  let edges =
    variants
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(variant, index) = pair
      src_object([
        #("cursor", SrcString(product_variant_cursor(variant, index))),
        #("node", product_variant_source(store, variant)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(variants, fn(variant) {
          product_variant_source(store, variant)
        }),
      ),
    ),
    #("pageInfo", connection_page_info_source(variants, product_variant_cursor)),
  ])
}

@internal
pub fn serialize_product_variant_object(
  store: Store,
  variant: ProductVariantRecord,
  selections: List(Selection),
  owner_field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let source = product_variant_source(store, variant)
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "metafield" ->
              serialize_product_metafield(
                store,
                variant.id,
                selection,
                variables,
              )
            "metafields" ->
              serialize_product_metafields_connection(
                store,
                variant.id,
                selection,
                variables,
              )
            _ -> project_graphql_field_value(source, selection, fragments)
          }
        _ -> project_graphql_field_value(source, owner_field, fragments)
      }
      #(key, value)
    }),
  )
}

// ===== from variants_l10 =====
@internal
pub fn serialize_product_variant_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_variant_by_id(store, id) {
        Some(variant) ->
          serialize_product_variant_object(
            store,
            variant,
            get_selected_child_fields(field, default_selected_field_options()),
            field,
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_product_variant_by_identifier_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_identifier_argument(field, variables) {
    Some(identifier) ->
      case read_string_field(identifier, "id") {
        Some(id) ->
          case store.get_effective_variant_by_id(store, id) {
            Some(variant) ->
              serialize_product_variant_object(
                store,
                variant,
                get_selected_child_fields(
                  field,
                  default_selected_field_options(),
                ),
                field,
                variables,
                fragments,
              )
            None -> json.null()
          }
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_product_variants_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let variants =
    store.list_effective_product_variants(store)
    |> list.sort(fn(left, right) {
      resource_ids.compare_shopify_resource_ids(left.id, right.id)
    })
  let ordered_variants = case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(variants)
    _ -> variants
  }
  let window =
    paginate_connection_items(
      ordered_variants,
      field,
      variables,
      product_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        serialize_product_variant_object(
          store,
          variant,
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          node_field,
          variables,
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn serialize_product_variants_for_product_connection(
  store: Store,
  product: ProductRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let variants = store.get_effective_variants_by_product_id(store, product.id)
  let window =
    paginate_connection_items(
      variants,
      field,
      variables,
      product_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        serialize_product_variant_object(
          store,
          variant,
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          node_field,
          variables,
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}
