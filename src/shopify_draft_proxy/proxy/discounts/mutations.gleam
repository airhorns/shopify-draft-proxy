//// Discounts mutation staging and validation dispatch.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/discounts/queries.{
  child_fields, compare_discount_timestamp,
  discount_matches_positive_search_term, discount_matches_type_filter,
  discount_node_source, discount_record_timestamp, filter_discounts,
  get_effective_discount_bulk_operation, handle_discount_query,
  handle_query_request, is_discount_query_root,
  local_has_discount_bulk_creation_id, local_has_discount_id,
  local_has_staged_discounts, process, reverse_order, root_query_payload,
  serialize_count, serialize_discount_connection, serialize_page_info,
  should_passthrough_in_live_hybrid, sort_discounts, sort_discounts_by_timestamp,
}
import shopify_draft_proxy/proxy/discounts/serializers.{
  app_discount_function_api_supported,
  app_discount_function_does_not_implement_error,
  app_discount_function_not_found_error,
  app_discount_missing_function_identifier_error,
  app_discount_multiple_function_identifiers_error, append_blank_title_error,
  apply_bulk_effects, automatic_record_from_hydrate_node,
  blank_subscription_field_error, blank_subscription_field_errors, bool_value,
  build_discount_record, bulk_invalid_saved_search_message,
  bulk_missing_selector_message, bulk_too_many_selector_message,
  bxgy_disallowed_subscription_errors, bxgy_disallowed_value_errors,
  bxgy_discount_on_quantity_quantity_blank_error,
  bxgy_missing_discount_on_quantity_errors, code_record_from_hydrate_node,
  customer_gets_fields, customer_gets_items_fields, customer_gets_value_fields,
  customer_gets_value_type_count, decimal_at_least, decimal_parts_at_least,
  default_discount_classes, derive_discount_status, derive_discount_status_ms,
  digits_only, discount_class_for_record, discount_classes_for_input,
  discount_code_blank_error, discount_record_from_hydrate,
  discount_type_from_typename, existing_discount_id, existing_discount_source,
  fetch_shop_subscription_capability, fetch_taken_code_error, has_object_field,
  has_subscription_validation_fields, infer_basic_discount_classes,
  input_or_existing_discount_source, input_value_is_present, invalid_date_range,
  invalid_free_shipping_combines, invalid_id_errors, is_bulk_rule_discount,
  items_targets_entitled_resources, json_get, json_get_string, json_to_code_pair,
  maybe_hydrate_discount, maybe_hydrate_discount_subscription_capability,
  maybe_hydrate_shopify_function, nested_has_all, non_null_node,
  normalize_function_api_type, owner_node_field, payload_json,
  primary_discount_class, product_discounts_with_tags_settings,
  read_bulk_saved_search_id, read_numeric_string,
  redeem_code_bulk_delete_target_ids, redeem_code_ids_matching_query,
  redeem_code_ids_selector_is_empty, redeem_code_ids_selector_present,
  redeem_code_matches_positive_search_term, selector_present, set_record_status,
  shop_sells_subscriptions_from_response, shopify_function_app_record_from_node,
  shopify_function_record_from_node, shopify_function_record_from_response,
  source_timestamp_ms, subscription_field_error, subscription_field_location,
  subscription_field_source, subscription_fields_not_permitted_errors,
  subscription_not_permitted_message, summary_for, synthetic_now,
  tag_add_remove_overlap, title_is_blank, trim_leading_zeroes, typename_for,
  validate_app_discount_function_input, validate_app_discount_function_reference,
  validate_basic_refs, validate_bulk_saved_search_selector,
  validate_bulk_search_selector, validate_bulk_selector, validate_bxgy_input,
  validate_cart_line_combination_tag_settings,
  validate_cart_line_combination_tag_top_level_errors,
  validate_context_customer_selection_conflict,
  validate_customer_gets_value_type_top_level_errors,
  validate_discount_code_input, validate_discount_input,
  validate_discount_items_refs, validate_discount_top_level_errors,
  validate_discount_update_input, validate_minimum_quantity_limit,
  validate_minimum_requirement, validate_minimum_subtotal_limit,
  validate_redeem_code_bulk_delete_after_hydrate,
  validate_redeem_code_bulk_delete_saved_search_selector,
  validate_redeem_code_bulk_delete_search_selector,
  validate_redeem_code_bulk_delete_selector_shape,
  validate_subscription_field_values, validate_subscription_fields,
}
import shopify_draft_proxy/proxy/discounts/types as discount_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SelectedFieldOptions, SrcBool, SrcFloat,
  SrcInt, SrcList, SrcNull, SrcObject, SrcString, field_locations_json,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  project_graphql_value,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, type RequiredArgument, MutationOutcome, RequiredArgument,
  single_root_log_draft, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DiscountBulkOperationRecord, type DiscountRecord,
  type ShopifyFunctionAppRecord, type ShopifyFunctionRecord, CapturedArray,
  CapturedBool, CapturedFloat, CapturedInt, CapturedNull, CapturedObject,
  CapturedString, DiscountBulkOperationRecord, DiscountRecord,
  ShopifyFunctionAppRecord, ShopifyFunctionRecord,
}

