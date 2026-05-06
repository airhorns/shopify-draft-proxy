//// Payments mutation dispatch and payment customization handling.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/functions
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, SrcList, SrcNull, SrcString, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  project_graphql_field_value, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, LogDraft, MutationOutcome,
}
import shopify_draft_proxy/proxy/payments/payment_methods.{
  create_credit_card_payment_method, create_payment_method_from_duplication_data,
  create_paypal_payment_method, create_remote_payment_method,
  dict_string_to_option, get_payment_method_duplication_data,
  get_payment_method_update_url, hydrate_customer_payment_method_context,
  revoke_payment_method, update_credit_card_payment_method,
  update_paypal_payment_method,
}
import shopify_draft_proxy/proxy/payments/payment_terms.{
  create_payment_terms, delete_payment_terms, hydrate_payment_schedule_context,
  maybe_hydrate_payment_terms_owner, send_payment_reminder, update_payment_terms,
}
import shopify_draft_proxy/proxy/payments/serializers.{
  payment_customization_source, project_payment_customization,
}
import shopify_draft_proxy/proxy/payments/types.{
  type MutationFieldResult, type UserError, MutationFieldResult, UserError,
  customization_app_id, decode_duplication_data, gid_tail, has_key,
  mutation_payload_result, option_string_source, project_payload,
  read_bool_field, read_string_field, user_errors_source,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types as state_types

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
      let store =
        hydrate_before_payments_mutation(store, fields, variables, upstream)
      handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        document,
        variables,
      )
    }
  }
}

fn hydrate_before_payments_mutation(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  let #(customer_ids, method_ids, owner_ids, schedule_ids) =
    list.fold(fields, #([], [], [], []), fn(acc, field) {
      let #(customer_acc, method_acc, owner_acc, schedule_acc) = acc
      let #(customers, methods, owners, schedules) =
        payment_mutation_hydrate_inputs(field, variables)
      #(
        list.append(customer_acc, customers),
        list.append(method_acc, methods),
        list.append(owner_acc, owners),
        list.append(schedule_acc, schedules),
      )
    })
  let with_payment_methods =
    hydrate_customer_payment_method_context(
      store,
      unique_strings(customer_ids, []),
      unique_strings(method_ids, []),
      upstream,
    )
  list.fold(unique_strings(owner_ids, []), with_payment_methods, fn(acc, id) {
    maybe_hydrate_payment_terms_owner(acc, id, upstream)
  })
  |> hydrate_payment_schedule_context(
    unique_strings(schedule_ids, []),
    upstream,
  )
}

fn payment_mutation_hydrate_inputs(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(List(String), List(String), List(String), List(String)) {
  case field {
    Field(name: name, ..) -> {
      let args = graphql_helpers.field_args(field, variables)
      case name.value {
        "customerPaymentMethodCreditCardCreate"
        | "customerPaymentMethodRemoteCreate"
        | "customerPaymentMethodPaypalBillingAgreementCreate" -> #(
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "customerId",
          )),
          [],
          [],
          [],
        )
        "customerPaymentMethodCreditCardUpdate"
        | "customerPaymentMethodPaypalBillingAgreementUpdate" -> #(
          [],
          option_to_list(graphql_helpers.read_arg_string_nonempty(args, "id")),
          [],
          [],
        )
        "customerPaymentMethodGetDuplicationData" -> #(
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "targetCustomerId",
          )),
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "customerPaymentMethodId",
          )),
          [],
          [],
        )
        "customerPaymentMethodCreateFromDuplicationData" -> {
          let method_id =
            graphql_helpers.read_arg_string_nonempty(
              args,
              "encryptedDuplicationData",
            )
            |> option.then(fn(raw) {
              case decode_duplication_data(raw) {
                Ok(payload) ->
                  dict_string_to_option(payload, "customerPaymentMethodId")
                Error(_) -> None
              }
            })
          #(
            option_to_list(graphql_helpers.read_arg_string_nonempty(
              args,
              "customerId",
            )),
            option_to_list(method_id),
            [],
            [],
          )
        }
        "customerPaymentMethodGetUpdateUrl" | "customerPaymentMethodRevoke" -> #(
          [],
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "customerPaymentMethodId",
          )),
          [],
          [],
        )
        "paymentTermsCreate" -> #(
          [],
          [],
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "referenceId",
          )),
          [],
        )
        "paymentReminderSend" -> #(
          [],
          [],
          [],
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "paymentScheduleId",
          )),
        )
        _ -> #([], [], [], [])
      }
    }
    _ -> #([], [], [], [])
  }
}

