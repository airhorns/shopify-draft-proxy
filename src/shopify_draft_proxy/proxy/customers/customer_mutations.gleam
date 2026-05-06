//// Customer domain internals split from proxy/customers.gleam.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/customers/customer_types.{
  type MutationFieldResult, type UserError, MutationFieldResult, UserError,
}
import shopify_draft_proxy/proxy/customers/hydration.{connection_nodes}
import shopify_draft_proxy/proxy/customers/inputs.{
  build_address, build_display_name, consent_collected_from_input,
  consent_level_from_input, consent_state_from_input,
  consent_updated_at_from_input, dedupe_customer_addresses,
  find_duplicate_customer_address, gid_tail, has_nested_object, json_get,
  json_get_string, make_email_consent, make_sms_consent, merge_address,
  normalize_tags, option_to_result, read_customer_metafields,
  read_normalized_optional_string, read_normalized_string_with_blank,
  read_obj_addresses, read_obj_array_strings, read_obj_bool,
  read_obj_list_objects, read_obj_string, result_to_option, split_tags,
  update_nullable_note, update_trimmed_nullable_string, validate_address_input,
}
import shopify_draft_proxy/proxy/customers/serializers.{
  address_customer_missing_result, address_id_mismatch_result,
  address_ownership_result, address_payload_json, address_to_default,
  address_unknown_result, customer_address_ownership_result,
  customer_payload_json, find_customer_by_email_or_phone,
}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_field_response_key,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type CustomerAddressRecord,
  type CustomerMetafieldRecord, type CustomerRecord, type OrderRecord,
  CapturedObject, CapturedString, CustomerDefaultEmailAddressRecord,
  CustomerDefaultPhoneNumberRecord, CustomerRecord, Money,
}

@internal
pub fn handle_customer_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let errors = validate_customer_create(store, input, upstream)
  case errors {
    [] -> {
      let #(id, after_id) =
        synthetic_identity.make_synthetic_gid(identity, "Customer")
      let #(timestamp, after_ts) =
        synthetic_identity.make_synthetic_timestamp(after_id)
      let #(customer, address_records, metafields) =
        build_created_customer(id, timestamp, input, after_ts)
      let #(stored_customer, store_after_customer) =
        store.stage_create_customer(store, customer)
      let store_after_addresses =
        list.fold(address_records, store_after_customer, fn(acc, address) {
          let #(_, next_store) =
            store.stage_upsert_customer_address(acc, address)
          next_store
        })
      let store_after_metafields =
        store.stage_customer_metafields(
          store_after_addresses,
          stored_customer.id,
          metafields,
        )
      let payload =
        customer_payload_json(
          store_after_metafields,
          "CustomerCreatePayload",
          Some(stored_customer),
          None,
          None,
          [],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: payload,
          staged_resource_ids: [stored_customer.id],
          root_name: "customerCreate",
        ),
        store_after_metafields,
        after_ts,
      )
    }
    _ -> {
      let payload =
        customer_payload_json(
          store,
          "CustomerCreatePayload",
          None,
          None,
          None,
          errors,
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          key: get_field_response_key(field),
          payload: payload,
          staged_resource_ids: [],
          root_name: "customerCreate",
        ),
        store,
        identity,
      )
    }
  }
}

@internal
pub fn build_created_customer(
  id: String,
  timestamp: String,
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(
  CustomerRecord,
  List(CustomerAddressRecord),
  List(CustomerMetafieldRecord),
) {
  let email = read_obj_string(input, "email")
  let phone = read_obj_string(input, "phone")
  let first_name = read_normalized_optional_string(input, "firstName")
  let last_name = read_normalized_optional_string(input, "lastName")
  let tags = normalize_tags(read_obj_array_strings(input, "tags"))
  let tax_exempt = read_obj_bool(input, "taxExempt") |> option.unwrap(False)
  let tax_exemptions = read_obj_array_strings(input, "taxExemptions")
  let display = build_display_name(first_name, last_name, email)
  let addresses_input = read_obj_addresses(input)
  let addresses =
    list.index_map(addresses_input, fn(address_input, index) {
      build_address(
        "gid://shopify/MailingAddress/" <> int.to_string(index + 1),
        id,
        index,
        address_input,
        first_name,
        last_name,
      )
    })
  let default_address = case addresses {
    [first, ..] -> Some(address_to_default(first))
    [] -> None
  }
  let metafields = read_customer_metafields(input, id, identity)
  #(
    CustomerRecord(
      id: id,
      first_name: first_name,
      last_name: last_name,
      display_name: display,
      email: email,
      legacy_resource_id: gid_tail(id),
      locale: read_obj_string(input, "locale") |> option.or(Some("en")),
      note: read_normalized_string_with_blank(input, "note"),
      can_delete: Some(True),
      verified_email: Some(True),
      data_sale_opt_out: False,
      tax_exempt: Some(tax_exempt),
      tax_exemptions: tax_exemptions,
      state: Some("DISABLED"),
      tags: tags,
      number_of_orders: Some("0"),
      amount_spent: Some(Money(amount: "0.0", currency_code: "USD")),
      default_email_address: case email {
        Some(e) ->
          Some(CustomerDefaultEmailAddressRecord(
            email_address: Some(e),
            marketing_state: consent_state_from_input(
              input,
              "emailMarketingConsent",
            ),
            marketing_opt_in_level: consent_level_from_input(
              input,
              "emailMarketingConsent",
            ),
            marketing_updated_at: consent_updated_at_from_input(
              input,
              "emailMarketingConsent",
            ),
          ))
        None -> None
      },
      default_phone_number: case phone {
        Some(p) ->
          Some(
            CustomerDefaultPhoneNumberRecord(
              phone_number: Some(p),
              marketing_state: consent_state_from_input(
                input,
                "smsMarketingConsent",
              ),
              marketing_opt_in_level: consent_level_from_input(
                input,
                "smsMarketingConsent",
              ),
              marketing_updated_at: consent_updated_at_from_input(
                input,
                "smsMarketingConsent",
              ),
              marketing_collected_from: case
                has_nested_object(input, "smsMarketingConsent")
              {
                True -> Some("OTHER")
                False ->
                  consent_collected_from_input(input, "smsMarketingConsent")
              },
            ),
          )
        None -> None
      },
      email_marketing_consent: make_email_consent(input),
      sms_marketing_consent: make_sms_consent(input),
      default_address: default_address,
      account_activation_token: None,
      created_at: Some(timestamp),
      updated_at: Some(timestamp),
    ),
    addresses,
    metafields,
  )
}