@internal
pub fn is_discount_mutation_root(name: String) -> Bool {
  case name {
    "discountCodeBasicCreate"
    | "discountCodeBasicUpdate"
    | "discountCodeBxgyCreate"
    | "discountCodeBxgyUpdate"
    | "discountCodeFreeShippingCreate"
    | "discountCodeFreeShippingUpdate"
    | "discountCodeAppCreate"
    | "discountCodeAppUpdate"
    | "discountCodeActivate"
    | "discountCodeDeactivate"
    | "discountCodeDelete"
    | "discountCodeBulkActivate"
    | "discountCodeBulkDeactivate"
    | "discountCodeBulkDelete"
    | "discountRedeemCodeBulkAdd"
    | "discountCodeRedeemCodeBulkDelete"
    | "discountRedeemCodeBulkDelete"
    | "discountAutomaticBasicCreate"
    | "discountAutomaticBasicUpdate"
    | "discountAutomaticBxgyCreate"
    | "discountAutomaticBxgyUpdate"
    | "discountAutomaticFreeShippingCreate"
    | "discountAutomaticFreeShippingUpdate"
    | "discountAutomaticAppCreate"
    | "discountAutomaticAppUpdate"
    | "discountAutomaticActivate"
    | "discountAutomaticDeactivate"
    | "discountAutomaticDelete"
    | "discountAutomaticBulkDelete" -> True
    _ -> False
  }
}

/// True iff any string-typed variable value in the request resolves to
/// a discount that's already in local state, or is a proxy-synthetic
/// gid. The dispatcher uses this to skip `LiveHybrid` passthrough so
/// that read-after-create reads of a synthetic id stay local (and so
/// that read-after-delete reads of a synthetic id correctly return
/// null instead of forwarding a synthetic gid upstream where it would
/// 404).
///
/// We scan every string variable value rather than keying on `"id"`
@internal
pub type MutationResult {
  MutationResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
  )
}

@internal
pub type RedeemCodeValidation {
  RedeemCodeValidation(code: String, accepted: Bool, errors: List(SourceValue))
}

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let operation_path = get_operation_path_label(document)
      handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
        document,
        operation_path,
        upstream,
      )
    }
  }
}

@internal
pub fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

@internal
pub fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  document: String,
  operation_path: String,
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], [], store, identity, [], [])
  let #(entries, all_errors, final_store, final_identity, staged_ids, drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, errors, current_store, current_identity, staged, drafts) =
        acc
      case field {
        Field(name: name, ..) -> {
          let top_level_errors =
            validate_required_field_arguments(
              field,
              variables,
              name.value,
              required_arguments_for_root(name.value),
              operation_path,
              document,
            )
          case top_level_errors {
            [_, ..] -> #(
              entries,
              list.append(errors, top_level_errors),
              current_store,
              current_identity,
              staged,
              drafts,
            )
            [] -> {
              let result =
                handle_discount_mutation_field(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  document,
                  fragments,
                  variables,
                  upstream,
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, [#(result.key, result.payload)])
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged, result.staged_resource_ids)
                _ -> staged
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  case result.staged_resource_ids {
                    [] -> store.Failed
                    _ -> store.Staged
                  },
                  "discounts",
                  "stage-locally",
                  Some("discount mutation staged locally in Gleam port"),
                )
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                list.append(drafts, [draft]),
              )
            }
          }
        }
        _ -> acc
      }
    })
  let envelope = mutation_envelope(entries, all_errors)
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: case all_errors {
      [] -> staged_ids
      _ -> []
    },
    log_drafts: drafts,
  )
}