fn option_to_list(value: Option(String)) -> List(String) {
  case value {
    Some(s) -> [s]
    None -> []
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  query: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [])
  let #(entries, final_store, final_identity, staged_ids) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entry_acc, current_store, current_identity, staged_acc) = acc
      let #(result, next_store, next_identity) =
        handle_mutation_field(
          current_store,
          current_identity,
          field,
          fragments,
          variables,
        )
      let result_staged = result.staged_resource_ids
      #(
        list.append(entry_acc, [#(result.key, result.payload)]),
        next_store,
        next_identity,
        list.append(staged_acc, result_staged),
      )
    })
  let root_names = root_names(fields)
  let drafts = case root_names {
    [] -> []
    [primary, ..] -> [
      LogDraft(
        operation_name: Some(primary),
        root_fields: root_names,
        primary_root_field: Some(primary),
        domain: "payments",
        execution: "stage-locally",
        query: Some(query),
        variables: Some(variables),
        staged_resource_ids: staged_ids,
        status: store.Staged,
        notes: Some(
          "Staged payments mutations locally in the in-memory draft store; payment credentials, gateway side effects, customer-facing URLs, and reminder delivery are scrubbed or synthetic.",
        ),
      ),
    ]
  }
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: drafts,
  )
}

fn root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}