@internal
pub fn validate_customer_create(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> List(UserError) {
  let email = read_obj_string(input, "email")
  let phone = read_obj_string(input, "phone")
  let id_errors = case dict.get(input, "id") {
    Ok(root_field.NullVal) | Error(_) -> []
    Ok(_) -> [UserError(["id"], "Cannot specify ID on creation", None)]
  }
  let nested_id_errors = validate_customer_create_nested_resource_ids(input)
  let address_errors = validate_customer_address_inputs(input, [])
  let consent_required_errors =
    customer_create_consent_required_errors(input, email, phone)
  let presence_errors = case email, phone {
    None, None ->
      case consent_required_errors {
        [] -> [
          UserError(
            field: [],
            message: "A name, phone number, or email address must be present",
            code: None,
          ),
        ]
        _ -> []
      }
    _, _ -> []
  }
  let local_errors = validate_customer_input_fields(store, input, None)
  list.append(
    list.append(
      list.append(list.append(id_errors, presence_errors), nested_id_errors),
      consent_required_errors,
    ),
    list.append(
      list.append(local_errors, address_errors),
      validate_upstream_duplicate_customer(input, local_errors, None, upstream),
    ),
  )
}

@internal
pub fn validate_customer_create_nested_resource_ids(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case customer_create_nested_id_errors(input, "addresses") {
    [] -> customer_create_nested_id_errors(input, "metafields")
    address_errors -> address_errors
  }
}

@internal
pub fn customer_create_nested_id_errors(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(UserError) {
  read_obj_list_objects(input, key)
  |> list.index_map(fn(item, index) {
    case dict.get(item, "id") {
      Ok(root_field.NullVal) | Error(_) -> []
      Ok(_) -> [
        UserError(
          field: [key, int.to_string(index), "id"],
          message: customer_create_nested_id_message(key),
          code: Some("INVALID"),
        ),
      ]
    }
  })
  |> list.flatten()
}

@internal
pub fn customer_create_nested_id_message(key: String) -> String {
  case key {
    "addresses" -> "Cannot specify address ID on creation"
    "metafields" -> "Cannot specify metafield ID on creation"
    _ -> "Cannot specify ID on creation"
  }
}

@internal
pub fn customer_create_consent_required_errors(
  input: Dict(String, root_field.ResolvedValue),
  email: Option(String),
  phone: Option(String),
) -> List(UserError) {
  let email_errors = case
    has_nested_object(input, "emailMarketingConsent"),
    email
  {
    True, None -> [
      UserError(
        field: ["emailMarketingConsent"],
        message: "An email address is required to set the email marketing consent state.",
        code: None,
      ),
    ]
    _, _ -> []
  }
  let sms_errors = case has_nested_object(input, "smsMarketingConsent"), phone {
    True, None -> [
      UserError(
        field: ["smsMarketingConsent"],
        message: "A phone number is required to set the SMS consent state.",
        code: None,
      ),
    ]
    _, _ -> []
  }
  list.append(email_errors, sms_errors)
}

@internal
pub fn validate_upstream_duplicate_customer(
  input: Dict(String, root_field.ResolvedValue),
  local_errors: List(UserError),
  exclude_customer_id: Option(String),
  upstream: UpstreamContext,
) -> List(UserError) {
  let has_email_error =
    local_errors |> list.any(fn(error) { error.field == ["email"] })
  let has_phone_error =
    local_errors |> list.any(fn(error) { error.field == ["phone"] })
  let email_error = case read_obj_string(input, "email"), has_email_error {
    Some(email), False ->
      case
        string.contains(email, "@")
        && upstream_customer_duplicate_exists(
          "email:" <> email,
          exclude_customer_id,
          upstream,
        )
      {
        True -> [UserError(["email"], "Email has already been taken", None)]
        False -> []
      }
    _, _ -> []
  }
  let phone_error = case read_obj_string(input, "phone"), has_phone_error {
    Some(phone), False ->
      case
        valid_phone(phone)
        && upstream_customer_duplicate_exists(
          "phone:" <> phone,
          exclude_customer_id,
          upstream,
        )
      {
        True -> [UserError(["phone"], "Phone has already been taken", None)]
        False -> []
      }
    _, _ -> []
  }
  list.append(email_error, phone_error)
}

@internal
pub fn upstream_customer_duplicate_exists(
  query_value: String,
  exclude_customer_id: Option(String),
  upstream: UpstreamContext,
) -> Bool {
  let query =
    "query CustomerDuplicateHydrate($query: String!) {
  customers(first: 1, query: $query) { nodes { id } }
}
"
  let variables = json.object([#("query", json.string(query_value))])
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "CustomerDuplicateHydrate",
      query,
      variables,
    )
  {
    Ok(value) ->
      case upstream_customer_id_result(value) {
        Some(id) -> id != option.unwrap(exclude_customer_id, "")
        None -> False
      }
    Error(_) -> False
  }
}

@internal
pub fn upstream_customer_id_result(value: commit.JsonValue) -> Option(String) {
  case json_get(value, "data") {
    Some(data) ->
      case connection_nodes(data, "customers") {
        [first, ..] -> json_get_string(first, "id")
        [] -> None
      }
    None -> None
  }
}

@internal
pub fn validate_customer_input_fields(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_customer_id: Option(String),
) -> List(UserError) {
  let scalar_errors =
    [
      validate_email(store, input, exclude_customer_id),
      validate_phone(store, input, exclude_customer_id),
      validate_locale(input),
    ]
    |> list.filter_map(fn(item) { item })
  let length_errors =
    list.flatten([
      validate_max_length(input, "firstName", "First name", 255),
      validate_max_length(input, "lastName", "Last name", 255),
      validate_max_length_with_code(
        input,
        "note",
        "Note",
        5000,
        Some(user_error_codes.too_long),
      ),
      validate_tag_lengths(input),
      validate_tag_count(input),
    ])
  list.append(scalar_errors, length_errors)
}

@internal
pub fn validate_customer_address_inputs(
  input: Dict(String, root_field.ResolvedValue),
  field_prefix: List(String),
) -> List(UserError) {
  read_obj_addresses(input)
  |> list.index_map(fn(address_input, index) {
    validate_address_input(
      address_input,
      None,
      list.append(field_prefix, ["addresses", int.to_string(index)]),
    )
  })
  |> list.flatten()
}

@internal
pub fn validate_email(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_customer_id: Option(String),
) -> Result(UserError, Nil) {
  use email <- result.try(read_obj_string(input, "email") |> option_to_result)
  case string.contains(email, "@") {
    False -> Ok(UserError(["email"], "Email is invalid", None))
    True ->
      case customer_email_exists(store, email, exclude_customer_id) {
        True -> Ok(UserError(["email"], "Email has already been taken", None))
        False -> Error(Nil)
      }
  }
}

@internal
pub fn validate_phone(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_customer_id: Option(String),
) -> Result(UserError, Nil) {
  use phone <- result.try(read_obj_string(input, "phone") |> option_to_result)
  case valid_phone(phone) {
    False -> Ok(UserError(["phone"], "Phone is invalid", None))
    True ->
      case customer_phone_exists(store, phone, exclude_customer_id) {
        True -> Ok(UserError(["phone"], "Phone has already been taken", None))
        False -> Error(Nil)
      }
  }
}

@internal
pub fn validate_locale(
  input: Dict(String, root_field.ResolvedValue),
) -> Result(UserError, Nil) {
  use locale <- result.try(read_obj_string(input, "locale") |> option_to_result)
  case valid_locale(locale) {
    True -> Error(Nil)
    False -> Ok(UserError(["locale"], "Locale is invalid", None))
  }
}

@internal
pub fn validate_max_length(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  label: String,
  max: Int,
) -> List(UserError) {
  validate_max_length_at(input, field, label, max, [field], None)
}

@internal
pub fn validate_max_length_with_code(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  label: String,
  max: Int,
  code: Option(String),
) -> List(UserError) {
  validate_max_length_at(input, field, label, max, [field], code)
}

@internal
pub fn validate_max_length_at(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  label: String,
  max: Int,
  error_field: List(String),
  code: Option(String),
) -> List(UserError) {
  case read_obj_string(input, field) {
    Some(value) ->
      case string.length(value) > max {
        True -> [
          UserError(
            error_field,
            label
              <> " is too long (maximum is "
              <> int.to_string(max)
              <> " characters)",
            code,
          ),
        ]
        False -> []
      }
    _ -> []
  }
}

@internal
pub fn validate_tag_lengths(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  validate_tag_lengths_at(input, ["tags"])
}

@internal
pub fn validate_tag_lengths_at(
  input: Dict(String, root_field.ResolvedValue),
  field: List(String),
) -> List(UserError) {
  read_obj_array_strings(input, "tags")
  |> list.flat_map(split_tags)
  |> list.filter(fn(tag) { string.length(tag) > 255 })
  |> list.map(fn(_) {
    UserError(field, "Tags is too long (maximum is 255 characters)", None)
  })
}

@internal
pub fn validate_tag_count(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  validate_tag_count_at(input, ["tags"])
}

@internal
pub fn validate_tag_count_at(
  input: Dict(String, root_field.ResolvedValue),
  field: List(String),
) -> List(UserError) {
  let count =
    normalize_tags(read_obj_array_strings(input, "tags")) |> list.length
  case count > 250 {
    True -> [
      UserError(
        field,
        "Tags cannot be more than 250",
        Some(user_error_codes.too_many_tags),
      ),
    ]
    False -> []
  }
}

@internal
pub fn customer_email_exists(
  store: Store,
  email: String,
  exclude_customer_id: Option(String),
) -> Bool {
  store.list_effective_customers(store)
  |> list.any(fn(customer) {
    customer.id != option.unwrap(exclude_customer_id, "")
    && option.unwrap(customer.email, "") |> string.lowercase
    == string.lowercase(email)
  })
}

@internal
pub fn customer_phone_exists(
  store: Store,
  phone: String,
  exclude_customer_id: Option(String),
) -> Bool {
  store.list_effective_customers(store)
  |> list.any(fn(customer) {
    customer.id != option.unwrap(exclude_customer_id, "")
    && customer.default_phone_number
    |> option.then(fn(value) { value.phone_number })
    |> option.unwrap("")
    == phone
  })
}

@internal
pub fn valid_phone(phone: String) -> Bool {
  string.starts_with(phone, "+")
  && string.length(phone) > 1
  && all_digits(string.drop_start(phone, 1))
}

@internal
pub fn all_digits(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) -> is_digit_string(grapheme) && all_digits(rest)
  }
}