@internal
pub fn mutation_envelope(
  entries: List(#(String, Json)),
  all_errors: List(Json),
) -> Json {
  case all_errors, entries {
    [], _ -> json.object([#("data", json.object(entries))])
    _, [] -> json.object([#("errors", json.preprocessed_array(all_errors))])
    _, _ ->
      json.object([
        #("errors", json.preprocessed_array(all_errors)),
        #("data", json.object(entries)),
      ])
  }
}

@internal
pub fn required_arguments_for_root(root: String) -> List(RequiredArgument) {
  case root {
    "discountCodeBasicCreate" -> [
      RequiredArgument("basicCodeDiscount", "DiscountCodeBasicInput!"),
    ]
    "discountCodeBasicUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("basicCodeDiscount", "DiscountCodeBasicInput!"),
    ]
    "discountCodeBxgyCreate" -> [
      RequiredArgument("bxgyCodeDiscount", "DiscountCodeBxgyInput!"),
    ]
    "discountCodeBxgyUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("bxgyCodeDiscount", "DiscountCodeBxgyInput!"),
    ]
    "discountAutomaticBasicCreate" -> [
      RequiredArgument("automaticBasicDiscount", "DiscountAutomaticBasicInput!"),
    ]
    "discountAutomaticBasicUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("automaticBasicDiscount", "DiscountAutomaticBasicInput!"),
    ]
    _ -> []
  }
}

@internal
pub fn handle_discount_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  document: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  case root {
    "discountCodeBasicCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "basic",
        "basicCodeDiscount",
        upstream,
      )
    "discountCodeBasicUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "basic",
        "basicCodeDiscount",
      )
    "discountCodeBxgyCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "bxgy",
        "bxgyCodeDiscount",
        upstream,
      )
    "discountCodeBxgyUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "bxgy",
        "bxgyCodeDiscount",
      )
    "discountCodeFreeShippingCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "free_shipping",
        "freeShippingCodeDiscount",
        upstream,
      )
    "discountCodeFreeShippingUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "free_shipping",
        "freeShippingCodeDiscount",
      )
    "discountCodeAppCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "app",
        "codeAppDiscount",
        upstream,
      )
    "discountCodeAppUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "code",
        "app",
        "codeAppDiscount",
      )
    "discountAutomaticBasicCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "basic",
        "automaticBasicDiscount",
        upstream,
      )
    "discountAutomaticBasicUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "basic",
        "automaticBasicDiscount",
      )
    "discountAutomaticBxgyCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "bxgy",
        "automaticBxgyDiscount",
        upstream,
      )
    "discountAutomaticBxgyUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "bxgy",
        "automaticBxgyDiscount",
      )
    "discountAutomaticFreeShippingCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "free_shipping",
        "freeShippingAutomaticDiscount",
        upstream,
      )
    "discountAutomaticFreeShippingUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "free_shipping",
        "freeShippingAutomaticDiscount",
      )
    "discountAutomaticAppCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "app",
        "automaticAppDiscount",
        upstream,
      )
    "discountAutomaticAppUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        "automatic",
        "app",
        "automaticAppDiscount",
      )
    "discountCodeActivate" | "discountAutomaticActivate" ->
      set_status(store, identity, root, field, fragments, variables, "ACTIVE")
    "discountCodeDeactivate" | "discountAutomaticDeactivate" ->
      set_status(store, identity, root, field, fragments, variables, "EXPIRED")
    "discountCodeDelete" | "discountAutomaticDelete" ->
      delete_discount(store, identity, root, field, variables)
    "discountCodeBulkActivate"
    | "discountCodeBulkDeactivate"
    | "discountCodeBulkDelete"
    | "discountAutomaticBulkDelete" ->
      bulk_job_payload(store, identity, root, field, variables, upstream)
    "discountRedeemCodeBulkAdd" ->
      redeem_code_bulk_add(
        store,
        identity,
        root,
        field,
        document,
        fragments,
        variables,
        upstream,
      )
    "discountCodeRedeemCodeBulkDelete" | "discountRedeemCodeBulkDelete" ->
      redeem_code_bulk_delete(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        upstream,
      )
    _ ->
      MutationResult(
        key: get_field_response_key(field),
        payload: json.null(),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
  }
}

@internal
pub fn create_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  document: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  owner_kind: String,
  discount_type: String,
  input_name: String,
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let input = discount_types.read_object_arg(field, variables, input_name)
  case input {
    None ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, [
          discount_types.user_error(["input"], "Input is required", "INVALID"),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    Some(input) -> {
      let top_level_errors =
        validate_discount_top_level_errors(input, field, document)
      case top_level_errors {
        [_, ..] ->
          MutationResult(
            key: key,
            payload: json.null(),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            top_level_errors: top_level_errors,
          )
        [] ->
          create_discount_after_top_level_validation(
            store,
            identity,
            root,
            field,
            fragments,
            owner_kind,
            discount_type,
            input_name,
            upstream,
            input,
            key,
          )
      }
    }
  }
}