fn handle_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "paymentCustomizationCreate" ->
          create_payment_customization(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "paymentCustomizationUpdate" ->
          update_payment_customization(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "paymentCustomizationDelete" ->
          delete_payment_customization(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "paymentCustomizationActivation" ->
          activate_payment_customizations(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodCreditCardCreate" ->
          create_credit_card_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodCreditCardUpdate" ->
          update_credit_card_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodRemoteCreate" ->
          create_remote_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodPaypalBillingAgreementCreate" ->
          create_paypal_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodPaypalBillingAgreementUpdate" ->
          update_paypal_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodGetDuplicationData" ->
          get_payment_method_duplication_data(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodCreateFromDuplicationData" ->
          create_payment_method_from_duplication_data(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodGetUpdateUrl" ->
          get_payment_method_update_url(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodRevoke" ->
          revoke_payment_method(store, identity, field, fragments, variables)
        "paymentTermsCreate" ->
          create_payment_terms(store, identity, field, fragments, variables)
        "paymentTermsUpdate" ->
          update_payment_terms(store, identity, field, fragments, variables)
        "paymentTermsDelete" ->
          delete_payment_terms(store, identity, field, fragments, variables)
        "paymentReminderSend" ->
          send_payment_reminder(store, identity, field, fragments, variables)
        _ -> #(
          MutationFieldResult(
            get_field_response_key(field),
            json.null(),
            [],
            name.value,
            None,
          ),
          store,
          identity,
        )
      }
    _ -> #(MutationFieldResult("", json.null(), [], "", None), store, identity)
  }
}

fn payment_customization_error(
  field: List(String),
  message: String,
  code: String,
) -> UserError {
  UserError(field: Some(field), message: message, code: Some(code))
}

fn required_customization_input_error(field_name: String) -> UserError {
  payment_customization_error(
    ["paymentCustomization", field_name],
    "Required input field must be present.",
    "REQUIRED_INPUT_FIELD",
  )
}

fn missing_function_error(function_id: String) -> UserError {
  payment_customization_error(
    ["paymentCustomization", "functionId"],
    "Function "
      <> function_id
      <> " not found. Ensure that it is released in the current app ("
      <> customization_app_id
      <> "), and that the app is installed.",
    "FUNCTION_NOT_FOUND",
  )
}

fn missing_function_handle_error(function_handle: String) -> UserError {
  payment_customization_error(
    ["paymentCustomization", "functionHandle"],
    "Could not find function with handle: " <> function_handle <> ".",
    "FUNCTION_NOT_FOUND",
  )
}

fn function_id_cannot_be_changed_error() -> UserError {
  payment_customization_error(
    ["paymentCustomization", "functionId"],
    "Function ID cannot be changed.",
    "FUNCTION_ID_CANNOT_BE_CHANGED",
  )
}

fn invalid_metafield_error(index: Int, field_name: String) -> UserError {
  payment_customization_error(
    ["paymentCustomization", "metafields", int.to_string(index), field_name],
    "Metafield namespace, key, and type must be present.",
    "INVALID_METAFIELDS",
  )
}

fn normalize_payment_customization_metafield_namespace(
  namespace: String,
) -> String {
  case string.starts_with(namespace, "$app:") {
    True ->
      "app--" <> customization_app_id <> "--" <> string.drop_start(namespace, 5)
    False -> namespace
  }
}

fn customization_not_found_error(field_name: String, id: String) -> UserError {
  payment_customization_error(
    [field_name],
    "Could not find PaymentCustomization with id: " <> id,
    "PAYMENT_CUSTOMIZATION_NOT_FOUND",
  )
}

fn customization_activation_not_found_error(ids: List(String)) -> UserError {
  payment_customization_error(
    ["ids"],
    "Could not find payment customizations with IDs: " <> string.join(ids, ", "),
    "PAYMENT_CUSTOMIZATION_NOT_FOUND",
  )
}

fn validate_create_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  let function_id = read_string_field(input, "functionId")
  let function_handle = read_string_field(input, "functionHandle")
  case
    has_key(input, "title"),
    has_key(input, "enabled"),
    function_id,
    function_handle
  {
    False, _, _, _ -> [required_customization_input_error("title")]
    _, False, _, _ -> [required_customization_input_error("enabled")]
    _, _, None, None -> [required_customization_input_error("functionId")]
    _, _, Some(function_id), _ ->
      case gid_tail(function_id) == "0" {
        True -> [missing_function_error(function_id)]
        False -> validate_payment_customization_metafield_input(input)
      }
    _, _, None, Some(_) -> validate_payment_customization_metafield_input(input)
  }
}

fn validate_update_input(
  store: Store,
  current: state_types.PaymentCustomizationRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  let function_errors = validate_update_function_input(store, current, input)
  case function_errors {
    [_, ..] -> function_errors
    [] -> validate_payment_customization_metafield_input(input)
  }
}

fn validate_update_function_input(
  store: Store,
  current: state_types.PaymentCustomizationRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case
    read_string_field(input, "functionId"),
    read_string_field(input, "functionHandle")
  {
    Some(function_id), _ -> validate_update_function_id(current, function_id)
    None, Some(function_handle) ->
      validate_update_function_handle(store, current, function_handle)
    None, None -> []
  }
}

fn validate_update_function_id(
  current: state_types.PaymentCustomizationRecord,
  function_id: String,
) -> List(UserError) {
  case function_id_matches_current(current, function_id) {
    True -> []
    False -> [function_id_cannot_be_changed_error()]
  }
}

fn validate_update_function_handle(
  store: Store,
  current: state_types.PaymentCustomizationRecord,
  function_handle: String,
) -> List(UserError) {
  case function_handle_matches_current(current, function_handle) {
    True -> []
    False ->
      case find_payment_shopify_function_by_handle(store, function_handle) {
        Some(record) ->
          case function_record_matches_current(current, record) {
            True -> []
            False -> [function_id_cannot_be_changed_error()]
          }
        None ->
          case raw_function_id_can_accept_handle(current) {
            True -> []
            False ->
              case
                list.is_empty(store.list_effective_shopify_functions(store))
              {
                True -> [function_id_cannot_be_changed_error()]
                False -> [missing_function_handle_error(function_handle)]
              }
          }
      }
  }
}

fn function_id_matches_current(
  current: state_types.PaymentCustomizationRecord,
  function_id: String,
) -> Bool {
  current.function_id == Some(function_id)
  || current.function_id
  |> option.map(fn(current_id) { gid_tail(current_id) == gid_tail(function_id) })
  |> option.unwrap(False)
  || case current.function_handle {
    Some(handle) ->
      function_id == functions.shopify_function_id_from_handle(handle)
    None -> False
  }
}

fn function_handle_matches_current(
  current: state_types.PaymentCustomizationRecord,
  function_handle: String,
) -> Bool {
  let normalized = functions.normalize_function_handle(function_handle)
  case current.function_handle {
    Some(handle) ->
      handle == function_handle
      || functions.normalize_function_handle(handle) == normalized
    None ->
      current.function_id
      == Some(functions.shopify_function_id_from_handle(function_handle))
  }
}

fn find_payment_shopify_function_by_handle(
  store: Store,
  function_handle: String,
) -> Option(state_types.ShopifyFunctionRecord) {
  let normalized = functions.normalize_function_handle(function_handle)
  let handle_id = functions.shopify_function_id_from_handle(function_handle)
  store.list_effective_shopify_functions(store)
  |> list.find(fn(record) {
    record.handle == Some(function_handle)
    || record.handle == Some(normalized)
    || record.id == handle_id
  })
  |> option.from_result
}

fn function_record_matches_current(
  current: state_types.PaymentCustomizationRecord,
  record: state_types.ShopifyFunctionRecord,
) -> Bool {
  current.function_id == Some(record.id)
  || case current.function_id, record.handle {
    Some(id), Some(handle) ->
      id == functions.shopify_function_id_from_handle(handle)
    _, _ -> False
  }
  || case current.function_handle, record.handle {
    Some(current_handle), Some(record_handle) ->
      functions.normalize_function_handle(current_handle)
      == functions.normalize_function_handle(record_handle)
    _, _ -> False
  }
}

fn raw_function_id_can_accept_handle(
  current: state_types.PaymentCustomizationRecord,
) -> Bool {
  case current.function_id, current.function_handle {
    Some(function_id), None ->
      !string.starts_with(function_id, "gid://shopify/ShopifyFunction/")
    _, _ -> False
  }
}

fn validate_payment_customization_metafield_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case dict.get(input, "metafields") {
    Ok(root_field.ListVal(items)) ->
      list.index_fold(items, [], fn(errors, item, index) {
        case item {
          root_field.ObjectVal(metafield_input) ->
            list.append(
              errors,
              payment_customization_metafield_shape_errors(
                metafield_input,
                index,
              ),
            )
          _ ->
            list.append(errors, [
              payment_customization_error(
                ["paymentCustomization", "metafields", int.to_string(index)],
                "Metafield input must be an object.",
                "INVALID_METAFIELDS",
              ),
            ])
        }
      })
    _ -> []
  }
}

fn payment_customization_metafield_shape_errors(
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(UserError) {
  let required = ["namespace", "key", "type"]
  required
  |> list.filter_map(fn(field_name) {
    case read_string_field(input, field_name) {
      Some(_) -> Error(Nil)
      None -> Ok(invalid_metafield_error(index, field_name))
    }
  })
}

fn payment_customization_metafield_input_objects(
  input: Dict(String, root_field.ResolvedValue),
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, "metafields") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.ObjectVal(metafield_input) -> Ok(metafield_input)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn apply_payment_customization_metafield_inputs(
  identity: SyntheticIdentityRegistry,
  payment_customization_id: String,
  existing: List(state_types.PaymentCustomizationMetafieldRecord),
  input: Dict(String, root_field.ResolvedValue),
) -> #(
  List(state_types.PaymentCustomizationMetafieldRecord),
  SyntheticIdentityRegistry,
) {
  payment_customization_metafield_input_objects(input)
  |> list.fold(#(existing, identity), fn(acc, metafield_input) {
    let #(records, current_identity) = acc
    let existing_metafield =
      find_payment_customization_metafield(records, metafield_input)
    let #(record, next_identity) =
      build_payment_customization_metafield_record(
        current_identity,
        payment_customization_id,
        existing_metafield,
        metafield_input,
      )
    #(upsert_payment_customization_metafield(records, record), next_identity)
  })
}

