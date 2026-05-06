//// Products-domain submodule: products_handlers.
//// Combines layered files: products_l07, products_l08, products_l11, products_l12, products_l13, products_l14, products_l15.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue, BoolVal}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, serialize_connection,
  serialize_empty_connection, src_object,
}

import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/customers/hydration as customer_hydration
import shopify_draft_proxy/proxy/customers/serializers as customer_serializers
import shopify_draft_proxy/proxy/mutation_helpers.{
  RequiredArgument, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/online_store/serializers as online_store_serializers
import shopify_draft_proxy/proxy/orders/common as orders_common
import shopify_draft_proxy/proxy/orders/hydration as orders_hydration

import shopify_draft_proxy/proxy/products/inventory_apply.{
  sync_product_inventory_summary, sync_product_set_inventory_summary,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type MutationFieldResult, type ProductUserError, ProductUserError,
  RecomputeProductTotalInventory, product_user_error,
  product_user_error_code_product_not_found,
}
import shopify_draft_proxy/proxy/products/products_core.{
  has_effective_product_metafield_owner, is_valid_product_status,
  make_product_preview_url, product_by_identifier,
  product_change_status_null_product_id_error, product_cursor,
  product_operation_user_error_source, product_set_max_input_size_errors,
  product_set_metafield_records, product_tag_values_validation_error,
  product_tags_max_input_size_errors, product_tags_validation_errors,
  read_tag_inputs, serialize_product_metafield,
  serialize_product_metafield_owner_selection,
  serialize_product_metafields_connection, sort_products, tags_update_root_name,
}
import shopify_draft_proxy/proxy/products/products_records.{
  created_product_record, product_count_for_field, product_cursor_for_field,
  product_set_product_field_errors, product_update_validation_errors,
  updated_product_record,
}
import shopify_draft_proxy/proxy/products/products_validation.{
  duplicated_product_record, explicit_product_handle_collision_errors,
  filtered_products, normalize_product_tags, product_scalar_validation_errors,
  product_set_shape_validation_errors, remove_product_tags_by_identity,
  resolve_product_set_existing_product,
}
import shopify_draft_proxy/proxy/products/publications_core.{
  selected_publication_id,
}
import shopify_draft_proxy/proxy/products/publications_feeds.{
  duplicate_product_relationships,
}
import shopify_draft_proxy/proxy/products/publications_publishable.{
  product_source_with_store_and_publication,
}
import shopify_draft_proxy/proxy/products/shared.{
  mutation_error_result, mutation_error_with_null_data_result,
  mutation_rejected_result, mutation_result, read_arg_bool_default_true,
  read_identifier_argument, read_object_list_field, read_string_argument,
  user_errors_source,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  make_default_option_record, make_default_variant_record,
}
import shopify_draft_proxy/proxy/products/variants_options.{
  make_product_create_option_graph, product_set_duplicate_variant_errors,
  product_set_option_records,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  product_set_requires_variants_for_options_errors,
  sync_product_options_with_variants,
}
import shopify_draft_proxy/proxy/products/variants_sources.{
  serialize_product_variants_for_product_connection,
}
import shopify_draft_proxy/proxy/products/variants_validation.{
  product_create_variant_errors, product_set_scalar_variant_errors,
  product_set_variant_records,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CustomerRecord, type DraftOrderRecord,
  type OnlineStoreContentRecord, type OrderRecord, type ProductOperationRecord,
  type ProductOperationUserErrorRecord, type ProductRecord, CapturedArray,
  CapturedObject, CapturedString, CustomerRecord, DraftOrderRecord,
  OnlineStoreContentRecord, OrderRecord, ProductOperationRecord,
  ProductOperationUserErrorRecord, ProductRecord,
}

// ===== from products_l07 =====
@internal
pub fn product_set_validation_errors(
  store: Store,
  input: Dict(String, ResolvedValue),
  existing: Option(ProductRecord),
) -> List(ProductOperationUserErrorRecord) {
  list.append(
    product_set_product_field_errors(store, input, existing),
    list.append(
      product_set_requires_variants_for_options_errors(input),
      list.append(
        product_set_duplicate_variant_errors(input),
        product_set_scalar_variant_errors(input),
      ),
    ),
  )
}

@internal
pub fn apply_product_set_graph(
  store: Store,
  identity: SyntheticIdentityRegistry,
  existing: Option(ProductRecord),
  product_id: String,
  input: Dict(String, ResolvedValue),
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  let #(store, identity, option_ids) = case
    dict.has_key(input, "productOptions")
  {
    True -> {
      let #(options, next_identity, ids) =
        product_set_option_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "productOptions"),
        )
      let next_store =
        store.replace_staged_options_for_product(store, product_id, options)
      #(next_store, next_identity, ids)
    }
    False ->
      case existing {
        Some(_) -> #(store, identity, [])
        None ->
          case store.get_effective_options_by_product_id(store, product_id) {
            [] -> {
              let assert Some(product) =
                store.get_effective_product_by_id(store, product_id)
              let #(option, next_identity, ids) =
                make_default_option_record(identity, product)
              let next_store =
                store.replace_staged_options_for_product(store, product_id, [
                  option,
                ])
              #(next_store, next_identity, ids)
            }
            _ -> #(store, identity, [])
          }
      }
  }
  let #(store, identity, variant_ids) = case dict.has_key(input, "variants") {
    True -> {
      let #(variants, next_identity, ids) =
        product_set_variant_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "variants"),
        )
      let synced_options =
        sync_product_options_with_variants(
          store.get_effective_options_by_product_id(store, product_id),
          variants,
        )
      let next_store =
        store
        |> store.replace_staged_variants_for_product(product_id, variants)
        |> store.replace_staged_options_for_product(product_id, synced_options)
      #(next_store, next_identity, ids)
    }
    False ->
      case existing {
        Some(_) -> #(store, identity, [])
        None ->
          case store.get_effective_variants_by_product_id(store, product_id) {
            [] -> {
              let assert Some(product) =
                store.get_effective_product_by_id(store, product_id)
              let #(variant, next_identity, ids) =
                make_default_variant_record(identity, product)
              let next_store =
                store.replace_staged_variants_for_product(store, product_id, [
                  variant,
                ])
              #(next_store, next_identity, ids)
            }
            _ -> #(store, identity, [])
          }
      }
  }
  let #(store, identity, metafield_ids) = case
    dict.has_key(input, "metafields")
  {
    True -> {
      let #(metafields, next_identity, ids) =
        product_set_metafield_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "metafields"),
        )
      let next_store =
        store.replace_staged_metafields_for_owner(store, product_id, metafields)
      #(next_store, next_identity, ids)
    }
    False -> #(store, identity, [])
  }
  #(
    store,
    identity,
    list.append(option_ids, list.append(variant_ids, metafield_ids)),
  )
}

