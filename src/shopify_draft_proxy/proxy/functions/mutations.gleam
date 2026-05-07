import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/functions/helpers.{
  normalize_function_handle, shopify_function_id_from_handle,
}
import shopify_draft_proxy/proxy/functions/serializers
import shopify_draft_proxy/proxy/functions/types as function_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationFieldResult, type MutationOutcome, LogDraft, MutationFieldResult,
  MutationOutcome,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, fetch_sync,
}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CartTransformMetafieldRecord, type CartTransformRecord,
  type ShopifyFunctionAppRecord, type ShopifyFunctionRecord,
  type ValidationMetafieldRecord, CartTransformMetafieldRecord,
  CartTransformRecord, ShopifyFunctionAppRecord, ShopifyFunctionRecord,
  TaxAppConfigurationRecord, ValidationMetafieldRecord, ValidationRecord,
}

const max_active_validations: Int = 25

const function_app_id: String = "347082227713"

/// Predicate matching the TS `FUNCTION_MUTATION_ROOTS` set.
@internal
pub fn is_function_mutation_root(name: String) -> Bool {
  case name {
    "validationCreate" -> True
    "validationUpdate" -> True
    "validationDelete" -> True
    "cartTransformCreate" -> True
    "cartTransformDelete" -> True
    "taxAppConfigure" -> True
    _ -> False
  }
}

/// Process a functions mutation document. Mirrors
/// `handleFunctionMutation`.
/// Pattern 2: dispatched LiveHybrid function metadata mutations first
/// try to hydrate referenced ShopifyFunction owner/app metadata from
/// upstream, then stage the mutation locally. Cart-transform creation
/// requires the referenced Function to resolve locally or from that
/// upstream lookup before it stages any local write.
@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let identity_for_handlers =
        reserve_multiroot_log_identity(identity, fields)
      let hydrated_store =
        hydrate_referenced_shopify_functions(store, fields, variables, upstream)
      handle_mutation_fields(
        hydrated_store,
        identity_for_handlers,
        request_path,
        document,
        fields,
        fragments,
        variables,
      )
    }
  }
}

fn reserve_multiroot_log_identity(
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
) -> SyntheticIdentityRegistry {
  case list.length(mutation_root_names(fields)) > 1 {
    True -> {
      let #(_, identity_after_reserved_id) =
        synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
      let #(_, identity_after_reserved_log) =
        synthetic_identity.make_synthetic_timestamp(identity_after_reserved_id)
      identity_after_reserved_log
    }
    False -> identity
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [])
  let #(data_entries, final_store, final_identity, all_staged) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "validationCreate" ->
              Some(handle_validation_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "validationUpdate" ->
              Some(handle_validation_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "validationDelete" ->
              Some(handle_validation_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "cartTransformCreate" ->
              Some(handle_cart_transform_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "cartTransformDelete" ->
              Some(handle_cart_transform_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "taxAppConfigure" ->
              Some(handle_tax_app_configure(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> #(
              list.append(entries, [#(result.key, result.payload)]),
              next_store,
              next_identity,
              list.append(staged_ids, result.staged_resource_ids),
            )
          }
        }
        _ -> acc
      }
    })
  let root_names = mutation_root_names(fields)
  let primary_root = case list.first(root_names) {
    Ok(name) -> Some(name)
    Error(_) -> None
  }
  let notes = case primary_root {
    Some("taxAppConfigure") ->
      "Staged locally in the in-memory tax app configuration metadata store; no tax calculation app callbacks are invoked."
    _ ->
      "Staged locally in the in-memory Shopify Functions metadata store; external Shopify Function code is not executed."
  }
  let log_drafts = case list.is_empty(all_staged) {
    True -> []
    False -> {
      let draft =
        LogDraft(
          operation_name: primary_root,
          root_fields: root_names,
          primary_root_field: primary_root,
          domain: "functions",
          execution: "stage-locally",
          query: None,
          variables: None,
          staged_resource_ids: all_staged,
          status: store_types.Staged,
          notes: Some(notes),
        )
      [draft]
    }
  }
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: log_drafts,
  )
}

fn mutation_root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}

// ---------------------------------------------------------------------------
// Mutation handlers
// ---------------------------------------------------------------------------

fn read_function_reference(
  input: Dict(String, root_field.ResolvedValue),
) -> function_types.FunctionReference {
  function_types.FunctionReference(
    function_id: graphql_helpers.read_arg_string(input, "functionId"),
    function_handle: graphql_helpers.read_arg_string(input, "functionHandle"),
  )
}

fn validation_enable_would_exceed_cap(
  store: Store,
  exclude_id: String,
  enable: Option(Bool),
) -> Bool {
  case enable {
    Some(True) ->
      active_validation_count_excluding(store, exclude_id)
      >= max_active_validations
    _ -> False
  }
}

fn active_validation_count_excluding(store: Store, exclude_id: String) -> Int {
  store.list_effective_validations(store)
  |> list.filter(fn(record) {
    record.id != exclude_id && record.enable == Some(True)
  })
  |> list.length
}

fn read_validation_metafields(
  input: Dict(String, root_field.ResolvedValue),
  validation_id: String,
  timestamp: String,
  identity: SyntheticIdentityRegistry,
) -> #(List(ValidationMetafieldRecord), SyntheticIdentityRegistry) {
  case dict.get(input, "metafields") {
    Ok(root_field.ListVal(items)) ->
      list.fold(items, #([], identity), fn(acc, item) {
        let #(rows, current_identity) = acc
        case item {
          root_field.ObjectVal(fields) ->
            case
              graphql_helpers.read_arg_string(fields, "namespace"),
              graphql_helpers.read_arg_string(fields, "key")
            {
              Some(namespace), Some(key) -> {
                let #(id, next_identity) =
                  synthetic_identity.make_synthetic_gid(
                    current_identity,
                    "Metafield",
                  )
                #(
                  list.append(rows, [
                    ValidationMetafieldRecord(
                      id: id,
                      validation_id: validation_id,
                      namespace: namespace,
                      key: key,
                      type_: graphql_helpers.read_arg_string(fields, "type"),
                      value: graphql_helpers.read_arg_string(fields, "value"),
                      compare_digest: None,
                      created_at: Some(timestamp),
                      updated_at: Some(timestamp),
                      owner_type: Some("VALIDATION"),
                    ),
                  ]),
                  next_identity,
                )
              }
              _, _ -> acc
            }
          _ -> acc
        }
      })
    _ -> #([], identity)
  }
}

