//// Products-domain submodule: media_core.
//// Combines layered files: media_l00, media_l01, media_l02, media_l03, media_l04, media_l05.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, ListVal, ObjectVal,
}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcList, SrcNull, SrcString,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, project_graphql_value, src_object,
}

import shopify_draft_proxy/proxy/products/collections_core.{
  read_collection_product_moves,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  find_variable_definition_location,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type CollectionProductMove, type MutationFieldResult, type ProductUserError,
  type VariantMediaInput, CollectionProductMove, ProductUserError,
  VariantMediaInput,
}
import shopify_draft_proxy/proxy/products/products_core.{
  enumerate_items, enumerate_strings,
}
import shopify_draft_proxy/proxy/products/shared.{
  connection_end_cursor, connection_start_cursor, dedupe_preserving_order,
  job_source, mutation_result, non_empty_string, read_arg_object_list,
  read_arg_string_list, read_string_field, resolved_value_to_json,
  user_errors_source,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  find_variant_by_id, option_to_result,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type ProductMediaRecord, type ProductRecord, type ProductVariantRecord,
  ProductMediaRecord, ProductVariantRecord,
}

// ===== from media_l00 =====
@internal
pub fn product_media_cursor(media: ProductMediaRecord, _index: Int) -> String {
  "cursor:" <> media.key
}

@internal
pub fn product_media_typename(media: ProductMediaRecord) -> String {
  case media.media_content_type {
    Some("IMAGE") -> "MediaImage"
    Some("VIDEO") -> "Video"
    Some("EXTERNAL_VIDEO") -> "ExternalVideo"
    Some("MODEL_3D") -> "Model3d"
    _ -> "Media"
  }
}

@internal
pub fn product_media_image_source(url: Option(String)) -> SourceValue {
  case url {
    Some(url) -> src_object([#("url", SrcString(url))])
    None -> SrcNull
  }
}

@internal
pub fn insert_product_media_at_position(
  media: List(ProductMediaRecord),
  record: ProductMediaRecord,
  position: Int,
) -> List(ProductMediaRecord) {
  let insertion_index = int.min(position, list.length(media))
  let before = list.take(media, insertion_index)
  let after = list.drop(media, insertion_index)
  list.append(before, [record, ..after])
}

@internal
pub fn is_create_media_content_type(value: String) -> Bool {
  case value {
    "VIDEO" | "EXTERNAL_VIDEO" | "MODEL_3D" | "IMAGE" -> True
    _ -> False
  }
}

@internal
pub fn is_valid_media_source(value: Option(String)) -> Bool {
  case value {
    Some(value) -> {
      let trimmed = string.trim(value)
      string.length(trimmed) > 0
      && {
        string.starts_with(trimmed, "http://")
        || string.starts_with(trimmed, "https://")
      }
    }
    None -> False
  }
}

@internal
pub fn make_synthetic_media_id(
  identity: SyntheticIdentityRegistry,
  media_content_type: String,
) -> #(String, SyntheticIdentityRegistry) {
  case media_content_type {
    "IMAGE" -> synthetic_identity.make_synthetic_gid(identity, "MediaImage")
    _ -> synthetic_identity.make_synthetic_gid(identity, "Media")
  }
}

@internal
pub fn make_synthetic_product_image_id(
  identity: SyntheticIdentityRegistry,
  media_content_type: String,
) -> #(Option(String), SyntheticIdentityRegistry) {
  case media_content_type {
    "IMAGE" -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "ProductImage")
      #(Some(id), next_identity)
    }
    _ -> #(None, identity)
  }
}

@internal
pub fn transition_created_media_to_processing(
  media: ProductMediaRecord,
) -> ProductMediaRecord {
  ProductMediaRecord(
    ..media,
    status: Some("PROCESSING"),
    image_url: None,
    preview_image_url: None,
  )
}

@internal
pub fn transition_media_to_ready(
  media: ProductMediaRecord,
) -> ProductMediaRecord {
  let ready_url =
    media.source_url
    |> option.or(media.image_url)
    |> option.or(media.preview_image_url)
  ProductMediaRecord(
    ..media,
    status: Some("READY"),
    image_url: ready_url,
    preview_image_url: ready_url,
  )
}

@internal
pub fn find_media_by_id(
  media: List(ProductMediaRecord),
  id: String,
) -> Option(ProductMediaRecord) {
  media
  |> list.find(fn(record) { record.id == Some(id) })
  |> option.from_result
}

@internal
pub fn media_record_id_result(
  media: ProductMediaRecord,
) -> Result(String, Nil) {
  case media.id {
    Some(id) -> Ok(id)
    None -> Error(Nil)
  }
}

