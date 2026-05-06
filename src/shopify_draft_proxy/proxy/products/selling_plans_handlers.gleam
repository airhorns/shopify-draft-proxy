//// Products-domain submodule: selling_plans_handlers.
//// Combines layered files: selling_plans_l10, selling_plans_l12, selling_plans_l13, selling_plans_l14, selling_plans_l15, selling_plans_l16.

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
  Variable, VariableDefinition, VariableValue,
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
import shopify_draft_proxy/proxy/products/products_core.{
  serialize_product_user_errors_json,
}
import shopify_draft_proxy/proxy/products/products_handlers.{
  serialize_product_list_connection, serialize_product_selection,
}
import shopify_draft_proxy/proxy/products/selling_plans_core.{
  find_selling_plan, make_selling_plan_group_record,
  product_variant_count_for_selling_plan_group, selling_plan_group_cursor,
  selling_plan_group_does_not_exist_error, selling_plan_group_staged_ids,
  serialize_selling_plans_connection,
  update_product_selling_plan_group_membership,
  update_variant_selling_plan_group_membership,
}
import shopify_draft_proxy/proxy/products/shared.{
  captured_object_field, dedupe_preserving_order, mutation_error_result,
  mutation_rejected_result, mutation_result, read_arg_string_list,
  read_int_field, read_list_field_length, read_object_field,
  read_object_list_field, read_string_argument, read_string_field,
  read_string_list_field, serialize_exact_count,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, MutationFieldResult,
  ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_helpers.{
  option_to_result, optional_int_json, optional_string,
  optional_string_list_json, optional_string_value_json, product_variant_cursor,
}
import shopify_draft_proxy/proxy/products/variants_sources.{
  serialize_product_variant_object,
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

// ===== from selling_plans_l10 =====
@internal
pub fn serialize_selling_plan_group_variants_connection(
  store: Store,
  group: SellingPlanGroupRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let product_id = read_string_argument(field, variables, "productId")
  let variants =
    group.product_variant_ids
    |> list.filter_map(fn(variant_id) {
      store.get_effective_variant_by_id(store, variant_id) |> option_to_result
    })
    |> list.filter(fn(variant) {
      case product_id {
        Some(id) -> variant.product_id == id
        None -> True
      }
    })
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

// ===== from selling_plans_l12 =====
@internal
pub fn product_selling_plan_group_mutation_payload(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  product: Option(ProductRecord),
  variant: Option(ProductVariantRecord),
  user_errors: List(ProductUserError),
) -> Json {
  json.object(
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "product" ->
              case product {
                Some(product) ->
                  serialize_product_selection(
                    store,
                    product,
                    selection,
                    variables,
                    fragments,
                  )
                None -> json.null()
              }
            "productVariant" ->
              case variant {
                Some(variant) ->
                  serialize_product_variant_object(
                    store,
                    variant,
                    get_selected_child_fields(
                      selection,
                      default_selected_field_options(),
                    ),
                    selection,
                    variables,
                    fragments,
                  )
                None -> json.null()
              }
            "userErrors" ->
              serialize_product_user_errors_json(user_errors, selection)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

// ===== from selling_plans_l13 =====
@internal
pub fn serialize_selling_plan_group_products_connection(
  store: Store,
  group: SellingPlanGroupRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let products =
    group.product_ids
    |> list.filter_map(fn(product_id) {
      store.get_effective_product_by_id(store, product_id) |> option_to_result
    })
  serialize_product_list_connection(
    products,
    store,
    field,
    variables,
    fragments,
  )
}

@internal
pub fn handle_product_selling_plan_group_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "productJoinSellingPlanGroups" | "productLeaveSellingPlanGroups" -> {
      let product_id = graphql_helpers.read_arg_string(args, "id")
      case
        product_id
        |> option.then(fn(id) { store.get_effective_product_by_id(store, id) })
      {
        None -> {
          let payload =
            product_selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              None,
              [
                ProductUserError(
                  ["id"],
                  "Product does not exist.",
                  Some("PRODUCT_DOES_NOT_EXIST"),
                ),
              ],
            )
          mutation_rejected_result(key, payload, store, identity)
        }
        Some(product) -> {
          let #(next_store, errors, staged_ids) =
            update_product_selling_plan_group_membership(
              store,
              product.id,
              read_arg_string_list(args, "sellingPlanGroupIds"),
              root_name == "productJoinSellingPlanGroups",
            )
          let payload =
            product_selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              store.get_effective_product_by_id(next_store, product.id),
              None,
              errors,
            )
          case errors {
            [_, ..] ->
              mutation_rejected_result(key, payload, next_store, identity)
            [] ->
              mutation_result(key, payload, next_store, identity, staged_ids)
          }
        }
      }
    }
    "productVariantJoinSellingPlanGroups"
    | "productVariantLeaveSellingPlanGroups" -> {
      let variant_id = graphql_helpers.read_arg_string(args, "id")
      case
        variant_id
        |> option.then(fn(id) { store.get_effective_variant_by_id(store, id) })
      {
        None -> {
          let payload =
            product_selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              None,
              [
                ProductUserError(
                  ["id"],
                  "Product variant does not exist.",
                  Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ),
              ],
            )
          mutation_rejected_result(key, payload, store, identity)
        }
        Some(variant) -> {
          let #(next_store, errors, staged_ids) =
            update_variant_selling_plan_group_membership(
              store,
              variant.id,
              read_arg_string_list(args, "sellingPlanGroupIds"),
              root_name == "productVariantJoinSellingPlanGroups",
            )
          let payload =
            product_selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              None,
              store.get_effective_variant_by_id(next_store, variant.id),
              errors,
            )
          case errors {
            [_, ..] ->
              mutation_rejected_result(key, payload, next_store, identity)
            [] ->
              mutation_result(key, payload, next_store, identity, staged_ids)
          }
        }
      }
    }
    _ -> mutation_result(key, json.null(), store, identity, [])
  }
}