fn read_cart_transform_metafields(
  input: Dict(String, root_field.ResolvedValue),
  cart_transform_id: String,
  timestamp: String,
  identity: SyntheticIdentityRegistry,
) -> #(
  List(CartTransformMetafieldRecord),
  List(function_types.UserError),
  SyntheticIdentityRegistry,
) {
  case dict.get(input, "metafields") {
    Ok(root_field.ListVal(items)) ->
      list.index_fold(items, #([], [], identity), fn(acc, item, index) {
        let #(rows, errors, current_identity) = acc
        case item {
          root_field.ObjectVal(fields) -> {
            let item_errors =
              cart_transform_metafield_input_errors(fields, index)
            case item_errors {
              [] -> {
                let assert Some(namespace) =
                  graphql_helpers.read_arg_string(fields, "namespace")
                let assert Some(key) =
                  graphql_helpers.read_arg_string(fields, "key")
                let #(id, next_identity) =
                  synthetic_identity.make_synthetic_gid(
                    current_identity,
                    "Metafield",
                  )
                #(
                  list.append(rows, [
                    CartTransformMetafieldRecord(
                      id: id,
                      cart_transform_id: cart_transform_id,
                      namespace: namespace,
                      key: key,
                      type_: graphql_helpers.read_arg_string(fields, "type"),
                      value: graphql_helpers.read_arg_string(fields, "value"),
                      compare_digest: None,
                      created_at: Some(timestamp),
                      updated_at: Some(timestamp),
                      owner_type: Some("CARTTRANSFORM"),
                    ),
                  ]),
                  errors,
                  next_identity,
                )
              }
              _ -> #(rows, list.append(errors, item_errors), current_identity)
            }
          }
          _ -> #(
            rows,
            list.append(errors, [
              invalid_cart_transform_metafield_error(index, "namespace"),
            ]),
            current_identity,
          )
        }
      })
    _ -> #([], [], identity)
  }
}