@internal
pub fn product_media_product_image_id_result(
  media: ProductMediaRecord,
) -> Result(String, Nil) {
  case media.product_image_id {
    Some(id) -> Ok(id)
    None -> Error(Nil)
  }
}

// ===== from media_l01 =====
@internal
pub fn product_media_preview_source(media: ProductMediaRecord) -> SourceValue {
  src_object([
    #("image", product_media_image_source(media.preview_image_url)),
  ])
}

@internal
pub fn apply_product_media_moves(
  media: List(ProductMediaRecord),
  moves: List(CollectionProductMove),
) -> List(ProductMediaRecord) {
  list.fold(moves, media, fn(current_media, move) {
    let CollectionProductMove(id: media_id, new_position: new_position) = move
    case find_media_by_id(current_media, media_id) {
      None -> current_media
      Some(record) -> {
        let without_record =
          current_media
          |> list.filter(fn(candidate) { candidate.id != Some(media_id) })
        insert_product_media_at_position(without_record, record, new_position)
      }
    }
  })
}

@internal
pub fn read_variant_media_inputs(
  raw_inputs: List(Dict(String, ResolvedValue)),
) -> List(VariantMediaInput) {
  raw_inputs
  |> list.filter_map(fn(input) {
    case read_string_field(input, "variantId") {
      Some(variant_id) ->
        Ok(VariantMediaInput(
          variant_id: variant_id,
          media_ids: read_arg_string_list(input, "mediaIds"),
        ))
      None -> Error(Nil)
    }
  })
}

@internal
pub fn make_created_media_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
  position: Int,
) -> #(ProductMediaRecord, SyntheticIdentityRegistry) {
  let media_content_type =
    read_string_field(input, "mediaContentType") |> option.unwrap("IMAGE")
  let #(media_id, identity_after_media) =
    make_synthetic_media_id(identity, media_content_type)
  let #(product_image_id, next_identity) =
    make_synthetic_product_image_id(identity_after_media, media_content_type)
  let source_url =
    option.then(read_string_field(input, "originalSource"), non_empty_string)
  #(
    ProductMediaRecord(
      key: product_id <> ":media:" <> int.to_string(position),
      product_id: product_id,
      position: position,
      id: Some(media_id),
      media_content_type: Some(media_content_type),
      alt: read_string_field(input, "alt"),
      status: Some("UPLOADED"),
      product_image_id: product_image_id,
      image_url: None,
      image_width: None,
      image_height: None,
      preview_image_url: None,
      source_url: source_url,
    ),
    next_identity,
  )
}

@internal
pub fn settle_media_to_ready(media: ProductMediaRecord) -> ProductMediaRecord {
  case media.status {
    Some("PROCESSING") -> transition_media_to_ready(media)
    _ -> media
  }
}

@internal
pub fn update_media_record(
  media: ProductMediaRecord,
  input: Dict(String, ResolvedValue),
) -> ProductMediaRecord {
  let next_image_url =
    option.then(
      read_string_field(input, "previewImageSource"),
      non_empty_string,
    )
    |> option.or(option.then(
      read_string_field(input, "originalSource"),
      non_empty_string,
    ))
    |> option.or(media.image_url)
    |> option.or(media.preview_image_url)
    |> option.or(media.source_url)
  ProductMediaRecord(
    ..media,
    alt: read_string_field(input, "alt") |> option.or(media.alt),
    status: Some("READY"),
    image_url: next_image_url,
    preview_image_url: next_image_url,
    source_url: media.source_url |> option.or(next_image_url),
  )
}

@internal
pub fn find_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media_id: Option(String),
) -> Option(Dict(String, ResolvedValue)) {
  case media_id {
    None -> None
    Some(id) ->
      updates
      |> list.find(fn(update) { read_string_field(update, "id") == Some(id) })
      |> option.from_result
  }
}

@internal
pub fn has_media_id(media: List(ProductMediaRecord), id: String) -> Bool {
  case find_media_by_id(media, id) {
    Some(_) -> True
    None -> False
  }
}

// ===== from media_l02 =====
@internal
pub fn product_media_source(media: ProductMediaRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString(product_media_typename(media))),
    #("id", graphql_helpers.option_string_source(media.id)),
    #("alt", graphql_helpers.option_string_source(media.alt)),
    #(
      "mediaContentType",
      graphql_helpers.option_string_source(media.media_content_type),
    ),
    #("status", graphql_helpers.option_string_source(media.status)),
    #("preview", product_media_preview_source(media)),
    #(
      "image",
      product_media_image_source(
        media.image_url |> option.or(media.preview_image_url),
      ),
    ),
    #("mediaErrors", SrcList([])),
    #("mediaWarnings", SrcList([])),
  ])
}