@internal
pub fn is_digit_string(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

@internal
pub fn valid_locale(locale: String) -> Bool {
  case string.length(locale) {
    2 -> True
    5 -> string.contains(locale, "-")
    _ -> False
  }
}

@internal
pub fn handle_customer_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let id = read_obj_string(input, "id")
  case id {
    Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(existing) -> {
          let input_errors =
            validate_customer_input_fields(store, input, Some(customer_id))
          let validation_errors =
            list.flatten([
              inline_consent_update_errors(input),
              input_errors,
              validate_customer_address_inputs(input, []),
              validate_upstream_duplicate_customer(
                input,
                input_errors,
                Some(customer_id),
                upstream,
              ),
            ])
          case validation_errors {
            [_, ..] as errors -> {
              let payload =
                customer_payload_json(
                  store,
                  "CustomerUpdatePayload",
                  None,
                  None,
                  None,
                  errors,
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerUpdate",
                ),
                store,
                identity,
              )
            }
            [] -> {
              let updated =
                update_customer_from_input(
                  existing,
                  input,
                  option.unwrap(existing.updated_at, ""),
                )
              case customer_identity_presence_errors(updated) {
                [_, ..] as errors -> {
                  let payload =
                    customer_payload_json(
                      store,
                      "CustomerUpdatePayload",
                      None,
                      None,
                      None,
                      errors,
                      field,
                      fragments,
                    )
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      payload,
                      [],
                      "customerUpdate",
                    ),
                    store,
                    identity,
                  )
                }
                [] -> {
                  let #(timestamp, after_ts) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  let updated =
                    update_customer_from_input(existing, input, timestamp)
                  let #(stored, store_after_customer) =
                    store.stage_update_customer(store, updated)
                  let store_after_metafields = case
                    read_customer_metafields(input, stored.id, after_ts)
                  {
                    [] -> store_after_customer
                    metafields ->
                      store.stage_customer_metafields(
                        store_after_customer,
                        stored.id,
                        metafields,
                      )
                  }
                  let #(payload_customer, store_after_addresses) =
                    replace_customer_input_addresses(
                      store_after_metafields,
                      stored,
                      input,
                    )
                  let payload =
                    customer_payload_json(
                      store_after_addresses,
                      "CustomerUpdatePayload",
                      Some(payload_customer),
                      None,
                      None,
                      [],
                      field,
                      fragments,
                    )
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      payload,
                      [payload_customer.id],
                      "customerUpdate",
                    ),
                    store_after_addresses,
                    after_ts,
                  )
                }
              }
            }
          }
        }
        None ->
          unknown_customer_result(
            store,
            identity,
            field,
            fragments,
            "CustomerUpdatePayload",
            "customerUpdate",
          )
      }
    None ->
      unknown_customer_result(
        store,
        identity,
        field,
        fragments,
        "CustomerUpdatePayload",
        "customerUpdate",
      )
  }
}

