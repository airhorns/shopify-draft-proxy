//// Products-domain submodule: collections_core.
//// Combines layered files: collections_l00, collections_l01, collections_l02, collections_l03, collections_l04, collections_l05, collections_l06, collections_l07, collections_l08, collections_l09, collections_l10.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order

import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, IntVal, ObjectVal, StringVal,
}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcList, SrcNull, SrcString,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, project_graphql_value, src_object,
}

import shopify_draft_proxy/proxy/products/product_types.{
  type CollectionProductMove, type CollectionProductPlacement,
  type CollectionRuleSetPresence, type MutationFieldResult,
  type ProductUserError, AppendProducts, CollectionProductMove,
  PrependReverseProducts, ProductUserError, RuleSetAbsent, RuleSetCustom,
  RuleSetSmart, blank_product_user_error, collection_handle_character_limit,
  collection_title_character_limit,
}
import shopify_draft_proxy/proxy/products/products_core.{
  dedup_base_and_next_suffix, ensure_unique_handle, enumerate_items,
  enumerate_strings, normalize_product_handle, updated_product_seo,
}
import shopify_draft_proxy/proxy/products/products_records.{product_source}
import shopify_draft_proxy/proxy/products/shared.{
  connection_page_info_source, count_source, dedupe_preserving_order, job_source,
  mutation_error_result, mutation_result, parse_unsigned_int_string,
  read_arg_object_list, read_arg_string_list, read_bool_argument,
  read_bool_field, read_non_empty_string_field, read_object_field,
  read_object_list_field, read_string_argument, read_string_field,
  read_string_list_field, resource_id_matches, resource_tail, user_errors_source,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  optional_int_json, optional_string_json, product_search_parse_options,
  product_string_match_options,
}

import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type AdminPlatformGenericNodeRecord, type CollectionImageRecord,
  type CollectionRecord, type CollectionRuleRecord, type CollectionRuleSetRecord,
  type ProductCollectionRecord, type ProductRecord,
  AdminPlatformGenericNodeRecord, CapturedBool, CapturedObject, CapturedString,
  CollectionRecord, CollectionRuleRecord, CollectionRuleSetRecord,
  ProductCollectionRecord, ProductSeoRecord,
}

const collection_product_ids_input_limit = 250

// ===== from collections_l00 =====
@internal
pub fn serialize_collection_rule(
  rule: CollectionRuleRecord,
  selections: List(Selection),
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "column" -> json.string(rule.column)
            "relation" -> json.string(rule.relation)
            "condition" -> json.string(rule.condition)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

@internal
pub fn collections_published_to_publication(
  store: Store,
  publication_id: String,
) -> List(CollectionRecord) {
  store.list_effective_collections(store)
  |> list.filter(fn(collection) {
    list.contains(collection.publication_ids, publication_id)
  })
}

@internal
pub fn compare_collections_by_sort_key(
  left: CollectionRecord,
  right: CollectionRecord,
  sort_key: Option(String),
) -> order.Order {
  case sort_key {
    Some("TITLE") ->
      case string.compare(left.title, right.title) {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    Some("UPDATED_AT") ->
      case
        resource_ids.compare_nullable_strings(left.updated_at, right.updated_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
  }
}

@internal
pub fn collection_product_cursor(
  entry: #(ProductRecord, ProductCollectionRecord),
  _index: Int,
) -> String {
  let #(_, membership) = entry
  membership.cursor |> option.unwrap(membership.product_id)
}

@internal
pub fn product_collection_cursor(
  entry: #(CollectionRecord, ProductCollectionRecord),
  _index: Int,
) -> String {
  let #(collection, membership) = entry
  membership.cursor |> option.unwrap(collection.id)
}

@internal
pub fn collection_products_count(
  store: Store,
  collection: CollectionRecord,
) -> Int {
  collection.products_count
  |> option.unwrap(
    list.length(store.list_effective_products_for_collection(
      store,
      collection.id,
    )),
  )
}

@internal
pub fn collection_rule_set_source(
  rule_set: Option(CollectionRuleSetRecord),
) -> SourceValue {
  case rule_set {
    None -> SrcNull
    Some(rule_set) ->
      src_object([
        #("appliedDisjunctively", SrcBool(rule_set.applied_disjunctively)),
        #(
          "rules",
          SrcList(
            list.map(rule_set.rules, fn(rule) {
              src_object([
                #("column", SrcString(rule.column)),
                #("relation", SrcString(rule.relation)),
                #("condition", SrcString(rule.condition)),
              ])
            }),
          ),
        ),
      ])
  }
}

@internal
pub fn collection_rule_set_has_rules(
  rule_set: CollectionRuleSetRecord,
) -> Bool {
  case rule_set.rules {
    [] -> False
    _ -> True
  }
}

@internal
pub fn insert_collection_entry(
  entries: List(#(ProductRecord, ProductCollectionRecord)),
  entry: #(ProductRecord, ProductCollectionRecord),
  position: Int,
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  let insertion_index = int.min(position, list.length(entries))
  let before = list.take(entries, insertion_index)
  let after = list.drop(entries, insertion_index)
  list.append(before, [entry, ..after])
}

@internal
pub fn product_already_in_collection(
  store: Store,
  collection_id: String,
  product_id: String,
) -> Bool {
  store.list_effective_collections_for_product(store, product_id)
  |> list.any(fn(entry) {
    let #(collection, _) = entry
    collection.id == collection_id
  })
}

// ===== from collections_l01 =====
@internal
pub fn collection_by_identifier(
  store: Store,
  identifier: Dict(String, ResolvedValue),
) -> Option(CollectionRecord) {
  case read_string_field(identifier, "id") {
    Some(id) -> store.get_effective_collection_by_id(store, id)
    None ->
      case read_string_field(identifier, "handle") {
        Some(handle) -> store.get_effective_collection_by_handle(store, handle)
        None -> None
      }
  }
}

@internal
pub fn serialize_collection_image(
  image: Option(CollectionImageRecord),
  selections: List(Selection),
) -> Json {
  case image {
    None -> json.null()
    Some(image) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "id" -> optional_string_json(image.id)
                "altText" -> optional_string_json(image.alt_text)
                "url" | "src" | "originalSrc" | "transformedSrc" ->
                  optional_string_json(image.url)
                "width" -> optional_int_json(image.width)
                "height" -> optional_int_json(image.height)
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        }),
      )
  }
}