@internal
pub fn create_discount_after_top_level_validation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  owner_kind: String,
  discount_type: String,
  input_name: String,
  upstream: UpstreamContext,
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> MutationResult {
  // Local input validation first (structural / pure-function checks).
  let store =
    maybe_hydrate_discount_subscription_capability(
      store,
      input_name,
      input,
      discount_type,
      upstream,
    )
  let store = case discount_type {
    "app" -> maybe_hydrate_shopify_function(store, input, upstream)
    _ -> store
  }
  let user_errors =
    validate_discount_input(
      store,
      input_name,
      input,
      discount_type,
      owner_kind == "code",
      None,
    )
  // Cross-discount uniqueness check: when local validation otherwise
  // passes and the input carries a `code`, ask upstream whether a
  // discount with that code already exists. If so, surface a TAKEN
  // error matching Shopify's response shape. We do this after local
  // validation so that pure-input errors (badRefs, BXGY shape, free-
  // shipping combinesWith) are not overshadowed by an upstream
  // call that would never have been issued in production for those
  // shapes either. In `Snapshot` mode (no transport, no upstream),
  // the lookup is skipped — the local-store check inside
  // `validate_discount_input` already rejects duplicates against
  // staged records, which is the cold-start expectation.
  let user_errors = case user_errors {
    [_, ..] -> user_errors
    [] ->
      case fetch_taken_code_error(input, input_name, owner_kind, upstream) {
        Some(err) -> [err]
        None -> []
      }
  }
  case user_errors {
    [_, ..] ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, user_errors),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    [] -> {
      // Pattern 2: when this is an app discount, hydrate the
      // referenced Shopify Function from upstream so the staged
      // record can project the function's metadata onto
      // `appDiscountType` (appKey, title, description). No-op when
      // the function is already in the local store, when no
      // transport is installed (Snapshot mode), or when the
      // upstream call fails. The miss falls through to the
      // legacy local-only behavior.
      let store = case discount_type {
        "app" -> maybe_hydrate_shopify_function(store, input, upstream)
        _ -> store
      }
      let #(id, next_identity) =
        synthetic_identity.make_proxy_synthetic_gid(identity, case owner_kind {
          "automatic" -> "DiscountAutomaticNode"
          _ -> "DiscountCodeNode"
        })
      let #(record, next_identity) =
        build_discount_record(
          store,
          next_identity,
          id,
          owner_kind,
          discount_type,
          input,
          None,
        )
      let #(record, next_store) = store.stage_discount(store, record)
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, Some(record), []),
        store: next_store,
        identity: next_identity,
        staged_resource_ids: [record.id],
        top_level_errors: [],
      )
    }
  }
}

@internal
pub fn update_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  document: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  owner_kind: String,
  target_discount_type: String,
  input_name: String,
) -> MutationResult {
  let key = get_field_response_key(field)
  let id = discount_types.read_string_arg(field, variables, "id")
  let input = discount_types.read_object_arg(field, variables, input_name)
  case id, input {
    Some(id), Some(input) -> {
      let top_level_errors =
        validate_discount_top_level_errors(input, field, document)
      case top_level_errors {
        [_, ..] ->
          MutationResult(
            key: key,
            payload: json.null(),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            top_level_errors: top_level_errors,
          )
        [] ->
          update_discount_after_top_level_validation(
            store,
            identity,
            root,
            field,
            fragments,
            owner_kind,
            target_discount_type,
            input_name,
            id,
            input,
            key,
          )
      }
    }
    _, _ ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, [
          discount_types.user_error(
            ["id"],
            "Discount does not exist",
            "NOT_FOUND",
          ),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
  }
}

@internal
pub fn update_discount_after_top_level_validation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  owner_kind: String,
  target_discount_type: String,
  input_name: String,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> MutationResult {
  let early_user_errors =
    validate_context_customer_selection_conflict(input_name, input)
  case early_user_errors {
    [_, ..] ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, early_user_errors),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    [] ->
      update_discount_existing_record(
        store,
        identity,
        root,
        field,
        fragments,
        owner_kind,
        target_discount_type,
        input_name,
        id,
        input,
        key,
      )
  }
}