@internal
pub fn customer_identity_presence_errors(
  customer: CustomerRecord,
) -> List(UserError) {
  case customer_has_contact_identity(customer) {
    True -> []
    False -> [
      UserError(
        [],
        "A name, phone number, or email address must be present",
        Some("INVALID"),
      ),
    ]
  }
}

@internal
pub fn customer_has_contact_identity(customer: CustomerRecord) -> Bool {
  option_has_text(customer.first_name)
  || option_has_text(customer.last_name)
  || option_has_text(customer.email)
  || option_has_text(
    customer.default_phone_number
    |> option.then(fn(value) { value.phone_number }),
  )
}

@internal
pub fn option_has_text(value: Option(String)) -> Bool {
  case value {
    Some(text) -> string.trim(text) != ""
    None -> False
  }
}

@internal
pub fn inline_consent_update_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  let email_errors = case has_nested_object(input, "emailMarketingConsent") {
    True -> [
      UserError(
        ["emailMarketingConsent"],
        "To update emailMarketingConsent, please use the customerEmailMarketingConsentUpdate Mutation instead",
        None,
      ),
    ]
    False -> []
  }
  let sms_errors = case has_nested_object(input, "smsMarketingConsent") {
    True -> [
      UserError(
        ["smsMarketingConsent"],
        "To update smsMarketingConsent, please use the customerSmsMarketingConsentUpdate Mutation instead",
        None,
      ),
    ]
    False -> []
  }
  list.append(email_errors, sms_errors)
}

@internal
pub fn handle_customer_set(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let identifier =
    graphql_helpers.read_arg_object(args, "identifier")
    |> option.unwrap(dict.new())
  case read_obj_string(identifier, "id") {
    Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(existing) ->
          case
            customer_set_preflight_errors(
              store,
              input,
              identifier,
              Some(existing),
            )
          {
            [_, ..] as errors ->
              customer_set_error_result(
                store,
                identity,
                field,
                fragments,
                errors,
              )
            [] ->
              update_from_set(
                store,
                identity,
                field,
                fragments,
                input,
                existing,
              )
          }
        None ->
          customer_set_unknown_id_result(store, identity, field, fragments)
      }
    None -> {
      let existing = find_customer_by_customer_set_identifier(store, identifier)
      case customer_set_preflight_errors(store, input, identifier, existing) {
        [_, ..] as errors ->
          customer_set_error_result(store, identity, field, fragments, errors)
        [] ->
          case existing {
            Some(existing) ->
              update_from_set(
                store,
                identity,
                field,
                fragments,
                input,
                existing,
              )
            None -> create_from_set(store, identity, field, fragments, input)
          }
      }
    }
  }
}

@internal
pub fn customer_set_error_result(store, identity, field, fragments, errors) {
  let payload =
    customer_payload_json(
      store,
      "CustomerSetPayload",
      None,
      None,
      None,
      errors,
      field,
      fragments,
    )
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      [],
      "customerSet",
    ),
    store,
    identity,
  )
}

@internal
pub fn customer_set_preflight_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  identifier: Dict(String, root_field.ResolvedValue),
  existing: Option(CustomerRecord),
) -> List(UserError) {
  list.flatten([
    validate_customer_address_inputs(input, ["input"]),
    customer_set_tag_note_validation_errors(input),
    customer_set_tax_exempt_null_errors(input),
    customer_set_identifier_alignment_errors(input, identifier),
    customer_set_create_identity_errors(input, existing),
    customer_set_duplicate_identity_errors(store, input, existing),
  ])
}

@internal
pub fn customer_set_tag_note_validation_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  list.flatten([
    validate_max_length_at(
      input,
      "note",
      "Note",
      5000,
      ["input", "note"],
      Some(user_error_codes.too_long),
    ),
    validate_tag_lengths_at(input, ["input", "tags"]),
    validate_tag_count_at(input, ["input", "tags"]),
  ])
}

@internal
pub fn customer_set_tax_exempt_null_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case dict.get(input, "taxExempt") {
    Ok(root_field.NullVal) -> [
      UserError(
        ["input", "taxExempt"],
        "Tax exempt is of unexpected type NilClass",
        None,
      ),
    ]
    _ -> []
  }
}

@internal
pub fn customer_set_identifier_alignment_errors(
  input: Dict(String, root_field.ResolvedValue),
  identifier: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  list.append(
    customer_set_identifier_value_errors(input, identifier, "email"),
    customer_set_identifier_value_errors(input, identifier, "phone"),
  )
}