fn find_payment_customization_metafield(
  records: List(state_types.PaymentCustomizationMetafieldRecord),
  input: Dict(String, root_field.ResolvedValue),
) -> Option(state_types.PaymentCustomizationMetafieldRecord) {
  case read_string_field(input, "id") {
    Some(id) ->
      records
      |> list.find(fn(record) { record.id == id })
      |> option.from_result
    None -> {
      let namespace =
        read_string_field(input, "namespace")
        |> option.map(normalize_payment_customization_metafield_namespace)
      let key = read_string_field(input, "key")
      records
      |> list.find(fn(record) {
        record.namespace == option.unwrap(namespace, "")
        && record.key == option.unwrap(key, "")
      })
      |> option.from_result
    }
  }
}

fn build_payment_customization_metafield_record(
  identity: SyntheticIdentityRegistry,
  payment_customization_id: String,
  existing: Option(state_types.PaymentCustomizationMetafieldRecord),
  input: Dict(String, root_field.ResolvedValue),
) -> #(
  state_types.PaymentCustomizationMetafieldRecord,
  SyntheticIdentityRegistry,
) {
  let #(metafield_id, identity_after_id) = case existing {
    Some(record) -> #(record.id, identity)
    None -> synthetic_identity.make_synthetic_gid(identity, "Metafield")
  }
  let #(timestamp, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  let type_ =
    read_string_field(input, "type")
    |> option.or(option.then(existing, fn(record) { record.type_ }))
  let raw_value =
    read_string_field(input, "value")
    |> option.or(option.then(existing, fn(record) { record.value }))
  let value = metafields.normalize_metafield_value(type_, raw_value)
  #(
    state_types.PaymentCustomizationMetafieldRecord(
      id: metafield_id,
      payment_customization_id: payment_customization_id,
      namespace: read_string_field(input, "namespace")
        |> option.map(normalize_payment_customization_metafield_namespace)
        |> option.unwrap(
          option.map(existing, fn(record) { record.namespace })
          |> option.unwrap(""),
        ),
      key: read_string_field(input, "key")
        |> option.unwrap(
          option.map(existing, fn(record) { record.key })
          |> option.unwrap(""),
        ),
      type_: type_,
      value: value,
      compare_digest: None,
      created_at: option.then(existing, fn(record) { record.created_at })
        |> option.or(Some(timestamp)),
      updated_at: Some(timestamp),
      owner_type: option.then(existing, fn(record) { record.owner_type })
        |> option.or(Some("PAYMENT_CUSTOMIZATION")),
    ),
    next_identity,
  )
}