@internal
pub fn serialize_collection_rule_set(
  rule_set: Option(CollectionRuleSetRecord),
  selections: List(Selection),
) -> Json {
  case rule_set {
    None -> json.null()
    Some(rule_set) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "appliedDisjunctively" ->
                  json.bool(rule_set.applied_disjunctively)
                "rules" ->
                  json.array(rule_set.rules, fn(rule) {
                    serialize_collection_rule(
                      rule,
                      get_selected_child_fields(
                        selection,
                        default_selected_field_options(),
                      ),
                    )
                  })
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        }),
      )
  }
}

@internal
pub fn collection_has_product_id(
  store: Store,
  collection_id: String,
  raw_value: String,
) -> Bool {
  let normalized =
    search_query_parser.strip_search_query_value_quotes(raw_value)
    |> string.trim
  store.list_effective_products_for_collection(store, collection_id)
  |> list.any(fn(entry) {
    let #(product, _) = entry
    product.id == normalized
    || product.legacy_resource_id == Some(normalized)
    || resource_tail(product.id) == normalized
    || resource_tail(normalized) == resource_tail(product.id)
  })
}

@internal
pub fn sort_collections(
  collections: List(CollectionRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(CollectionRecord) {
  let sort_key = read_string_argument(field, variables, "sortKey")
  let sorted =
    list.sort(collections, fn(left, right) {
      compare_collections_by_sort_key(left, right, sort_key)
    })
  case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(sorted)
    _ -> sorted
  }
}

@internal
pub fn collection_cursor_for_field(
  collection: CollectionRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> String {
  case read_string_argument(field, variables, "sortKey") {
    Some("TITLE") ->
      collection.title_cursor
      |> option.unwrap(option.unwrap(collection.cursor, collection.id))
    Some("UPDATED_AT") ->
      collection.updated_at_cursor
      |> option.unwrap(option.unwrap(collection.cursor, collection.id))
    _ -> option.unwrap(collection.cursor, collection.id)
  }
}

@internal
pub fn collection_has_product(
  store: Store,
  collection_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_string_argument(field, variables, "id") {
    Some(product_id) ->
      store.list_effective_products_for_collection(store, collection_id)
      |> list.any(fn(entry) {
        let #(product, _) = entry
        product.id == product_id
      })
    None -> False
  }
}

@internal
pub fn collection_title_validation_errors(
  title: String,
) -> List(ProductUserError) {
  case string.length(title) > collection_title_character_limit {
    True -> [
      ProductUserError(
        ["title"],
        "Title is too long (maximum is 255 characters)",
        Some("INVALID"),
      ),
    ]
    False -> []
  }
}

@internal
pub fn collection_handle_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case read_string_field(input, "handle") {
    Some(handle) ->
      case string.length(handle) > collection_handle_character_limit {
        True -> [
          ProductUserError(
            ["handle"],
            "Handle is too long (maximum is 255 characters)",
            Some("INVALID"),
          ),
        ]
        False -> []
      }
    _ -> []
  }
}

@internal
pub fn collection_is_smart(collection: CollectionRecord) -> Bool {
  case collection.rule_set {
    Some(rule_set) -> collection_rule_set_has_rules(rule_set)
    None -> collection.is_smart
  }
}

fn collection_product_ids_max_input_size_error(
  root_name: String,
  product_ids: List(String),
  field: Selection,
  document: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The input array size of "
        <> int.to_string(list.length(product_ids))
        <> " is greater than the maximum allowed of "
        <> int.to_string(collection_product_ids_input_limit)
        <> ".",
      ),
    ),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #("path", json.array([root_name, "productIds"], json.string)),
    #(
      "extensions",
      json.object([#("code", json.string("MAX_INPUT_SIZE_EXCEEDED"))]),
    ),
  ])
}