@internal
pub fn customer_set_identifier_value_errors(
  input: Dict(String, root_field.ResolvedValue),
  identifier: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(UserError) {
  case read_obj_string(identifier, key) {
    Some(identifier_value) ->
      case read_obj_string(input, key) {
        None -> [
          UserError(
            ["input"],
            "The input field corresponding to the identifier is required.",
            None,
          ),
        ]
        Some(input_value) if input_value != identifier_value -> [
          UserError(
            ["input"],
            "The identifier value does not match the value of the corresponding field in the input.",
            None,
          ),
        ]
        Some(_) -> []
      }
    None -> []
  }
}

@internal
pub fn customer_set_create_identity_errors(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CustomerRecord),
) -> List(UserError) {
  case existing {
    Some(_) -> []
    None ->
      case customer_set_input_has_identity(input) {
        True -> []
        False -> [
          UserError(
            ["input"],
            "A name, phone number, or email address must be present",
            None,
          ),
        ]
      }
  }
}

@internal
pub fn customer_set_input_has_identity(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  option_has_text(read_obj_string(input, "firstName"))
  || option_has_text(read_obj_string(input, "lastName"))
  || option_has_text(read_obj_string(input, "email"))
  || option_has_text(read_obj_string(input, "phone"))
}

@internal
pub fn customer_set_duplicate_identity_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(CustomerRecord),
) -> List(UserError) {
  case existing {
    Some(_) -> []
    None ->
      list.append(
        customer_set_duplicate_email_errors(store, input),
        customer_set_duplicate_phone_errors(store, input),
      )
  }
}

@internal
pub fn customer_set_duplicate_email_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_obj_string(input, "email") {
    Some(email) ->
      case customer_email_exists(store, email, None) {
        True -> [
          UserError(["input", "email"], "Email has already been taken", None),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn customer_set_duplicate_phone_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_obj_string(input, "phone") {
    Some(phone) ->
      case customer_phone_exists(store, phone, None) {
        True -> [
          UserError(["input", "phone"], "Phone has already been taken", None),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn find_customer_by_customer_set_identifier(
  store: Store,
  identifier: Dict(String, root_field.ResolvedValue),
) -> Option(CustomerRecord) {
  let email = read_obj_string(identifier, "email")
  let phone = read_obj_string(identifier, "phone")
  find_customer_by_email_or_phone(
    store.list_effective_customers(store),
    email,
    phone,
  )
}

@internal
pub fn update_from_set(store, identity, field, fragments, input, existing) {
  let #(timestamp, after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let updated = update_customer_from_set_input(existing, input, timestamp)
  let #(stored, store_after_customer) =
    store.stage_update_customer(store, updated)
  let #(payload_customer, next_store) =
    replace_customer_set_input_addresses(store_after_customer, stored, input)
  let payload =
    customer_payload_json(
      next_store,
      "CustomerSetPayload",
      Some(payload_customer),
      None,
      None,
      [],
      field,
      fragments,
    )
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      [payload_customer.id],
      "customerSet",
    ),
    next_store,
    after_ts,
  )
}

@internal
pub fn customer_set_unknown_id_result(store, identity, field, fragments) {
  let payload =
    customer_payload_json(
      store,
      "CustomerSetPayload",
      None,
      None,
      None,
      [
        UserError(
          ["input"],
          "Resource matching the identifier was not found.",
          Some("INVALID"),
        ),
      ],
      field,
      fragments,
    )
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      [],
      "customerSet",
    ),
    store,
    identity,
  )
}

@internal
pub fn create_from_set(store, identity, field, fragments, input) {
  let #(id, after_id) =
    synthetic_identity.make_synthetic_gid(identity, "Customer")
  let #(timestamp, after_ts) =
    synthetic_identity.make_synthetic_timestamp(after_id)
  let #(customer, addresses, metafields) =
    build_created_customer(id, timestamp, input, after_ts)
  let #(stored, store_after_customer) =
    store.stage_create_customer(store, customer)
  let store_after_addresses =
    list.fold(addresses, store_after_customer, fn(acc, address) {
      let #(_, next_store) = store.stage_upsert_customer_address(acc, address)
      next_store
    })
  let store_after_metafields =
    store.stage_customer_metafields(
      store_after_addresses,
      stored.id,
      metafields,
    )
  let payload =
    customer_payload_json(
      store_after_metafields,
      "CustomerSetPayload",
      Some(stored),
      None,
      None,
      [],
      field,
      fragments,
    )
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      [stored.id],
      "customerSet",
    ),
    store_after_metafields,
    after_ts,
  )
}

@internal
pub fn update_customer_from_input(
  existing: CustomerRecord,
  input: Dict(String, root_field.ResolvedValue),
  timestamp: String,
) -> CustomerRecord {
  let first_name =
    update_trimmed_nullable_string(existing.first_name, input, "firstName")
  let last_name =
    update_trimmed_nullable_string(existing.last_name, input, "lastName")
  let email = update_trimmed_nullable_string(existing.email, input, "email")
  let phone =
    update_trimmed_nullable_string(
      existing.default_phone_number |> option.then(fn(v) { v.phone_number }),
      input,
      "phone",
    )
  CustomerRecord(
    ..existing,
    first_name: first_name,
    last_name: last_name,
    display_name: build_display_name(first_name, last_name, email),
    email: email,
    locale: update_trimmed_nullable_string(existing.locale, input, "locale"),
    note: update_nullable_note(existing.note, input),
    verified_email: read_obj_bool(input, "verifiedEmail")
      |> option.or(existing.verified_email),
    tax_exempt: read_obj_bool(input, "taxExempt")
      |> option.or(existing.tax_exempt),
    tax_exemptions: case read_obj_array_strings(input, "taxExemptions") {
      [] -> existing.tax_exemptions
      values -> values
    },
    tags: case read_obj_array_strings(input, "tags") {
      [] -> existing.tags
      values -> normalize_tags(values)
    },
    default_email_address: case email {
      Some(e) ->
        Some(CustomerDefaultEmailAddressRecord(
          email_address: Some(e),
          marketing_state: existing.default_email_address
            |> option.then(fn(v) { v.marketing_state })
            |> option.or(
              existing.email_marketing_consent
              |> option.then(fn(v) { v.marketing_state }),
            ),
          marketing_opt_in_level: existing.default_email_address
            |> option.then(fn(v) { v.marketing_opt_in_level })
            |> option.or(
              existing.email_marketing_consent
              |> option.then(fn(v) { v.marketing_opt_in_level }),
            ),
          marketing_updated_at: existing.default_email_address
            |> option.then(fn(v) { v.marketing_updated_at })
            |> option.or(
              existing.email_marketing_consent
              |> option.then(fn(v) { v.consent_updated_at }),
            ),
        ))
      None -> None
    },
    default_phone_number: case phone {
      Some(p) ->
        Some(CustomerDefaultPhoneNumberRecord(
          phone_number: Some(p),
          marketing_state: existing.default_phone_number
            |> option.then(fn(v) { v.marketing_state })
            |> option.or(
              existing.sms_marketing_consent
              |> option.then(fn(v) { v.marketing_state }),
            ),
          marketing_opt_in_level: existing.default_phone_number
            |> option.then(fn(v) { v.marketing_opt_in_level })
            |> option.or(
              existing.sms_marketing_consent
              |> option.then(fn(v) { v.marketing_opt_in_level }),
            ),
          marketing_updated_at: existing.default_phone_number
            |> option.then(fn(v) { v.marketing_updated_at })
            |> option.or(
              existing.sms_marketing_consent
              |> option.then(fn(v) { v.consent_updated_at }),
            ),
          marketing_collected_from: existing.default_phone_number
            |> option.then(fn(v) { v.marketing_collected_from })
            |> option.or(
              existing.sms_marketing_consent
              |> option.then(fn(v) { v.consent_collected_from }),
            ),
        ))
      None -> None
    },
    email_marketing_consent: case email {
      Some(_) -> existing.email_marketing_consent
      None -> None
    },
    sms_marketing_consent: case phone {
      Some(_) -> existing.sms_marketing_consent
      None -> None
    },
    updated_at: Some(timestamp),
  )
}

