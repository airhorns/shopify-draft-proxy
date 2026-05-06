//// Products-domain submodule: media_handlers.
//// Combines layered files: media_l12, media_l13, media_l14, media_l15.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcList, SrcNull, SrcString,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, project_graphql_value, src_object,
}

import shopify_draft_proxy/proxy/products/media_core.{
  first_unknown_media_id, handle_product_reorder_media,
  invalid_create_media_content_type,
  invalid_product_media_product_id_variable_error, is_valid_media_source,
  make_created_media_record, media_record_id_result,
  product_media_product_image_id_result, product_media_source,
  product_update_media_payload, product_update_media_payload_with_media_value,
  read_variant_media_inputs, stage_product_update_media,
  stage_variant_media_memberships, transition_created_media_to_processing,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type MutationFieldResult, type ProductUserError, ProductUserError,
}
import shopify_draft_proxy/proxy/products/products_core.{enumerate_items}
import shopify_draft_proxy/proxy/products/products_handlers.{
  product_source_with_store,
}
import shopify_draft_proxy/proxy/products/shared.{
  dedupe_preserving_order, mutation_error_result, mutation_result,
  read_arg_object_list, read_arg_string_list, read_string_field,
  user_errors_source,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{option_to_result}
import shopify_draft_proxy/proxy/products/variants_sources.{
  product_variant_source,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type ProductMediaRecord, type ProductRecord, type ProductVariantRecord,
}

// ===== from media_l12 =====
@internal
pub fn product_create_media_payload(
  store: Store,
  media: List(ProductMediaRecord),
  user_errors: List(ProductUserError),
  product: Option(ProductRecord),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(product) -> product_source_with_store(store, product)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductCreateMediaPayload")),
      #("media", SrcList(list.map(media, product_media_source))),
      #("mediaUserErrors", user_errors_source(user_errors)),
      #("product", product_value),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_delete_media_payload(
  store: Store,
  product: Option(ProductRecord),
  deleted_media_ids: SourceValue,
  deleted_product_image_ids: SourceValue,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(product) -> product_source_with_store(store, product)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductDeleteMediaPayload")),
      #("deletedMediaIds", deleted_media_ids),
      #("deletedProductImageIds", deleted_product_image_ids),
      #("mediaUserErrors", user_errors_source(user_errors)),
      #("product", product_value),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_variant_media_payload(
  store: Store,
  product: Option(ProductRecord),
  variants: List(ProductVariantRecord),
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
      #("__typename", SrcString("ProductVariantMediaPayload")),
      #("product", product_value),
      #(
        "productVariants",
        SrcList(
          list.map(variants, fn(variant) {
            product_variant_source(store, variant)
          }),
        ),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from media_l13 =====
@internal
pub fn stage_product_create_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let existing_media =
    store.get_effective_media_by_product_id(store, product_id)
  let initial = #(identity, [], [])
  let #(next_identity, created_reversed, user_errors_reversed) =
    inputs
    |> enumerate_items()
    |> list.fold(initial, fn(acc, entry) {
      let #(current_identity, created, errors) = acc
      let #(input, index) = entry
      let media_content_type =
        read_string_field(input, "mediaContentType") |> option.unwrap("IMAGE")
      case
        media_content_type == "IMAGE"
        && !is_valid_media_source(read_string_field(input, "originalSource"))
      {
        True -> #(current_identity, created, [
          ProductUserError(
            ["media", int.to_string(index), "originalSource"],
            "Image URL is invalid",
            None,
          ),
          ..errors
        ])
        False -> {
          let position = list.length(existing_media) + list.length(created)
          let #(record, identity_after_record) =
            make_created_media_record(
              current_identity,
              product_id,
              input,
              position,
            )
          #(identity_after_record, [record, ..created], errors)
        }
      }
    })
  let created_media = list.reverse(created_reversed)
  let user_errors = list.reverse(user_errors_reversed)
  let response_store = case created_media {
    [] -> store
    _ ->
      store.replace_staged_media_for_product(
        store,
        product_id,
        list.append(existing_media, created_media),
      )
  }
  let final_store = case created_media {
    [] -> store
    _ ->
      store.replace_staged_media_for_product(
        store,
        product_id,
        list.append(
          existing_media,
          list.map(created_media, transition_created_media_to_processing),
        ),
      )
  }
  let product = store.get_effective_product_by_id(response_store, product_id)
  let staged_ids = created_media |> list.filter_map(media_record_id_result)
  mutation_result(
    key,
    product_create_media_payload(
      response_store,
      created_media,
      user_errors,
      product,
      field,
      fragments,
    ),
    final_store,
    next_identity,
    staged_ids,
  )
}