// ===== from selling_plans_l14 =====
@internal
pub fn serialize_selling_plan_group_object(
  store: Store,
  group: SellingPlanGroupRecord,
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "__typename" -> json.string("SellingPlanGroup")
            "id" -> json.string(group.id)
            "appId" -> optional_string(group.app_id)
            "name" -> json.string(group.name)
            "merchantCode" -> json.string(group.merchant_code)
            "description" -> optional_string(group.description)
            "options" -> json.array(group.options, json.string)
            "position" -> optional_int_json(group.position)
            "summary" -> optional_string(group.summary)
            "createdAt" -> optional_string(group.created_at)
            "productsCount" ->
              serialize_exact_count(selection, list.length(group.product_ids))
            "productVariantsCount" ->
              serialize_exact_count(
                selection,
                product_variant_count_for_selling_plan_group(
                  store,
                  group,
                  selection,
                  variables,
                ),
              )
            "appliesToProduct" ->
              json.bool(
                case read_string_argument(selection, variables, "productId") {
                  Some(product_id) ->
                    list.contains(group.product_ids, product_id)
                  None -> False
                },
              )
            "appliesToProductVariant" ->
              json.bool(
                case
                  read_string_argument(selection, variables, "productVariantId")
                {
                  Some(variant_id) ->
                    list.contains(group.product_variant_ids, variant_id)
                  None -> False
                },
              )
            "appliesToProductVariants" ->
              json.bool(
                case read_string_argument(selection, variables, "productId") {
                  Some(product_id) ->
                    list.any(group.product_variant_ids, fn(variant_id) {
                      case
                        store.get_effective_variant_by_id(store, variant_id)
                      {
                        Some(variant) -> variant.product_id == product_id
                        None -> False
                      }
                    })
                  None -> False
                },
              )
            "products" ->
              serialize_selling_plan_group_products_connection(
                store,
                group,
                selection,
                variables,
                fragments,
              )
            "productVariants" ->
              serialize_selling_plan_group_variants_connection(
                store,
                group,
                selection,
                variables,
                fragments,
              )
            "sellingPlans" ->
              serialize_selling_plans_connection(
                group.selling_plans,
                selection,
                variables,
                fragments,
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

// ===== from selling_plans_l15 =====
@internal
pub fn serialize_selling_plan_group_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_selling_plan_group_by_id(store, id) {
        Some(group) ->
          serialize_selling_plan_group_object(
            store,
            group,
            get_selected_child_fields(field, default_selected_field_options()),
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_selling_plan_groups_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let groups = store.list_effective_selling_plan_groups(store)
  case groups {
    [] -> serialize_empty_connection(field, default_selected_field_options())
    _ -> {
      let window =
        paginate_connection_items(
          groups,
          field,
          variables,
          selling_plan_group_cursor,
          default_connection_window_options(),
        )
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: window.items,
          has_next_page: window.has_next_page,
          has_previous_page: window.has_previous_page,
          get_cursor_value: selling_plan_group_cursor,
          serialize_node: fn(group, node_field, _index) {
            serialize_selling_plan_group_object(
              store,
              group,
              get_selected_child_fields(
                node_field,
                default_selected_field_options(),
              ),
              variables,
              fragments,
            )
          },
          selected_field_options: default_selected_field_options(),
          page_info_options: default_connection_page_info_options(),
        ),
      )
    }
  }
}

@internal
pub fn selling_plan_group_mutation_payload(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  group: Option(SellingPlanGroupRecord),
  user_errors: List(ProductUserError),
  deleted_selling_plan_ids: Option(Option(List(String))),
  deleted_selling_plan_group_id: Option(Option(String)),
  removed_product_ids: Option(Option(List(String))),
  removed_product_variant_ids: Option(Option(List(String))),
) -> Json {
  json.object(
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "sellingPlanGroup" ->
              case group {
                Some(group) ->
                  serialize_selling_plan_group_object(
                    store,
                    group,
                    get_selected_child_fields(
                      selection,
                      default_selected_field_options(),
                    ),
                    variables,
                    fragments,
                  )
                None -> json.null()
              }
            "userErrors" ->
              serialize_product_user_errors_json(user_errors, selection)
            "deletedSellingPlanIds" ->
              optional_string_list_json(deleted_selling_plan_ids)
            "deletedSellingPlanGroupId" ->
              optional_string_value_json(deleted_selling_plan_group_id)
            "removedProductIds" ->
              optional_string_list_json(removed_product_ids)
            "removedProductVariantIds" ->
              optional_string_list_json(removed_product_variant_ids)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

// ===== from selling_plans_l16 =====
@internal
pub fn handle_selling_plan_group_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "sellingPlanGroupCreate" -> {
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let resources =
        graphql_helpers.read_arg_object(args, "resources")
        |> option.unwrap(dict.new())
      case selling_plan_group_input_errors(input, None) {
        [_, ..] as user_errors ->
          mutation_rejected_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              user_errors,
              None,
              None,
              None,
              None,
            ),
            store,
            identity,
          )
        [] -> {
          let #(group, next_identity) =
            make_selling_plan_group_record(identity, input, None, resources)
          let #(_, next_store) =
            store.upsert_staged_selling_plan_group(store, group)
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              Some(group),
              [],
              None,
              None,
              None,
              None,
            ),
            next_store,
            next_identity,
            selling_plan_group_staged_ids(group),
          )
        }
      }
    }
    "sellingPlanGroupUpdate" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              Some(None),
              None,
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(existing) -> {
          let input =
            graphql_helpers.read_arg_object(args, "input")
            |> option.unwrap(dict.new())
          case selling_plan_group_input_errors(input, Some(existing)) {
            [_, ..] as user_errors ->
              mutation_rejected_result(
                key,
                selling_plan_group_mutation_payload(
                  store,
                  field,
                  variables,
                  fragments,
                  None,
                  user_errors,
                  Some(None),
                  None,
                  None,
                  None,
                ),
                store,
                identity,
              )
            [] -> {
              let deleted_plan_ids =
                read_string_list_field(input, "sellingPlansToDelete")
                |> option.unwrap([])
                |> list.filter(fn(plan_id) {
                  list.any(existing.selling_plans, fn(plan) {
                    plan.id == plan_id
                  })
                })
              let #(group, next_identity) =
                make_selling_plan_group_record(
                  identity,
                  input,
                  Some(existing),
                  dict.new(),
                )
              let #(_, next_store) =
                store.upsert_staged_selling_plan_group(store, group)
              mutation_result(
                key,
                selling_plan_group_mutation_payload(
                  next_store,
                  field,
                  variables,
                  fragments,
                  Some(group),
                  [],
                  Some(Some(deleted_plan_ids)),
                  None,
                  None,
                  None,
                ),
                next_store,
                next_identity,
                selling_plan_group_staged_ids(group),
              )
            }
          }
        }
      }
    }
    "sellingPlanGroupDelete" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              Some(None),
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          let next_store =
            store.delete_staged_selling_plan_group(store, group.id)
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              None,
              [],
              None,
              Some(Some(group.id)),
              None,
              None,
            ),
            next_store,
            identity,
            [group.id],
          )
        }
      }
    }
    "sellingPlanGroupAddProducts" | "sellingPlanGroupAddProductVariants" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              None,
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          let errors = case root_name {
            "sellingPlanGroupAddProducts" ->
              selling_plan_group_add_product_errors(
                store,
                group,
                read_arg_string_list(args, "productIds"),
              )
            _ ->
              selling_plan_group_add_variant_errors(
                store,
                group,
                read_arg_string_list(args, "productVariantIds"),
              )
          }
          case errors {
            [_, ..] ->
              mutation_rejected_result(
                key,
                selling_plan_group_mutation_payload(
                  store,
                  field,
                  variables,
                  fragments,
                  None,
                  errors,
                  None,
                  None,
                  None,
                  None,
                ),
                store,
                identity,
              )
            [] -> {
              let next_group = case root_name {
                "sellingPlanGroupAddProducts" ->
                  SellingPlanGroupRecord(
                    ..group,
                    product_ids: dedupe_preserving_order(list.append(
                      group.product_ids,
                      read_arg_string_list(args, "productIds"),
                    )),
                  )
                _ ->
                  SellingPlanGroupRecord(
                    ..group,
                    product_variant_ids: dedupe_preserving_order(list.append(
                      group.product_variant_ids,
                      read_arg_string_list(args, "productVariantIds"),
                    )),
                  )
              }
              let #(_, next_store) =
                store.upsert_staged_selling_plan_group(store, next_group)
              mutation_result(
                key,
                selling_plan_group_mutation_payload(
                  next_store,
                  field,
                  variables,
                  fragments,
                  Some(next_group),
                  [],
                  None,
                  None,
                  None,
                  None,
                ),
                next_store,
                identity,
                [next_group.id],
              )
            }
          }
        }
      }
    }
    "sellingPlanGroupRemoveProducts"
    | "sellingPlanGroupRemoveProductVariants" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              None,
              case root_name {
                "sellingPlanGroupRemoveProducts" -> Some(None)
                _ -> None
              },
              case root_name {
                "sellingPlanGroupRemoveProductVariants" -> Some(None)
                _ -> None
              },
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          case root_name {
            "sellingPlanGroupRemoveProducts" -> {
              let requested = read_arg_string_list(args, "productIds")
              case first_malformed_shopify_gid(requested, "Product") {
                Some(invalid) ->
                  mutation_error_result(key, store, identity, [
                    invalid_gid_list_variable_error(
                      remove_resource_variable_name(field, "productIds"),
                      "[ID!]!",
                      invalid.0,
                      invalid.1,
                    ),
                  ])
                None -> {
                  let removed =
                    group.product_ids
                    |> list.filter(fn(product_id) {
                      list.contains(requested, product_id)
                    })
                  let next_group =
                    SellingPlanGroupRecord(
                      ..group,
                      product_ids: group.product_ids
                        |> list.filter(fn(product_id) {
                          !list.contains(requested, product_id)
                        }),
                    )
                  let #(_, next_store) =
                    store.upsert_staged_selling_plan_group(store, next_group)
                  mutation_result(
                    key,
                    selling_plan_group_mutation_payload(
                      next_store,
                      field,
                      variables,
                      fragments,
                      None,
                      [],
                      None,
                      None,
                      Some(Some(removed)),
                      None,
                    ),
                    next_store,
                    identity,
                    [next_group.id],
                  )
                }
              }
            }
            _ -> {
              let requested = read_arg_string_list(args, "productVariantIds")
              case first_malformed_shopify_gid(requested, "ProductVariant") {
                Some(invalid) ->
                  mutation_error_result(key, store, identity, [
                    invalid_gid_list_variable_error(
                      remove_resource_variable_name(field, "productVariantIds"),
                      "[ID!]!",
                      invalid.0,
                      invalid.1,
                    ),
                  ])
                None -> {
                  let removed =
                    group.product_variant_ids
                    |> list.filter(fn(variant_id) {
                      list.contains(requested, variant_id)
                    })
                  let next_group =
                    SellingPlanGroupRecord(
                      ..group,
                      product_variant_ids: group.product_variant_ids
                        |> list.filter(fn(variant_id) {
                          !list.contains(requested, variant_id)
                        }),
                    )
                  let #(_, next_store) =
                    store.upsert_staged_selling_plan_group(store, next_group)
                  mutation_result(
                    key,
                    selling_plan_group_mutation_payload(
                      next_store,
                      field,
                      variables,
                      fragments,
                      None,
                      [],
                      None,
                      None,
                      None,
                      Some(Some(removed)),
                    ),
                    next_store,
                    identity,
                    [next_group.id],
                  )
                }
              }
            }
          }
        }
      }
    }
    _ -> mutation_result(key, json.null(), store, identity, [])
  }
}