fn product_ids_exceed_collection_input_limit(
  product_ids: List(String),
) -> Bool {
  list.length(product_ids) > collection_product_ids_input_limit
}

fn collection_job_record(job_id: String) -> AdminPlatformGenericNodeRecord {
  AdminPlatformGenericNodeRecord(
    id: job_id,
    typename: "Job",
    data: CapturedObject([
      #("id", CapturedString(job_id)),
      #("done", CapturedBool(True)),
      #("query", CapturedObject([#("__typename", CapturedString("QueryRoot"))])),
    ]),
  )
}

fn stage_collection_job(store: Store, job_id: String) -> Store {
  store.upsert_staged_admin_platform_generic_nodes(store, [
    collection_job_record(job_id),
  ])
}

@internal
pub fn remove_products_from_collection(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
) -> Store {
  let normalized_product_ids = dedupe_preserving_order(product_ids)
  list.fold(normalized_product_ids, store, fn(current_store, product_id) {
    case store.get_effective_product_by_id(current_store, product_id) {
      None -> current_store
      Some(_) -> {
        let next_memberships =
          store.list_effective_collections_for_product(
            current_store,
            product_id,
          )
          |> list.filter(fn(entry) {
            let #(existing_collection, _) = entry
            existing_collection.id != collection.id
          })
          |> list.map(fn(entry) {
            let #(_, membership) = entry
            membership
          })
        store.replace_staged_collections_for_product(
          current_store,
          product_id,
          next_memberships,
        )
      }
    }
  })
}

