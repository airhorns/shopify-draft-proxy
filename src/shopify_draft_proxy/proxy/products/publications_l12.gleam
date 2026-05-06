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
import shopify_draft_proxy/proxy/products/products_l11.{
  product_source_with_store,
}
import shopify_draft_proxy/proxy/products/publications_l00.{
  remove_publication_targets,
}
import shopify_draft_proxy/proxy/products/publications_l01.{
  merge_publication_targets, selected_publication_id,
}
import shopify_draft_proxy/proxy/products/publications_l02.{
  combined_listing_update_payload,
}
import shopify_draft_proxy/proxy/products/publications_l10.{
  product_source_with_store_and_publication,
}
import shopify_draft_proxy/proxy/products/publications_l11.{
  publishable_mutation_payload,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_list_field_length, read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_rejected_result, mutation_result, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, MutationFieldResult,
  ProductUserError,
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
pub fn publishable_product_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  publication_targets: List(String),
  is_publish: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  case publication_targets {
    [] ->
      mutation_result(
        key,
        publishable_mutation_payload(
          store,
          Some(product_source_with_store(store, product)),
          [ProductUserError(["input"], "Publication target is required", None)],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    _ -> {
      let next_publication_ids = case is_publish {
        True ->
          merge_publication_targets(
            product.publication_ids,
            publication_targets,
          )
        False ->
          remove_publication_targets(
            product.publication_ids,
            publication_targets,
          )
      }
      let next_product =
        ProductRecord(..product, publication_ids: next_publication_ids)
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      mutation_result(
        key,
        publishable_mutation_payload(
          next_store,
          Some(product_source_with_store_and_publication(
            next_store,
            next_product,
            selected_publication_id(
              get_selected_child_fields(field, default_selected_field_options()),
              variables,
            ),
          )),
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity,
        [next_product.id],
      )
    }
  }
}

@internal
pub fn handle_combined_listing_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let parent_product_id =
    graphql_helpers.read_arg_string(args, "parentProductId")
  let parent_product = case parent_product_id {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None -> None
  }
  case parent_product {
    None ->
      mutation_result(
        key,
        combined_listing_update_payload(
          SrcNull,
          [
            ProductUserError(
              ["parentProductId"],
              "Product does not exist",
              Some("PARENT_PRODUCT_NOT_FOUND"),
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
      let user_errors =
        combined_listing_update_validation_errors(store, product, args)
      case user_errors {
        [] -> {
          let #(next_store, staged_ids) =
            stage_combined_listing_update(store, product, args)
          mutation_result(
            key,
            combined_listing_update_payload(
              product_source_with_store(
                next_store,
                store.get_effective_product_by_id(next_store, product.id)
                  |> option.unwrap(product),
              ),
              [],
              field,
              fragments,
            ),
            next_store,
            identity,
            staged_ids,
          )
        }
        _ ->
          mutation_rejected_result(
            key,
            combined_listing_update_payload(
              SrcNull,
              user_errors,
              field,
              fragments,
            ),
            store,
            identity,
          )
      }
    }
  }
}

type CombinedListingChildInput {
  CombinedListingChildInput(
    list_name: String,
    index: Int,
    child_product_id: Option(String),
    selected_parent_option_values_count: Int,
  )
}

fn combined_listing_update_validation_errors(
  store: Store,
  parent_product: ProductRecord,
  args: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  let added_inputs = combined_listing_child_inputs(args, "productsAdded")
  let edited_inputs = combined_listing_child_inputs(args, "productsEdited")
  let child_inputs = list.append(added_inputs, edited_inputs)
  let child_ids = child_inputs |> list.filter_map(child_input_id)
  let removed_ids =
    graphql_helpers.read_arg_string_list(args, "productsRemovedIds")
    |> option.unwrap([])
  let parent_role_errors = case parent_product.combined_listing_role {
    Some("PARENT") -> []
    _ -> [
      combined_listing_user_error(
        ["parentProductId"],
        "The product must be a combined listing.",
        "PARENT_PRODUCT_MUST_BE_A_COMBINED_LISTING",
      ),
    ]
  }
  let title_errors = case graphql_helpers.read_arg_string(args, "title") {
    Some(title) ->
      case string.length(title) > 255 {
        True -> [
          combined_listing_user_error(
            ["title"],
            "The title cannot be longer than 255 characters.",
            "TITLE_TOO_LONG",
          ),
        ]
        False -> []
      }
    _ -> []
  }
  let option_values_errors =
    missing_options_and_values_errors(args, child_inputs)
  let duplicate_errors = case has_duplicate_strings(child_ids) {
    True -> [
      combined_listing_user_error(
        duplicate_child_field_path(child_inputs),
        "The field cannot receive duplicated products.",
        "CANNOT_HAVE_DUPLICATED_PRODUCTS",
      ),
    ]
    False -> []
  }
  [
    parent_role_errors,
    title_errors,
    option_values_errors,
    duplicate_errors,
    parent_as_child_errors(parent_product.id, child_inputs),
    missing_child_product_errors(store, child_inputs),
    selected_parent_option_values_errors(child_inputs),
    already_child_errors(store, added_inputs),
    edit_and_remove_errors(edited_inputs, removed_ids),
  ]
  |> list.flatten
}

fn missing_options_and_values_errors(
  args: Dict(String, ResolvedValue),
  child_inputs: List(CombinedListingChildInput),
) -> List(ProductUserError) {
  case child_inputs, read_object_list_field(args, "optionsAndValues") {
    [], _ -> []
    _, [] -> [
      combined_listing_user_error(
        ["optionsAndValues"],
        "Options and values must be present when adding or editing products.",
        "MISSING_OPTION_VALUES",
      ),
    ]
    _, _ -> []
  }
}

fn combined_listing_child_inputs(
  args: Dict(String, ResolvedValue),
  list_name: String,
) -> List(CombinedListingChildInput) {
  read_object_list_field(args, list_name)
  |> list.index_map(fn(input, index) {
    CombinedListingChildInput(
      list_name: list_name,
      index: index,
      child_product_id: read_string_field(input, "childProductId"),
      selected_parent_option_values_count: read_list_field_length(
        input,
        "selectedParentOptionValues",
      )
        |> option.unwrap(0),
    )
  })
}

fn child_input_id(input: CombinedListingChildInput) -> Result(String, Nil) {
  case input.child_product_id {
    Some(id) -> Ok(id)
    None -> Error(Nil)
  }
}

fn parent_as_child_errors(
  parent_product_id: String,
  inputs: List(CombinedListingChildInput),
) -> List(ProductUserError) {
  inputs
  |> list.filter_map(fn(input) {
    case input.child_product_id == Some(parent_product_id) {
      True ->
        Ok(combined_listing_user_error(
          [input.list_name],
          "A parent product cannot have itself as child.",
          "CANNOT_HAVE_PARENT_AS_CHILD",
        ))
      False -> Error(Nil)
    }
  })
}

fn missing_child_product_errors(
  store: Store,
  inputs: List(CombinedListingChildInput),
) -> List(ProductUserError) {
  inputs
  |> list.filter_map(fn(input) {
    case input.child_product_id {
      Some(id) ->
        case store.get_effective_product_by_id(store, id) {
          Some(_) -> Error(Nil)
          None ->
            Ok(combined_listing_user_error(
              [input.list_name],
              "The product with ID(s) [\"" <> id <> "\"] could not be found.",
              "PRODUCT_NOT_FOUND",
            ))
        }
      None -> Error(Nil)
    }
  })
}

fn selected_parent_option_values_errors(
  inputs: List(CombinedListingChildInput),
) -> List(ProductUserError) {
  inputs
  |> list.filter_map(fn(input) {
    case input.selected_parent_option_values_count > 0 {
      True -> Error(Nil)
      False ->
        Ok(combined_listing_user_error(
          child_input_field_path(input, "selectedParentOptionValues"),
          "The selected option values cannot be empty.",
          "MUST_HAVE_SELECTED_OPTION_VALUES",
        ))
    }
  })
}

fn already_child_errors(
  store: Store,
  inputs: List(CombinedListingChildInput),
) -> List(ProductUserError) {
  inputs
  |> list.filter_map(fn(input) {
    case input.child_product_id {
      Some(id) ->
        case store.get_effective_product_by_id(store, id) {
          Some(product) ->
            case product.combined_listing_parent_id {
              Some(_) ->
                Ok(combined_listing_user_error(
                  [input.list_name],
                  "A product can't belong to more than one product Combined Listing.",
                  "PRODUCT_IS_ALREADY_A_CHILD",
                ))
              None -> Error(Nil)
            }
          None -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

fn edit_and_remove_errors(
  edited_inputs: List(CombinedListingChildInput),
  removed_ids: List(String),
) -> List(ProductUserError) {
  edited_inputs
  |> list.filter_map(fn(input) {
    case input.child_product_id {
      Some(id) ->
        case list.contains(removed_ids, id) {
          True ->
            Ok(combined_listing_user_error(
              [input.list_name],
              "Cannot edit and remove same child products.",
              "EDIT_AND_REMOVE_ON_SAME_PRODUCTS",
            ))
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

fn duplicate_child_field_path(
  inputs: List(CombinedListingChildInput),
) -> List(String) {
  case inputs {
    [first, ..] -> [first.list_name]
    [] -> ["productsAdded"]
  }
}

fn stage_combined_listing_update(
  store: Store,
  parent_product: ProductRecord,
  args: Dict(String, ResolvedValue),
) -> #(Store, List(String)) {
  let added_ids =
    combined_listing_child_inputs(args, "productsAdded")
    |> list.filter_map(child_input_id)
  let removed_ids =
    graphql_helpers.read_arg_string_list(args, "productsRemovedIds")
    |> option.unwrap([])
  let next_child_ids =
    list.append(parent_product.combined_listing_child_ids, added_ids)
    |> dedupe_strings
    |> list.filter(fn(id) { !list.contains(removed_ids, id) })
  let next_title =
    graphql_helpers.read_arg_string(args, "title")
    |> option.unwrap(parent_product.title)
  let next_parent =
    ProductRecord(
      ..parent_product,
      title: next_title,
      combined_listing_role: Some("PARENT"),
      combined_listing_parent_id: None,
      combined_listing_child_ids: next_child_ids,
    )
  let #(_, store_after_parent) = store.upsert_staged_product(store, next_parent)
  let store_after_added =
    list.fold(added_ids, store_after_parent, fn(current, child_id) {
      case store.get_effective_product_by_id(current, child_id) {
        Some(child_product) -> {
          let next_child =
            ProductRecord(
              ..child_product,
              combined_listing_role: Some("CHILD"),
              combined_listing_parent_id: Some(parent_product.id),
              combined_listing_child_ids: [],
            )
          let #(_, next_store) =
            store.upsert_staged_product(current, next_child)
          next_store
        }
        None -> current
      }
    })
  let next_store =
    list.fold(removed_ids, store_after_added, fn(current, child_id) {
      case store.get_effective_product_by_id(current, child_id) {
        Some(child_product) ->
          case
            child_product.combined_listing_parent_id == Some(parent_product.id)
          {
            True -> {
              let next_child =
                ProductRecord(
                  ..child_product,
                  combined_listing_role: None,
                  combined_listing_parent_id: None,
                  combined_listing_child_ids: [],
                )
              let #(_, next_store) =
                store.upsert_staged_product(current, next_child)
              next_store
            }
            False -> current
          }
        None -> current
      }
    })
  #(
    next_store,
    [parent_product.id, ..list.append(added_ids, removed_ids)] |> dedupe_strings,
  )
}

fn has_duplicate_strings(values: List(String)) -> Bool {
  list.length(dedupe_strings(values)) < list.length(values)
}

fn dedupe_strings(values: List(String)) -> List(String) {
  let #(reversed, _) =
    list.fold(values, #([], dict.new()), fn(acc, value) {
      let #(items, seen) = acc
      case dict.has_key(seen, value) {
        True -> #(items, seen)
        False -> #([value, ..items], dict.insert(seen, value, True))
      }
    })
  list.reverse(reversed)
}

fn child_input_field_path(
  input: CombinedListingChildInput,
  field_name: String,
) -> List(String) {
  [input.list_name, int.to_string(input.index), field_name]
}

fn combined_listing_user_error(
  path: List(String),
  message: String,
  code: String,
) -> ProductUserError {
  ProductUserError(path, message, Some(code))
}

@internal
pub fn product_publication_payload(
  typename: String,
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
      #("__typename", SrcString(typename)),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}