fn selling_plan_group_add_product_errors(
  store: Store,
  group: SellingPlanGroupRecord,
  requested: List(String),
) -> List(ProductUserError) {
  let unknown =
    requested
    |> list.filter(fn(product_id) {
      case store.get_effective_product_by_id(store, product_id) {
        Some(_) -> False
        None -> True
      }
    })
  case unknown {
    [_, ..] ->
      list.map(unknown, fn(product_id) {
        ProductUserError(
          ["productIds"],
          "Product " <> product_id <> " does not exist.",
          Some("NOT_FOUND"),
        )
      })
    [] ->
      case
        requested != [] && all_ids_are_members(requested, group.product_ids)
      {
        True -> [resource_already_taken_error(["productIds"])]
        False -> []
      }
  }
}

fn selling_plan_group_add_variant_errors(
  store: Store,
  group: SellingPlanGroupRecord,
  requested: List(String),
) -> List(ProductUserError) {
  let unknown =
    requested
    |> list.filter(fn(variant_id) {
      case store.get_effective_variant_by_id(store, variant_id) {
        Some(_) -> False
        None -> True
      }
    })
  case unknown {
    [_, ..] ->
      list.map(unknown, fn(variant_id) {
        ProductUserError(
          ["productVariantIds"],
          "Product variant " <> variant_id <> " does not exist.",
          Some("NOT_FOUND"),
        )
      })
    [] ->
      case
        requested != []
        && all_ids_are_members(requested, group.product_variant_ids)
      {
        True -> [resource_already_taken_error(["productVariantIds"])]
        False -> []
      }
  }
}