@internal
pub fn update_customer_from_set_input(
  existing: CustomerRecord,
  input: Dict(String, root_field.ResolvedValue),
  timestamp: String,
) -> CustomerRecord {
  let updated = update_customer_from_input(existing, input, timestamp)
  CustomerRecord(
    ..updated,
    tax_exemptions: update_customer_set_string_list(
      existing.tax_exemptions,
      input,
      "taxExemptions",
    ),
    tags: case read_present_string_list(input, "tags") {
      Some(values) -> normalize_tags(values)
      None -> updated.tags
    },
  )
}

@internal
pub fn update_customer_set_string_list(
  existing: List(String),
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(String) {
  case read_present_string_list(input, key) {
    Some(values) -> values
    None -> existing
  }
}

@internal
pub fn read_present_string_list(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(List(String)) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
      |> Some
    _ -> None
  }
}

@internal
pub fn unknown_customer_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  typename: String,
  root_name: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    customer_payload_json(
      store,
      typename,
      None,
      None,
      None,
      [
        UserError(
          field: ["id"],
          message: "Customer does not exist",
          code: Some("CUSTOMER_DOES_NOT_EXIST"),
        ),
      ],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root_name),
    store,
    identity,
  )
}

@internal
pub fn customer_missing_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  typename: String,
  root_name: String,
  error_field: List(String),
  message: String,
  code: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    customer_payload_json(
      store,
      typename,
      None,
      None,
      None,
      [UserError(error_field, message, code)],
      field,
      fragments,
    )
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], root_name),
    store,
    identity,
  )
}

@internal
pub fn handle_customer_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let id = read_obj_string(input, "id")
  case id {
    Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(_) -> {
          case customer_has_associated_orders(store, customer_id) {
            True -> {
              let payload =
                customer_payload_json(
                  store,
                  "CustomerDeletePayload",
                  None,
                  None,
                  None,
                  [
                    UserError(
                      ["id"],
                      "Customer can’t be deleted because they have associated orders",
                      None,
                    ),
                  ],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerDelete",
                ),
                store,
                identity,
              )
            }
            False -> {
              let next_store = store.stage_delete_customer(store, customer_id)
              let payload =
                customer_payload_json(
                  next_store,
                  "CustomerDeletePayload",
                  None,
                  Some(customer_id),
                  None,
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [customer_id],
                  "customerDelete",
                ),
                next_store,
                identity,
              )
            }
          }
        }
        None -> {
          let payload =
            customer_payload_json(
              store,
              "CustomerDeletePayload",
              None,
              None,
              None,
              [
                UserError(["id"], "Customer can't be found", None),
              ],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              "customerDelete",
            ),
            store,
            identity,
          )
        }
      }
    None -> {
      let payload =
        customer_payload_json(
          store,
          "CustomerDeletePayload",
          None,
          None,
          None,
          [
            UserError(["id"], "Customer can't be found", None),
          ],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "customerDelete",
        ),
        store,
        identity,
      )
    }
  }
}

@internal
pub fn customer_has_associated_orders(
  store: Store,
  customer_id: String,
) -> Bool {
  case store.list_effective_customer_order_summaries(store, customer_id) {
    [_, ..] -> True
    [] ->
      store.list_effective_orders(store)
      |> list.any(fn(order) { order_customer_id(order) == Some(customer_id) })
  }
}

@internal
pub fn order_customer_id(order: OrderRecord) -> Option(String) {
  captured_object_field(order.data, "customer")
  |> option.then(fn(customer) { captured_string_field(customer, "id") })
}