@internal
pub fn apply_collection_product_moves(
  entries: List(#(ProductRecord, ProductCollectionRecord)),
  moves: List(CollectionProductMove),
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  list.fold(moves, entries, fn(current_entries, move) {
    let CollectionProductMove(id: product_id, new_position: new_position) = move
    case
      list.find(current_entries, fn(entry) {
        let #(product, _) = entry
        product.id == product_id
      })
    {
      Error(_) -> current_entries
      Ok(entry) -> {
        let without_entry =
          current_entries
          |> list.filter(fn(candidate) {
            let #(product, _) = candidate
            product.id != product_id
          })
        insert_collection_entry(without_entry, entry, new_position)
      }
    }
  })
}

@internal
pub fn read_collection_product_ids(
  input: Dict(String, ResolvedValue),
) -> List(String) {
  read_string_list_field(input, "products")
  |> option.unwrap([])
  |> dedupe_preserving_order
}

@internal
pub fn read_collection_rule_set(
  input: Dict(String, ResolvedValue),
) -> Option(CollectionRuleSetRecord) {
  use rule_set <- option.then(read_object_field(input, "ruleSet"))
  Some(CollectionRuleSetRecord(
    applied_disjunctively: read_bool_field(rule_set, "appliedDisjunctively")
      |> option.unwrap(False),
    rules: read_object_list_field(rule_set, "rules")
      |> list.filter_map(fn(rule) {
        case
          read_string_field(rule, "column"),
          read_string_field(rule, "relation"),
          read_string_field(rule, "condition")
        {
          Some(column), Some(relation), Some(condition) ->
            Ok(CollectionRuleRecord(
              column: column,
              relation: relation,
              condition: condition,
            ))
          _, _, _ -> Error(Nil)
        }
      }),
  ))
}

// ===== from collections_l02 =====
@internal
pub fn collection_matches_positive_query_term(
  store: Store,
  collection: CollectionRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let value = search_query_parser.search_query_term_value(term)
  case option.map(term.field, string.lowercase) {
    None ->
      search_query_parser.matches_search_query_string(
        Some(collection.title),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
      || search_query_parser.matches_search_query_string(
        Some(collection.handle),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("title") ->
      search_query_parser.matches_search_query_string(
        Some(collection.title),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("handle") ->
      search_query_parser.matches_search_query_string(
        Some(collection.handle),
        value,
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("collection_type") -> {
      let normalized =
        search_query_parser.strip_search_query_value_quotes(value)
        |> string.trim
        |> string.lowercase
      case normalized {
        "smart" -> collection.is_smart
        "custom" -> !collection.is_smart
        _ -> True
      }
    }
    Some("id") ->
      resource_id_matches(collection.id, collection.legacy_resource_id, value)
    Some("product_id") -> collection_has_product_id(store, collection.id, value)
    Some("updated_at") -> True
    Some("product_publication_status")
    | Some("publishable_status")
    | Some("published_at")
    | Some("published_status") -> True
    _ -> True
  }
}

@internal
pub fn collection_rule_set_presence(
  input: Dict(String, ResolvedValue),
) -> CollectionRuleSetPresence {
  case dict.get(input, "ruleSet") {
    Error(_) -> RuleSetAbsent
    Ok(ObjectVal(_)) ->
      case read_collection_rule_set(input) {
        Some(rule_set) ->
          case collection_rule_set_has_rules(rule_set) {
            True -> RuleSetSmart
            False -> RuleSetCustom
          }
        None -> RuleSetCustom
      }
    _ -> RuleSetCustom
  }
}

@internal
pub fn read_collection_reorder_position(
  fields: Dict(String, ResolvedValue),
) -> Option(Int) {
  case dict.get(fields, "newPosition") {
    Ok(IntVal(value)) -> Some(int.max(0, value))
    Ok(StringVal(value)) -> parse_unsigned_int_string(value)
    _ -> None
  }
}

@internal
pub fn collection_add_products_v2_payload(
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
      #("__typename", SrcString("CollectionAddProductsV2Payload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn collection_delete_payload(
  deleted_collection_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let deleted_value = case deleted_collection_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionDeletePayload")),
      #("deletedCollectionId", deleted_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn collection_remove_products_payload(
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
      #("__typename", SrcString("CollectionRemoveProductsPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn collection_reorder_products_payload(
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
      #("__typename", SrcString("CollectionReorderProductsPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn updated_collection_record(
  identity: SyntheticIdentityRegistry,
  collection: CollectionRecord,
  input: Dict(String, ResolvedValue),
) -> #(CollectionRecord, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap(collection.title)
  let handle =
    read_non_empty_string_field(input, "handle")
    |> option.unwrap(collection.handle)
  let rule_set =
    read_collection_rule_set(input)
    |> option.or(collection.rule_set)
  #(
    CollectionRecord(
      ..collection,
      title: title,
      handle: handle,
      updated_at: Some(updated_at),
      description: read_string_field(input, "description")
        |> option.or(collection.description),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.or(collection.description_html),
      sort_order: read_string_field(input, "sortOrder")
        |> option.or(collection.sort_order),
      template_suffix: read_string_field(input, "templateSuffix")
        |> option.or(collection.template_suffix),
      seo: updated_product_seo(collection.seo, input),
      rule_set: rule_set,
      is_smart: rule_set
        |> option.map(collection_rule_set_has_rules)
        |> option.unwrap(collection.is_smart),
    ),
    next_identity,
  )
}

@internal
pub fn slugify_collection_handle(title: String) -> String {
  let normalized = normalize_product_handle(title)
  let handle = case normalized {
    "" -> "untitled-collection"
    _ -> normalized
  }
  case string.ends_with(handle, "product") {
    True -> string.drop_end(handle, 7) <> "collection"
    False -> handle
  }
}

@internal
pub fn collection_handle_should_dedupe(
  input: Dict(String, ResolvedValue),
) -> Bool {
  case read_non_empty_string_field(input, "handle") {
    Some(_) -> False
    None -> True
  }
}

// ===== from collections_l03 =====
@internal
pub fn filtered_collections(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(CollectionRecord) {
  search_query_parser.apply_search_query(
    store.list_effective_collections(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    fn(collection, term) {
      collection_matches_positive_query_term(store, collection, term)
    },
  )
}

@internal
pub fn collection_create_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  let title_errors = case read_non_empty_string_field(input, "title") {
    None -> [blank_product_user_error(["title"], "Title can't be blank")]
    Some(title) -> collection_title_validation_errors(title)
  }
  list.append(title_errors, collection_handle_validation_errors(input))
}

@internal
pub fn collection_type_update_errors(
  collection: CollectionRecord,
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case collection_is_smart(collection), collection_rule_set_presence(input) {
    False, RuleSetSmart -> [
      ProductUserError(
        ["id"],
        "Cannot update rule set of a custom collection",
        None,
      ),
    ]
    True, RuleSetCustom -> [
      ProductUserError(
        ["id"],
        "Cannot update rule set of a smart collection",
        None,
      ),
    ]
    _, _ -> []
  }
}

@internal
pub fn stage_collection_product_memberships(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
  placement: CollectionProductPlacement,
) -> #(Store, Option(CollectionRecord), List(ProductUserError)) {
  let existing_product_ids =
    product_ids
    |> list.filter(fn(product_id) {
      case store.get_effective_product_by_id(store, product_id) {
        Some(_) -> True
        None -> False
      }
    })
  case existing_product_ids {
    [] -> #(store, Some(collection), [])
    _ -> {
      let existing_positions =
        store.list_effective_products_for_collection(store, collection.id)
        |> list.map(fn(entry) {
          let #(_, membership) = entry
          membership.position
        })
      let first_position = case placement {
        AppendProducts ->
          case existing_positions {
            [] -> 0
            _ -> {
              list.fold(existing_positions, -1, int.max) + 1
            }
          }
        PrependReverseProducts ->
          case existing_positions {
            [] -> 0
            [first, ..rest] ->
              list.fold(rest, first, int.min)
              - list.length(existing_product_ids)
          }
      }
      let positioned_product_ids = case placement {
        AppendProducts -> existing_product_ids
        PrependReverseProducts -> list.reverse(existing_product_ids)
      }
      let existing_memberships =
        store.list_effective_products_for_collection(store, collection.id)
      let memberships =
        positioned_product_ids
        |> enumerate_strings()
        |> list.map(fn(entry) {
          let #(product_id, index) = entry
          ProductCollectionRecord(
            collection_id: collection.id,
            product_id: product_id,
            position: first_position + index,
            cursor: None,
          )
        })
      let next_count =
        list.length(existing_memberships) + list.length(memberships)
      let next_collection =
        CollectionRecord(..collection, products_count: Some(next_count))
      let next_store =
        store
        |> store.upsert_staged_collections([next_collection])
        |> store.upsert_staged_product_collections(memberships)
      #(next_store, Some(next_collection), [])
    }
  }
}

@internal
pub fn handle_collection_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let collection_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case collection_id {
    None ->
      mutation_result(
        key,
        collection_delete_payload(
          None,
          [
            ProductUserError(["input", "id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_delete_payload(
              None,
              [
                ProductUserError(["input", "id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let next_store = store.delete_staged_collection(store, collection_id)
          mutation_result(
            key,
            collection_delete_payload(Some(collection_id), [], field, fragments),
            next_store,
            identity,
            [collection_id],
          )
        }
      }
  }
}

@internal
pub fn handle_collection_remove_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  document: String,
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_ids = read_arg_string_list(args, "productIds")
  case product_ids_exceed_collection_input_limit(product_ids) {
    True ->
      mutation_error_result(key, store, identity, [
        collection_product_ids_max_input_size_error(
          "collectionRemoveProducts",
          product_ids,
          field,
          document,
        ),
      ])
    False ->
      case graphql_helpers.read_arg_string(args, "id") {
        None ->
          mutation_result(
            key,
            collection_remove_products_payload(
              None,
              [
                ProductUserError(["id"], "Collection id is required", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection_id) ->
          case store.get_effective_collection_by_id(store, collection_id) {
            None ->
              mutation_result(
                key,
                collection_remove_products_payload(
                  None,
                  [
                    ProductUserError(["id"], "Collection not found", None),
                  ],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(collection) ->
              case collection_is_smart(collection) {
                True ->
                  mutation_result(
                    key,
                    collection_remove_products_payload(
                      None,
                      [
                        ProductUserError(
                          ["id"],
                          "Can't manually remove products from a smart collection",
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
                False -> {
                  let next_store =
                    remove_products_from_collection(
                      store,
                      collection,
                      product_ids,
                    )
                  let next_count =
                    store.list_effective_products_for_collection(
                      next_store,
                      collection.id,
                    )
                    |> list.length
                  let next_collection =
                    CollectionRecord(
                      ..collection,
                      products_count: Some(next_count),
                    )
                  let next_store =
                    store.upsert_staged_collections(next_store, [
                      next_collection,
                    ])
                  let #(job_id, next_identity) =
                    synthetic_identity.make_synthetic_gid(identity, "Job")
                  let next_store = stage_collection_job(next_store, job_id)
                  mutation_result(
                    key,
                    collection_remove_products_payload(
                      Some(job_id),
                      [],
                      field,
                      fragments,
                    ),
                    next_store,
                    next_identity,
                    [collection.id],
                  )
                }
              }
          }
      }
  }
}

@internal
pub fn read_collection_product_moves(
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(List(CollectionProductMove), List(ProductUserError)) {
  case raw_moves {
    [] -> #([], [
      ProductUserError(
        ["moves"],
        "At least one move is required",
        Some("INVALID_MOVE"),
      ),
    ])
    _ -> {
      let too_many_errors = case list.length(raw_moves) > 250 {
        True -> [
          ProductUserError(
            ["moves"],
            "Too many moves were provided",
            Some("INVALID_MOVE"),
          ),
        ]
        False -> []
      }
      let result =
        raw_moves
        |> enumerate_items()
        |> list.fold(#([], too_many_errors), fn(acc, entry) {
          let #(moves, errors) = acc
          let #(raw_move, index) = entry
          let product_id = read_string_field(raw_move, "id")
          let new_position = read_collection_reorder_position(raw_move)
          let errors = case product_id {
            Some(_) -> errors
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "id"],
                  "Product id is required",
                  Some("INVALID_MOVE"),
                ),
              ])
          }
          let errors = case new_position {
            Some(_) -> errors
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "newPosition"],
                  "Position is invalid",
                  Some("INVALID_MOVE"),
                ),
              ])
          }
          case product_id, new_position {
            Some(id), Some(position) -> #(
              list.append(moves, [
                CollectionProductMove(id: id, new_position: position),
              ]),
              errors,
            )
            _, _ -> #(moves, errors)
          }
        })
      result
    }
  }
}

@internal
pub fn ensure_unique_collection_handle(store: Store, handle: String) -> String {
  let in_use = fn(candidate) {
    store.get_effective_collection_by_handle(store, candidate) != None
  }
  case in_use(handle) {
    True -> {
      let #(base_handle, suffix) = dedup_base_and_next_suffix(handle)
      ensure_unique_handle(base_handle, suffix, in_use)
    }
    False -> handle
  }
}

// ===== from collections_l04 =====
@internal
pub fn collection_update_validation_errors(
  collection: CollectionRecord,
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  let title_errors = case read_string_field(input, "title") {
    Some(title) -> collection_title_validation_errors(title)
    _ -> []
  }
  list.append(
    title_errors,
    list.append(
      collection_handle_validation_errors(input),
      collection_type_update_errors(collection, input),
    ),
  )
}

@internal
pub fn add_products_to_collection(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
  placement: CollectionProductPlacement,
) -> #(Store, Option(CollectionRecord), List(ProductUserError)) {
  let normalized_product_ids = dedupe_preserving_order(product_ids)
  case normalized_product_ids {
    [] -> #(store, None, [
      ProductUserError(
        ["productIds"],
        "At least one product id is required",
        None,
      ),
    ])
    _ ->
      case collection_is_smart(collection) {
        True -> #(store, None, [
          ProductUserError(
            ["id"],
            "Can't manually add products to a smart collection",
            None,
          ),
        ])
        False ->
          case
            list.find(normalized_product_ids, fn(product_id) {
              product_already_in_collection(store, collection.id, product_id)
            })
          {
            Ok(_) -> #(store, None, [
              ProductUserError(
                ["productIds"],
                "Product is already in the collection",
                None,
              ),
            ])
            Error(_) ->
              stage_collection_product_memberships(
                store,
                collection,
                normalized_product_ids,
                placement,
              )
          }
      }
  }
}