fn resource_already_taken_error(field: List(String)) -> ProductUserError {
  ProductUserError(field, "Resource has already been taken", Some("TAKEN"))
}

fn all_ids_are_members(requested: List(String), members: List(String)) -> Bool {
  list.all(requested, fn(id) { list.contains(members, id) })
}

fn first_malformed_shopify_gid(
  ids: List(String),
  resource_type: String,
) -> Option(#(String, Int)) {
  ids
  |> list.index_map(fn(id, index) { #(id, index) })
  |> list.find(fn(indexed) { !is_shopify_gid_type(indexed.0, resource_type) })
  |> option.from_result
}

fn is_shopify_gid_type(id: String, resource_type: String) -> Bool {
  string.starts_with(id, "gid://shopify/" <> resource_type <> "/")
}

fn remove_resource_variable_name(
  field: Selection,
  argument_name: String,
) -> String {
  case field {
    Field(arguments: arguments, ..) ->
      arguments
      |> list.find_map(fn(argument) {
        case argument {
          Argument(name: name, value: VariableValue(variable), ..)
            if name.value == argument_name
          -> {
            let Variable(name: variable_name, ..) = variable
            Ok(variable_name.value)
          }
          _ -> Error(Nil)
        }
      })
      |> result.unwrap(argument_name)
    _ -> argument_name
  }
}

fn invalid_gid_list_variable_error(
  variable_name: String,
  variable_type: String,
  value: String,
  index: Int,
) -> Json {
  let problem = "Invalid global id '" <> value <> "'"
  json.object([
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type "
        <> variable_type
        <> " was provided invalid value for "
        <> int.to_string(index)
        <> " ("
        <> problem
        <> ")",
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", json.array([value], json.string)),
        #(
          "problems",
          json.preprocessed_array([
            json.object([
              #("path", json.preprocessed_array([json.int(index)])),
              #("explanation", json.string(problem)),
              #("message", json.string(problem)),
            ]),
          ]),
        ),
      ]),
    ),
  ])
}