@internal
pub fn update_discount_existing_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  owner_kind: String,
  target_discount_type: String,
  input_name: String,
  id: String,
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> MutationResult {
  let existing = store.get_effective_discount_by_id(store, id)
  case existing {
    None ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, [
          discount_types.user_error(
            ["id"],
            "Discount does not exist",
            "INVALID",
          ),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    Some(existing_record) -> {
      let user_errors = validate_discount_update_input(input, existing_record)
      let user_errors = case user_errors {
        [_, ..] -> user_errors
        [] ->
          validate_discount_input(
            store,
            input_name,
            input,
            target_discount_type,
            False,
            Some(existing_record.id),
          )
      }
      case user_errors {
        [_, ..] ->
          MutationResult(
            key: key,
            payload: payload_json(root, field, fragments, None, user_errors),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            top_level_errors: [],
          )
        [] -> {
          let #(record, next_identity) =
            build_discount_record(
              store,
              identity,
              id,
              owner_kind,
              target_discount_type,
              input,
              Some(existing_record),
            )
          let #(record, next_store) = store.stage_discount(store, record)
          MutationResult(
            key: key,
            payload: payload_json(root, field, fragments, Some(record), []),
            store: next_store,
            identity: next_identity,
            staged_resource_ids: [record.id],
            top_level_errors: [],
          )
        }
      }
    }
  }
}

@internal
pub fn set_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  status: String,
) -> MutationResult {
  let key = get_field_response_key(field)
  case discount_types.read_string_arg(field, variables, "id") {
    Some(id) ->
      case store.get_effective_discount_by_id(store, id) {
        Some(record) -> {
          let user_errors = case status {
            "ACTIVE" -> app_discount_activation_errors(store, record)
            _ -> []
          }
          case user_errors {
            [_, ..] ->
              MutationResult(
                key,
                payload_json(root, field, fragments, None, user_errors),
                store,
                identity,
                [],
                [],
              )
            [] -> {
              let #(updated_at, next_identity) =
                synthetic_identity.make_synthetic_timestamp(identity)
              let transition_timestamp = case status {
                "ACTIVE" ->
                  case record.status {
                    "ACTIVE" -> None
                    _ -> Some(updated_at)
                  }
                "EXPIRED" -> Some(updated_at)
                _ -> None
              }
              let record =
                DiscountRecord(
                  ..record,
                  status: status,
                  payload: discount_types.update_payload_status(
                      record.payload,
                      status,
                      transition_timestamp,
                    )
                    |> discount_types.update_payload_updated_at(updated_at),
                )
              let #(record, next_store) = store.stage_discount(store, record)
              MutationResult(
                key,
                payload_json(root, field, fragments, Some(record), []),
                next_store,
                next_identity,
                [record.id],
                [],
              )
            }
          }
        }
        None ->
          MutationResult(
            key,
            payload_json(root, field, fragments, None, [
              discount_types.user_error(
                ["id"],
                "Discount does not exist",
                "INVALID",
              ),
            ]),
            store,
            identity,
            [],
            [],
          )
      }
    None ->
      MutationResult(
        key,
        payload_json(root, field, fragments, None, [
          discount_types.user_error(["id"], "ID is required", "INVALID"),
        ]),
        store,
        identity,
        [],
        [],
      )
  }
}

@internal
pub fn app_discount_activation_errors(
  store: Store,
  record: DiscountRecord,
) -> List(SourceValue) {
  case record.discount_type {
    "app" ->
      case discount_app_function_reference(record) {
        Some(reference) ->
          case discount_types.find_shopify_function(store, reference) {
            Some(_) -> []
            None -> activation_failed_user_errors()
          }
        None -> activation_failed_user_errors()
      }
    _ -> []
  }
}

@internal
pub fn activation_failed_user_errors() -> List(SourceValue) {
  [
    discount_types.user_error(
      ["id"],
      "Discount could not be activated.",
      "INTERNAL_ERROR",
    ),
  ]
}

@internal
pub fn discount_app_function_reference(
  record: DiscountRecord,
) -> Option(String) {
  case discount_types.captured_to_source(record.payload) {
    SrcObject(fields) -> {
      let owner = case record.owner_kind {
        "automatic" -> dict.get(fields, "automaticDiscount")
        _ -> dict.get(fields, "codeDiscount")
      }
      case owner {
        Ok(SrcObject(discount)) ->
          case dict.get(discount, "appDiscountType") {
            Ok(SrcObject(app_discount_type)) ->
              case dict.get(app_discount_type, "functionId") {
                Ok(SrcString(reference)) -> Some(reference)
                _ -> None
              }
            _ -> None
          }
        _ -> None
      }
    }
    _ -> None
  }
}

@internal
pub fn delete_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationResult {
  let key = get_field_response_key(field)
  let id =
    discount_types.read_string_arg(field, variables, "id") |> option.unwrap("")
  let #(next_store, next_identity) = case
    store.get_effective_discount_by_id(store, id)
  {
    Some(_) -> {
      let #(_, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      #(store.delete_staged_discount(store, id), next_identity)
    }
    None -> #(store.delete_staged_discount(store, id), identity)
  }
  let payload =
    json.object(
      list.map(child_fields(field), fn(child) {
        let child_key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "deletedCodeDiscountId" | "deletedAutomaticDiscountId" -> #(
                child_key,
                json.string(id),
              )
              "userErrors" -> #(child_key, json.array([], fn(x) { x }))
              _ -> #(child_key, json.null())
            }
          _ -> #(child_key, json.null())
        }
      }),
    )
  MutationResult(key, payload, next_store, next_identity, [id], [])
}