@internal
pub fn stage_product_delete_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  media_ids: List(String),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  case first_unknown_media_id(media_ids, effective_media) {
    Some(media_id) ->
      mutation_result(
        key,
        product_delete_media_payload(
          store,
          None,
          SrcNull,
          SrcNull,
          [
            ProductUserError(
              ["mediaIds"],
              "Media id " <> media_id <> " does not exist",
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
    None -> {
      let deleted_media =
        effective_media
        |> list.filter(fn(media) {
          case media.id {
            Some(id) -> list.contains(media_ids, id)
            None -> False
          }
        })
      let next_media =
        effective_media
        |> list.filter(fn(media) {
          case media.id {
            Some(id) -> !list.contains(media_ids, id)
            None -> True
          }
        })
      let next_store =
        store.replace_staged_media_for_product(store, product_id, next_media)
      let deleted_media_ids =
        deleted_media |> list.filter_map(media_record_id_result)
      let deleted_product_image_ids =
        deleted_media |> list.filter_map(product_media_product_image_id_result)
      let product = store.get_effective_product_by_id(next_store, product_id)
      mutation_result(
        key,
        product_delete_media_payload(
          next_store,
          product,
          SrcList(list.map(deleted_media_ids, SrcString)),
          SrcList(list.map(deleted_product_image_ids, SrcString)),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        deleted_media_ids,
      )
    }
  }
}

@internal
pub fn handle_product_variant_media_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variant_media_payload(
          store,
          None,
          [],
          [ProductUserError(["productId"], "Product id is required", None)],
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
            product_variant_media_payload(
              store,
              None,
              [],
              [ProductUserError(["productId"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) -> {
          let inputs =
            read_variant_media_inputs(read_arg_object_list(args, "variantMedia"))
          let is_append = case field {
            Field(name: name, ..) -> name.value == "productVariantAppendMedia"
            _ -> False
          }
          let #(next_store, updated_variant_ids, user_errors) =
            stage_variant_media_memberships(
              store,
              product_id,
              inputs,
              is_append,
            )
          let response_store = case user_errors {
            [] -> next_store
            _ -> store
          }
          let variants =
            updated_variant_ids
            |> dedupe_preserving_order
            |> list.filter_map(fn(variant_id) {
              store.get_effective_variant_by_id(response_store, variant_id)
              |> option_to_result
            })
          let staged_ids = case user_errors {
            [] -> [product_id, ..dedupe_preserving_order(updated_variant_ids)]
            _ -> []
          }
          mutation_result(
            key,
            product_variant_media_payload(
              response_store,
              Some(product),
              variants,
              user_errors,
              field,
              fragments,
            ),
            response_store,
            identity,
            staged_ids,
          )
        }
      }
  }
}

@internal
pub fn product_media_not_found_payload(
  store: Store,
  shape: String,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case shape {
    "delete" ->
      product_delete_media_payload(
        store,
        None,
        SrcNull,
        SrcNull,
        [ProductUserError(["productId"], "Product does not exist", None)],
        field,
        fragments,
      )
    "create" ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("ProductCreateMediaPayload")),
          #("media", SrcNull),
          #(
            "mediaUserErrors",
            user_errors_source([
              ProductUserError(["productId"], "Product does not exist", None),
            ]),
          ),
          #("product", SrcNull),
        ]),
        get_selected_child_fields(field, default_selected_field_options()),
        fragments,
      )
    _ ->
      product_update_media_payload_with_media_value(
        SrcNull,
        [ProductUserError(["productId"], "Product does not exist", None)],
        field,
        fragments,
      )
  }
}

// ===== from media_l14 =====
@internal
pub fn handle_product_create_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_id = graphql_helpers.read_arg_string(args, "productId")
  case product_id {
    Some("") ->
      mutation_error_result(key, store, identity, [
        invalid_product_media_product_id_variable_error("", document),
      ])
    _ ->
      case invalid_create_media_content_type(args, document) {
        Some(error) -> mutation_error_result(key, store, identity, [error])
        None ->
          case product_id {
            None ->
              mutation_result(
                key,
                product_create_media_payload(
                  store,
                  [],
                  [
                    ProductUserError(
                      ["productId"],
                      "Product id is required",
                      None,
                    ),
                  ],
                  None,
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
                    product_media_not_found_payload(
                      store,
                      "create",
                      field,
                      fragments,
                    ),
                    store,
                    identity,
                    [],
                  )
                Some(_) ->
                  stage_product_create_media(
                    store,
                    identity,
                    key,
                    product_id,
                    read_arg_object_list(args, "media"),
                    field,
                    fragments,
                  )
              }
          }
      }
  }
}

@internal
pub fn handle_product_update_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_id = graphql_helpers.read_arg_string(args, "productId")
  case product_id {
    Some("") ->
      mutation_error_result(key, store, identity, [
        invalid_product_media_product_id_variable_error("", document),
      ])
    None ->
      mutation_result(
        key,
        product_update_media_payload(
          [],
          [ProductUserError(["productId"], "Product id is required", None)],
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
            product_media_not_found_payload(store, "update", field, fragments),
            store,
            identity,
            [],
          )
        Some(_) ->
          stage_product_update_media(
            store,
            identity,
            key,
            product_id,
            read_arg_object_list(args, "media"),
            field,
            fragments,
          )
      }
  }
}

@internal
pub fn handle_product_delete_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_id = graphql_helpers.read_arg_string(args, "productId")
  case product_id {
    Some("") ->
      mutation_error_result(key, store, identity, [
        invalid_product_media_product_id_variable_error("", document),
      ])
    None ->
      mutation_result(
        key,
        product_delete_media_payload(
          store,
          None,
          SrcList([]),
          SrcList([]),
          [ProductUserError(["productId"], "Product id is required", None)],
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
            product_media_not_found_payload(store, "delete", field, fragments),
            store,
            identity,
            [],
          )
        Some(_) ->
          stage_product_delete_media(
            store,
            identity,
            key,
            product_id,
            read_arg_string_list(args, "mediaIds"),
            field,
            fragments,
          )
      }
  }
}

// ===== from media_l15 =====
@internal
pub fn handle_product_media_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  case root_name {
    "productCreateMedia" ->
      handle_product_create_media(
        store,
        identity,
        document,
        field,
        fragments,
        variables,
      )
    "productUpdateMedia" ->
      handle_product_update_media(
        store,
        identity,
        document,
        field,
        fragments,
        variables,
      )
    "productDeleteMedia" ->
      handle_product_delete_media(
        store,
        identity,
        document,
        field,
        fragments,
        variables,
      )
    "productReorderMedia" ->
      handle_product_reorder_media(store, identity, field, fragments, variables)
    _ ->
      mutation_result(
        get_field_response_key(field),
        json.null(),
        store,
        identity,
        [],
      )
  }
}