@internal
pub fn reorder_collection_products(
  store: Store,
  collection: CollectionRecord,
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(Store, List(ProductUserError)) {
  case
    collection.is_smart
    || case collection.sort_order {
      Some(sort_order) -> sort_order != "MANUAL"
      None -> False
    }
  {
    True -> #(store, [
      ProductUserError(
        ["id"],
        "Can't reorder products unless collection is manually sorted",
        Some("MANUALLY_SORTED_COLLECTION"),
      ),
    ])
    False -> {
      let #(moves, user_errors) = read_collection_product_moves(raw_moves)
      let ordered_entries =
        store.list_effective_products_for_collection(store, collection.id)
      let product_ids_in_collection =
        ordered_entries
        |> list.map(fn(entry) {
          let #(product, _) = entry
          product.id
        })
      let user_errors =
        list.fold(enumerate_items(moves), user_errors, fn(errors, entry) {
          let #(move, index) = entry
          let CollectionProductMove(id: product_id, new_position: _) = move
          case store.get_effective_product_by_id(store, product_id) {
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "id"],
                  "Product does not exist",
                  Some("INVALID_MOVE"),
                ),
              ])
            Some(_) ->
              case list.contains(product_ids_in_collection, product_id) {
                True -> errors
                False ->
                  list.append(errors, [
                    ProductUserError(
                      ["moves", int.to_string(index), "id"],
                      "Product is not in the collection",
                      Some("INVALID_MOVE"),
                    ),
                  ])
              }
          }
        })
      case user_errors {
        [] -> {
          let reordered_entries =
            apply_collection_product_moves(ordered_entries, moves)
          let next_store =
            reordered_entries
            |> enumerate_items()
            |> list.fold(store, fn(current_store, entry) {
              let #(#(product, _), position) = entry
              let next_memberships =
                store.list_effective_collections_for_product(
                  current_store,
                  product.id,
                )
                |> list.map(fn(collection_entry) {
                  let #(existing_collection, membership) = collection_entry
                  case existing_collection.id == collection.id {
                    True ->
                      ProductCollectionRecord(..membership, position: position)
                    False -> membership
                  }
                })
              store.replace_staged_collections_for_product(
                current_store,
                product.id,
                next_memberships,
              )
            })
          #(next_store, [])
        }
        _ -> #(store, user_errors)
      }
    }
  }
}