fn upsert_payment_customization_metafield(
  records: List(state_types.PaymentCustomizationMetafieldRecord),
  record: state_types.PaymentCustomizationMetafieldRecord,
) -> List(state_types.PaymentCustomizationMetafieldRecord) {
  case records {
    [] -> [record]
    [first, ..rest] -> {
      case payment_customization_metafield_matches(first, record) {
        True -> [record, ..rest]
        False -> [first, ..upsert_payment_customization_metafield(rest, record)]
      }
    }
  }
}

fn payment_customization_metafield_matches(
  left: state_types.PaymentCustomizationMetafieldRecord,
  right: state_types.PaymentCustomizationMetafieldRecord,
) -> Bool {
  left.id == right.id
  || { left.namespace == right.namespace && left.key == right.key }
}

fn create_payment_customization(store, identity, field, fragments, variables) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "paymentCustomization",
    )
    |> option.unwrap(dict.new())
  let errors = validate_create_input(input)
  case errors {
    [_, ..] -> #(
      MutationFieldResult(
        get_field_response_key(field),
        customization_payload(None, errors, field, fragments, variables),
        [],
        "paymentCustomizationCreate",
        Some(
          "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
        ),
      ),
      store,
      identity,
    )
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "PaymentCustomization")
      let #(metafields, next_identity) =
        apply_payment_customization_metafield_inputs(
          next_identity,
          id,
          [],
          input,
        )
      let record =
        state_types.PaymentCustomizationRecord(
          id: id,
          title: read_string_field(input, "title"),
          enabled: read_bool_field(input, "enabled"),
          function_id: read_string_field(input, "functionId"),
          function_handle: read_string_field(input, "functionHandle"),
          metafields: metafields,
        )
      let next_store = store.upsert_staged_payment_customization(store, record)
      #(
        MutationFieldResult(
          get_field_response_key(field),
          customization_payload(Some(record), [], field, fragments, variables),
          [id],
          "paymentCustomizationCreate",
          Some(
            "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
          ),
        ),
        next_store,
        next_identity,
      )
    }
  }
}