@internal
pub fn captured_object_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      fields
      |> list.find_map(fn(pair) {
        let #(key, item) = pair
        case key == name {
          True -> Ok(item)
          False -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

@internal
pub fn captured_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn handle_customer_address_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let address_input =
    graphql_helpers.read_arg_object(args, "address")
    |> option.unwrap(dict.new())
  let set_default =
    graphql_helpers.read_arg_bool(args, "setAsDefault") |> option.unwrap(False)
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          case validate_address_input(address_input, None, ["address"]) {
            [_, ..] as errors -> {
              let payload =
                address_payload_json(
                  store,
                  "CustomerAddressCreatePayload",
                  None,
                  None,
                  errors,
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerAddressCreate",
                ),
                store,
                identity,
              )
            }
            [] -> {
              let #(address_id, after_id) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "MailingAddress",
                )
              let existing_count =
                store.list_effective_customer_addresses(store, id)
                |> list.length()
              let address =
                build_address(
                  address_id,
                  id,
                  existing_count,
                  address_input,
                  customer.first_name,
                  customer.last_name,
                )
              case find_duplicate_customer_address(store, id, address, None) {
                Some(_) -> {
                  let payload =
                    address_payload_json(
                      store,
                      "CustomerAddressCreatePayload",
                      None,
                      None,
                      [
                        UserError(["address"], "Address already exists", None),
                      ],
                      field,
                      fragments,
                    )
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      payload,
                      [],
                      "customerAddressCreate",
                    ),
                    store,
                    identity,
                  )
                }
                None -> {
                  let #(_, store_after_address) =
                    store.stage_upsert_customer_address(store, address)
                  let next_store = case
                    set_default || customer.default_address == None
                  {
                    True -> {
                      let updated =
                        CustomerRecord(
                          ..customer,
                          default_address: Some(address_to_default(address)),
                        )
                      let #(_, s) =
                        store.stage_update_customer(
                          store_after_address,
                          updated,
                        )
                      s
                    }
                    False -> store_after_address
                  }
                  let payload =
                    address_payload_json(
                      next_store,
                      "CustomerAddressCreatePayload",
                      Some(address),
                      None,
                      [],
                      field,
                      fragments,
                    )
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      payload,
                      [address.id],
                      "customerAddressCreate",
                    ),
                    next_store,
                    after_id,
                  )
                }
              }
            }
          }
        }
        None ->
          address_unknown_result(
            store,
            identity,
            field,
            fragments,
            "CustomerAddressCreatePayload",
            "customerAddressCreate",
          )
      }
    None ->
      address_unknown_result(
        store,
        identity,
        field,
        fragments,
        "CustomerAddressCreatePayload",
        "customerAddressCreate",
      )
  }
}

@internal
pub fn handle_customer_address_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let address_id = graphql_helpers.read_arg_string_nonempty(args, "addressId")
  let address_input =
    graphql_helpers.read_arg_object(args, "address")
    |> option.unwrap(dict.new())
  let set_default =
    graphql_helpers.read_arg_bool(args, "setAsDefault") |> option.unwrap(False)
  case customer_id, address_id {
    Some(cid), Some(aid) ->
      case address_input_id_mismatches(address_input, aid) {
        True ->
          address_id_mismatch_result(
            store,
            identity,
            field,
            fragments,
            "CustomerAddressUpdatePayload",
            "customerAddressUpdate",
          )
        False ->
          case store.get_effective_customer_by_id(store, cid) {
            Some(customer) ->
              case store.get_effective_customer_address_by_id(store, aid) {
                Some(existing) ->
                  case existing.customer_id == customer.id {
                    True -> {
                      case
                        validate_address_input(address_input, Some(existing), [
                          "address",
                        ])
                      {
                        [_, ..] as errors -> {
                          let payload =
                            address_payload_json(
                              store,
                              "CustomerAddressUpdatePayload",
                              None,
                              None,
                              errors,
                              field,
                              fragments,
                            )
                          #(
                            MutationFieldResult(
                              get_field_response_key(field),
                              payload,
                              [],
                              "customerAddressUpdate",
                            ),
                            store,
                            identity,
                          )
                        }
                        [] -> {
                          let updated = merge_address(existing, address_input)
                          let #(_, store_after_address) =
                            store.stage_upsert_customer_address(store, updated)
                          let next_store = case set_default {
                            True -> {
                              let #(_, s) =
                                store.stage_update_customer(
                                  store_after_address,
                                  CustomerRecord(
                                    ..customer,
                                    default_address: Some(address_to_default(
                                      updated,
                                    )),
                                  ),
                                )
                              s
                            }
                            False -> store_after_address
                          }
                          let payload =
                            address_payload_json(
                              next_store,
                              "CustomerAddressUpdatePayload",
                              Some(updated),
                              None,
                              [],
                              field,
                              fragments,
                            )
                          #(
                            MutationFieldResult(
                              get_field_response_key(field),
                              payload,
                              [updated.id],
                              "customerAddressUpdate",
                            ),
                            next_store,
                            identity,
                          )
                        }
                      }
                    }
                    False ->
                      address_ownership_result(
                        store,
                        identity,
                        field,
                        fragments,
                        "CustomerAddressUpdatePayload",
                        "customerAddressUpdate",
                      )
                  }
                None ->
                  address_unknown_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "CustomerAddressUpdatePayload",
                    "customerAddressUpdate",
                  )
              }
            None ->
              address_customer_missing_result(
                store,
                identity,
                field,
                fragments,
                "CustomerAddressUpdatePayload",
                "customerAddressUpdate",
              )
          }
      }
    _, _ ->
      address_unknown_result(
        store,
        identity,
        field,
        fragments,
        "CustomerAddressUpdatePayload",
        "customerAddressUpdate",
      )
  }
}

@internal
pub fn address_input_id_mismatches(
  address_input: Dict(String, root_field.ResolvedValue),
  address_id: String,
) -> Bool {
  case dict.get(address_input, "id") {
    Error(_) -> False
    Ok(root_field.StringVal(nested_id)) -> nested_id != address_id
    Ok(root_field.IntVal(nested_id)) -> int.to_string(nested_id) != address_id
    Ok(_) -> True
  }
}

@internal
pub fn handle_customer_address_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let address_id = graphql_helpers.read_arg_string_nonempty(args, "addressId")
  case customer_id, address_id {
    Some(cid), Some(aid) ->
      case store.get_effective_customer_by_id(store, cid) {
        Some(customer) ->
          case store.get_effective_customer_address_by_id(store, aid) {
            Some(address) ->
              case address.customer_id == customer.id {
                True -> {
                  let store_after_delete =
                    store.stage_delete_customer_address(store, aid)
                  let next_store = {
                    let current_default =
                      customer.default_address |> option.then(fn(a) { a.id })
                    case current_default == Some(aid) {
                      True -> {
                        let replacement =
                          store.list_effective_customer_addresses(
                            store_after_delete,
                            customer.id,
                          )
                          |> list.first()
                          |> result_to_option()
                        let updated =
                          CustomerRecord(
                            ..customer,
                            default_address: replacement
                              |> option.map(address_to_default),
                          )
                        let #(_, s) =
                          store.stage_update_customer(
                            store_after_delete,
                            updated,
                          )
                        s
                      }
                      False -> store_after_delete
                    }
                  }
                  let payload =
                    address_payload_json(
                      next_store,
                      "CustomerAddressDeletePayload",
                      None,
                      Some(aid),
                      [],
                      field,
                      fragments,
                    )
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      payload,
                      [aid],
                      "customerAddressDelete",
                    ),
                    next_store,
                    identity,
                  )
                }
                False ->
                  address_ownership_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "CustomerAddressDeletePayload",
                    "customerAddressDelete",
                  )
              }
            None ->
              address_unknown_result(
                store,
                identity,
                field,
                fragments,
                "CustomerAddressDeletePayload",
                "customerAddressDelete",
              )
          }
        None ->
          address_customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerAddressDeletePayload",
            "customerAddressDelete",
          )
      }
    _, _ ->
      address_unknown_result(
        store,
        identity,
        field,
        fragments,
        "CustomerAddressDeletePayload",
        "customerAddressDelete",
      )
  }
}