@internal
pub fn created_collection_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(CollectionRecord, SyntheticIdentityRegistry) {
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap("Untitled collection")
  let #(updated_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_timestamp,
      "Collection",
    )
  let handle = case read_non_empty_string_field(input, "handle") {
    Some(handle) -> normalize_product_handle(handle)
    None -> slugify_collection_handle(title)
  }
  let handle = case collection_handle_should_dedupe(input) {
    True -> ensure_unique_collection_handle(store, handle)
    False -> handle
  }
  let rule_set = read_collection_rule_set(input)
  #(
    CollectionRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: handle,
      publication_ids: [],
      updated_at: Some(updated_at),
      description: read_string_field(input, "description"),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.or(Some("")),
      image: None,
      sort_order: read_string_field(input, "sortOrder")
        |> option.or(Some("BEST_SELLING")),
      template_suffix: read_string_field(input, "templateSuffix"),
      seo: updated_product_seo(
        ProductSeoRecord(title: None, description: None),
        input,
      ),
      rule_set: rule_set,
      products_count: Some(0),
      is_smart: rule_set
        |> option.map(collection_rule_set_has_rules)
        |> option.unwrap(False),
      cursor: None,
      title_cursor: None,
      updated_at_cursor: None,
    ),
    next_identity,
  )
}