@internal
pub fn bulk_job_payload(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let args =
    root_field.get_field_arguments(field, variables)
    |> result.unwrap(dict.new())
  let user_errors = validate_bulk_selector(store, root, args)
  case user_errors {
    [_, ..] -> {
      let payload =
        project_graphql_value(
          SrcObject(
            dict.from_list([
              #("job", SrcNull),
              #("userErrors", SrcList(user_errors)),
            ]),
          ),
          child_fields(field),
          dict.new(),
        )
      MutationResult(key, payload, store, identity, [], [])
    }
    [] -> {
      // Pattern 2: hydrate every id this bulk operation touches before
      // applying the local effects. Without hydration, references to
      // base discounts only seeded upstream silently no-op (set-status
      // checks `get_effective_discount_by_id` first), so subsequent
      // count and node-by-id read targets see incorrect totals. A
      // cassette miss is a silent no-op so the legacy local-only
      // behavior applies in Snapshot mode.
      let ids = discount_types.read_string_array(args, "ids", [])
      let #(store, identity_after_hydrate) =
        list.fold(ids, #(store, identity), fn(acc, id) {
          let #(current_store, current_identity) = acc
          maybe_hydrate_discount(current_store, current_identity, id, upstream)
        })
      let #(job_id, next_identity) =
        discount_types.make_discount_async_gid(
          store,
          identity_after_hydrate,
          "Job",
        )
      let job =
        SrcObject(
          dict.from_list([
            #("id", SrcString(job_id)),
            #("done", SrcBool(True)),
            #("query", SrcNull),
          ]),
        )
      let #(next_store, identity_after_effects) =
        apply_bulk_effects(store, root, args, next_identity)
      let payload =
        project_graphql_value(
          SrcObject(
            dict.from_list([
              #("job", job),
              #("userErrors", SrcList([])),
            ]),
          ),
          child_fields(field),
          dict.new(),
        )
      MutationResult(
        key,
        payload,
        next_store,
        identity_after_effects,
        [job_id],
        [],
      )
    }
  }
}

@internal
pub fn redeem_code_bulk_add(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  document: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let discount_id =
    discount_types.read_string_arg(field, variables, "discountId")
  let #(codes, schema_input_codes) =
    discount_types.read_codes_arg_with_shape(field, variables, "codes")
  let too_many_errors =
    validate_redeem_code_bulk_add_size(field, document, codes)
  case too_many_errors {
    [_, ..] ->
      MutationResult(key, json.null(), store, identity, [], too_many_errors)
    [] ->
      redeem_code_bulk_add_after_size_validation(
        store,
        identity,
        root,
        field,
        fragments,
        discount_id,
        codes,
        schema_input_codes,
        upstream,
        key,
      )
  }
}

@internal
pub fn redeem_code_bulk_add_after_size_validation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  discount_id: Option(String),
  codes: List(String),
  schema_input_codes: Bool,
  upstream: UpstreamContext,
  key: String,
) -> MutationResult {
  let #(store, identity) = case discount_id {
    Some(id) -> maybe_hydrate_discount(store, identity, id, upstream)
    None -> #(store, identity)
  }
  case discount_id {
    None ->
      redeem_code_bulk_add_user_error(
        store,
        identity,
        root,
        field,
        fragments,
        key,
        discount_types.user_error(
          ["discountId"],
          "Code discount does not exist.",
          "INVALID",
        ),
      )
    Some(id) ->
      case store.get_effective_discount_by_id(store, id), codes {
        None, _ ->
          redeem_code_bulk_add_user_error(
            store,
            identity,
            root,
            field,
            fragments,
            key,
            discount_types.user_error(
              ["discountId"],
              "Code discount does not exist.",
              "INVALID",
            ),
          )
        Some(_), [] ->
          redeem_code_bulk_add_user_error(
            store,
            identity,
            root,
            field,
            fragments,
            key,
            discount_types.user_error(
              ["codes"],
              "Codes can't be blank",
              "BLANK",
            ),
          )
        Some(record), [_, ..] -> {
          let #(bulk_id, identity) =
            discount_types.make_discount_async_gid(
              store,
              identity,
              "DiscountRedeemCodeBulkCreation",
            )
          let validations = validate_redeem_codes(codes)
          let accepted_codes =
            validations
            |> list.filter(fn(item) { item.accepted })
            |> list.map(fn(item) { item.code })
          let #(updated, identity, created_nodes) =
            discount_types.append_codes(store, record, accepted_codes, identity)
          let #(next_store, identity) = case accepted_codes {
            [] -> #(store, identity)
            [_, ..] -> {
              let #(updated_at, identity) =
                synthetic_identity.make_synthetic_timestamp(identity)
              let updated =
                discount_types.bump_discount_updated_at(updated, updated_at)
              let #(_, next_store) = store.stage_discount(store, updated)
              #(next_store, identity)
            }
          }
          let final_bulk_creation =
            redeem_code_bulk_creation_source(
              bulk_id,
              validations,
              created_nodes,
              False,
            )
          let #(_, next_store) =
            store.stage_discount_bulk_operation(
              next_store,
              DiscountBulkOperationRecord(
                id: bulk_id,
                operation: "discountRedeemCodeBulkAdd",
                discount_id: id,
                status: "COMPLETED",
                payload: discount_types.source_to_captured(final_bulk_creation),
              ),
            )
          let mutation_bulk_creation =
            redeem_code_bulk_creation_source(
              bulk_id,
              validations,
              created_nodes,
              schema_input_codes,
            )
          let payload =
            project_graphql_value(
              SrcObject(
                dict.from_list([
                  #("bulkCreation", mutation_bulk_creation),
                  #("userErrors", SrcList([])),
                ]),
              ),
              child_fields(field),
              fragments,
            )
          MutationResult(key, payload, next_store, identity, [id, bulk_id], [])
        }
      }
  }
}