fn selling_plan_group_input_errors(
  input: Dict(String, ResolvedValue),
  existing: Option(SellingPlanGroupRecord),
) -> List(ProductUserError) {
  let existing_plans = case existing {
    Some(group) -> group.selling_plans
    None -> []
  }
  list.append(
    selling_plan_group_scalar_errors(input),
    list.append(
      selling_plan_input_list_errors(
        read_object_list_field(input, "sellingPlansToCreate"),
        ["input", "sellingPlansToCreate"],
        "create",
        False,
        [],
      ),
      selling_plan_input_list_errors(
        read_object_list_field(input, "sellingPlansToUpdate"),
        ["input", "sellingPlansToUpdate"],
        "update",
        True,
        existing_plans,
      ),
    ),
  )
}

fn selling_plan_group_scalar_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  let option_errors = case read_list_field_length(input, "options") {
    Some(count) if count > 3 -> [
      ProductUserError(
        ["input", "options"],
        "Too many selling plan group options (maximum 3 options)",
        Some("TOO_LONG"),
      ),
    ]
    _ -> []
  }
  let position_errors = case read_int_field(input, "position") {
    Some(position) if position < -2_147_483_648 || position > 2_147_483_647 -> [
      ProductUserError(
        ["input", "position"],
        int32_position_message(),
        Some("INVALID"),
      ),
    ]
    _ -> []
  }
  list.append(option_errors, position_errors)
}