// ===== from collections_l05 =====
@internal
pub fn handle_collection_add_products_v2(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  document: String,
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_ids = read_arg_string_list(args, "productIds")
  case product_ids_exceed_collection_input_limit(product_ids) {
    True ->
      mutation_error_result(key, store, identity, [
        collection_product_ids_max_input_size_error(
          "collectionAddProductsV2",
          product_ids,
          field,
          document,
        ),
      ])
    False ->
      case graphql_helpers.read_arg_string(args, "id") {
        None ->
          mutation_result(
            key,
            collection_add_products_v2_payload(
              None,
              [ProductUserError(["id"], "Collection id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection_id) ->
          case store.get_effective_collection_by_id(store, collection_id) {
            None ->
              mutation_result(
                key,
                collection_add_products_v2_payload(
                  None,
                  [ProductUserError(["id"], "Collection does not exist", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(collection) -> {
              let placement = case collection.sort_order {
                Some("MANUAL") -> AppendProducts
                _ -> PrependReverseProducts
              }
              let #(next_store, result_collection, user_errors) =
                add_products_to_collection(
                  store,
                  collection,
                  product_ids,
                  placement,
                )
              case user_errors, result_collection {
                [], Some(record) -> {
                  let #(job_id, next_identity) =
                    synthetic_identity.make_synthetic_gid(identity, "Job")
                  let next_store = stage_collection_job(next_store, job_id)
                  mutation_result(
                    key,
                    collection_add_products_v2_payload(
                      Some(job_id),
                      [],
                      field,
                      fragments,
                    ),
                    next_store,
                    next_identity,
                    [record.id],
                  )
                }
                _, _ ->
                  mutation_result(
                    key,
                    collection_add_products_v2_payload(
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
}

@internal
pub fn handle_collection_reorder_products(
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
        collection_reorder_products_payload(
          None,
          [
            ProductUserError(["id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_reorder_products_payload(
              None,
              [
                ProductUserError(
                  ["id"],
                  "Collection not found",
                  Some("COLLECTION_NOT_FOUND"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let result =
            reorder_collection_products(
              store,
              collection,
              read_arg_object_list(args, "moves"),
            )
          let #(next_store, user_errors) = result
          case user_errors {
            [] -> {
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              mutation_result(
                key,
                collection_reorder_products_payload(
                  Some(job_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [collection.id],
              )
            }
            _ ->
              mutation_result(
                key,
                collection_reorder_products_payload(
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

// ===== from collections_l06 =====
@internal
pub fn collection_products_connection_source(
  store: Store,
  collection: CollectionRecord,
) -> SourceValue {
  let products =
    store.list_effective_products_for_collection(store, collection.id)
  let edges =
    products
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(entry, index) = pair
      let #(product, _) = entry
      src_object([
        #("cursor", SrcString(collection_product_cursor(entry, index))),
        #("node", product_source(product)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(products, fn(entry) {
          let #(product, _) = entry
          product_source(product)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(products, collection_product_cursor),
    ),
  ])
}

// ===== from collections_l07 =====
@internal
pub fn collection_source_with_store_and_publication(
  store: Store,
  collection: CollectionRecord,
  publication_id: Option(String),
) -> SourceValue {
  let publication_count = list.length(collection.publication_ids)
  let published_on_publication = case publication_id {
    Some(id) -> list.contains(collection.publication_ids, id)
    None -> False
  }
  src_object([
    #("__typename", SrcString("Collection")),
    #("id", SrcString(collection.id)),
    #(
      "legacyResourceId",
      graphql_helpers.option_string_source(collection.legacy_resource_id),
    ),
    #("title", SrcString(collection.title)),
    #("handle", SrcString(collection.handle)),
    #("updatedAt", graphql_helpers.option_string_source(collection.updated_at)),
    #(
      "description",
      graphql_helpers.option_string_source(collection.description),
    ),
    #(
      "descriptionHtml",
      graphql_helpers.option_string_source(collection.description_html),
    ),
    #("publishedOnPublication", SrcBool(published_on_publication)),
    #("availablePublicationsCount", count_source(publication_count)),
    #("resourcePublicationsCount", count_source(publication_count)),
    #("publicationCount", count_source(publication_count)),
    #(
      "productsCount",
      count_source(collection_products_count(store, collection)),
    ),
    #("sortOrder", graphql_helpers.option_string_source(collection.sort_order)),
    #(
      "templateSuffix",
      graphql_helpers.option_string_source(collection.template_suffix),
    ),
    #("ruleSet", collection_rule_set_source(collection.rule_set)),
    #("products", collection_products_connection_source(store, collection)),
  ])
}

// ===== from collections_l08 =====
@internal
pub fn collection_source_with_store(
  store: Store,
  collection: CollectionRecord,
) -> SourceValue {
  collection_source_with_store_and_publication(store, collection, None)
}

// ===== from collections_l09 =====
@internal
pub fn serialize_collection_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_collection_by_id(store, id) {
    Some(collection) ->
      project_graphql_value(
        collection_source_with_store(store, collection),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

@internal
pub fn product_collections_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let collections =
    store.list_effective_collections_for_product(store, product.id)
  let edges =
    collections
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(entry, index) = pair
      let #(collection, _) = entry
      src_object([
        #("cursor", SrcString(product_collection_cursor(entry, index))),
        #("node", collection_source_with_store(store, collection)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(collections, fn(entry) {
          let #(collection, _) = entry
          collection_source_with_store(store, collection)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(collections, product_collection_cursor),
    ),
  ])
}

@internal
pub fn collection_add_products_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let collection_value = case collection {
    Some(record) -> collection_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionAddProductsPayload")),
      #("collection", collection_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn collection_update_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let collection_value = case collection {
    Some(record) -> collection_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionUpdatePayload")),
      #("collection", collection_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from collections_l10 =====
@internal
pub fn handle_collection_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let collection_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case collection_id {
    None ->
      mutation_result(
        key,
        collection_update_payload(
          store,
          None,
          [
            ProductUserError(["input", "id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id), input {
        None, _ ->
          mutation_result(
            key,
            collection_update_payload(
              store,
              None,
              [
                ProductUserError(["input", "id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection), Some(input) -> {
          let user_errors =
            collection_update_validation_errors(collection, input)
          case user_errors {
            [] -> {
              let #(next_collection, next_identity) =
                updated_collection_record(identity, collection, input)
              let next_store =
                store.upsert_staged_collections(store, [next_collection])
              mutation_result(
                key,
                collection_update_payload(
                  next_store,
                  Some(next_collection),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [next_collection.id],
              )
            }
            _ ->
              mutation_result(
                key,
                collection_update_payload(
                  store,
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
        Some(_), None ->
          mutation_result(
            key,
            collection_update_payload(
              store,
              None,
              [
                ProductUserError(
                  ["input", "id"],
                  "Collection id is required",
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
      }
  }
}

@internal
pub fn handle_collection_add_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  document: String,
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let product_ids = read_arg_string_list(args, "productIds")
  case product_ids_exceed_collection_input_limit(product_ids) {
    True ->
      mutation_error_result(key, store, identity, [
        collection_product_ids_max_input_size_error(
          "collectionAddProducts",
          product_ids,
          field,
          document,
        ),
      ])
    False ->
      case graphql_helpers.read_arg_string(args, "id") {
        None ->
          mutation_result(
            key,
            collection_add_products_payload(
              store,
              None,
              [
                ProductUserError(["id"], "Collection id is required", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection_id) ->
          case store.get_effective_collection_by_id(store, collection_id) {
            None ->
              mutation_result(
                key,
                collection_add_products_payload(
                  store,
                  None,
                  [
                    ProductUserError(["id"], "Collection does not exist", None),
                  ],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(collection) -> {
              let result =
                add_products_to_collection(
                  store,
                  collection,
                  product_ids,
                  AppendProducts,
                )
              let #(next_store, result_collection, user_errors) = result
              let staged_ids = case user_errors, result_collection {
                [], Some(record) -> [record.id]
                _, _ -> []
              }
              mutation_result(
                key,
                collection_add_products_payload(
                  next_store,
                  result_collection,
                  user_errors,
                  field,
                  fragments,
                ),
                next_store,
                identity,
                staged_ids,
              )
            }
          }
      }
  }
}