@internal
pub fn first_missing_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media: List(ProductMediaRecord),
) -> Option(Dict(String, ResolvedValue)) {
  updates
  |> list.find(fn(update) {
    case read_string_field(update, "id") {
      Some(id) -> !has_media_id(media, id)
      None -> True
    }
  })
  |> option.from_result
}

@internal
pub fn first_non_ready_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media: List(ProductMediaRecord),
) -> Option(Int) {
  updates
  |> enumerate_items()
  |> list.find_map(fn(entry) {
    let #(update, index) = entry
    case read_string_field(update, "id") {
      Some(id) ->
        case find_media_by_id(media, id) {
          Some(record) ->
            case record.status {
              Some("READY") -> Error(Nil)
              _ -> Ok(index)
            }
          None -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn first_unknown_media_id(
  media_ids: List(String),
  media: List(ProductMediaRecord),
) -> Option(String) {
  media_ids
  |> list.find(fn(id) { !has_media_id(media, id) })
  |> option.from_result
}

@internal
pub fn product_update_media_payload_with_media_value(
  media: SourceValue,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductUpdateMediaPayload")),
      #("media", media),
      #("userErrors", user_errors_source(user_errors)),
      #("mediaUserErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_reorder_media_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductReorderMediaPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
      #("mediaUserErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from media_l03 =====
@internal
pub fn product_media_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let media = store.get_effective_media_by_product_id(store, product.id)
  src_object([
    #(
      "edges",
      SrcList(
        list.map(enumerate_items(media), fn(entry) {
          let #(record, index) = entry
          src_object([
            #("cursor", SrcString(product_media_cursor(record, index))),
            #("node", product_media_source(record)),
          ])
        }),
      ),
    ),
    #("nodes", SrcList(list.map(media, product_media_source))),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", connection_start_cursor(media, product_media_cursor)),
        #("endCursor", connection_end_cursor(media, product_media_cursor)),
      ]),
    ),
  ])
}

@internal
pub fn variant_media_connection_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  let product_media =
    store.get_effective_media_by_product_id(store, variant.product_id)
  let media =
    variant.media_ids
    |> list.filter_map(fn(media_id) {
      find_media_by_id(product_media, media_id) |> option_to_result
    })
  src_object([
    #(
      "edges",
      SrcList(
        list.map(enumerate_items(media), fn(entry) {
          let #(record, index) = entry
          src_object([
            #("cursor", SrcString(product_media_cursor(record, index))),
            #("node", product_media_source(record)),
          ])
        }),
      ),
    ),
    #("nodes", SrcList(list.map(media, product_media_source))),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", connection_start_cursor(media, product_media_cursor)),
        #("endCursor", connection_end_cursor(media, product_media_cursor)),
      ]),
    ),
  ])
}

@internal
pub fn first_unknown_media_index(
  media_ids: List(String),
  product_media_ids: List(String),
) -> Option(Int) {
  media_ids
  |> enumerate_strings()
  |> list.find(fn(entry) {
    let #(media_id, _) = entry
    !list.contains(product_media_ids, media_id)
  })
  |> result.map(fn(entry) {
    let #(_, index) = entry
    index
  })
  |> option.from_result
}

fn variant_media_variant_error(index: Int) -> ProductUserError {
  ProductUserError(
    ["variantMedia", int.to_string(index), "variantId"],
    "Variant does not exist on the specified product.",
    Some("PRODUCT_VARIANT_DOES_NOT_EXIST_ON_PRODUCT"),
  )
}

fn variant_media_product_media_error(input_index: Int) -> ProductUserError {
  ProductUserError(
    ["variantMedia", int.to_string(input_index), "mediaIds"],
    "Media does not exist on the specified product.",
    Some("MEDIA_DOES_NOT_EXIST_ON_PRODUCT"),
  )
}

fn variant_media_processing_error(input_index: Int) -> ProductUserError {
  ProductUserError(
    ["variantMedia", int.to_string(input_index), "mediaIds"],
    "Non-ready media cannot be attached to variants.",
    Some("NON_READY_MEDIA"),
  )
}

fn variant_media_unattached_error(input_index: Int) -> ProductUserError {
  ProductUserError(
    ["variantMedia", int.to_string(input_index), "variantId"],
    "The specified media is not attached to the specified variant.",
    Some("MEDIA_IS_NOT_ATTACHED_TO_VARIANT"),
  )
}