@internal
pub fn redeem_code_bulk_add_user_error(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  fragments: FragmentMap,
  key: String,
  error: SourceValue,
) -> MutationResult {
  let payload =
    project_graphql_value(
      SrcObject(
        dict.from_list([
          #("bulkCreation", SrcNull),
          #("userErrors", SrcList([error])),
        ]),
      ),
      child_fields(field),
      fragments,
    )
  MutationResult(key, payload, store, identity, [], [])
}

@internal
pub fn validate_redeem_code_bulk_add_size(
  field: Selection,
  document: String,
  codes: List(String),
) -> List(Json) {
  let count = list.length(codes)
  case count > 250 {
    False -> []
    True -> [
      json.object([
        #(
          "message",
          json.string(
            "The input array size of "
            <> int.to_string(count)
            <> " is greater than the maximum allowed of 250.",
          ),
        ),
        #("locations", field_locations_json(field, document)),
        #(
          "path",
          json.array(["discountRedeemCodeBulkAdd", "codes"], json.string),
        ),
        #(
          "extensions",
          json.object([#("code", json.string("MAX_INPUT_SIZE_EXCEEDED"))]),
        ),
      ]),
    ]
  }
}

@internal
pub fn validate_redeem_codes(
  codes: List(String),
) -> List(RedeemCodeValidation) {
  let #(items, _) =
    list.fold(codes, #([], []), fn(acc, code) {
      let #(items, seen) = acc
      let pure_errors = redeem_code_value_errors(code)
      case pure_errors {
        [_, ..] -> #(
          [RedeemCodeValidation(code, False, pure_errors), ..items],
          seen,
        )
        [] ->
          case list.contains(seen, code) {
            True -> #(
              [
                RedeemCodeValidation(code, False, [
                  discount_types.user_error_with_code(
                    ["code"],
                    "Codes must be unique within BulkDiscountCodeCreation",
                    None,
                  ),
                ]),
                ..items
              ],
              seen,
            )
            False -> #([RedeemCodeValidation(code, True, []), ..items], [
              code,
              ..seen
            ])
          }
      }
    })
  list.reverse(items)
}

@internal
pub fn redeem_code_value_errors(code: String) -> List(SourceValue) {
  case code == "" {
    True -> [
      discount_types.user_error_with_code(
        ["code"],
        "is too short (minimum is 1 character)",
        None,
      ),
    ]
    False ->
      case string.contains(code, "\n") || string.contains(code, "\r") {
        True -> [
          discount_types.user_error_with_code(
            ["code"],
            "cannot contain newline characters.",
            None,
          ),
        ]
        False ->
          case string.length(code) > 255 {
            True -> [
              discount_types.user_error_with_code(
                ["code"],
                "is too long (maximum is 255 characters)",
                None,
              ),
            ]
            False -> []
          }
      }
  }
}

@internal
pub fn redeem_code_bulk_creation_source(
  id: String,
  validations: List(RedeemCodeValidation),
  created_nodes: List(#(String, String)),
  pending: Bool,
) -> SourceValue {
  let failed_count =
    validations
    |> list.filter(fn(item) { !item.accepted })
    |> list.length
  let imported_count = list.length(validations) - failed_count
  SrcObject(
    dict.from_list([
      #("id", SrcString(id)),
      #("done", SrcBool(!pending)),
      #("codesCount", SrcInt(list.length(validations))),
      #(
        "importedCount",
        SrcInt(case pending {
          True -> 0
          False -> imported_count
        }),
      ),
      #(
        "failedCount",
        SrcInt(case pending {
          True -> 0
          False -> failed_count
        }),
      ),
      #(
        "codes",
        SrcObject(
          dict.from_list([
            #(
              "nodes",
              SrcList(
                list.map(validations, fn(item) {
                  redeem_code_bulk_creation_code_source(
                    item,
                    created_nodes,
                    pending,
                  )
                }),
              ),
            ),
            #("edges", SrcList([])),
            #(
              "pageInfo",
              SrcObject(
                dict.from_list([
                  #("hasNextPage", SrcBool(False)),
                  #("hasPreviousPage", SrcBool(False)),
                  #("startCursor", SrcNull),
                  #("endCursor", SrcNull),
                ]),
              ),
            ),
          ]),
        ),
      ),
    ]),
  )
}