fn cart_transform_metafield_input_errors(
  fields: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(function_types.UserError) {
  let missing_errors =
    ["namespace", "key", "type", "value"]
    |> list.filter_map(fn(attribute) {
      case graphql_helpers.read_arg_string(fields, attribute) {
        Some(_) -> Error(Nil)
        None -> Ok(invalid_cart_transform_metafield_error(index, attribute))
      }
    })
  let value_errors = case
    graphql_helpers.read_arg_string(fields, "type"),
    graphql_helpers.read_arg_string(fields, "value")
  {
    Some("json"), Some(value) ->
      case json.parse(value, commit.json_value_decoder()) {
        Ok(_) -> []
        Error(_) -> [invalid_cart_transform_metafield_json_error(index, value)]
      }
    _, _ -> []
  }
  list.append(missing_errors, value_errors)
}

fn invalid_cart_transform_metafield_error(
  index: Int,
  attribute: String,
) -> function_types.UserError {
  function_types.UserError(
    field: ["metafields", int.to_string(index), attribute],
    message: "may not be empty",
    code: Some("INVALID_METAFIELDS"),
  )
}

fn invalid_cart_transform_metafield_json_error(
  index: Int,
  value: String,
) -> function_types.UserError {
  function_types.UserError(
    field: ["metafields", int.to_string(index), "value"],
    message: "is invalid JSON: unexpected token '"
      <> invalid_json_error_token(value)
      <> "' at line 1 column 1.",
    code: Some("INVALID_METAFIELDS"),
  )
}

fn invalid_json_error_token(value: String) -> String {
  value
  |> string.trim
  |> string.split(" ")
  |> list.first
  |> result.unwrap("")
}

fn missing_cart_transform_function_error() -> function_types.UserError {
  function_types.UserError(
    field: ["functionHandle"],
    message: "Either function_id or function_handle must be provided.",
    code: Some("MISSING_FUNCTION_IDENTIFIER"),
  )
}

fn multiple_function_identifiers_error() -> function_types.UserError {
  function_types.UserError(
    field: ["functionHandle"],
    message: "Only one of function_id or function_handle can be provided, not both.",
    code: Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
  )
}

fn validation_missing_function_identifier_error() -> function_types.UserError {
  function_types.UserError(
    field: ["validation", "functionHandle"],
    message: "Either function_id or function_handle must be provided.",
    code: Some("MISSING_FUNCTION_IDENTIFIER"),
  )
}

fn validation_multiple_function_identifiers_error() -> function_types.UserError {
  function_types.UserError(
    field: ["validation"],
    message: "Only one of function_id or function_handle can be provided, not both.",
    code: Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
  )
}

fn validation_function_not_found_error(
  field_name: String,
) -> function_types.UserError {
  function_types.UserError(
    field: ["validation", field_name],
    message: "Extension not found.",
    code: Some("NOT_FOUND"),
  )
}

fn function_not_found_error(
  field_name: String,
  value: String,
) -> function_types.UserError {
  function_types.UserError(
    field: [field_name],
    message: function_not_found_message(field_name, value),
    code: Some("FUNCTION_NOT_FOUND"),
  )
}

fn function_not_found_message(field_name: String, value: String) -> String {
  case field_name {
    "functionId" ->
      "Function "
      <> value
      <> " not found. Ensure that it is released in the current app ("
      <> function_app_id
      <> "), and that the app is installed."
    "functionHandle" -> "Could not find function with handle: " <> value <> "."
    _ -> "Could not find function with " <> field_name <> ": " <> value <> "."
  }
}

fn function_does_not_implement_error(
  field_name: String,
) -> function_types.UserError {
  function_types.UserError(
    field: [field_name],
    message: cart_transform_function_api_mismatch_message,
    code: Some("FUNCTION_DOES_NOT_IMPLEMENT"),
  )
}

fn function_id_api_mismatch_error(
  field_name: String,
) -> function_types.UserError {
  function_types.UserError(
    field: [field_name],
    message: cart_transform_function_api_mismatch_message,
    code: Some("FUNCTION_NOT_FOUND"),
  )
}

const cart_transform_function_api_mismatch_message: String = "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run]."

fn validation_function_does_not_implement_error(
  field_name: String,
) -> function_types.UserError {
  function_types.UserError(
    field: ["validation", field_name],
    message: "Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}].",
    code: Some("FUNCTION_DOES_NOT_IMPLEMENT"),
  )
}

fn function_already_registered_error(
  field_name: String,
) -> function_types.UserError {
  function_types.UserError(
    field: [field_name],
    message: "Could not enable cart transform because it is already registered",
    code: Some("FUNCTION_ALREADY_REGISTERED"),
  )
}

fn max_validations_activated_error() -> function_types.UserError {
  function_types.UserError(
    field: [],
    message: "Cannot have more than 25 active validation functions.",
    code: Some("MAX_VALIDATIONS_ACTIVATED"),
  )
}

fn validation_not_found_error() -> function_types.UserError {
  function_types.UserError(
    field: ["id"],
    message: "Extension not found.",
    code: Some("NOT_FOUND"),
  )
}

fn cart_transform_delete_not_found_error(
  id: String,
) -> function_types.UserError {
  let canonical_id = canonical_cart_transform_id(id)
  function_types.UserError(
    field: ["id"],
    message: "Could not find cart transform with id: " <> canonical_id,
    code: Some("NOT_FOUND"),
  )
}

fn unauthorized_app_scope_error() -> function_types.UserError {
  function_types.UserError(
    field: ["base"],
    message: "The app is not authorized to access this Function resource.",
    code: Some("UNAUTHORIZED_APP_SCOPE"),
  )
}

fn canonical_validation_id(id: String) -> String {
  resource_ids.canonical_shopify_resource_gid("Validation", id)
}

fn canonical_cart_transform_id(id: String) -> String {
  resource_ids.canonical_shopify_resource_gid("CartTransform", id)
}

fn handle_validation_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = case graphql_helpers.read_arg_object(args, "validation") {
    Some(d) -> d
    None -> dict.new()
  }
  let reference = read_function_reference(input)
  case reference.function_id, reference.function_handle {
    None, None -> {
      let payload =
        serializers.validation_mutation_payload(store, field, fragments, None, [
          validation_missing_function_identifier_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(_), Some(_) -> {
      let payload =
        serializers.validation_mutation_payload(store, field, fragments, None, [
          validation_multiple_function_identifiers_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    _, _ -> {
      let enable =
        graphql_helpers.read_arg_bool(input, "enable")
        |> option.or(Some(False))
      case validation_enable_would_exceed_cap(store, "", enable) {
        True -> {
          let payload =
            serializers.validation_mutation_payload(
              store,
              field,
              fragments,
              None,
              [
                max_validations_activated_error(),
              ],
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
        False -> {
          case resolve_validation_function(store, reference) {
            Error(user_error) -> {
              let payload =
                serializers.validation_mutation_payload(
                  store,
                  field,
                  fragments,
                  None,
                  [
                    user_error,
                  ],
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [],
                ),
                store,
                identity,
              )
            }
            Ok(shopify_fn) -> {
              let title = graphql_helpers.read_arg_string(input, "title")
              let final_title = case title {
                Some(t) -> Some(t)
                None -> shopify_fn.title
              }
              let #(timestamp, identity_after_ts) =
                synthetic_identity.make_synthetic_timestamp(identity)
              let #(validation_id, identity_final) =
                synthetic_identity.make_synthetic_gid(
                  identity_after_ts,
                  "Validation",
                )
              let #(metafields, identity_after_metafields) =
                read_validation_metafields(
                  input,
                  validation_id,
                  timestamp,
                  identity_final,
                )
              let block_on_failure = case
                graphql_helpers.read_arg_bool(input, "blockOnFailure")
              {
                Some(b) -> Some(b)
                None -> Some(False)
              }
              let function_handle = case reference.function_handle {
                Some(_) -> reference.function_handle
                None -> shopify_fn.handle
              }
              let validation =
                ValidationRecord(
                  id: validation_id,
                  title: final_title,
                  enable: enable,
                  block_on_failure: block_on_failure,
                  function_id: reference.function_id,
                  function_handle: function_handle,
                  shopify_function_id: Some(shopify_fn.id),
                  metafields: metafields,
                  created_at: Some(timestamp),
                  updated_at: Some(timestamp),
                )
              let #(_, store_final) =
                store.upsert_staged_validation(store, validation)
              let payload =
                serializers.validation_mutation_payload(
                  store_final,
                  field,
                  fragments,
                  Some(validation),
                  [],
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [
                    validation.id,
                  ],
                ),
                store_final,
                identity_after_metafields,
              )
            }
          }
        }
      }
    }
  }
}

fn handle_validation_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = case graphql_helpers.read_arg_string(args, "id") {
    Some(s) -> s
    None -> ""
  }
  case store.get_effective_validation_by_id(store, id) {
    None -> {
      let payload =
        serializers.validation_mutation_payload(store, field, fragments, None, [
          validation_not_found_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(current) -> {
      let input = case graphql_helpers.read_arg_object(args, "validation") {
        Some(d) -> d
        None -> dict.new()
      }
      let #(maybe_shopify_fn, store_after_fn, identity_after_fn) = case
        current.shopify_function_id
      {
        Some(fn_id) -> #(
          store.get_effective_shopify_function_by_id(store, fn_id),
          store,
          identity,
        )
        None -> #(None, store, identity)
      }
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_fn)
      let new_title = case graphql_helpers.read_arg_string(input, "title") {
        Some(s) -> Some(s)
        None -> current.title
      }
      let new_enable =
        graphql_helpers.read_arg_bool(input, "enable")
        |> option.or(Some(False))
      case validation_enable_would_exceed_cap(store, current.id, new_enable) {
        True -> {
          let payload =
            serializers.validation_mutation_payload(
              store,
              field,
              fragments,
              None,
              [
                max_validations_activated_error(),
              ],
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
        False -> {
          let new_block_on_failure = case
            graphql_helpers.read_arg_bool(input, "blockOnFailure")
          {
            Some(b) -> Some(b)
            None -> Some(False)
          }
          let #(new_metafields, identity_after_metafields) = case
            dict.has_key(input, "metafields")
          {
            True ->
              read_validation_metafields(
                input,
                current.id,
                timestamp,
                identity_after_ts,
              )
            False -> #(current.metafields, identity_after_ts)
          }
          let new_shopify_function_id = case maybe_shopify_fn {
            Some(fn_record) -> Some(fn_record.id)
            None -> current.shopify_function_id
          }
          let updated =
            ValidationRecord(
              id: current.id,
              title: new_title,
              enable: new_enable,
              block_on_failure: new_block_on_failure,
              function_id: current.function_id,
              function_handle: current.function_handle,
              shopify_function_id: new_shopify_function_id,
              metafields: new_metafields,
              created_at: current.created_at,
              updated_at: Some(timestamp),
            )
          let #(_, store_final) =
            store.upsert_staged_validation(store_after_fn, updated)
          let payload =
            serializers.validation_mutation_payload(
              store_final,
              field,
              fragments,
              Some(updated),
              [],
            )
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [
                updated.id,
              ],
            ),
            store_final,
            identity_after_metafields,
          )
        }
      }
    }
  }
}

fn handle_validation_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = case graphql_helpers.read_arg_string(args, "id") {
    Some(s) -> canonical_validation_id(s)
    None -> canonical_validation_id("")
  }
  case store.get_effective_validation_by_id(store, id) {
    None -> {
      let payload =
        serializers.delete_payload(field, fragments, None, [
          validation_not_found_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(_) -> {
      let next_store = store.delete_staged_validation(store, id)
      let payload = serializers.delete_payload(field, fragments, Some(id), [])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          id,
        ]),
        next_store,
        identity,
      )
    }
  }
}

fn handle_cart_transform_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = case graphql_helpers.read_arg_object(args, "cartTransform") {
    Some(d) -> d
    None -> args
  }
  let reference = read_function_reference(input)
  case reference.function_id, reference.function_handle {
    None, None -> {
      let payload =
        serializers.cart_transform_mutation_payload(field, fragments, None, [
          missing_cart_transform_function_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(_), Some(_) -> {
      let payload =
        serializers.cart_transform_mutation_payload(field, fragments, None, [
          multiple_function_identifiers_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    _, _ -> {
      let title = graphql_helpers.read_arg_string(input, "title")
      let #(resolution, store_after_fn, identity_after_fn) =
        resolve_cart_transform_function(store, identity, reference)
      case resolution {
        Error(user_error) -> {
          let payload =
            serializers.cart_transform_mutation_payload(field, fragments, None, [
              user_error,
            ])
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store_after_fn,
            identity_after_fn,
          )
        }
        Ok(shopify_fn) -> {
          let field_name = cart_transform_reference_field(reference)
          case cart_transform_function_in_use(store_after_fn, shopify_fn) {
            True -> {
              let payload =
                serializers.cart_transform_mutation_payload(
                  field,
                  fragments,
                  None,
                  [
                    function_already_registered_error(field_name),
                  ],
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [],
                ),
                store_after_fn,
                identity_after_fn,
              )
            }
            False -> {
              let #(timestamp, identity_after_ts) =
                synthetic_identity.make_synthetic_timestamp(identity_after_fn)
              let #(cart_transform_id, identity_final) =
                synthetic_identity.make_synthetic_gid(
                  identity_after_ts,
                  "CartTransform",
                )
              let #(metafields, metafield_errors, identity_after_metafields) =
                read_cart_transform_metafields(
                  input,
                  cart_transform_id,
                  timestamp,
                  identity_final,
                )
              let final_title = case title {
                Some(t) -> Some(t)
                None -> shopify_fn.title
              }
              let function_handle = case reference.function_handle {
                Some(_) -> reference.function_handle
                None -> shopify_fn.handle
              }
              let block_on_failure = case
                graphql_helpers.read_arg_bool(input, "blockOnFailure")
              {
                Some(b) -> Some(b)
                None -> Some(False)
              }
              case metafield_errors {
                [_, ..] -> {
                  let payload =
                    serializers.cart_transform_mutation_payload(
                      field,
                      fragments,
                      None,
                      metafield_errors,
                    )
                  #(
                    MutationFieldResult(
                      key: key,
                      payload: payload,
                      staged_resource_ids: [],
                    ),
                    store_after_fn,
                    identity_after_metafields,
                  )
                }
                [] -> {
                  let cart_transform =
                    CartTransformRecord(
                      id: cart_transform_id,
                      title: final_title,
                      block_on_failure: block_on_failure,
                      function_id: Some(shopify_fn.id),
                      function_handle: function_handle,
                      shopify_function_id: Some(shopify_fn.id),
                      metafields: metafields,
                      created_at: Some(timestamp),
                      updated_at: Some(timestamp),
                    )
                  let #(_, store_final) =
                    store.upsert_staged_cart_transform(
                      store_after_fn,
                      cart_transform,
                    )
                  let payload =
                    serializers.cart_transform_mutation_payload(
                      field,
                      fragments,
                      Some(cart_transform),
                      [],
                    )
                  #(
                    MutationFieldResult(
                      key: key,
                      payload: payload,
                      staged_resource_ids: [cart_transform.id],
                    ),
                    store_final,
                    identity_after_metafields,
                  )
                }
              }
            }
          }
        }
      }
    }
  }
}

fn handle_cart_transform_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = case graphql_helpers.read_arg_string(args, "id") {
    Some(s) -> canonical_cart_transform_id(s)
    None -> canonical_cart_transform_id("")
  }
  case store.get_effective_cart_transform_by_id(store, id) {
    None -> {
      let payload =
        serializers.delete_payload(field, fragments, None, [
          cart_transform_delete_not_found_error(id),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(record) -> {
      case cart_transform_delete_authorization_error(store, record) {
        Some(error) -> {
          let payload =
            serializers.delete_payload(field, fragments, None, [error])
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
        None -> {
          let next_store = store.delete_staged_cart_transform(store, id)
          let payload =
            serializers.delete_payload(field, fragments, Some(id), [])
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [id],
            ),
            next_store,
            identity,
          )
        }
      }
    }
  }
}

fn cart_transform_delete_authorization_error(
  store: Store,
  record: CartTransformRecord,
) -> Option(function_types.UserError) {
  use function_id <- option.then(record.shopify_function_id)
  use function_record <- option.then(store.get_effective_shopify_function_by_id(
    store,
    function_id,
  ))
  use function_app_key <- option.then(shopify_function_app_key(function_record))
  use current_installation <- option.then(store.get_current_app_installation(
    store,
  ))
  use current_app <- option.then(store.get_effective_app_by_id(
    store,
    current_installation.app_id,
  ))
  use current_app_key <- option.then(current_app.api_key)
  case function_app_key == current_app_key {
    True -> None
    False -> Some(unauthorized_app_scope_error())
  }
}

fn shopify_function_app_key(record: ShopifyFunctionRecord) -> Option(String) {
  case record.app_key {
    Some(key) -> Some(key)
    None ->
      case record.app {
        Some(app) -> app.api_key
        None -> None
      }
  }
}

fn handle_tax_app_configure(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let ready = graphql_helpers.read_arg_bool(args, "ready")
  let user_errors = case ready {
    None -> [
      function_types.UserError(
        field: ["ready"],
        message: "Ready must be true or false",
        code: Some("INVALID"),
      ),
    ]
    Some(_) -> []
  }
  let #(configuration_after, next_store, next_identity, staged_id) = case
    ready
  {
    Some(value) -> {
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let state = case value {
        True -> "READY"
        False -> "NOT_READY"
      }
      let configuration =
        TaxAppConfigurationRecord(
          id: "gid://shopify/TaxAppConfiguration/local",
          ready: value,
          state: state,
          updated_at: Some(timestamp),
        )
      let updated_store =
        store.set_staged_tax_app_configuration(store, configuration)
      #(Some(configuration), updated_store, identity_after_ts, [
        configuration.id,
      ])
    }
    None -> #(
      store.get_effective_tax_app_configuration(store),
      store,
      identity,
      [],
    )
  }
  let payload =
    serializers.tax_app_payload(
      field,
      fragments,
      configuration_after,
      user_errors,
    )
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: staged_id,
    ),
    next_store,
    next_identity,
  )
}

// ---------------------------------------------------------------------------
// Upstream ShopifyFunction hydration
// ---------------------------------------------------------------------------

fn hydrate_referenced_shopify_functions(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  list.fold(fields, store, fn(acc, field) {
    case function_reference_for_mutation(field, variables) {
      Some(#(reference, api_type)) ->
        hydrate_shopify_function_reference(acc, reference, api_type, upstream)
      None -> acc
    }
  })
}

fn function_reference_for_mutation(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(#(function_types.FunctionReference, String)) {
  case field {
    Field(name: name, ..) -> {
      let args = graphql_helpers.field_args(field, variables)
      case name.value {
        "validationCreate" -> {
          let input = case graphql_helpers.read_arg_object(args, "validation") {
            Some(d) -> d
            None -> dict.new()
          }
          let reference = read_function_reference(input)
          case reference.function_id, reference.function_handle {
            Some(_), Some(_) -> None
            None, None -> None
            _, _ -> Some(#(reference, "VALIDATION"))
          }
        }
        "validationUpdate" -> None
        "cartTransformCreate" -> {
          let input = case
            graphql_helpers.read_arg_object(args, "cartTransform")
          {
            Some(d) -> d
            None -> args
          }
          let reference = read_function_reference(input)
          case reference.function_id, reference.function_handle {
            Some(_), Some(_) -> None
            None, None -> None
            _, _ -> Some(#(reference, "CART_TRANSFORM"))
          }
        }
        _ -> None
      }
    }
    _ -> None
  }
}

fn hydrate_shopify_function_reference(
  store: Store,
  reference: function_types.FunctionReference,
  api_type: String,
  upstream: UpstreamContext,
) -> Store {
  case reference.function_id, reference.function_handle {
    None, None -> store
    _, _ ->
      case find_existing_shopify_function(store, reference) {
        Some(_) -> store
        None ->
          case fetch_shopify_function(upstream, reference, api_type) {
            Some(record) -> store.upsert_base_shopify_functions(store, [record])
            None -> store
          }
      }
  }
}

fn fetch_shopify_function(
  upstream: UpstreamContext,
  reference: function_types.FunctionReference,
  api_type: String,
) -> Option(ShopifyFunctionRecord) {
  case reference.function_id {
    Some(id) -> fetch_shopify_function_by_id(upstream, id)
    None ->
      case reference.function_handle {
        Some(handle) ->
          fetch_shopify_function_by_handle(upstream, handle, api_type)
        None -> None
      }
  }
}

const function_hydrate_selection: String = " id title handle apiType description appKey app { __typename id title handle apiKey } "

fn fetch_shopify_function_by_id(
  upstream: UpstreamContext,
  id: String,
) -> Option(ShopifyFunctionRecord) {
  let query =
    "query FunctionHydrateById($id: String!) { shopifyFunction(id: $id) {"
    <> function_hydrate_selection
    <> " } }"
  let variables = json.object([#("id", json.string(id))])
  case
    fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "FunctionHydrateById",
      query,
      variables,
    )
  {
    Ok(response) -> shopify_function_from_id_response(response)
    Error(_) -> None
  }
}

fn fetch_shopify_function_by_handle(
  upstream: UpstreamContext,
  handle: String,
  api_type: String,
) -> Option(ShopifyFunctionRecord) {
  let query =
    "query FunctionHydrateByHandle { shopifyFunctions(first: 50, apiType: "
    <> api_type
    <> ") { nodes {"
    <> function_hydrate_selection
    <> " } } }"
  let variables =
    json.object([
      #("handle", json.string(handle)),
      #("apiType", json.string(api_type)),
    ])
  case
    fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "FunctionHydrateByHandle",
      query,
      variables,
    )
  {
    Ok(response) -> shopify_function_from_handle_response(response, handle)
    Error(_) -> None
  }
}

fn shopify_function_from_id_response(
  value: commit.JsonValue,
) -> Option(ShopifyFunctionRecord) {
  use data <- option.then(json_get(value, "data"))
  use node <- option.then(non_null_json(json_get(data, "shopifyFunction")))
  shopify_function_from_json(node)
}

fn shopify_function_from_handle_response(
  value: commit.JsonValue,
  handle: String,
) -> Option(ShopifyFunctionRecord) {
  use data <- option.then(json_get(value, "data"))
  use connection <- option.then(
    non_null_json(json_get(data, "shopifyFunctions")),
  )
  use nodes <- option.then(json_get_array(connection, "nodes"))
  list.find_map(nodes, fn(node) {
    case shopify_function_from_json(node) {
      Some(record) ->
        case shopify_function_matches_handle(record, handle) {
          True -> Ok(record)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> result_to_option
}

fn shopify_function_matches_handle(
  record: ShopifyFunctionRecord,
  handle: String,
) -> Bool {
  let normalized = normalize_function_handle(handle)
  let handle_id = shopify_function_id_from_handle(handle)
  record.handle == Some(handle)
  || record.handle == Some(normalized)
  || record.id == handle_id
}

fn shopify_function_from_json(
  node: commit.JsonValue,
) -> Option(ShopifyFunctionRecord) {
  use id <- option.then(json_get_string(node, "id"))
  Some(ShopifyFunctionRecord(
    id: id,
    title: json_get_string(node, "title"),
    handle: json_get_string(node, "handle"),
    api_type: json_get_string(node, "apiType"),
    description: json_get_string(node, "description"),
    app_key: json_get_string(node, "appKey"),
    app: non_null_json(json_get(node, "app"))
      |> option.then(shopify_function_app_from_json),
  ))
}

fn shopify_function_app_from_json(
  node: commit.JsonValue,
) -> Option(ShopifyFunctionAppRecord) {
  Some(ShopifyFunctionAppRecord(
    typename: json_get_string(node, "__typename"),
    id: json_get_string(node, "id"),
    title: json_get_string(node, "title"),
    handle: json_get_string(node, "handle"),
    api_key: json_get_string(node, "apiKey"),
  ))
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(name, child) if name == key -> Ok(child)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn non_null_json(value: Option(commit.JsonValue)) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(v) -> Some(v)
    None -> None
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get_array(
  value: commit.JsonValue,
  key: String,
) -> Option(List(commit.JsonValue)) {
  case json_get(value, key) {
    Some(commit.JsonArray(items)) -> Some(items)
    _ -> None
  }
}

/// Look up an existing `ShopifyFunctionRecord` matching the supplied
/// reference. Mirrors `findExistingShopifyFunction`. Match order:
///   1. exact-id match (when functionId provided)
///   2. exact-handle match
///   3. normalized-handle match
///   4. handle-derived id match
fn find_existing_shopify_function(
  store: Store,
  reference: function_types.FunctionReference,
) -> Option(ShopifyFunctionRecord) {
  case reference.function_id {
    Some(id) -> store.get_effective_shopify_function_by_id(store, id)
    None ->
      case reference.function_handle {
        None -> None
        Some(handle) -> {
          let normalized = normalize_function_handle(handle)
          let handle_based_id = shopify_function_id_from_handle(handle)
          let candidates = store.list_effective_shopify_functions(store)
          list.find(candidates, fn(record) {
            record.handle == Some(handle)
            || record.handle == Some(normalized)
            || record.id == handle_based_id
          })
          |> result_to_option
        }
      }
  }
}

fn resolve_validation_function(
  store: Store,
  reference: function_types.FunctionReference,
) -> Result(ShopifyFunctionRecord, function_types.UserError) {
  let field_name = cart_transform_reference_field(reference)
  case find_existing_shopify_function(store, reference) {
    None -> Error(validation_function_not_found_error(field_name))
    Some(record) ->
      case validation_function_api_supported(record) {
        True -> Ok(record)
        False -> Error(validation_function_does_not_implement_error(field_name))
      }
  }
}

fn resolve_cart_transform_function(
  store: Store,
  identity: SyntheticIdentityRegistry,
  reference: function_types.FunctionReference,
) -> #(
  Result(ShopifyFunctionRecord, function_types.UserError),
  Store,
  SyntheticIdentityRegistry,
) {
  let field_name = cart_transform_reference_field(reference)
  let value = cart_transform_reference_value(reference)
  case find_existing_shopify_function(store, reference) {
    None -> #(
      Error(function_not_found_error(field_name, value)),
      store,
      identity,
    )
    Some(record) ->
      case cart_transform_function_api_supported(record) {
        True -> #(Ok(record), store, identity)
        False -> {
          let user_error = case field_name {
            "functionId" -> function_id_api_mismatch_error(field_name)
            _ -> function_does_not_implement_error(field_name)
          }
          #(Error(user_error), store, identity)
        }
      }
  }
}

fn cart_transform_reference_field(
  reference: function_types.FunctionReference,
) -> String {
  case reference.function_id {
    Some(_) -> "functionId"
    None -> "functionHandle"
  }
}

fn cart_transform_reference_value(
  reference: function_types.FunctionReference,
) -> String {
  case reference.function_id {
    Some(id) -> id
    None ->
      case reference.function_handle {
        Some(handle) -> handle
        None -> ""
      }
  }
}

fn cart_transform_function_api_supported(
  record: ShopifyFunctionRecord,
) -> Bool {
  case record.api_type {
    None -> True
    Some(api_type) -> normalize_function_api_type(api_type) == "CART_TRANSFORM"
  }
}

fn validation_function_api_supported(record: ShopifyFunctionRecord) -> Bool {
  case record.api_type {
    None -> True
    Some(api_type) -> {
      let normalized = normalize_function_api_type(api_type)
      normalized == "VALIDATION" || normalized == "CART_CHECKOUT_VALIDATION"
    }
  }
}

fn normalize_function_api_type(api_type: String) -> String {
  api_type
  |> string.uppercase
  |> string.replace("-", "_")
}

fn cart_transform_function_in_use(
  store: Store,
  shopify_fn: ShopifyFunctionRecord,
) -> Bool {
  store.list_effective_cart_transforms(store)
  |> list.any(fn(record) {
    record.shopify_function_id == Some(shopify_fn.id)
    || record.function_id == Some(shopify_fn.id)
  })
}

fn result_to_option(result: Result(a, b)) -> Option(a) {
  case result {
    Ok(value) -> Some(value)
    Error(_) -> None
  }
}