@internal
pub fn handle_customer_update_default_address(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let address_id = graphql_helpers.read_arg_string_nonempty(args, "addressId")
  case customer_id, address_id {
    Some(cid), Some(aid) ->
      case store.get_effective_customer_by_id(store, cid) {
        Some(customer) ->
          case store.get_effective_customer_address_by_id(store, aid) {
            Some(address) ->
              case address.customer_id == customer.id {
                True -> {
                  let updated =
                    CustomerRecord(
                      ..customer,
                      default_address: Some(address_to_default(address)),
                    )
                  let #(_, next_store) =
                    store.stage_update_customer(store, updated)
                  let payload =
                    customer_payload_json(
                      next_store,
                      "CustomerUpdateDefaultAddressPayload",
                      Some(updated),
                      None,
                      None,
                      [],
                      field,
                      fragments,
                    )
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      payload,
                      [cid, aid],
                      "customerUpdateDefaultAddress",
                    ),
                    next_store,
                    identity,
                  )
                }
                False ->
                  customer_address_ownership_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "CustomerUpdateDefaultAddressPayload",
                    "customerUpdateDefaultAddress",
                    customer,
                  )
              }
            None ->
              unknown_customer_result(
                store,
                identity,
                field,
                fragments,
                "CustomerUpdateDefaultAddressPayload",
                "customerUpdateDefaultAddress",
              )
          }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerUpdateDefaultAddressPayload",
            "customerUpdateDefaultAddress",
            ["customerId"],
            "Customer does not exist",
            Some("CUSTOMER_DOES_NOT_EXIST"),
          )
      }
    _, _ ->
      unknown_customer_result(
        store,
        identity,
        field,
        fragments,
        "CustomerUpdateDefaultAddressPayload",
        "customerUpdateDefaultAddress",
      )
  }
}

@internal
pub fn handle_customer_tax_exemptions(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  mode: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let exemptions = case dict.get(args, "taxExemptions") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
  let root = case mode {
    "add" -> "customerAddTaxExemptions"
    "remove" -> "customerRemoveTaxExemptions"
    _ -> "customerReplaceTaxExemptions"
  }
  let typename = case mode {
    "add" -> "CustomerAddTaxExemptionsPayload"
    "remove" -> "CustomerRemoveTaxExemptionsPayload"
    _ -> "CustomerReplaceTaxExemptionsPayload"
  }
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let next_exemptions = case mode {
            "add" ->
              normalize_tags(list.append(customer.tax_exemptions, exemptions))
            "remove" ->
              list.filter(customer.tax_exemptions, fn(e) {
                !list.contains(exemptions, e)
              })
            _ -> normalize_tags(exemptions)
          }
          let #(updated_at, next_identity) = case
            next_exemptions == customer.tax_exemptions
          {
            True -> #(customer.updated_at, identity)
            False -> {
              let #(ts, after_ts) =
                synthetic_identity.make_synthetic_timestamp(identity)
              #(Some(ts), after_ts)
            }
          }
          let updated =
            CustomerRecord(
              ..customer,
              tax_exemptions: next_exemptions,
              updated_at: updated_at,
            )
          let #(_, next_store) = store.stage_update_customer(store, updated)
          let payload =
            customer_payload_json(
              next_store,
              typename,
              Some(updated),
              None,
              None,
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [id],
              root,
            ),
            next_store,
            next_identity,
          )
        }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            typename,
            root,
            ["customerId"],
            "Customer does not exist.",
            None,
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        typename,
        root,
        ["customerId"],
        "Customer does not exist.",
        None,
      )
  }
}

@internal
pub fn replace_customer_input_addresses(
  store: Store,
  customer: CustomerRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(CustomerRecord, Store) {
  case dict.get(input, "addresses") {
    Ok(root_field.ListVal([_, ..])) ->
      replace_customer_addresses(store, customer, read_obj_addresses(input))
    _ -> #(customer, store)
  }
}

@internal
pub fn replace_customer_set_input_addresses(
  store: Store,
  customer: CustomerRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(CustomerRecord, Store) {
  case dict.get(input, "addresses") {
    Ok(root_field.ListVal(_)) ->
      replace_customer_addresses(store, customer, read_obj_addresses(input))
    _ -> #(customer, store)
  }
}

@internal
pub fn replace_customer_addresses(
  store: Store,
  customer: CustomerRecord,
  address_inputs: List(Dict(String, root_field.ResolvedValue)),
) -> #(CustomerRecord, Store) {
  let store_after_deletes =
    store.list_effective_customer_addresses(store, customer.id)
    |> list.fold(store, fn(acc, address) {
      store.stage_delete_customer_address(acc, address.id)
    })
  let addresses =
    address_inputs
    |> list.index_map(fn(address_input, index) {
      build_address(
        "gid://shopify/MailingAddress/" <> int.to_string(index + 1),
        customer.id,
        index,
        address_input,
        customer.first_name,
        customer.last_name,
      )
    })
    |> dedupe_customer_addresses([])
  let store_after_addresses =
    list.fold(addresses, store_after_deletes, fn(acc, address) {
      let #(_, next_store) = store.stage_upsert_customer_address(acc, address)
      next_store
    })
  let default_address =
    addresses
    |> list.first()
    |> result_to_option()
    |> option.map(address_to_default)
  let customer_after_addresses =
    CustomerRecord(..customer, default_address: default_address)
  let #(stored, final_store) =
    store.stage_update_customer(store_after_addresses, customer_after_addresses)
  #(stored, final_store)
}
