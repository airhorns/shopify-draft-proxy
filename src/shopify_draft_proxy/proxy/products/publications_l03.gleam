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
import shopify_draft_proxy/proxy/products/products_l00.{
  json_string_array_literal,
}
import shopify_draft_proxy/proxy/products/publications_l01.{
  missing_variant_relationship_ids,
}
import shopify_draft_proxy/proxy/products/publications_l02.{
  product_bundle_mutation_payload, product_feed_create_payload,
  product_feed_delete_payload, product_full_sync_payload,
  product_resource_feedback_source,
  product_variant_relationship_bulk_update_payload,
  shop_resource_feedback_source,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_arg_object_list, read_int_field, read_object_field,
  read_object_list_field, read_string_argument, read_string_field,
  read_string_list_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_rejected_result, mutation_result, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type NullableFieldUserError, type ProductUserError,
  MutationFieldResult, NullableFieldUserError, ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid, make_synthetic_gid,
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
pub fn serialize_product_resource_feedback_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_resource_feedback(store, id) {
        Some(feedback) ->
          project_graphql_value(
            product_resource_feedback_source(feedback),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn handle_product_feed_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  // The captured local-runtime fixture comes from the TS path where the
  // mutation-log entry consumes the first synthetic id before the feed is
  // minted, so preserve that observable id sequence for this staged root.
  let #(_, identity_after_log_slot) =
    synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
  let #(feed_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_log_slot,
      "ProductFeed",
    )
  let feed =
    ProductFeedRecord(
      id: feed_id,
      country: read_string_field(input, "country"),
      language: read_string_field(input, "language"),
      status: "ACTIVE",
    )
  let #(staged_feed, next_store) = store.upsert_staged_product_feed(store, feed)
  mutation_result(
    key,
    product_feed_create_payload(staged_feed, [], field, fragments),
    next_store,
    next_identity,
    [staged_feed.id],
  )
}

@internal
pub fn handle_product_feed_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  case id {
    Some(feed_id) ->
      case store.get_effective_product_feed_by_id(store, feed_id) {
        Some(_) -> {
          let next_store = store.delete_staged_product_feed(store, feed_id)
          mutation_result(
            key,
            product_feed_delete_payload(Some(feed_id), [], field, fragments),
            next_store,
            identity,
            [feed_id],
          )
        }
        None ->
          mutation_result(
            key,
            product_feed_delete_payload(
              None,
              [
                ProductUserError(["id"], "ProductFeed does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      mutation_result(
        key,
        product_feed_delete_payload(
          None,
          [ProductUserError(["id"], "ProductFeed does not exist", None)],
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
pub fn handle_product_full_sync(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  case id {
    Some(feed_id) ->
      case store.get_effective_product_feed_by_id(store, feed_id) {
        Some(_) ->
          mutation_result(
            key,
            product_full_sync_payload(Some(feed_id), [], field, fragments),
            store,
            identity,
            [feed_id],
          )
        None ->
          mutation_result(
            key,
            product_full_sync_payload(
              None,
              [
                ProductUserError(["id"], "ProductFeed does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      mutation_result(
        key,
        product_full_sync_payload(
          None,
          [ProductUserError(["id"], "ProductFeed does not exist", None)],
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
pub fn handle_product_bundle_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let product_id = read_string_field(input, "productId")
  let existing_product = case product_id {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None -> None
  }
  let components = read_object_list_field(input, "components")
  let user_errors = case root_name, product_id, existing_product {
    "productBundleUpdate", _, None -> [
      NullableFieldUserError(None, "Product does not exist"),
    ]
    _, _, _ -> {
      case components {
        [] -> [
          NullableFieldUserError(None, "At least one component is required."),
        ]
        _ -> validate_product_bundle_components(store, input, components)
      }
    }
  }
  case user_errors {
    [] -> {
      let #(operation_id, next_identity) =
        make_synthetic_gid(identity, "ProductBundleOperation")
      let completed_operation =
        ProductOperationRecord(
          id: operation_id,
          type_name: "ProductBundleOperation",
          product_id: None,
          new_product_id: None,
          status: "ACTIVE",
          user_errors: [],
        )
      let #(staged_operation, next_store) =
        store.stage_product_operation(store, completed_operation)
      let initial_operation =
        ProductOperationRecord(..staged_operation, status: "CREATED")
      mutation_result(
        key,
        product_bundle_mutation_payload(
          root_name,
          Some(initial_operation),
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        [operation_id],
      )
    }
    _ ->
      mutation_rejected_result(
        key,
        product_bundle_mutation_payload(
          root_name,
          None,
          user_errors,
          field,
          fragments,
        ),
        store,
        identity,
      )
  }
}

const product_bundle_quantity_max = 2000

fn validate_product_bundle_components(
  store: Store,
  input: Dict(String, ResolvedValue),
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let missing_product_tails =
    components
    |> list.filter_map(fn(component) {
      case read_string_field(component, "productId") {
        Some(id) ->
          case store.get_effective_product_by_id(store, id) {
            Some(_) -> Error(Nil)
            None -> Ok(resource_id_tail(id))
          }
        None -> Error(Nil)
      }
    })
  case missing_product_tails {
    [] -> {
      list.append(
        product_bundle_option_mapping_errors(store, components),
        list.append(
          product_bundle_quantity_errors(components),
          list.append(
            product_bundle_quantity_option_errors(components),
            product_bundle_consolidated_option_errors(input, components),
          ),
        ),
      )
    }
    _ -> [
      NullableFieldUserError(
        None,
        "Failed to locate the following products: "
          <> numeric_id_array_literal(missing_product_tails),
      ),
    ]
  }
}

fn product_bundle_option_mapping_errors(
  store: Store,
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let invalid_product_tails =
    components
    |> list.filter_map(fn(component) {
      case read_string_field(component, "productId") {
        Some(id) ->
          case store.get_effective_product_by_id(store, id) {
            Some(_) -> {
              let options = store.get_effective_options_by_product_id(store, id)
              case product_bundle_component_options_valid(component, options) {
                True -> Error(Nil)
                False -> Ok(resource_id_tail(id))
              }
            }
            None -> Error(Nil)
          }
        None -> Error(Nil)
      }
    })
  case invalid_product_tails {
    [] -> []
    _ -> [
      NullableFieldUserError(
        None,
        "Mapping of components targeting products need to map all of the options of the product. Missing or invalid options found for components targeting product_ids "
          <> numeric_id_array_literal(invalid_product_tails)
          <> ".",
      ),
    ]
  }
}

fn product_bundle_component_options_valid(
  component: Dict(String, ResolvedValue),
  options: List(ProductOptionRecord),
) -> Bool {
  let selections = read_object_list_field(component, "optionSelections")
  list.length(selections) == list.length(options)
  && list.all(options, fn(option) {
    case product_bundle_selection_for_option(selections, option.id) {
      Some(selection) -> {
        let values =
          read_string_list_field(selection, "values") |> option.unwrap([])
        let valid_values =
          list.map(option.option_values, fn(value) { value.name })
        values != []
        && list.all(values, fn(value) { list.contains(valid_values, value) })
        && read_string_field(selection, "name") == Some(option.name)
      }
      None -> False
    }
  })
}

fn product_bundle_selection_for_option(
  selections: List(Dict(String, ResolvedValue)),
  option_id: String,
) -> Option(Dict(String, ResolvedValue)) {
  case
    selections
    |> list.filter(fn(selection) {
      read_string_field(selection, "componentOptionId") == Some(option_id)
    })
  {
    [selection] -> Some(selection)
    _ -> None
  }
}

fn product_bundle_quantity_errors(
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let exceeding_product_tails =
    components
    |> list.filter_map(fn(component) {
      case
        read_int_field(component, "quantity"),
        read_string_field(component, "productId")
      {
        Some(quantity), Some(product_id)
          if quantity > product_bundle_quantity_max
        -> Ok(resource_id_tail(product_id))
        _, _ -> Error(Nil)
      }
    })
  case exceeding_product_tails {
    [] -> []
    _ -> [
      NullableFieldUserError(
        None,
        "Quantity cannot be greater than "
          <> int.to_string(product_bundle_quantity_max)
          <> ". The following products have a quantity that exceeds the maximum: "
          <> numeric_id_array_literal(exceeding_product_tails),
      ),
    ]
  }
}

fn product_bundle_quantity_option_errors(
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let invalid_product_tails =
    components
    |> list.filter_map(fn(component) {
      case
        read_object_field(component, "quantityOption"),
        read_string_field(component, "productId")
      {
        Some(quantity_option), Some(product_id) -> {
          case read_object_list_field(quantity_option, "values") {
            [_] -> Ok(resource_id_tail(product_id))
            [] -> Ok(resource_id_tail(product_id))
            _ -> Error(Nil)
          }
        }
        _, _ -> Error(Nil)
      }
    })
  case invalid_product_tails {
    [] -> []
    _ -> [
      NullableFieldUserError(
        None,
        "Quantity options must have at least two values. Invalid quantity options found for components targeting product_ids "
          <> numeric_id_array_literal(invalid_product_tails)
          <> ".",
      ),
    ]
  }
}

fn product_bundle_consolidated_option_errors(
  input: Dict(String, ResolvedValue),
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let component_options =
    components
    |> list.flat_map(fn(component) {
      read_object_list_field(component, "optionSelections")
      |> list.filter_map(fn(selection) {
        case
          read_string_field(selection, "componentOptionId"),
          read_string_list_field(selection, "values")
        {
          Some(id), Some(values) -> Ok(#(id, values))
          _, _ -> Error(Nil)
        }
      })
    })
  let invalid =
    read_object_list_field(input, "consolidatedOptions")
    |> list.any(fn(component) {
      read_string_field(component, "optionName") == Some("")
      || {
        read_object_list_field(component, "optionSelections")
        |> list.any(fn(selection) {
          read_object_list_field(selection, "components")
          |> list.any(fn(selection_component) {
            case read_string_field(selection_component, "componentOptionId") {
              Some(component_option_id) ->
                !component_option_value_exists(
                  component_options,
                  component_option_id,
                  read_string_field(selection_component, "componentOptionValue"),
                )
              None -> False
            }
          })
        })
      }
    })
  case invalid {
    True -> [
      NullableFieldUserError(
        None,
        "Consolidated option selections are invalid.",
      ),
    ]
    False -> []
  }
}

fn component_option_value_exists(
  component_options: List(#(String, List(String))),
  component_option_id: String,
  component_option_value: Option(String),
) -> Bool {
  case component_option_value {
    Some(value) ->
      component_options
      |> list.any(fn(option) {
        let #(option_id, values) = option
        option_id == component_option_id && list.contains(values, value)
      })
    None -> False
  }
}

fn numeric_id_array_literal(values: List(String)) -> String {
  "[" <> string.join(values, ",") <> "]"
}

fn resource_id_tail(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(tail) -> tail
    Error(_) -> id
  }
}

@internal
pub fn handle_product_variant_relationship_bulk_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inputs = read_arg_object_list(args, "input")
  let missing_ids =
    inputs
    |> list.flat_map(missing_variant_relationship_ids(store))
  let user_errors = case missing_ids {
    [] -> []
    _ -> [
      ProductUserError(
        ["input"],
        "The product variants with ID(s) "
          <> json_string_array_literal(missing_ids)
          <> " could not be found.",
        Some("PRODUCT_VARIANTS_NOT_FOUND"),
      ),
    ]
  }
  mutation_result(
    key,
    product_variant_relationship_bulk_update_payload(
      user_errors,
      field,
      fragments,
    ),
    store,
    identity,
    [],
  )
}

@internal
pub fn bulk_product_resource_feedback_create_payload(
  feedback: List(ProductResourceFeedbackRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("BulkProductResourceFeedbackCreatePayload")),
      #(
        "feedback",
        SrcList(list.map(feedback, product_resource_feedback_source)),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn shop_resource_feedback_create_payload(
  feedback: Option(ShopResourceFeedbackRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let feedback_value = case feedback {
    Some(record) -> shop_resource_feedback_source(record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ShopResourceFeedbackCreatePayload")),
      #("feedback", feedback_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}