fn selling_plan_input_list_errors(
  plans: List(Dict(String, ResolvedValue)),
  field_prefix: List(String),
  action: String,
  require_id: Bool,
  existing_plans: List(SellingPlanRecord),
) -> List(ProductUserError) {
  selling_plan_input_list_errors_loop(
    plans,
    field_prefix,
    action,
    require_id,
    existing_plans,
    0,
  )
}

fn selling_plan_input_list_errors_loop(
  plans: List(Dict(String, ResolvedValue)),
  field_prefix: List(String),
  action: String,
  require_id: Bool,
  existing_plans: List(SellingPlanRecord),
  index: Int,
) -> List(ProductUserError) {
  case plans {
    [] -> []
    [plan, ..rest] -> {
      let existing_plan = case require_id, read_string_field(plan, "id") {
        True, Some(plan_id) -> find_selling_plan(existing_plans, plan_id)
        _, _ -> None
      }
      list.append(
        selling_plan_input_errors(
          plan,
          list.append(field_prefix, [int.to_string(index)]),
          action,
          require_id,
          existing_plan,
        ),
        selling_plan_input_list_errors_loop(
          rest,
          field_prefix,
          action,
          require_id,
          existing_plans,
          index + 1,
        ),
      )
    }
  }
}

fn selling_plan_input_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
  action: String,
  require_id: Bool,
  existing_plan: Option(SellingPlanRecord),
) -> List(ProductUserError) {
  let id_errors = case require_id, read_string_field(input, "id") {
    True, None -> [
      ProductUserError(
        list.append(field_prefix, ["id"]),
        "Id must be specificed to update a Selling Plan.",
        Some("PLAN_ID_MUST_BE_SPECIFIED_TO_UPDATE"),
      ),
    ]
    _, _ -> []
  }
  let option_errors = case read_list_field_length(input, "options") {
    Some(count) if count > 3 -> [
      ProductUserError(
        list.append(field_prefix, ["options"]),
        "Too many selling plan options (maximum 3 options)",
        Some("TOO_LONG"),
      ),
    ]
    _ -> []
  }
  let pricing_policy_errors = case
    read_list_field_length(input, "pricingPolicies")
  {
    Some(count) if count > 2 -> [
      ProductUserError(
        list.append(field_prefix, ["pricingPolicies"]),
        "Selling plans to "
          <> action
          <> " pricing policies can't have more than 2 pricing policies",
        Some("SELLING_PLAN_PRICING_POLICIES_LIMIT"),
      ),
    ]
    _ -> []
  }
  let position_errors = case read_int_field(input, "position") {
    Some(position) if position < -2_147_483_648 || position > 2_147_483_647 -> [
      ProductUserError(
        list.append(field_prefix, ["position"]),
        int32_position_message(),
        Some("INVALID"),
      ),
    ]
    _ -> []
  }
  let delivery_policy_errors =
    delivery_policy_update_union_errors(input, field_prefix, existing_plan)
  let policy_errors = case delivery_policy_errors {
    [] ->
      case
        input_policy_kind(input, "billingPolicy"),
        input_policy_kind(input, "deliveryPolicy")
      {
        Some(billing_kind), Some(delivery_kind)
          if billing_kind != delivery_kind
        -> [
          ProductUserError(
            field_prefix,
            "billing and delivery policy types must be the same.",
            Some("BILLING_AND_DELIVERY_POLICY_TYPES_MUST_BE_THE_SAME"),
          ),
        ]
        _, _ -> []
      }
    _ -> []
  }
  list.append(
    id_errors,
    list.append(
      option_errors,
      list.append(
        pricing_policy_errors,
        list.append(
          position_errors,
          list.append(delivery_policy_errors, policy_errors),
        ),
      ),
    ),
  )
}