fn variant_media_input_errors(
  variant: ProductVariantRecord,
  product_media: List(ProductMediaRecord),
  media_ids: List(String),
  input_index: Int,
  is_append: Bool,
) -> List(ProductUserError) {
  list.fold(enumerate_strings(media_ids), [], fn(errors, entry) {
    let #(media_id, _media_index) = entry
    case find_media_by_id(product_media, media_id) {
      None ->
        list.append(errors, [
          variant_media_product_media_error(input_index),
        ])
      Some(media) ->
        case is_append, media.status {
          True, Some("READY") -> errors
          True, _ ->
            list.append(errors, [
              variant_media_processing_error(input_index),
            ])
          False, _ ->
            case list.contains(variant.media_ids, media_id) {
              True -> errors
              False ->
                list.append(errors, [
                  variant_media_unattached_error(input_index),
                ])
            }
        }
    }
  })
}

@internal
pub fn product_update_media_payload(
  media: List(ProductMediaRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  product_update_media_payload_with_media_value(
    SrcList(list.map(media, product_media_source)),
    user_errors,
    field,
    fragments,
  )
}

@internal
pub fn invalid_product_media_product_id_variable_error(
  product_id: String,
  document: String,
) -> Json {
  let message = "Invalid global id '" <> product_id <> "'"
  let base = [
    #(
      "message",
      json.string("Variable $productId of type ID! was provided invalid value"),
    ),
  ]
  let with_locations = case
    find_variable_definition_location(document, "productId")
  {
    Some(loc) ->
      list.append(base, [
        #("locations", graphql_helpers.locations_json(loc, document)),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", json.string(product_id)),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #("path", json.preprocessed_array([])),
                #("explanation", json.string(message)),
                #("message", json.string(message)),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

@internal
pub fn invalid_product_media_content_type_variable_error(
  media_values: List(ResolvedValue),
  media_index: Int,
  media_content_type: String,
  document: String,
) -> Json {
  let explanation =
    "Expected \""
    <> media_content_type
    <> "\" to be one of: VIDEO, EXTERNAL_VIDEO, MODEL_3D, IMAGE"
  let base = [
    #(
      "message",
      json.string(
        "Variable $media of type [CreateMediaInput!]! was provided invalid value for "
        <> int.to_string(media_index)
        <> ".mediaContentType ("
        <> explanation
        <> ")",
      ),
    ),
  ]
  let with_locations = case
    find_variable_definition_location(document, "media")
  {
    Some(loc) ->
      list.append(base, [
        #("locations", graphql_helpers.locations_json(loc, document)),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", json.array(media_values, resolved_value_to_json)),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #(
                  "path",
                  json.preprocessed_array([
                    json.int(media_index),
                    json.string("mediaContentType"),
                  ]),
                ),
                #("explanation", json.string(explanation)),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

// ===== from media_l04 =====
@internal
pub fn stage_product_update_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  updates: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  case first_missing_media_update(updates, effective_media) {
    Some(update) -> {
      let media_id = read_string_field(update, "id")
      let media_value = case media_id {
        Some(_) -> SrcNull
        None -> SrcList([])
      }
      let error = case media_id {
        Some(id) ->
          ProductUserError(
            ["media"],
            "Media id " <> id <> " does not exist",
            None,
          )
        None -> ProductUserError(["media", "id"], "Media id is required", None)
      }
      mutation_result(
        key,
        product_update_media_payload_with_media_value(
          media_value,
          [error],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    }
    None ->
      case first_non_ready_media_update(updates, effective_media) {
        Some(index) ->
          mutation_result(
            key,
            product_update_media_payload(
              [],
              [
                ProductUserError(
                  ["media", int.to_string(index), "id"],
                  "Non-ready media cannot be updated.",
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
          let updated_media =
            effective_media
            |> list.map(fn(media) {
              case find_media_update(updates, media.id) {
                Some(update) -> update_media_record(media, update)
                None -> settle_media_to_ready(media)
              }
            })
          let changed_media =
            updated_media
            |> list.filter(fn(media) {
              case find_media_update(updates, media.id) {
                Some(_) -> True
                None -> False
              }
            })
          let next_store =
            store.replace_staged_media_for_product(
              store,
              product_id,
              updated_media,
            )
          mutation_result(
            key,
            product_update_media_payload(changed_media, [], field, fragments),
            next_store,
            identity,
            changed_media |> list.filter_map(media_record_id_result),
          )
        }
      }
  }
}

@internal
pub fn reorder_product_media(
  store: Store,
  product_id: String,
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(Store, List(ProductUserError)) {
  let #(moves, user_errors) = read_collection_product_moves(raw_moves)
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  let media_ids =
    effective_media
    |> list.filter_map(media_record_id_result)
  let user_errors =
    list.fold(enumerate_items(moves), user_errors, fn(errors, entry) {
      let #(move, index) = entry
      let CollectionProductMove(id: media_id, new_position: _) = move
      case list.contains(media_ids, media_id) {
        True -> errors
        False ->
          list.append(errors, [
            ProductUserError(
              ["moves", int.to_string(index), "id"],
              "Media does not exist",
              None,
            ),
          ])
      }
    })
  case user_errors {
    [] -> {
      let reordered_media =
        apply_product_media_moves(effective_media, moves)
        |> enumerate_items()
        |> list.map(fn(entry) {
          let #(media, position) = entry
          ProductMediaRecord(..media, position: position)
        })
      #(
        store.replace_staged_media_for_product(
          store,
          product_id,
          reordered_media,
        ),
        [],
      )
    }
    _ -> #(store, user_errors)
  }
}

@internal
pub fn stage_variant_media_memberships(
  store: Store,
  product_id: String,
  inputs: List(VariantMediaInput),
  is_append: Bool,
) -> #(Store, List(String), List(ProductUserError)) {
  let effective_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let product_media = store.get_effective_media_by_product_id(store, product_id)
  let #(next_variants, updated_variant_ids, user_errors) =
    list.fold(enumerate_items(inputs), #([], [], []), fn(acc, item) {
      let #(updated_variants, updated_ids, errors) = acc
      let #(entry, index) = item
      let VariantMediaInput(variant_id: variant_id, media_ids: media_ids) =
        entry
      case find_variant_by_id(effective_variants, variant_id) {
        None -> #(
          updated_variants,
          updated_ids,
          list.append(errors, [
            variant_media_variant_error(index),
          ]),
        )
        Some(variant) -> {
          let media_errors =
            variant_media_input_errors(
              variant,
              product_media,
              media_ids,
              index,
              is_append,
            )
          case media_errors {
            [] -> {
              let next_media_ids = case is_append {
                True ->
                  dedupe_preserving_order(list.append(
                    variant.media_ids,
                    media_ids,
                  ))
                False ->
                  list.filter(variant.media_ids, fn(media_id) {
                    !list.contains(media_ids, media_id)
                  })
              }
              #(
                [
                  ProductVariantRecord(..variant, media_ids: next_media_ids),
                  ..updated_variants
                ],
                [variant.id, ..updated_ids],
                errors,
              )
            }
            _ -> #(
              updated_variants,
              updated_ids,
              list.append(errors, media_errors),
            )
          }
        }
      }
    })
  case user_errors {
    [] -> {
      let staged_variants =
        effective_variants
        |> list.map(fn(variant) {
          find_variant_by_id(next_variants, variant.id)
          |> option.unwrap(variant)
        })
      #(
        store.replace_staged_variants_for_product(
          store,
          product_id,
          staged_variants,
        ),
        list.reverse(updated_variant_ids),
        [],
      )
    }
    _ -> #(store, list.reverse(updated_variant_ids), user_errors)
  }
}

@internal
pub fn invalid_create_media_content_type(
  args: Dict(String, ResolvedValue),
  document: String,
) -> Option(Json) {
  case dict.get(args, "media") {
    Ok(ListVal(values)) ->
      values
      |> enumerate_items()
      |> list.find_map(fn(entry) {
        let #(value, index) = entry
        case value {
          ObjectVal(fields) ->
            case read_string_field(fields, "mediaContentType") {
              Some(media_content_type) ->
                case is_create_media_content_type(media_content_type) {
                  True -> Error(Nil)
                  False ->
                    Ok(invalid_product_media_content_type_variable_error(
                      values,
                      index,
                      media_content_type,
                      document,
                    ))
                }
              None -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

// ===== from media_l05 =====
@internal
pub fn handle_product_reorder_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        product_reorder_media_payload(
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
            product_reorder_media_payload(
              None,
              [ProductUserError(["id"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let #(next_store, user_errors) =
            reorder_product_media(
              store,
              product_id,
              read_arg_object_list(args, "moves"),
            )
          case user_errors {
            [] -> {
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              mutation_result(
                key,
                product_reorder_media_payload(
                  Some(job_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [product_id],
              )
            }
            _ ->
              mutation_result(
                key,
                product_reorder_media_payload(
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
          }
        }
      }
  }
}