// ===== from products_l08 =====
@internal
pub fn product_create_validation_errors(
  store: Store,
  input: Dict(String, ResolvedValue),
  input_root: String,
) -> List(ProductUserError) {
  let field_prefix = case input_root {
    "input" -> ["input"]
    _ -> []
  }
  let handle_errors =
    explicit_product_handle_collision_errors(store, input, None)
    |> list.map(fn(error) {
      let ProductUserError(field: path, message: message, code: code) = error
      ProductUserError(field: ["input", ..path], message: message, code: code)
    })
  list.append(
    product_scalar_validation_errors(input, field_prefix, require_title: True),
    list.append(
      product_tags_validation_errors(input),
      list.append(product_create_variant_errors(input), handle_errors),
    ),
  )
}

// ===== from products_l11 =====
@internal
pub fn serialize_product_selection(
  store: Store,
  product: ProductRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  let source =
    product_source_with_store_and_publication(
      store,
      product,
      selected_publication_id(selections, variables),
    )
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "metafield" -> #(
              key,
              serialize_product_metafield(
                store,
                product.id,
                selection,
                variables,
              ),
            )
            "metafields" -> #(
              key,
              serialize_product_metafields_connection(
                store,
                product.id,
                selection,
                variables,
              ),
            )
            "variants" -> #(
              key,
              serialize_product_variants_for_product_connection(
                store,
                product,
                selection,
                variables,
                fragments,
              ),
            )
            _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn product_source_with_store(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  product_source_with_store_and_publication(store, product, None)
}

// ===== from products_l12 =====
@internal
pub fn serialize_product_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_by_id(store, id) {
        Some(product) ->
          serialize_product_selection(
            store,
            product,
            field,
            variables,
            fragments,
          )
        None ->
          case has_effective_product_metafield_owner(store, id) {
            True ->
              serialize_product_metafield_owner_selection(
                store,
                id,
                field,
                variables,
              )
            False -> json.null()
          }
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_product_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_product_by_id(store, id) {
    Some(product) ->
      project_graphql_value(
        product_source_with_store(store, product),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

@internal
pub fn serialize_product_by_identifier_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_identifier_argument(field, variables) {
    Some(identifier) ->
      case product_by_identifier(store, identifier) {
        Some(product) ->
          serialize_product_selection(
            store,
            product,
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
pub fn serialize_products_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let products =
    filtered_products(store, field, variables)
    |> sort_products(field, variables)
  case products {
    [] -> serialize_empty_connection(field, default_selected_field_options())
    _ -> {
      let window =
        paginate_connection_items(
          products,
          field,
          variables,
          product_cursor,
          default_connection_window_options(),
        )
      let count = product_count_for_field(store, field, variables)
      let has_next_page =
        window.has_next_page || count > list.length(window.items)
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: window.items,
          has_next_page: has_next_page,
          has_previous_page: window.has_previous_page,
          get_cursor_value: fn(product, index) {
            product_cursor_for_field(product, index, field, variables)
          },
          serialize_node: fn(product, node_field, _index) {
            project_graphql_value(
              product_source_with_store(store, product),
              get_selected_child_fields(
                node_field,
                default_selected_field_options(),
              ),
              fragments,
            )
          },
          selected_field_options: default_selected_field_options(),
          page_info_options: ConnectionPageInfoOptions(
            include_inline_fragments: False,
            prefix_cursors: False,
            include_cursors: True,
            fallback_start_cursor: None,
            fallback_end_cursor: None,
          ),
        ),
      )
    }
  }
}

@internal
pub fn serialize_product_list_connection(
  products: List(ProductRecord),
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let window =
    paginate_connection_items(
      products,
      field,
      variables,
      product_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_cursor,
      serialize_node: fn(product, node_field, _index) {
        project_graphql_value(
          product_source_with_store(store, product),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn product_operation_source(
  store: Store,
  operation: ProductOperationRecord,
) -> SourceValue {
  let status = case operation.type_name, operation.status {
    "ProductDeleteOperation", "CREATED" -> "COMPLETE"
    "ProductDeleteOperation", "RUNNING" -> "COMPLETE"
    _, status -> status
  }
  src_object([
    #("__typename", SrcString(operation.type_name)),
    #("id", SrcString(operation.id)),
    #("status", SrcString(status)),
    #("deletedProductId", case operation.type_name, status {
      "ProductDeleteOperation", "COMPLETE" ->
        case operation.product_id {
          Some(product_id) -> SrcString(product_id)
          None -> SrcNull
        }
      _, _ -> SrcNull
    }),
    #("product", case operation.product_id {
      Some(product_id) ->
        case store.get_effective_product_by_id(store, product_id) {
          Some(product) -> product_source_with_store(store, product)
          None -> SrcNull
        }
      None -> SrcNull
    }),
    #("newProduct", case operation.new_product_id {
      Some(product_id) ->
        case store.get_effective_product_by_id(store, product_id) {
          Some(product) -> product_source_with_store(store, product)
          None -> SrcNull
        }
      None -> SrcNull
    }),
    #(
      "userErrors",
      SrcList(list.map(
        operation.user_errors,
        product_operation_user_error_source,
      )),
    ),
  ])
}