fn delivery_policy_update_union_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
  existing_plan: Option(SellingPlanRecord),
) -> List(ProductUserError) {
  case
    input_policy_kind(input, "deliveryPolicy"),
    existing_plan
    |> option.then(fn(plan) {
      captured_policy_kind(plan.data, "deliveryPolicy")
    })
  {
    Some(input_kind), Some(existing_kind) if input_kind != existing_kind -> [
      ProductUserError(
        field_prefix,
        "Only one of fixed or recurring delivery policy is allowed",
        Some("ONLY_ONE_OF_FIXED_OR_RECURRING_DELIVERY"),
      ),
    ]
    _, _ -> []
  }
}

fn input_policy_kind(
  input: Dict(String, ResolvedValue),
  field_name: String,
) -> Option(String) {
  case read_object_field(input, field_name) {
    Some(policy) ->
      case
        read_object_field(policy, "fixed"),
        read_object_field(policy, "recurring")
      {
        Some(_), None -> Some("fixed")
        None, Some(_) -> Some("recurring")
        _, _ -> None
      }
    None -> None
  }
}

fn captured_policy_kind(
  value: CapturedJsonValue,
  field_name: String,
) -> Option(String) {
  case captured_object_field(value, field_name) {
    Some(policy) ->
      case captured_object_field(policy, "__typename") {
        Some(CapturedString("SellingPlanFixedBillingPolicy"))
        | Some(CapturedString("SellingPlanFixedDeliveryPolicy")) ->
          Some("fixed")
        Some(CapturedString("SellingPlanRecurringBillingPolicy"))
        | Some(CapturedString("SellingPlanRecurringDeliveryPolicy")) ->
          Some("recurring")
        _ -> None
      }
    None -> None
  }
}

fn int32_position_message() -> String {
  "Position must be within the range of -2,147,483,648 to 2,147,483,647"
}