@internal
pub fn redeem_code_bulk_creation_code_source(
  validation: RedeemCodeValidation,
  created_nodes: List(#(String, String)),
  pending: Bool,
) -> SourceValue {
  let redeem_code = case pending, validation.accepted {
    True, _ -> SrcNull
    False, True ->
      case find_created_redeem_code_id(created_nodes, validation.code) {
        Some(id) ->
          SrcObject(
            dict.from_list([
              #("id", SrcString(id)),
              #("code", SrcString(validation.code)),
            ]),
          )
        None -> SrcNull
      }
    False, False -> SrcNull
  }
  SrcObject(
    dict.from_list([
      #("code", SrcString(validation.code)),
      #(
        "errors",
        SrcList(case pending {
          True -> []
          False -> validation.errors
        }),
      ),
      #("discountRedeemCode", redeem_code),
    ]),
  )
}

@internal
pub fn find_created_redeem_code_id(
  nodes: List(#(String, String)),
  code: String,
) -> Option(String) {
  case
    nodes
    |> list.find(fn(pair) {
      let #(_, node_code) = pair
      node_code == code
    })
  {
    Ok(pair) -> {
      let #(id, _) = pair
      Some(id)
    }
    Error(_) -> None
  }
}

@internal
pub fn redeem_code_bulk_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let args =
    root_field.get_field_arguments(field, variables)
    |> result.unwrap(dict.new())
  let discount_id = discount_types.read_string(args, "discountId")
  let selector_errors = validate_redeem_code_bulk_delete_selector_shape(args)
  case selector_errors {
    [_, ..] ->
      MutationResult(
        key,
        redeem_code_bulk_delete_payload(
          field,
          fragments,
          SrcNull,
          selector_errors,
        ),
        store,
        identity,
        [],
        [],
      )
    [] -> {
      // Same Pattern 2 hydration as redeem_code_bulk_add: pull the prior
      // record from upstream before validating discount existence so real
      // Shopify-side discounts can be targeted by local staged deletions.
      let #(store, identity) = case discount_id {
        Some(id) -> maybe_hydrate_discount(store, identity, id, upstream)
        None -> #(store, identity)
      }
      let user_errors =
        validate_redeem_code_bulk_delete_after_hydrate(store, args)
      case user_errors {
        [_, ..] ->
          MutationResult(
            key,
            redeem_code_bulk_delete_payload(
              field,
              fragments,
              SrcNull,
              user_errors,
            ),
            store,
            identity,
            [],
            [],
          )
        [] -> {
          let #(next_store, identity_after_update) = case discount_id {
            Some(id) ->
              case store.get_effective_discount_by_id(store, id) {
                Some(record) -> {
                  let ids =
                    redeem_code_bulk_delete_target_ids(store, record, args)
                  let #(updated_at, identity) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  let updated =
                    discount_types.remove_codes_by_ids(record, ids, updated_at)
                  let #(_, s) = store.stage_discount(store, updated)
                  #(s, identity)
                }
                None -> #(store, identity)
              }
            None -> #(store, identity)
          }
          let #(job_id, next_identity) =
            discount_types.make_discount_async_gid(
              store,
              identity_after_update,
              "Job",
            )
          let job =
            SrcObject(
              dict.from_list([
                #("id", SrcString(job_id)),
                #("done", SrcBool(True)),
                #("query", SrcNull),
              ]),
            )
          MutationResult(
            key,
            redeem_code_bulk_delete_payload(field, fragments, job, []),
            next_store,
            next_identity,
            discount_types.option_to_list(discount_id),
            [],
          )
        }
      }
    }
  }
}

@internal
pub fn redeem_code_bulk_delete_payload(
  field: Selection,
  fragments: FragmentMap,
  job: SourceValue,
  user_errors: List(SourceValue),
) -> Json {
  project_graphql_value(
    SrcObject(
      dict.from_list([
        #("job", job),
        #("userErrors", SrcList(user_errors)),
      ]),
    ),
    child_fields(field),
    fragments,
  )
}