@internal
pub fn product_update_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductUpdatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_create_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductCreatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn tags_update_payload(
  _store: Store,
  is_add: Bool,
  node: SourceValue,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let typename = case is_add {
    True -> "TagsAddPayload"
    False -> "TagsRemovePayload"
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("node", node),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_change_status_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductChangeStatusPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from products_l13 =====
@internal
pub fn serialize_product_operation_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_operation_by_id(store, id) {
        Some(operation) ->
          project_graphql_value(
            product_operation_source(store, operation),
            graphql_helpers.field_raw_selections(field),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_product_operation_node_by_id(
  store: Store,
  id: String,
  selection: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_product_operation_by_id(store, id) {
    Some(operation) ->
      project_graphql_value(
        product_operation_source(store, operation),
        selection,
        fragments,
      )
    None -> json.null()
  }
}

@internal
pub fn product_duplicate_payload(
  store: Store,
  new_product: Option(ProductRecord),
  operation: Option(ProductOperationRecord),
  user_errors: List(ProductOperationUserErrorRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let operation_value = case operation {
    Some(operation) -> product_operation_source(store, operation)
    None -> SrcNull
  }
  let source =
    src_object([
      #("__typename", SrcString("ProductDuplicatePayload")),
      #("productDuplicateOperation", operation_value),
      #(
        "userErrors",
        SrcList(list.map(user_errors, product_operation_user_error_source)),
      ),
    ])
  let entries =
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "newProduct" ->
              case new_product {
                Some(product) -> #(
                  key,
                  serialize_product_selection(
                    store,
                    product,
                    selection,
                    variables,
                    fragments,
                  ),
                )
                None -> #(key, json.null())
              }
            _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn product_set_payload(
  store: Store,
  product: Option(ProductRecord),
  operation: Option(ProductOperationRecord),
  user_errors: List(ProductOperationUserErrorRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let operation_value = case operation {
    Some(operation) -> product_operation_source(store, operation)
    None -> SrcNull
  }
  let source =
    src_object([
      #("__typename", SrcString("ProductSetPayload")),
      #("productSetOperation", operation_value),
      #(
        "userErrors",
        SrcList(list.map(user_errors, product_operation_user_error_source)),
      ),
    ])
  let entries =
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "product" ->
              case product {
                Some(product) -> #(
                  key,
                  serialize_product_selection(
                    store,
                    product,
                    selection,
                    variables,
                    fragments,
                  ),
                )
                None -> #(key, json.null())
              }
            _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn handle_product_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shopify_admin_origin: String,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  // Real Shopify accepts both `productCreate(product: ProductCreateInput!)`
  // (current schema) and `productCreate(input: ProductInput!)` (older API
  // versions / pre-2024). Reading only `product` made the proxy fabricate
  // a misleading `["title"], "Title can't be blank"` userError when the
  // legacy shape was used; emit a structurally honest top-level error
  // instead when neither shows up at all.
  let #(input, input_root) = case
    graphql_helpers.read_arg_object(args, "product")
  {
    Some(d) -> #(Some(d), "product")
    None -> #(graphql_helpers.read_arg_object(args, "input"), "input")
  }
  case input {
    None -> {
      let errors =
        validate_required_field_arguments(
          field,
          variables,
          "productCreate",
          [RequiredArgument("product", "ProductCreateInput!")],
          operation_path,
          document,
        )
      mutation_error_result(key, store, identity, errors)
    }
    Some(input) ->
      case
        product_tags_max_input_size_errors("productCreate", input_root, input)
      {
        [_, ..] as errors -> mutation_error_result(key, store, identity, errors)
        [] -> {
          let user_errors =
            product_create_validation_errors(store, input, input_root)
          case user_errors {
            [_, ..] ->
              mutation_rejected_result(
                key,
                product_create_payload(
                  store,
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
            [] -> {
              let #(product, identity_after_product) =
                created_product_record(
                  store,
                  identity,
                  shopify_admin_origin,
                  input,
                )
              let #(options, default_variant, identity_after_graph, graph_ids) =
                make_product_create_option_graph(
                  identity_after_product,
                  product,
                  read_object_list_field(input, "productOptions"),
                )
              let #(_, next_store) = store.upsert_staged_product(store, product)
              let next_store =
                next_store
                |> store.replace_staged_options_for_product(product.id, options)
                |> store.replace_staged_variants_for_product(product.id, [
                  default_variant,
                ])
              let #(synced_product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_graph,
                  product.id,
                  RecomputeProductTotalInventory,
                )
              let synced_product = synced_product |> option.unwrap(product)
              mutation_result(
                key,
                product_create_payload(
                  next_store,
                  Some(synced_product),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                [synced_product.id, ..graph_ids],
              )
            }
          }
        }
      }
  }
}

@internal
pub fn handle_product_change_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  case
    product_change_status_null_product_id_error(document, operation_path, field)
  {
    Some(error) -> mutation_error_result(key, store, identity, [error])
    None -> {
      let args = graphql_helpers.field_args(field, variables)
      case graphql_helpers.read_arg_string(args, "productId") {
        None ->
          mutation_result(
            key,
            product_change_status_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product_id) -> {
          let status = graphql_helpers.read_arg_string(args, "status")
          case is_valid_product_status(status) {
            False ->
              mutation_result(
                key,
                product_change_status_payload(
                  store,
                  None,
                  [
                    ProductUserError(
                      ["status"],
                      "Product status is required",
                      None,
                    ),
                  ],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            True ->
              case store.get_effective_product_by_id(store, product_id) {
                None ->
                  mutation_result(
                    key,
                    product_change_status_payload(
                      store,
                      None,
                      [
                        product_user_error(
                          ["productId"],
                          "Product does not exist",
                          product_user_error_code_product_not_found,
                        ),
                      ],
                      field,
                      fragments,
                    ),
                    store,
                    identity,
                    [],
                  )
                Some(product) -> {
                  let assert Some(next_status) = status
                  let #(updated_at, next_identity) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  let next_product =
                    ProductRecord(
                      ..product,
                      status: next_status,
                      updated_at: Some(updated_at),
                    )
                  let #(_, next_store) =
                    store.upsert_staged_product(store, next_product)
                  mutation_result(
                    key,
                    product_change_status_payload(
                      next_store,
                      Some(next_product),
                      [],
                      field,
                      fragments,
                    ),
                    next_store,
                    next_identity,
                    [next_product.id],
                  )
                }
              }
          }
        }
      }
    }
  }
}

@internal
pub fn handle_product_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "product")
  case input {
    Some(input) ->
      case
        product_tags_max_input_size_errors("productUpdate", "product", input)
      {
        [_, ..] as errors -> mutation_error_result(key, store, identity, errors)
        [] -> {
          let id = graphql_helpers.read_arg_string(input, "id")
          case id {
            None ->
              mutation_result(
                key,
                product_update_payload(
                  store,
                  None,
                  [
                    ProductUserError(["id"], "Product does not exist", None),
                  ],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(product_id) ->
              case store.get_effective_product_by_id(store, product_id) {
                None ->
                  mutation_result(
                    key,
                    product_update_payload(
                      store,
                      None,
                      [
                        ProductUserError(["id"], "Product does not exist", None),
                      ],
                      field,
                      fragments,
                    ),
                    store,
                    identity,
                    [],
                  )
                Some(product) ->
                  case product_update_validation_errors(input) {
                    [_, ..] as validation_errors ->
                      mutation_rejected_result(
                        key,
                        product_update_payload(
                          store,
                          Some(product),
                          validation_errors,
                          field,
                          fragments,
                        ),
                        store,
                        identity,
                      )
                    [] -> {
                      let #(next_product, next_identity) =
                        updated_product_record(identity, product, input)
                      let #(_, next_store) =
                        store.upsert_staged_product(store, next_product)
                      mutation_result(
                        key,
                        product_update_payload(
                          next_store,
                          Some(next_product),
                          [],
                          field,
                          fragments,
                        ),
                        next_store,
                        next_identity,
                        [next_product.id],
                      )
                    }
                  }
              }
          }
        }
      }
    None ->
      mutation_result(
        key,
        product_update_payload(
          store,
          None,
          [ProductUserError(["id"], "Product does not exist", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_tags_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case
    product_tags_max_input_size_errors(tags_update_root_name(is_add), "", args)
  {
    [_, ..] as errors -> mutation_error_result(key, store, identity, errors)
    [] ->
      case graphql_helpers.read_arg_string(args, "id") {
        None ->
          mutation_result(
            key,
            tags_update_payload(
              store,
              is_add,
              SrcNull,
              [ProductUserError(["id"], "Product id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product_id) ->
          handle_tags_update_for_id(
            store,
            identity,
            key,
            product_id,
            is_add,
            field,
            fragments,
            args,
            upstream,
          )
      }
  }
}

fn handle_tags_update_for_id(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  id: String,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> MutationFieldResult {
  case taggable_gid_type(id) {
    Some("Product") ->
      case store.get_effective_product_by_id(store, id) {
        Some(product) ->
          apply_product_tags_update(
            store,
            identity,
            key,
            product,
            is_add,
            field,
            fragments,
            args,
          )
        None ->
          tags_update_not_found_result(
            store,
            identity,
            key,
            is_add,
            field,
            fragments,
            "Product not found",
          )
      }
    Some("Order") -> {
      let hydrated_store =
        orders_hydration.maybe_hydrate_order_by_id(store, id, upstream)
      case store.get_order_by_id(hydrated_store, id) {
        Some(order) ->
          apply_order_tags_update(
            hydrated_store,
            identity,
            key,
            order,
            is_add,
            field,
            fragments,
            args,
          )
        None ->
          tags_update_not_found_result(
            hydrated_store,
            identity,
            key,
            is_add,
            field,
            fragments,
            "Order not found",
          )
      }
    }
    Some("DraftOrder") -> {
      let hydrated_store =
        orders_hydration.maybe_hydrate_draft_order_by_id(store, id, upstream)
      case store.get_draft_order_by_id(hydrated_store, id) {
        Some(draft_order) ->
          apply_draft_order_tags_update(
            hydrated_store,
            identity,
            key,
            draft_order,
            is_add,
            field,
            fragments,
            args,
          )
        None ->
          tags_update_not_found_result(
            hydrated_store,
            identity,
            key,
            is_add,
            field,
            fragments,
            "Draft order not found",
          )
      }
    }
    Some("Customer") -> {
      let #(hydrated_store, hydrated_identity) =
        customer_hydration.maybe_hydrate_customer(store, identity, id, upstream)
      case store.get_effective_customer_by_id(hydrated_store, id) {
        Some(customer) ->
          apply_customer_tags_update(
            hydrated_store,
            hydrated_identity,
            key,
            customer,
            is_add,
            field,
            fragments,
            args,
          )
        None ->
          tags_update_not_found_result(
            hydrated_store,
            hydrated_identity,
            key,
            is_add,
            field,
            fragments,
            "Customer not found",
          )
      }
    }
    Some("Article") -> {
      let hydrated_store = maybe_hydrate_article_for_tags(store, id, upstream)
      case store.get_effective_online_store_content_by_id(hydrated_store, id) {
        Some(article) if article.kind == "article" ->
          apply_article_tags_update(
            hydrated_store,
            identity,
            key,
            article,
            is_add,
            field,
            fragments,
            args,
          )
        _ ->
          tags_update_not_found_result(
            hydrated_store,
            identity,
            key,
            is_add,
            field,
            fragments,
            "Article not found",
          )
      }
    }
    // Shopify can expose DiscountNode through the discount_tags_api flag.
    // Local discount tag state is not modeled yet, so keep it out of the
    // supported gid_type set for now instead of fabricating support.
    _ ->
      mutation_error_with_null_data_result(key, store, identity, [
        invalid_taggable_gid_error(field, id, is_add),
      ])
  }
}

fn apply_product_tags_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let tags = read_tag_inputs(args, is_add)
  case
    tags_validation_result_with_node(
      store,
      identity,
      key,
      is_add,
      field,
      fragments,
      tags,
      product_source_with_store(store, product),
    )
  {
    Some(result) -> result
    None -> {
      let next_tags = updated_tags(product.tags, tags, is_add)
      let next_product = ProductRecord(..product, tags: next_tags)
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      mutation_result(
        key,
        tags_update_payload(
          next_store,
          is_add,
          product_source_with_store(next_store, next_product),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        [next_product.id],
      )
    }
  }
}

fn apply_order_tags_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  order: OrderRecord,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let tags = read_tag_inputs(args, is_add)
  case
    tags_validation_result_with_node(
      store,
      identity,
      key,
      is_add,
      field,
      fragments,
      tags,
      taggable_captured_source("Order", order.data, captured_tags(order.data)),
    )
  {
    Some(result) -> result
    None -> {
      let next_tags = updated_tags(captured_tags(order.data), tags, is_add)
      let next_order =
        OrderRecord(..order, data: replace_captured_tags(order.data, next_tags))
      let next_store = store.stage_order(store, next_order)
      mutation_result(
        key,
        tags_update_payload(
          next_store,
          is_add,
          taggable_captured_source("Order", next_order.data, next_tags),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        [next_order.id],
      )
    }
  }
}

fn apply_draft_order_tags_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  draft_order: DraftOrderRecord,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let tags = read_tag_inputs(args, is_add)
  case
    tags_validation_result_with_node(
      store,
      identity,
      key,
      is_add,
      field,
      fragments,
      tags,
      taggable_captured_source(
        "DraftOrder",
        draft_order.data,
        captured_tags(draft_order.data),
      ),
    )
  {
    Some(result) -> result
    None -> {
      let next_tags =
        updated_tags(captured_tags(draft_order.data), tags, is_add)
      let next_draft_order =
        DraftOrderRecord(
          ..draft_order,
          data: replace_captured_tags(draft_order.data, next_tags),
        )
      let next_store = store.stage_draft_order(store, next_draft_order)
      mutation_result(
        key,
        tags_update_payload(
          next_store,
          is_add,
          taggable_captured_source(
            "DraftOrder",
            next_draft_order.data,
            next_tags,
          ),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        [next_draft_order.id],
      )
    }
  }
}

fn apply_customer_tags_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  customer: CustomerRecord,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let tags = read_tag_inputs(args, is_add)
  case
    tags_validation_result_with_node(
      store,
      identity,
      key,
      is_add,
      field,
      fragments,
      tags,
      customer_serializers.customer_to_source(store, customer),
    )
  {
    Some(result) -> result
    None -> {
      let next_tags = updated_tags(customer.tags, tags, is_add)
      let next_customer = CustomerRecord(..customer, tags: next_tags)
      let #(_, next_store) = store.stage_update_customer(store, next_customer)
      mutation_result(
        key,
        tags_update_payload(
          next_store,
          is_add,
          customer_serializers.customer_to_source(next_store, next_customer),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        [next_customer.id],
      )
    }
  }
}

fn apply_article_tags_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  article: OnlineStoreContentRecord,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  args: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let tags = read_tag_inputs(args, is_add)
  case
    tags_validation_result_with_node(
      store,
      identity,
      key,
      is_add,
      field,
      fragments,
      tags,
      article_tag_source(store, article, captured_tags(article.data)),
    )
  {
    Some(result) -> result
    None -> {
      let next_tags = updated_tags(captured_tags(article.data), tags, is_add)
      let next_article =
        OnlineStoreContentRecord(
          ..article,
          data: replace_captured_tags(article.data, next_tags),
        )
      let #(_, next_store) =
        store.upsert_staged_online_store_content(store, next_article)
      mutation_result(
        key,
        tags_update_payload(
          next_store,
          is_add,
          article_tag_source(next_store, next_article, next_tags),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        [next_article.id],
      )
    }
  }
}

fn tags_validation_result_with_node(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  tags: List(String),
  node: SourceValue,
) -> Option(MutationFieldResult) {
  case product_tag_values_validation_error(tags) {
    Some(error) ->
      Some(
        mutation_result(
          key,
          tags_update_payload(store, is_add, node, [error], field, fragments),
          store,
          identity,
          [],
        ),
      )
    None ->
      case tags {
        [] ->
          Some(
            mutation_result(
              key,
              tags_update_payload(
                store,
                is_add,
                SrcNull,
                [
                  ProductUserError(
                    ["tags"],
                    "At least one tag is required",
                    None,
                  ),
                ],
                field,
                fragments,
              ),
              store,
              identity,
              [],
            ),
          )
        _ -> None
      }
  }
}

fn tags_update_not_found_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  message: String,
) -> MutationFieldResult {
  mutation_result(
    key,
    tags_update_payload(
      store,
      is_add,
      SrcNull,
      [ProductUserError(["id"], message, None)],
      field,
      fragments,
    ),
    store,
    identity,
    [],
  )
}

fn maybe_hydrate_article_for_tags(
  store_in: Store,
  id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(id)
    || option.is_some(store.get_effective_online_store_content_by_id(
      store_in,
      id,
    ))
  {
    True -> store_in
    False -> {
      let query =
        "query TagsArticleHydrate($id: ID!) {
  article(id: $id) {
    __typename
    id
    title
    handle
    tags
    createdAt
    updatedAt
    blog { id }
  }
}
"
      let variables = json.object([#("id", json.string(id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "TagsArticleHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_article_for_tags_response(store_in, value, id)
        Error(_) -> store_in
      }
    }
  }
}

fn hydrate_article_for_tags_response(
  store_in: Store,
  value: commit.JsonValue,
  fallback_id: String,
) -> Store {
  case orders_common.json_get(value, "data") {
    Some(data) ->
      case orders_common.json_get(data, "article") {
        Some(article) ->
          case
            orders_common.non_null_json(article)
            |> option.then(article_record_from_hydrate(_, fallback_id))
          {
            Some(record) ->
              store.upsert_base_online_store_content(store_in, [record])
            None -> store_in
          }
        None -> store_in
      }
    None -> store_in
  }
}

fn article_record_from_hydrate(
  node: commit.JsonValue,
  fallback_id: String,
) -> Option(OnlineStoreContentRecord) {
  let id =
    orders_common.json_get_string(node, "id")
    |> option.or(Some(fallback_id))
  use article_id <- option.then(id)
  Some(OnlineStoreContentRecord(
    id: article_id,
    kind: "article",
    cursor: None,
    parent_id: article_blog_id(node),
    created_at: orders_common.json_get_string(node, "createdAt"),
    updated_at: orders_common.json_get_string(node, "updatedAt"),
    data: orders_common.captured_json_from_commit(node),
  ))
}

fn article_blog_id(node: commit.JsonValue) -> Option(String) {
  use blog <- option.then(orders_common.json_get(node, "blog"))
  orders_common.json_get_string(blog, "id")
}

fn updated_tags(
  existing: List(String),
  input: List(String),
  is_add: Bool,
) -> List(String) {
  case is_add {
    True -> normalize_product_tags(list.append(existing, input))
    False ->
      normalize_product_tags(remove_product_tags_by_identity(existing, input))
  }
}

fn captured_tags(value: CapturedJsonValue) -> List(String) {
  case value {
    CapturedObject(fields) ->
      fields
      |> list.find_map(fn(pair) {
        case pair {
          #("tags", CapturedArray(values)) -> Ok(values)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
      |> option.unwrap([])
      |> list.filter_map(fn(value) {
        case value {
          CapturedString(tag) -> Ok(tag)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn replace_captured_tags(
  value: CapturedJsonValue,
  tags: List(String),
) -> CapturedJsonValue {
  let tag_value = CapturedArray(list.map(tags, CapturedString))
  case value {
    CapturedObject(fields) ->
      CapturedObject(upsert_captured_field(fields, "tags", tag_value))
    _ -> CapturedObject([#("tags", tag_value)])
  }
}

fn upsert_captured_field(
  fields: List(#(String, CapturedJsonValue)),
  key: String,
  value: CapturedJsonValue,
) -> List(#(String, CapturedJsonValue)) {
  case fields {
    [] -> [#(key, value)]
    [#(field_key, _), ..rest] if field_key == key -> [#(key, value), ..rest]
    [first, ..rest] -> [first, ..upsert_captured_field(rest, key, value)]
  }
}

fn taggable_captured_source(
  typename: String,
  data: CapturedJsonValue,
  tags: List(String),
) -> SourceValue {
  let base = case orders_common.captured_json_source(data) {
    SrcObject(fields) -> fields
    _ -> dict.new()
  }
  SrcObject(
    base
    |> dict.insert("__typename", SrcString(typename))
    |> dict.insert("tags", SrcList(list.map(tags, SrcString))),
  )
}

fn article_tag_source(
  store: Store,
  article: OnlineStoreContentRecord,
  tags: List(String),
) -> SourceValue {
  let base = case
    online_store_serializers.content_payload_source(store, article)
  {
    SrcObject(fields) -> fields
    _ -> dict.new()
  }
  SrcObject(
    base
    |> dict.insert("__typename", SrcString("Article"))
    |> dict.insert("tags", SrcList(list.map(tags, SrcString))),
  )
}

fn taggable_gid_type(id: String) -> Option(String) {
  case string.split(id, "/") {
    ["gid:", "", "shopify", type_name, tail] if tail != "" -> Some(type_name)
    ["gid:", "", "shopify", type_name, tail, ..] if tail != "" ->
      Some(type_name)
    _ -> None
  }
}

fn invalid_taggable_gid_error(
  _field: Selection,
  _id: String,
  is_add: Bool,
) -> Json {
  json.object([
    #("message", json.string("invalid id")),
    #("path", json.array([tags_update_root_name(is_add)], json.string)),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
  ])
}

// ===== from products_l14 =====
@internal
pub fn stage_product_set(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  existing: Option(ProductRecord),
  input: Dict(String, ResolvedValue),
  shopify_admin_origin: String,
  synchronous: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(product, identity_after_product) = case existing {
    Some(product) -> updated_product_record(identity, product, input)
    None -> {
      let #(created, next_identity) =
        created_product_record(store, identity, shopify_admin_origin, input)
      #(
        ProductRecord(
          ..created,
          online_store_preview_url: Some(make_product_preview_url(created)),
        ),
        next_identity,
      )
    }
  }
  let #(_, store_after_product) = store.upsert_staged_product(store, product)
  let #(store_after_graph, identity_after_graph, staged_ids) =
    apply_product_set_graph(
      store_after_product,
      identity_after_product,
      existing,
      product.id,
      input,
    )
  let #(synced_product, next_store, identity_after_summary) =
    sync_product_set_inventory_summary(
      store_after_graph,
      identity_after_graph,
      product.id,
      existing,
    )
  let product = synced_product |> option.unwrap(product)
  case synchronous {
    True ->
      mutation_result(
        key,
        product_set_payload(
          next_store,
          Some(product),
          None,
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity_after_summary,
        [product.id, ..staged_ids],
      )
    False -> {
      let #(operation_id, identity_after_operation) =
        synthetic_identity.make_synthetic_gid(
          identity_after_summary,
          "ProductSetOperation",
        )
      let operation =
        ProductOperationRecord(
          id: operation_id,
          type_name: "ProductSetOperation",
          product_id: Some(product.id),
          new_product_id: None,
          status: "COMPLETE",
          user_errors: [],
        )
      let #(staged_operation, store_after_operation) =
        store.stage_product_operation(next_store, operation)
      let initial_operation =
        ProductOperationRecord(
          ..staged_operation,
          product_id: None,
          status: "CREATED",
        )
      mutation_result(
        key,
        product_set_payload(
          store_after_operation,
          None,
          Some(initial_operation),
          [],
          field,
          variables,
          fragments,
        ),
        store_after_operation,
        identity_after_operation,
        [product.id, operation_id, ..staged_ids],
      )
    }
  }
}

@internal
pub fn stage_missing_async_product_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(operation_id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "ProductDuplicateOperation")
  let operation =
    ProductOperationRecord(
      id: operation_id,
      type_name: "ProductDuplicateOperation",
      product_id: None,
      new_product_id: None,
      status: "COMPLETE",
      user_errors: [
        ProductOperationUserErrorRecord(
          Some(["productId"]),
          "Product does not exist",
          None,
        ),
      ],
    )
  let #(staged_operation, next_store) =
    store.stage_product_operation(store, operation)
  let initial_operation =
    ProductOperationRecord(
      ..staged_operation,
      status: "CREATED",
      user_errors: [],
    )
  mutation_result(
    key,
    product_duplicate_payload(
      next_store,
      None,
      Some(initial_operation),
      [],
      field,
      variables,
      fragments,
    ),
    next_store,
    next_identity,
    [operation_id],
  )
}

@internal
pub fn stage_product_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  source_product: ProductRecord,
  new_title: Option(String),
  synchronous: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(duplicate_product, identity_after_product) =
    duplicated_product_record(store, identity, source_product, new_title)
  let #(_, store_after_product) =
    store.upsert_staged_product(store, duplicate_product)
  let #(store_after_relationships, identity_after_relationships, staged_ids) =
    duplicate_product_relationships(
      store_after_product,
      identity_after_product,
      product_id,
      duplicate_product.id,
    )
  case synchronous {
    True ->
      mutation_result(
        key,
        product_duplicate_payload(
          store_after_relationships,
          Some(duplicate_product),
          None,
          [],
          field,
          variables,
          fragments,
        ),
        store_after_relationships,
        identity_after_relationships,
        [duplicate_product.id, ..staged_ids],
      )
    False -> {
      let #(operation_id, identity_after_operation) =
        synthetic_identity.make_synthetic_gid(
          identity_after_relationships,
          "ProductDuplicateOperation",
        )
      let operation =
        ProductOperationRecord(
          id: operation_id,
          type_name: "ProductDuplicateOperation",
          product_id: Some(product_id),
          new_product_id: Some(duplicate_product.id),
          status: "COMPLETE",
          user_errors: [],
        )
      let #(staged_operation, next_store) =
        store.stage_product_operation(store_after_relationships, operation)
      let initial_operation =
        ProductOperationRecord(
          ..staged_operation,
          new_product_id: None,
          status: "CREATED",
        )
      mutation_result(
        key,
        product_duplicate_payload(
          next_store,
          None,
          Some(initial_operation),
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity_after_operation,
        [duplicate_product.id, operation_id, ..staged_ids],
      )
    }
  }
}

// ===== from products_l15 =====
@internal
pub fn handle_product_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_id = graphql_helpers.read_arg_string(args, "productId")
  let synchronous = case dict.get(args, "synchronous") {
    Ok(BoolVal(False)) -> False
    _ -> True
  }
  case product_id {
    None ->
      mutation_result(
        key,
        product_duplicate_payload(
          store,
          None,
          None,
          [
            ProductOperationUserErrorRecord(
              Some(["productId"]),
              "Product id is required",
              None,
            ),
          ],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          case synchronous {
            False ->
              stage_missing_async_product_duplicate(
                store,
                identity,
                key,
                field,
                variables,
                fragments,
              )
            True ->
              mutation_result(
                key,
                product_duplicate_payload(
                  store,
                  None,
                  None,
                  [
                    ProductOperationUserErrorRecord(
                      Some(["productId"]),
                      "Product not found",
                      None,
                    ),
                  ],
                  field,
                  variables,
                  fragments,
                ),
                store,
                identity,
                [],
              )
          }
        Some(source_product) ->
          stage_product_duplicate(
            store,
            identity,
            key,
            product_id,
            source_product,
            graphql_helpers.read_arg_string(args, "newTitle"),
            synchronous,
            field,
            variables,
            fragments,
          )
      }
  }
}

@internal
pub fn handle_product_set(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shopify_admin_origin: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_object(args, "input") {
    None ->
      mutation_result(
        key,
        product_set_payload(
          store,
          None,
          None,
          [
            ProductOperationUserErrorRecord(
              Some(["input"]),
              "Product input is required",
              None,
            ),
          ],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(input) -> {
      case product_set_max_input_size_errors(input) {
        [_, ..] as errors -> mutation_error_result(key, store, identity, errors)
        [] ->
          case product_set_shape_validation_errors(input) {
            [] ->
              case
                resolve_product_set_existing_product(
                  store,
                  graphql_helpers.read_arg_object(args, "identifier"),
                  input,
                )
              {
                Ok(existing) ->
                  case product_set_validation_errors(store, input, existing) {
                    [] ->
                      stage_product_set(
                        store,
                        identity,
                        key,
                        existing,
                        input,
                        shopify_admin_origin,
                        read_arg_bool_default_true(args, "synchronous"),
                        field,
                        variables,
                        fragments,
                      )
                    errors ->
                      mutation_rejected_result(
                        key,
                        product_set_payload(
                          store,
                          None,
                          None,
                          errors,
                          field,
                          variables,
                          fragments,
                        ),
                        store,
                        identity,
                      )
                  }
                Error(error) ->
                  mutation_rejected_result(
                    key,
                    product_set_payload(
                      store,
                      None,
                      None,
                      [error],
                      field,
                      variables,
                      fragments,
                    ),
                    store,
                    identity,
                  )
              }
            errors ->
              mutation_rejected_result(
                key,
                product_set_payload(
                  store,
                  None,
                  None,
                  errors,
                  field,
                  variables,
                  fragments,
                ),
                store,
                identity,
              )
          }
      }
    }
  }
}