fn update_payment_customization(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  let id =
    graphql_helpers.read_arg_string_nonempty(args, "id") |> option.unwrap("")
  case store.get_effective_payment_customization_by_id(store, id) {
    None -> #(
      MutationFieldResult(
        get_field_response_key(field),
        customization_payload(
          None,
          [customization_not_found_error("id", id)],
          field,
          fragments,
          variables,
        ),
        [],
        "paymentCustomizationUpdate",
        Some(
          "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
        ),
      ),
      store,
      identity,
    )
    Some(current) -> {
      let input =
        graphql_helpers.read_arg_object(args, "paymentCustomization")
        |> option.unwrap(dict.new())
      let errors = validate_update_input(store, current, input)
      case errors {
        [_, ..] -> #(
          MutationFieldResult(
            get_field_response_key(field),
            customization_payload(None, errors, field, fragments, variables),
            [],
            "paymentCustomizationUpdate",
            Some(
              "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
            ),
          ),
          store,
          identity,
        )
        [] -> {
          let #(metafields, next_identity) =
            apply_payment_customization_metafield_inputs(
              identity,
              current.id,
              current.metafields,
              input,
            )
          let updated =
            state_types.PaymentCustomizationRecord(
              ..current,
              title: read_string_field(input, "title")
                |> option.or(current.title),
              enabled: read_bool_field(input, "enabled")
                |> option.or(current.enabled),
              function_id: current.function_id,
              function_handle: current.function_handle,
              metafields: metafields,
            )
          let next_store =
            store.upsert_staged_payment_customization(store, updated)
          #(
            MutationFieldResult(
              get_field_response_key(field),
              customization_payload(
                Some(updated),
                [],
                field,
                fragments,
                variables,
              ),
              [updated.id],
              "paymentCustomizationUpdate",
              Some(
                "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
              ),
            ),
            next_store,
            next_identity,
          )
        }
      }
    }
  }
}

fn delete_payment_customization(store, identity, field, fragments, variables) {
  let id =
    graphql_helpers.read_arg_string_nonempty(
      graphql_helpers.field_args(field, variables),
      "id",
    )
    |> option.unwrap("")
  case store.get_effective_payment_customization_by_id(store, id) {
    None ->
      mutation_payload_result(
        store,
        identity,
        field,
        delete_customization_payload(
          None,
          [customization_not_found_error("id", id)],
          field,
          fragments,
        ),
        [],
        "paymentCustomizationDelete",
        Some(
          "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
        ),
      )
    Some(_) ->
      mutation_payload_result(
        store.delete_staged_payment_customization(store, id),
        identity,
        field,
        delete_customization_payload(Some(id), [], field, fragments),
        [id],
        "paymentCustomizationDelete",
        Some(
          "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
        ),
      )
  }
}

fn activate_payment_customizations(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  let ids = read_string_list(args, "ids") |> unique_strings([])
  let enabled =
    graphql_helpers.read_arg_bool(args, "enabled") |> option.unwrap(False)
  let #(next_store, updated_ids, missing_ids) =
    list.fold(ids, #(store, [], []), fn(acc, id) {
      let #(current_store, updated, missing) = acc
      case store.get_effective_payment_customization_by_id(current_store, id) {
        Some(record) -> {
          let next =
            store.upsert_staged_payment_customization(
              current_store,
              state_types.PaymentCustomizationRecord(
                ..record,
                enabled: Some(enabled),
              ),
            )
          #(next, list.append(updated, [id]), missing)
        }
        None -> #(current_store, updated, list.append(missing, [id]))
      }
    })
  let errors = case missing_ids {
    [] -> []
    _ -> [customization_activation_not_found_error(missing_ids)]
  }
  mutation_payload_result(
    next_store,
    identity,
    field,
    activation_payload(updated_ids, errors, field, fragments),
    updated_ids,
    "paymentCustomizationActivation",
    Some(
      "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
    ),
  )
}

fn read_string_list(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(String) {
  case dict.get(args, key) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn unique_strings(items: List(String), seen: List(String)) -> List(String) {
  case items {
    [] -> []
    [first, ..rest] ->
      case list.contains(seen, first) {
        True -> unique_strings(rest, seen)
        False -> [first, ..unique_strings(rest, [first, ..seen])]
      }
  }
}

fn customization_payload(
  customization: Option(state_types.PaymentCustomizationRecord),
  errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let source =
    src_object([
      #("paymentCustomization", case customization {
        Some(record) -> payment_customization_source(record)
        None -> SrcNull
      }),
      #("userErrors", user_errors_source(errors)),
    ])
  let entries =
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value, customization {
            "paymentCustomization", Some(record) -> #(
              key,
              project_payment_customization(
                record,
                selection,
                fragments,
                variables,
              ),
            )
            _, _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

fn delete_customization_payload(
  deleted_id: Option(String),
  errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_payload(field, fragments, [
    #("deletedId", option_string_source(deleted_id)),
    #("userErrors", user_errors_source(errors)),
  ])
}

fn activation_payload(
  ids: List(String),
  errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_payload(field, fragments, [
    #("ids", SrcList(list.map(ids, SrcString))),
    #("userErrors", user_errors_source(errors)),
  ])
}
