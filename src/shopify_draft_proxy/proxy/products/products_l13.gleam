//// Internal products-domain implementation split from proxy/products.gleam.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Location, type ObjectField, type Selection,
  type VariableDefinition, Argument, Directive, Field, InlineFragment, NullValue,
  ObjectField, ObjectValue, OperationDefinition, SelectionSet, StringValue,
  VariableDefinition, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, type RootFieldError, BoolVal, FloatVal, IntVal, ListVal,
  NullVal, ObjectVal, StringVal, get_field_arguments, get_root_fields,
}
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_field_value, project_graphql_value,
  serialize_connection, serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, RequiredArgument,
  build_null_argument_error, find_argument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/products/inventory_l05.{
  sync_product_inventory_summary,
}
import shopify_draft_proxy/proxy/products/products_l00.{
  is_valid_product_status, product_change_status_null_product_id_error,
  product_operation_user_error_source, tags_update_root_name,
}
import shopify_draft_proxy/proxy/products/products_l01.{
  product_tag_values_validation_error, product_tags_max_input_size_errors,
  read_tag_inputs,
}
import shopify_draft_proxy/proxy/products/products_l03.{
  remove_product_tags_by_identity,
}
import shopify_draft_proxy/proxy/products/products_l04.{normalize_product_tags}
import shopify_draft_proxy/proxy/products/products_l05.{
  created_product_record, product_update_validation_errors,
  updated_product_record,
}
import shopify_draft_proxy/proxy/products/products_l08.{
  product_create_validation_errors,
}
import shopify_draft_proxy/proxy/products/products_l11.{
  serialize_product_selection,
}
import shopify_draft_proxy/proxy/products/products_l12.{
  product_change_status_payload, product_create_payload,
  product_operation_source, product_update_payload, tags_update_payload,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_object_list_field, read_string_argument,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_error_result, mutation_rejected_result, mutation_result,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, MutationFieldResult,
  ProductUserError, RecomputeProductTotalInventory, product_user_error,
  product_user_error_code_product_not_found,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l04.{
  make_product_create_option_graph,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type ChannelRecord, type CollectionImageRecord,
  type CollectionRecord, type CollectionRuleRecord, type CollectionRuleSetRecord,
  type InventoryItemRecord, type InventoryLevelRecord,
  type InventoryLocationRecord, type InventoryMeasurementRecord,
  type InventoryQuantityRecord, type InventoryShipmentLineItemRecord,
  type InventoryShipmentRecord, type InventoryShipmentTrackingRecord,
  type InventoryTransferLineItemRecord,
  type InventoryTransferLocationSnapshotRecord, type InventoryTransferRecord,
  type InventoryWeightRecord, type InventoryWeightValue, type LocationRecord,
  type ProductCategoryRecord, type ProductCollectionRecord,
  type ProductFeedRecord, type ProductMediaRecord, type ProductMetafieldRecord,
  type ProductOperationRecord, type ProductOperationUserErrorRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductResourceFeedbackRecord, type ProductSeoRecord,
  type ProductVariantRecord, type ProductVariantSelectedOptionRecord,
  type PublicationRecord, type SellingPlanGroupRecord, type SellingPlanRecord,
  type ShopResourceFeedbackRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, CollectionRecord,
  CollectionRuleRecord, CollectionRuleSetRecord, InventoryItemRecord,
  InventoryLevelRecord, InventoryLocationRecord, InventoryMeasurementRecord,
  InventoryQuantityRecord, InventoryShipmentLineItemRecord,
  InventoryShipmentRecord, InventoryShipmentTrackingRecord,
  InventoryTransferLineItemRecord, InventoryTransferLocationSnapshotRecord,
  InventoryTransferRecord, InventoryWeightFloat, InventoryWeightInt,
  InventoryWeightRecord, LocationRecord, ProductCollectionRecord,
  ProductFeedRecord, ProductMediaRecord, ProductMetafieldRecord,
  ProductOperationRecord, ProductOperationUserErrorRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductResourceFeedbackRecord,
  ProductSeoRecord, ProductVariantRecord, ProductVariantSelectedOptionRecord,
  PublicationRecord, SellingPlanGroupRecord, SellingPlanRecord,
  ShopResourceFeedbackRecord,
}

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
              None,
              [ProductUserError(["id"], "Product id is required", None)],
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
                tags_update_payload(
                  store,
                  is_add,
                  None,
                  [ProductUserError(["id"], "Product not found", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(product) -> {
              let tags = read_tag_inputs(args, is_add)
              case product_tag_values_validation_error(tags) {
                Some(error) ->
                  mutation_result(
                    key,
                    tags_update_payload(
                      store,
                      is_add,
                      Some(product),
                      [error],
                      field,
                      fragments,
                    ),
                    store,
                    identity,
                    [],
                  )
                None ->
                  case tags {
                    [] ->
                      mutation_result(
                        key,
                        tags_update_payload(
                          store,
                          is_add,
                          None,
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
                      )
                    _ -> {
                      let next_tags = case is_add {
                        True ->
                          normalize_product_tags(list.append(product.tags, tags))
                        False ->
                          normalize_product_tags(
                            remove_product_tags_by_identity(product.tags, tags),
                          )
                      }
                      let next_product =
                        ProductRecord(..product, tags: next_tags)
                      let #(_, next_store) =
                        store.upsert_staged_product(store, next_product)
                      mutation_result(
                        key,
                        tags_update_payload(
                          next_store,
                          is_add,
                          Some(next_product),
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
            }
          }
      }
  }
}
