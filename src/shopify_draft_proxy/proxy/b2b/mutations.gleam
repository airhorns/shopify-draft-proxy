//// B2B local mutation dispatch and lifecycle staging.

import gleam/dict.{type Dict}

import gleam/float
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/b2b/assignments.{
  handle_address_delete, handle_assign_address, handle_assign_staff,
  handle_remove_staff, handle_tax_settings_update,
}

import shopify_draft_proxy/proxy/b2b/role_assignments.{
  assignment_ref, build_role_assignment, handle_contact_assign_role,
  handle_contact_assign_roles, handle_contact_revoke_role,
  handle_contact_revoke_roles, handle_location_assign_roles,
  handle_location_revoke_roles, stage_role_assignments,
}
import shopify_draft_proxy/proxy/b2b/serializers.{
  address_from_input, address_id, append_unique, bulk_action_limit_reached,
  bulk_action_limit_reached_error, company_contact_cap_error,
  company_contact_cap_reached, company_contact_mutation_error, company_contacts,
  company_update_empty_input_error, contact_create_empty_input_error,
  contact_update_empty_input_error, customer_contact_source, customer_email,
  data_get, detailed_user_error, empty_payload, existing_orders_error,
  existing_orders_error_at, field_path, find_company_contact_by_customer_id,
  has_any_non_null_input, indexed_field_path, location_update_empty_input_error,
  make_gid, maybe_put_bool, maybe_put_string, no_input_error,
  optional_src_string, put_source, read_object, read_object_sources, read_string,
  read_string_list, remove_string, resource_not_found,
  serialize_mutation_payload, source_id, source_string, source_to_value,
  timestamp, user_error, validate_company_input, validate_contact_input,
  validate_location_input,
}
import shopify_draft_proxy/proxy/b2b/types as b2b_types
import shopify_draft_proxy/proxy/b2b_user_error_codes as user_error_code
import shopify_draft_proxy/proxy/graphql_helpers.{
  type SourceValue, SrcBool, SrcInt, SrcList, SrcObject, SrcString,
  get_document_fragments, get_field_response_key, src_object,
}

import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, single_root_log_draft,
}

import shopify_draft_proxy/proxy/phone_numbers
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type B2BCompanyContactRecord, type B2BCompanyContactRoleRecord,
  type B2BCompanyLocationRecord, type B2BCompanyRecord, type CapturedJsonValue,
  type StorePropertyValue, B2BCompanyContactRecord, B2BCompanyContactRoleRecord,
  B2BCompanyLocationRecord, B2BCompanyRecord, CapturedObject, CapturedString,
  StorePropertyInt, StorePropertyList, StorePropertyObject, StorePropertyString,
}

@internal
pub fn is_b2b_mutation_root(name: String) -> Bool {
  case name {
    "companiesDelete"
    | "companyAddressDelete"
    | "companyAssignCustomerAsContact"
    | "companyAssignMainContact"
    | "companyContactAssignRole"
    | "companyContactAssignRoles"
    | "companyContactCreate"
    | "companyContactDelete"
    | "companyContactRemoveFromCompany"
    | "companyContactRevokeRole"
    | "companyContactRevokeRoles"
    | "companyContactsDelete"
    | "companyContactUpdate"
    | "companyCreate"
    | "companyDelete"
    | "companyLocationAssignAddress"
    | "companyLocationAssignRoles"
    | "companyLocationAssignStaffMembers"
    | "companyLocationCreate"
    | "companyLocationDelete"
    | "companyLocationRemoveStaffMembers"
    | "companyLocationRevokeRoles"
    | "companyLocationsDelete"
    | "companyLocationTaxSettingsUpdate"
    | "companyLocationUpdate"
    | "companyRevokeMainContact"
    | "companyUpdate" -> True
    // Explicit boundary: local staging cannot emulate outbound email delivery.
    "companyContactSendWelcomeEmail" -> False
    _ -> False
  }
}

/// True iff any string variable names a B2B resource that is already
/// local, deleted locally, or proxy-synthetic. LiveHybrid passthrough
/// is disabled in that case so read-after-write and read-after-delete
@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  _upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let initial = #([], store, identity, [], [])
      let #(entries, final_store, final_identity, staged_ids, drafts) =
        list.fold(fields, initial, fn(acc, field) {
          let #(
            data_entries,
            current_store,
            current_identity,
            all_ids,
            all_drafts,
          ) = acc
          case field {
            Field(name: name, ..) -> {
              let result =
                dispatch_mutation_root(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  variables,
                )
              let payload_json =
                serialize_mutation_payload(
                  result.store,
                  result.payload,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_ids,
                  status_for(result),
                  b2b_types.domain,
                  "stage-locally",
                  Some(
                    "Staged locally in the in-memory B2B company draft store.",
                  ),
                )
              let all_drafts = case should_log_result(result) {
                True -> list.append(all_drafts, [draft])
                False -> all_drafts
              }
              #(
                list.append(data_entries, [
                  #(get_field_response_key(field), payload_json),
                ]),
                result.store,
                result.identity,
                list.append(all_ids, result.staged_ids),
                all_drafts,
              )
            }
            _ -> acc
          }
        })
      MutationOutcome(
        data: json.object([#("data", json.object(entries))]),
        store: final_store,
        identity: final_identity,
        staged_resource_ids: staged_ids,
        log_drafts: drafts,
      )
    }
  }
}

@internal
pub fn status_for(result: b2b_types.RootResult) -> store.EntryStatus {
  case result.staged_ids, result.payload.user_errors {
    [_, ..], _ -> store_types.Staged
    [], [] -> store_types.Staged
    [], _ -> store_types.Failed
  }
}

@internal
pub fn should_log_result(result: b2b_types.RootResult) -> Bool {
  !is_empty_input_result(result)
}

@internal
pub fn is_empty_input_result(result: b2b_types.RootResult) -> Bool {
  case result.staged_ids, result.payload.user_errors {
    [], [error] ->
      error.code == user_error_code.no_input
      || error == company_update_empty_input_error()
    _, _ -> False
  }
}

@internal
pub fn dispatch_mutation_root(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> b2b_types.RootResult {
  let args = graphql_helpers.field_args(field, variables)
  case root {
    "companyCreate" -> handle_company_create(store, identity, args)
    "companyUpdate" -> handle_company_update(store, identity, args)
    "companyDelete" -> handle_company_delete(store, identity, args)
    "companiesDelete" -> handle_companies_delete(store, identity, args)
    "companyContactCreate" -> handle_contact_create(store, identity, args)
    "companyContactUpdate" -> handle_contact_update(store, identity, args)
    "companyContactDelete" -> handle_contact_delete(store, identity, args)
    "companyContactsDelete" -> handle_contacts_delete(store, identity, args)
    "companyAssignCustomerAsContact" ->
      handle_assign_customer_as_contact(store, identity, args)
    "companyContactRemoveFromCompany" ->
      handle_contact_remove_from_company(store, identity, args)
    "companyAssignMainContact" ->
      handle_assign_main_contact(store, identity, args)
    "companyRevokeMainContact" ->
      handle_revoke_main_contact(store, identity, args)
    "companyLocationCreate" -> handle_location_create(store, identity, args)
    "companyLocationUpdate" -> handle_location_update(store, identity, args)
    "companyLocationDelete" -> handle_location_delete(store, identity, args)
    "companyLocationsDelete" -> handle_locations_delete(store, identity, args)
    "companyLocationAssignAddress" ->
      handle_assign_address(store, identity, args)
    "companyAddressDelete" -> handle_address_delete(store, identity, args)
    "companyLocationAssignStaffMembers" ->
      handle_assign_staff(store, identity, args)
    "companyLocationRemoveStaffMembers" ->
      handle_remove_staff(store, identity, args)
    "companyLocationTaxSettingsUpdate" ->
      handle_tax_settings_update(store, identity, args)
    "companyContactAssignRole" ->
      handle_contact_assign_role(store, identity, args)
    "companyContactAssignRoles" ->
      handle_contact_assign_roles(store, identity, args)
    "companyLocationAssignRoles" ->
      handle_location_assign_roles(store, identity, args)
    "companyContactRevokeRole" ->
      handle_contact_revoke_role(store, identity, args)
    "companyContactRevokeRoles" ->
      handle_contact_revoke_roles(store, identity, args)
    "companyLocationRevokeRoles" ->
      handle_location_revoke_roles(store, identity, args)
    _ -> b2b_types.RootResult(empty_payload([]), store, identity, [])
  }
}

@internal
pub fn company_data_from_input(
  input: Dict(String, root_field.ResolvedValue),
  now: String,
  existing: Dict(String, StorePropertyValue),
) -> Dict(String, StorePropertyValue) {
  existing
  |> maybe_put_string(input, "name")
  |> maybe_put_string(input, "note")
  |> maybe_put_string(input, "externalId")
  |> maybe_put_string(input, "customerSince")
  |> dict.insert("updatedAt", StorePropertyString(now))
}

@internal
pub fn contact_data_from_input(
  input: Dict(String, root_field.ResolvedValue),
  now: String,
  existing: Dict(String, StorePropertyValue),
) -> Dict(String, StorePropertyValue) {
  list.fold(
    [
      "firstName",
      "lastName",
      "email",
      "title",
      "locale",
      "phone",
      "note",
      "notes",
    ],
    existing,
    fn(acc, key) { maybe_put_string(acc, input, key) },
  )
  |> dict.insert("updatedAt", StorePropertyString(now))
}

fn contact_customer_data_from_input(
  contact_data: Dict(String, StorePropertyValue),
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, StorePropertyValue) {
  case dict.get(contact_data, "customer") {
    Ok(StorePropertyObject(customer)) -> {
      let customer =
        list.fold(
          ["firstName", "lastName", "email", "phone"],
          customer,
          fn(acc, key) { maybe_put_string(acc, input, key) },
        )
      dict.insert(contact_data, "customer", StorePropertyObject(customer))
    }
    _ -> contact_data
  }
}

@internal
pub fn prepare_contact_create_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> #(Dict(String, root_field.ResolvedValue), List(b2b_types.UserError)) {
  prepare_contact_create_input_with_prefix(store, input, ["input"])
}

@internal
pub fn prepare_contact_create_input_with_prefix(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> #(Dict(String, root_field.ResolvedValue), List(b2b_types.UserError)) {
  prepare_contact_input(
    store,
    ensure_contact_locale(store, input),
    None,
    True,
    prefix,
    "Email is invalid",
  )
}

@internal
pub fn prepare_contact_update_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  contact_id: String,
) -> #(Dict(String, root_field.ResolvedValue), List(b2b_types.UserError)) {
  prepare_contact_input(
    store,
    input,
    Some(contact_id),
    False,
    ["input"],
    "Email address is invalid",
  )
}

@internal
pub fn prepare_contact_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_contact_id: Option(String),
  default_locale: Bool,
  prefix: List(String),
  email_error_message: String,
) -> #(Dict(String, root_field.ResolvedValue), List(b2b_types.UserError)) {
  let input = case default_locale {
    True -> ensure_contact_locale(store, input)
    False -> input
  }
  let input = rename_contact_note_input(input)
  let #(input, phone_errors) = normalize_contact_phone_input(store, input)
  let errors =
    []
    |> list.append(phone_errors)
    |> list.append(validate_contact_email_input(
      input,
      prefix,
      email_error_message,
    ))
    |> list.append(validate_contact_locale_input(input))
    |> list.append(validate_contact_notes_input(input))
    |> list.append(validate_contact_duplicate_email(
      store,
      input,
      exclude_contact_id,
    ))
    |> list.append(validate_contact_duplicate_phone(
      store,
      input,
      exclude_contact_id,
    ))
  #(input, errors)
}

@internal
pub fn validate_contact_email_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
  message: String,
) -> List(b2b_types.UserError) {
  case read_string(input, "email") {
    Some(email) ->
      case valid_email_address(email) {
        True -> []
        False -> [
          user_error(
            Some(field_path(prefix, "email")),
            message,
            user_error_code.invalid,
          ),
        ]
      }
    None -> []
  }
}

@internal
pub fn ensure_contact_locale(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case dict.has_key(input, "locale") {
    True -> input
    False ->
      dict.insert(input, "locale", root_field.StringVal(primary_locale(store)))
  }
}

@internal
pub fn primary_locale(store: Store) -> String {
  store.list_effective_shop_locales(store, None)
  |> list.find(fn(locale) { locale.primary })
  |> result.map(fn(locale) { locale.locale })
  |> result.unwrap("en")
}

@internal
pub fn rename_contact_note_input(
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(input, "note") {
    Ok(value) -> input |> dict.delete("note") |> dict.insert("notes", value)
    Error(_) -> input
  }
}

@internal
pub fn normalize_contact_phone_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> #(Dict(String, root_field.ResolvedValue), List(b2b_types.UserError)) {
  case dict.get(input, "phone") {
    Ok(root_field.StringVal(value)) ->
      case normalize_phone(store, value) {
        Ok(phone) -> #(
          dict.insert(input, "phone", root_field.StringVal(phone)),
          [],
        )
        Error(_) -> #(input, [
          user_error(
            Some(["input", "phone"]),
            "Phone is invalid",
            user_error_code.invalid,
          ),
        ])
      }
    _ -> #(input, [])
  }
}

@internal
pub fn validate_contact_locale_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(b2b_types.UserError) {
  case dict.get(input, "locale") {
    Ok(root_field.StringVal(value)) ->
      case valid_locale_format(value) {
        True -> []
        False -> [
          detailed_user_error(
            Some(["input", "locale"]),
            "Invalid locale format.",
            user_error_code.invalid,
            b2b_types.invalid_locale_format_detail,
          ),
        ]
      }
    _ -> []
  }
}

@internal
pub fn validate_contact_notes_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(b2b_types.UserError) {
  case dict.get(input, "notes") {
    Ok(root_field.StringVal(value)) ->
      case contains_html_tag(value) {
        True -> [
          detailed_user_error(
            Some(["input", "note"]),
            "Notes cannot contain HTML tags",
            user_error_code.invalid,
            b2b_types.contains_html_tags_detail,
          ),
        ]
        False -> []
      }
    _ -> []
  }
}

@internal
pub fn validate_contact_duplicate_email(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_contact_id: Option(String),
) -> List(b2b_types.UserError) {
  case read_string(input, "email") {
    Some(email) ->
      case contact_email_exists(store, email, exclude_contact_id) {
        True -> [
          detailed_user_error(
            Some(["input", "email"]),
            "Email address has already been taken.",
            user_error_code.taken,
            b2b_types.duplicate_email_address_detail,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn validate_contact_duplicate_phone(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_contact_id: Option(String),
) -> List(b2b_types.UserError) {
  case read_string(input, "phone") {
    Some(phone) ->
      case contact_phone_exists(store, phone, exclude_contact_id) {
        True -> [
          detailed_user_error(
            Some(["input", "phone"]),
            "Phone number has already been taken.",
            user_error_code.taken,
            b2b_types.duplicate_phone_number_detail,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn validate_duplicate_company_external_id(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_company_id: Option(String),
  prefix: List(String),
) -> List(b2b_types.UserError) {
  case read_string(input, "externalId") {
    Some(external_id) ->
      case company_external_id_exists(store, external_id, exclude_company_id) {
        True -> [
          detailed_user_error(
            Some(field_path(prefix, "externalId")),
            "External id has already been taken.",
            user_error_code.taken,
            b2b_types.duplicate_external_id_detail,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn validate_duplicate_location_external_id(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_location_id: Option(String),
  prefix: List(String),
) -> List(b2b_types.UserError) {
  case read_string(input, "externalId") {
    Some(external_id) ->
      case
        location_external_id_exists(store, external_id, exclude_location_id)
      {
        True -> [
          detailed_user_error(
            Some(field_path(prefix, "externalId")),
            "External id has already been taken.",
            user_error_code.taken,
            b2b_types.duplicate_location_external_id_detail,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn company_external_id_exists(
  store: Store,
  external_id: String,
  exclude_company_id: Option(String),
) -> Bool {
  let excluded = option.unwrap(exclude_company_id, "")
  store.list_effective_b2b_companies(store)
  |> list.any(fn(company) {
    company.id != excluded
    && source_string(data_get(company.data, "externalId")) == external_id
  })
}

@internal
pub fn location_external_id_exists(
  store: Store,
  external_id: String,
  exclude_location_id: Option(String),
) -> Bool {
  let excluded = option.unwrap(exclude_location_id, "")
  store.list_effective_b2b_company_locations(store)
  |> list.any(fn(location) {
    location.id != excluded
    && source_string(data_get(location.data, "externalId")) == external_id
  })
}

@internal
pub fn contact_email_exists(
  store: Store,
  email: String,
  exclude_contact_id: Option(String),
) -> Bool {
  let excluded = option.unwrap(exclude_contact_id, "")
  store.list_effective_b2b_company_contacts(store)
  |> list.any(fn(contact) {
    contact.id != excluded
    && source_string(data_get(contact.data, "email")) |> string.lowercase
    == string.lowercase(email)
  })
}

@internal
pub fn contact_phone_exists(
  store: Store,
  phone: String,
  exclude_contact_id: Option(String),
) -> Bool {
  let excluded = option.unwrap(exclude_contact_id, "")
  store.list_effective_b2b_company_contacts(store)
  |> list.any(fn(contact) {
    contact.id != excluded
    && case source_string(data_get(contact.data, "phone")) {
      "" -> False
      existing ->
        case normalize_phone(store, existing) {
          Ok(normalized) -> normalized == phone
          Error(_) -> existing == phone
        }
    }
  })
}

@internal
pub fn normalize_phone(store: Store, phone: String) -> Result(String, Nil) {
  phone_numbers.normalize_for_store(store, phone)
}

@internal
pub fn is_digit_string(grapheme: String) -> Bool {
  string.contains("0123456789", grapheme)
}

@internal
pub fn valid_locale_format(locale: String) -> Bool {
  case string.split(locale, on: "-") {
    [language, ..subtags] ->
      valid_locale_language(language) && list.all(subtags, valid_locale_subtag)
    _ -> False
  }
}

@internal
pub fn valid_locale_language(language: String) -> Bool {
  let length = string.length(language)
  case length >= 2 && length <= 3 {
    True -> all_alpha(language)
    False -> False
  }
}

@internal
pub fn valid_locale_subtag(subtag: String) -> Bool {
  let length = string.length(subtag)
  length >= 1 && length <= 8 && all_alphanumeric(subtag)
}

@internal
pub fn all_alpha(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) -> is_alpha(grapheme) && all_alpha(rest)
  }
}

@internal
pub fn all_alphanumeric(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) ->
      { is_alpha(grapheme) || is_digit_string(grapheme) }
      && all_alphanumeric(rest)
  }
}

@internal
pub fn is_alpha(grapheme: String) -> Bool {
  string.contains(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    grapheme,
  )
}

@internal
pub fn valid_email_address(email: String) -> Bool {
  let trimmed = string.trim(email)
  trimmed == email
  && !contains_email_whitespace(email)
  && case string.split(email, on: "@") {
    [local, domain] ->
      local != ""
      && domain != ""
      && string.contains(domain, ".")
      && !string.starts_with(domain, ".")
      && !string.ends_with(domain, ".")
      && !string.contains(domain, "..")
    _ -> False
  }
}

@internal
pub fn contains_email_whitespace(email: String) -> Bool {
  email
  |> string.to_utf_codepoints
  |> list.any(fn(codepoint) {
    let code = string.utf_codepoint_to_int(codepoint)
    code == 0x09
    || code == 0x0a
    || code == 0x0b
    || code == 0x0c
    || code == 0x0d
    || code == 0x20
  })
}

@internal
pub fn contains_html_tag(value: String) -> Bool {
  string.contains(value, "<") && string.contains(value, ">")
}

@internal
pub fn location_data_from_input(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  now: String,
  existing: Dict(String, StorePropertyValue),
) -> #(Dict(String, StorePropertyValue), SyntheticIdentityRegistry) {
  let data =
    list.fold(
      ["name", "phone", "locale", "externalId", "note", "taxRegistrationId"],
      existing,
      fn(acc, key) { maybe_put_string(acc, input, key) },
    )
    |> maybe_put_bool(input, "billingSameAsShipping")
    |> maybe_put_bool(input, "taxExempt")
    |> dict.insert("updatedAt", StorePropertyString(now))
  let data = case dict.get(input, "taxExemptions") {
    Ok(root_field.ListVal(items)) ->
      dict.insert(
        data,
        "taxExemptions",
        StorePropertyList(
          list.filter_map(items, fn(item) {
            case item {
              root_field.StringVal(value) -> Ok(StorePropertyString(value))
              _ -> Error(Nil)
            }
          }),
        ),
      )
    _ -> data
  }
  let #(data, identity) = case dict.get(input, "billingAddress") {
    Ok(root_field.ObjectVal(address_input)) -> {
      let existing_id = address_id(data_get(data, "billingAddress"))
      let #(address, next_identity) =
        address_from_input(identity, address_input, existing_id)
      #(put_source(data, "billingAddress", address), next_identity)
    }
    _ -> #(data, identity)
  }
  let #(data, identity) = case dict.get(input, "shippingAddress") {
    Ok(root_field.ObjectVal(address_input)) -> {
      let existing_id = address_id(data_get(data, "shippingAddress"))
      let #(address, next_identity) =
        address_from_input(identity, address_input, existing_id)
      #(put_source(data, "shippingAddress", address), next_identity)
    }
    _ -> #(data, identity)
  }
  let data = case
    data_get(data, "billingSameAsShipping"),
    data_get(data, "shippingAddress")
  {
    SrcBool(True), SrcObject(_) as address ->
      put_source(data, "billingAddress", address)
    _, _ -> data
  }
  #(data, identity)
}

@internal
pub fn refresh_company_counts(company: B2BCompanyRecord) -> B2BCompanyRecord {
  B2BCompanyRecord(
    ..company,
    data: company.data
      |> dict.insert(
        "contactsCount",
        StorePropertyObject(
          dict.from_list([
            #("count", StorePropertyInt(list.length(company.contact_ids))),
          ]),
        ),
      )
      |> dict.insert(
        "locationsCount",
        StorePropertyObject(
          dict.from_list([
            #("count", StorePropertyInt(list.length(company.location_ids))),
          ]),
        ),
      ),
  )
}

@internal
pub fn stage_company(
  store: Store,
  company: B2BCompanyRecord,
) -> #(B2BCompanyRecord, Store) {
  store.upsert_staged_b2b_company(store, refresh_company_counts(company))
}

@internal
pub fn create_default_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  company_id: String,
) -> #(List(B2BCompanyContactRoleRecord), Store, SyntheticIdentityRegistry) {
  let #(admin_id, identity) = make_gid(identity, "CompanyContactRole")
  let #(ordering_id, identity) = make_gid(identity, "CompanyContactRole")
  let roles = [
    B2BCompanyContactRoleRecord(
      id: admin_id,
      cursor: None,
      company_id: company_id,
      data: dict.from_list([
        #("id", StorePropertyString(admin_id)),
        #("name", StorePropertyString("Location admin")),
        #("note", StorePropertyString("System-defined Location admin role")),
      ]),
    ),
    B2BCompanyContactRoleRecord(
      id: ordering_id,
      cursor: None,
      company_id: company_id,
      data: dict.from_list([
        #("id", StorePropertyString(ordering_id)),
        #("name", StorePropertyString("Ordering only")),
        #("note", StorePropertyString("System-defined Ordering only role")),
      ]),
    ),
  ]
  let #(store, staged_roles) =
    list.fold(roles, #(store, []), fn(acc, role) {
      let #(current_store, collected) = acc
      let #(staged, next_store) =
        store.upsert_staged_b2b_company_contact_role(current_store, role)
      #(next_store, list.append(collected, [staged]))
    })
  #(staged_roles, store, identity)
}

@internal
pub fn create_contact(
  store: Store,
  identity: SyntheticIdentityRegistry,
  company_id: String,
  input: Dict(String, root_field.ResolvedValue),
  _is_main: Bool,
) -> #(B2BCompanyContactRecord, Store, SyntheticIdentityRegistry) {
  let #(id, identity) = make_gid(identity, "CompanyContact")
  let #(now, identity) = timestamp(identity)
  let base =
    dict.from_list([
      #("id", StorePropertyString(id)),
      #("createdAt", StorePropertyString(now)),
      #("roleAssignments", StorePropertyList([])),
    ])
  let data = contact_data_from_input(input, now, base)
  let #(data, identity) = case read_string(input, "email") {
    Some(email) -> {
      let #(customer_id, next_identity) = make_gid(identity, "Customer")
      #(
        data
          |> dict.insert("customerId", StorePropertyString(customer_id))
          |> dict.insert(
            "customer",
            source_to_value(
              src_object([
                #("__typename", SrcString("Customer")),
                #("id", SrcString(customer_id)),
                #("email", SrcString(email)),
                #(
                  "firstName",
                  read_string(input, "firstName") |> optional_src_string,
                ),
                #(
                  "lastName",
                  read_string(input, "lastName") |> optional_src_string,
                ),
              ]),
            ),
          ),
        next_identity,
      )
    }
    None -> #(data, identity)
  }
  let contact =
    B2BCompanyContactRecord(
      id: id,
      cursor: None,
      company_id: company_id,
      data: data,
    )
  let #(contact, store) =
    store.upsert_staged_b2b_company_contact(store, contact)
  #(contact, store, identity)
}

@internal
pub fn create_location(
  store: Store,
  identity: SyntheticIdentityRegistry,
  company_id: String,
  input: Dict(String, root_field.ResolvedValue),
  fallback_name: String,
) -> #(B2BCompanyLocationRecord, Store, SyntheticIdentityRegistry) {
  let #(id, identity) = make_gid(identity, "CompanyLocation")
  let #(now, identity) = timestamp(identity)
  let input = case read_string(input, "name") {
    Some(_) -> input
    None -> dict.insert(input, "name", root_field.StringVal(fallback_name))
  }
  let base =
    dict.from_list([
      #("id", StorePropertyString(id)),
      #("createdAt", StorePropertyString(now)),
      #("roleAssignments", StorePropertyList([])),
      #("staffMemberAssignments", StorePropertyList([])),
    ])
  let #(data, identity) = location_data_from_input(identity, input, now, base)
  let location =
    B2BCompanyLocationRecord(
      id: id,
      cursor: None,
      company_id: company_id,
      data: data,
    )
  let #(location, store) =
    store.upsert_staged_b2b_company_location(store, location)
  #(location, store, identity)
}

@internal
pub fn location_create_fallback_name(
  company: B2BCompanyRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  case read_string(read_object(input, "shippingAddress"), "address1") {
    Some(address1) -> address1
    None -> {
      let company_name = source_string(data_get(company.data, "name"))
      case company_name {
        "" -> "Company location"
        _ -> company_name
      }
    }
  }
}

@internal
pub fn handle_company_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
) -> b2b_types.RootResult {
  let input = read_object(args, "input")
  let #(company_input, company_errors) =
    validate_company_input(read_object(input, "company"), ["input", "company"])
  let #(location_input, location_errors) =
    validate_location_input(read_object(input, "companyLocation"), [
      "input",
      "companyLocation",
    ])
  let company_errors =
    company_errors
    |> list.append(
      validate_duplicate_company_external_id(store, company_input, None, [
        "input",
        "company",
      ]),
    )
  let location_errors =
    location_errors
    |> list.append(
      validate_duplicate_location_external_id(store, location_input, None, [
        "input",
        "companyLocation",
      ]),
    )
  let #(contact_input, contact_errors) = case
    dict.get(input, "companyContact")
  {
    Ok(root_field.ObjectVal(raw_contact_input)) -> {
      let #(prepared, prepare_errors) =
        prepare_contact_create_input_with_prefix(store, raw_contact_input, [
          "input",
          "companyContact",
        ])
      let #(validated, validation_errors) =
        validate_contact_input(prepared, ["input", "companyContact"])
      #(Some(validated), list.append(prepare_errors, validation_errors))
    }
    _ -> #(None, [])
  }
  let validation_errors =
    company_errors
    |> list.append(location_errors)
    |> list.append(contact_errors)
  let name = read_string(company_input, "name") |> option_string("")
  case validation_errors, string.trim(name) {
    [_, ..], _ ->
      b2b_types.RootResult(
        empty_payload(validation_errors),
        store,
        identity,
        [],
      )
    [], "" ->
      b2b_types.RootResult(
        empty_payload([
          user_error(
            Some(["input", "company", "name"]),
            "Name can't be blank",
            user_error_code.blank,
          ),
        ]),
        store,
        identity,
        [],
      )
    _, _ -> {
      let #(company_id, identity) = make_gid(identity, "Company")
      let #(now, identity) = timestamp(identity)
      let #(roles, store, identity) =
        create_default_roles(store, identity, company_id)
      let #(location, store, identity) =
        create_location(store, identity, company_id, location_input, name)
      let #(contact, store, identity) = case contact_input {
        Some(contact_input) -> {
          let #(created, next_store, next_identity) =
            create_contact(store, identity, company_id, contact_input, True)
          #(Some(created), next_store, next_identity)
        }
        None -> #(None, store, identity)
      }
      let company =
        B2BCompanyRecord(
          id: company_id,
          cursor: None,
          data: company_data_from_input(
            company_input,
            now,
            dict.from_list([
              #("id", StorePropertyString(company_id)),
              #("createdAt", StorePropertyString(now)),
            ]),
          ),
          main_contact_id: case contact {
            Some(c) -> Some(c.id)
            None -> None
          },
          contact_ids: case contact {
            Some(c) -> [c.id]
            None -> []
          },
          location_ids: [location.id],
          contact_role_ids: list.map(roles, fn(role) { role.id }),
        )
      let #(company, store) = stage_company(store, company)
      let #(store, identity, assignment_ids) = case
        contact,
        list.drop(roles, 1)
      {
        Some(c), [ordering, ..] -> {
          let #(assignment, next_identity) =
            build_role_assignment(identity, c, ordering, location)
          let #(next_store, staged_ids) =
            stage_role_assignments(store, [assignment])
          #(
            next_store,
            next_identity,
            list.append([source_id(assignment)], staged_ids),
          )
        }
        _, _ -> #(store, identity, [])
      }
      let payload =
        b2b_types.Payload(..empty_payload([]), company: Some(company))
      b2b_types.RootResult(
        payload,
        store,
        identity,
        [company.id, location.id]
          |> list.append(list.map(roles, fn(role) { role.id }))
          |> list.append(case contact {
            Some(c) -> [c.id]
            None -> []
          })
          |> list.append(assignment_ids),
      )
    }
  }
}

@internal
pub fn option_string(value: Option(String), fallback: String) -> String {
  case value {
    Some(value) -> value
    None -> fallback
  }
}

@internal
pub fn handle_company_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyId") {
    Some(company_id) ->
      case store.get_effective_b2b_company_by_id(store, company_id) {
        Some(company) -> {
          let raw_input = read_object(args, "input")
          case reject_customer_since_update(raw_input) {
            [_, ..] as errors ->
              b2b_types.RootResult(empty_payload(errors), store, identity, [])
            [] -> {
              case dict.is_empty(raw_input), has_any_non_null_input(raw_input) {
                True, _ ->
                  b2b_types.RootResult(
                    empty_payload([company_update_empty_input_error()]),
                    store,
                    identity,
                    [],
                  )
                _, False ->
                  b2b_types.RootResult(
                    empty_payload([no_input_error()]),
                    store,
                    identity,
                    [],
                  )
                _, True -> {
                  let #(input, validation_errors) =
                    validate_company_input(raw_input, ["input"])
                  let validation_errors =
                    validation_errors
                    |> list.append(
                      validate_duplicate_company_external_id(
                        store,
                        input,
                        Some(company_id),
                        ["input"],
                      ),
                    )
                  let name = case dict.get(input, "name") {
                    Ok(root_field.StringVal(value)) -> value
                    _ -> source_string(data_get(company.data, "name"))
                  }
                  case validation_errors, string.trim(name) {
                    [_, ..], _ ->
                      b2b_types.RootResult(
                        empty_payload(validation_errors),
                        store,
                        identity,
                        [],
                      )
                    [], "" ->
                      b2b_types.RootResult(
                        empty_payload([
                          user_error(
                            Some(["input", "name"]),
                            "Name can't be blank",
                            user_error_code.blank,
                          ),
                        ]),
                        store,
                        identity,
                        [],
                      )
                    _, _ -> {
                      let #(now, identity) = timestamp(identity)
                      let updated =
                        B2BCompanyRecord(
                          ..company,
                          data: company_data_from_input(
                            input,
                            now,
                            company.data,
                          ),
                        )
                      let #(updated, store) = stage_company(store, updated)
                      b2b_types.RootResult(
                        b2b_types.Payload(
                          ..empty_payload([]),
                          company: Some(updated),
                        ),
                        store,
                        identity,
                        [updated.id],
                      )
                    }
                  }
                }
              }
            }
          }
        }
        None -> not_found_result(store, identity, "company", ["companyId"])
      }
    None -> not_found_result(store, identity, "company", ["companyId"])
  }
}

@internal
pub fn reject_customer_since_update(
  input: Dict(String, root_field.ResolvedValue),
) -> List(b2b_types.UserError) {
  case dict.get(input, "customerSince") {
    Ok(_) -> [
      user_error(
        Some(["input", "customerSince"]),
        "This field may only be set on creation.",
        user_error_code.invalid_input,
      ),
    ]
    Error(_) -> []
  }
}

@internal
pub fn not_found_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field_name: String,
  field_path: List(String),
) -> b2b_types.RootResult {
  let payload = case field_name {
    "company" ->
      b2b_types.Payload(
        ..empty_payload([resource_not_found(field_path)]),
        company: None,
      )
    "companyContact" ->
      case field_path {
        ["companyContactId"] ->
          b2b_types.Payload(
            ..empty_payload([
              user_error(
                Some(field_path),
                "The company contact doesn't exist.",
                user_error_code.resource_not_found,
              ),
            ]),
            company_contact: None,
          )
        _ ->
          b2b_types.Payload(
            ..empty_payload([resource_not_found(field_path)]),
            company_contact: None,
          )
      }
    "companyLocation" ->
      case field_path {
        ["companyLocationId"] ->
          b2b_types.Payload(
            ..empty_payload([
              user_error(
                Some(["input"]),
                "The company location doesn't exist",
                user_error_code.resource_not_found,
              ),
            ]),
            company_location: None,
          )
        _ ->
          b2b_types.Payload(
            ..empty_payload([resource_not_found(field_path)]),
            company_location: None,
          )
      }
    _ -> empty_payload([resource_not_found(field_path)])
  }
  b2b_types.RootResult(payload, store, identity, [])
}

@internal
pub fn delete_company_tree(
  store: Store,
  company_id: String,
) -> #(Store, List(String)) {
  case store.get_effective_b2b_company_by_id(store, company_id) {
    None -> #(store, [])
    Some(company) -> {
      let store =
        list.fold(
          company.contact_ids,
          store,
          store.delete_staged_b2b_company_contact,
        )
      let store =
        list.fold(
          company.location_ids,
          store,
          store.delete_staged_b2b_company_location,
        )
      let store =
        list.fold(
          company.contact_role_ids,
          store,
          store.delete_staged_b2b_company_contact_role,
        )
      let store = store.delete_staged_b2b_company(store, company_id)
      #(
        store,
        [company_id]
          |> list.append(company.contact_ids)
          |> list.append(company.location_ids)
          |> list.append(company.contact_role_ids),
      )
    }
  }
}

@internal
pub fn handle_company_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_b2b_company_by_id(store, id) {
        Some(_) -> {
          let #(store, ids) = delete_company_tree(store, id)
          b2b_types.RootResult(
            b2b_types.Payload(..empty_payload([]), deleted_company_id: Some(id)),
            store,
            identity,
            ids,
          )
        }
        None ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([resource_not_found(["id"])]),
              deleted_company_id: None,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([resource_not_found(["id"])]),
          deleted_company_id: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_companies_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let company_ids = read_string_list(args, "companyIds")
  case bulk_action_limit_reached(company_ids) {
    True ->
      b2b_types.RootResult(
        empty_payload([bulk_action_limit_reached_error("companyIds")]),
        store,
        identity,
        [],
      )
    False -> handle_companies_delete_under_limit(store, identity, company_ids)
  }
}

@internal
pub fn handle_companies_delete_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  company_ids: List(String),
) -> b2b_types.RootResult {
  let #(store, deleted, staged, errors) =
    company_ids
    |> list.index_map(fn(id, index) { #(id, index) })
    |> list.fold(#(store, [], [], []), fn(acc, entry) {
      let #(id, index) = entry
      let #(current_store, deleted, staged, errors) = acc
      case store.get_effective_b2b_company_by_id(current_store, id) {
        Some(_) -> {
          let #(next_store, ids) = delete_company_tree(current_store, id)
          #(
            next_store,
            list.append(deleted, [id]),
            list.append(staged, ids),
            errors,
          )
        }
        None -> #(
          current_store,
          deleted,
          staged,
          list.append(errors, [
            resource_not_found(indexed_field_path("companyIds", index)),
          ]),
        )
      }
    })
  b2b_types.RootResult(
    b2b_types.Payload(..empty_payload(errors), deleted_company_ids: deleted),
    store,
    identity,
    staged,
  )
}

@internal
pub fn handle_contact_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyId") {
    Some(company_id) ->
      case store.get_effective_b2b_company_by_id(store, company_id) {
        Some(company) -> {
          case company_contact_cap_reached(company) {
            True ->
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([company_contact_cap_error()]),
                  company_contact: None,
                ),
                store,
                identity,
                [],
              )
            False -> {
              let raw_input = read_object(args, "input")
              case dict.is_empty(raw_input), has_any_non_null_input(raw_input) {
                True, _ ->
                  b2b_types.RootResult(
                    b2b_types.Payload(
                      ..empty_payload([contact_create_empty_input_error()]),
                      company_contact: None,
                    ),
                    store,
                    identity,
                    [],
                  )
                _, False ->
                  b2b_types.RootResult(
                    b2b_types.Payload(
                      ..empty_payload([no_input_error()]),
                      company_contact: None,
                    ),
                    store,
                    identity,
                    [],
                  )
                _, True -> {
                  let #(prepared, prepare_errors) =
                    prepare_contact_create_input(store, raw_input)
                  let #(input, validation_errors) =
                    validate_contact_input(prepared, ["input"])
                  let errors = list.append(prepare_errors, validation_errors)
                  case errors {
                    [_, ..] ->
                      b2b_types.RootResult(
                        b2b_types.Payload(
                          ..empty_payload(errors),
                          company_contact: None,
                        ),
                        store,
                        identity,
                        [],
                      )
                    [] -> {
                      let #(contact, store, identity) =
                        create_contact(
                          store,
                          identity,
                          company_id,
                          input,
                          False,
                        )
                      let #(company, store) =
                        stage_company(
                          store,
                          B2BCompanyRecord(
                            ..company,
                            contact_ids: append_unique(
                              company.contact_ids,
                              contact.id,
                            ),
                          ),
                        )
                      b2b_types.RootResult(
                        b2b_types.Payload(
                          ..empty_payload([]),
                          company_contact: Some(contact),
                        ),
                        store,
                        identity,
                        [contact.id, company.id],
                      )
                    }
                  }
                }
              }
            }
          }
        }
        None ->
          not_found_result(store, identity, "companyContact", ["companyId"])
      }
    None -> not_found_result(store, identity, "companyContact", ["companyId"])
  }
}

@internal
pub fn handle_contact_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyContactId") {
    Some(contact_id) ->
      case store.get_effective_b2b_company_contact_by_id(store, contact_id) {
        Some(contact) -> {
          let raw_input = read_object(args, "input")
          case dict.is_empty(raw_input), has_any_non_null_input(raw_input) {
            True, _ ->
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([contact_update_empty_input_error()]),
                  company_contact: None,
                ),
                store,
                identity,
                [],
              )
            _, False ->
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([no_input_error()]),
                  company_contact: None,
                ),
                store,
                identity,
                [],
              )
            _, True -> {
              let #(prepared, prepare_errors) =
                prepare_contact_update_input(store, raw_input, contact_id)
              let #(input, validation_errors) =
                validate_contact_input(prepared, ["input"])
              let errors = list.append(prepare_errors, validation_errors)
              case errors {
                [_, ..] ->
                  b2b_types.RootResult(
                    b2b_types.Payload(
                      ..empty_payload(errors),
                      company_contact: None,
                    ),
                    store,
                    identity,
                    [],
                  )
                [] -> {
                  let #(now, identity) = timestamp(identity)
                  let data =
                    contact_data_from_input(input, now, contact.data)
                    |> contact_customer_data_from_input(input)
                  let updated = B2BCompanyContactRecord(..contact, data: data)
                  let #(updated, store) =
                    store.upsert_staged_b2b_company_contact(store, updated)
                  b2b_types.RootResult(
                    b2b_types.Payload(
                      ..empty_payload([]),
                      company_contact: Some(updated),
                    ),
                    store,
                    identity,
                    [updated.id],
                  )
                }
              }
            }
          }
        }
        None ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([
                user_error(
                  Some(["companyContactId"]),
                  "The company contact doesn't exist.",
                  user_error_code.resource_not_found,
                ),
              ]),
              company_contact: None,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([
            user_error(
              Some(["companyContactId"]),
              "The company contact doesn't exist.",
              user_error_code.resource_not_found,
            ),
          ]),
          company_contact: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn delete_contact(
  store: Store,
  contact_id: String,
) -> #(Store, List(String)) {
  case store.get_effective_b2b_company_contact_by_id(store, contact_id) {
    None -> #(store, [])
    Some(contact) -> {
      let #(store, cascade_ids) =
        remove_role_assignments_for_contact(store, contact_id)
      let store = case
        store.get_effective_b2b_company_by_id(store, contact.company_id)
      {
        Some(company) -> {
          let #(_, next_store) =
            stage_company(
              store,
              B2BCompanyRecord(
                ..company,
                main_contact_id: case company.main_contact_id {
                  Some(id) if id == contact_id -> None
                  other -> other
                },
                contact_ids: remove_string(company.contact_ids, contact_id),
              ),
            )
          next_store
        }
        None -> store
      }
      let store = store.delete_staged_b2b_company_contact(store, contact_id)
      #(store, [contact_id, contact.company_id] |> list.append(cascade_ids))
    }
  }
}

@internal
pub fn contact_has_associated_orders(
  store: Store,
  contact: B2BCompanyContactRecord,
) -> Bool {
  contact_has_associated_order_marker(contact)
  || contact_has_staged_order_history(store, contact.id)
}

@internal
pub fn contact_has_associated_order_marker(
  contact: B2BCompanyContactRecord,
) -> Bool {
  case data_get(contact.data, "ordersCount") {
    SrcInt(count) if count > 0 -> True
    _ ->
      case data_get(contact.data, "associatedOrdersCount") {
        SrcInt(count) if count > 0 -> True
        _ ->
          case data_get(contact.data, "hasAssociatedOrders") {
            SrcBool(True) -> True
            _ ->
              case data_get(contact.data, "orders") {
                SrcList([_, ..]) -> True
                _ -> False
              }
          }
      }
  }
}

@internal
pub fn contact_has_staged_order_history(
  store: Store,
  contact_id: String,
) -> Bool {
  list.any(store.list_effective_orders(store), fn(order) {
    purchasing_entity_contact_id(order.data) == Some(contact_id)
  })
  || list.any(store.list_effective_draft_orders(store), fn(draft_order) {
    completed_draft_order_references_contact(draft_order.data, contact_id)
  })
}

@internal
pub fn completed_draft_order_references_contact(
  data: CapturedJsonValue,
  contact_id: String,
) -> Bool {
  case captured_string_field(data, "status") {
    Some("COMPLETED") -> purchasing_entity_contact_id(data) == Some(contact_id)
    _ -> False
  }
  || case captured_object_field(data, "order") {
    Some(order) -> purchasing_entity_contact_id(order) == Some(contact_id)
    None -> False
  }
}

@internal
pub fn purchasing_entity_contact_id(data: CapturedJsonValue) -> Option(String) {
  data
  |> captured_object_field("purchasingEntity")
  |> option.then(fn(entity) {
    entity
    |> captured_object_field("contact")
    |> option.then(fn(contact) { captured_string_field(contact, "id") })
  })
}

@internal
pub fn captured_object_field(
  data: CapturedJsonValue,
  field: String,
) -> Option(CapturedJsonValue) {
  case data {
    CapturedObject(fields) -> captured_field(fields, field)
    _ -> None
  }
}

@internal
pub fn captured_string_field(
  data: CapturedJsonValue,
  field: String,
) -> Option(String) {
  case captured_object_field(data, field) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_field(
  fields: List(#(String, CapturedJsonValue)),
  field: String,
) -> Option(CapturedJsonValue) {
  case fields {
    [] -> None
    [#(key, value), ..] if key == field -> Some(value)
    [_, ..rest] -> captured_field(rest, field)
  }
}

@internal
pub type LocationDeleteBlocker {
  OnlyLocationOfCompany
  LocationHasOrders
  LocationHasStoreCredit
}

@internal
pub fn handle_contact_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyContactId") {
    Some(id) ->
      case store.get_effective_b2b_company_contact_by_id(store, id) {
        Some(contact) ->
          case contact_has_associated_orders(store, contact) {
            True ->
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([existing_orders_error()]),
                  deleted_company_contact_id: None,
                ),
                store,
                identity,
                [],
              )
            False -> {
              let #(store, ids) = delete_contact(store, id)
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([]),
                  deleted_company_contact_id: Some(id),
                ),
                store,
                identity,
                ids,
              )
            }
          }
        None ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([
                user_error(
                  Some(["companyContactId"]),
                  "The company contact doesn't exist.",
                  user_error_code.resource_not_found,
                ),
              ]),
              deleted_company_contact_id: None,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([
            user_error(
              Some(["companyContactId"]),
              "The company contact doesn't exist.",
              user_error_code.resource_not_found,
            ),
          ]),
          deleted_company_contact_id: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_contacts_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let contact_ids = read_string_list(args, "companyContactIds")
  case bulk_action_limit_reached(contact_ids) {
    True ->
      b2b_types.RootResult(
        empty_payload([bulk_action_limit_reached_error("companyContactIds")]),
        store,
        identity,
        [],
      )
    False -> handle_contacts_delete_under_limit(store, identity, contact_ids)
  }
}

@internal
pub fn handle_contacts_delete_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  contact_ids: List(String),
) -> b2b_types.RootResult {
  let #(store, deleted, staged, errors) =
    contact_ids
    |> list.index_map(fn(id, index) { #(id, index) })
    |> list.fold(#(store, [], [], []), fn(acc, entry) {
      let #(id, index) = entry
      let #(current_store, deleted, staged, errors) = acc
      case store.get_effective_b2b_company_contact_by_id(current_store, id) {
        Some(contact) ->
          case contact_has_associated_orders(current_store, contact) {
            True -> #(
              current_store,
              deleted,
              staged,
              list.append(errors, [
                existing_orders_error_at(indexed_field_path(
                  "companyContactIds",
                  index,
                )),
              ]),
            )
            False -> {
              let #(next_store, ids) = delete_contact(current_store, id)
              #(
                next_store,
                list.append(deleted, [id]),
                list.append(staged, ids),
                errors,
              )
            }
          }
        None -> #(
          current_store,
          deleted,
          staged,
          list.append(errors, [
            user_error(
              Some(indexed_field_path("companyContactIds", index)),
              "The company contact doesn't exist.",
              user_error_code.resource_not_found,
            ),
          ]),
        )
      }
    })
  b2b_types.RootResult(
    b2b_types.Payload(
      ..empty_payload(errors),
      deleted_company_contact_ids: deleted,
    ),
    store,
    identity,
    staged,
  )
}

@internal
pub fn handle_assign_customer_as_contact(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyId"), read_string(args, "customerId") {
    Some(company_id), Some(customer_id) ->
      case store.get_effective_b2b_company_by_id(store, company_id) {
        Some(company) -> {
          let contacts = company_contacts(store, company)
          case store.get_effective_customer_by_id(store, customer_id) {
            None ->
              company_contact_mutation_error(
                store,
                identity,
                ["customerId"],
                "Customer does not exist.",
                user_error_code.resource_not_found,
                Some(b2b_types.customer_not_found_detail),
              )
            Some(customer) ->
              case find_company_contact_by_customer_id(contacts, customer_id) {
                Some(_) ->
                  company_contact_mutation_error(
                    store,
                    identity,
                    ["companyId"],
                    "Customer is already associated with a company contact.",
                    user_error_code.invalid_input,
                    Some(b2b_types.customer_already_a_contact_detail),
                  )
                None ->
                  case customer_email(customer) {
                    None ->
                      company_contact_mutation_error(
                        store,
                        identity,
                        ["companyId"],
                        "Customer must have an email address.",
                        user_error_code.invalid_input,
                        Some(b2b_types.customer_email_must_exist_detail),
                      )
                    Some(email) ->
                      case company_contact_cap_reached(company) {
                        True ->
                          b2b_types.RootResult(
                            b2b_types.Payload(
                              ..empty_payload([company_contact_cap_error()]),
                              company_contact: None,
                            ),
                            store,
                            identity,
                            [],
                          )
                        False -> {
                          let #(input, errors) =
                            prepare_contact_create_input(store, dict.new())
                          case errors {
                            [_, ..] ->
                              b2b_types.RootResult(
                                b2b_types.Payload(
                                  ..empty_payload(errors),
                                  company_contact: None,
                                ),
                                store,
                                identity,
                                [],
                              )
                            [] -> {
                              let #(contact, store, identity) =
                                create_contact(
                                  store,
                                  identity,
                                  company_id,
                                  input,
                                  False,
                                )
                              let contact =
                                B2BCompanyContactRecord(
                                  ..contact,
                                  data: contact.data
                                    |> dict.insert(
                                      "customerId",
                                      StorePropertyString(customer_id),
                                    )
                                    |> dict.insert(
                                      "customer",
                                      customer_contact_source(customer, email)
                                        |> source_to_value,
                                    ),
                                )
                              let #(contact, store) =
                                store.upsert_staged_b2b_company_contact(
                                  store,
                                  contact,
                                )
                              let #(company, store) =
                                stage_company(
                                  store,
                                  B2BCompanyRecord(
                                    ..company,
                                    contact_ids: append_unique(
                                      company.contact_ids,
                                      contact.id,
                                    ),
                                  ),
                                )
                              b2b_types.RootResult(
                                b2b_types.Payload(
                                  ..empty_payload([]),
                                  company_contact: Some(contact),
                                ),
                                store,
                                identity,
                                [contact.id, company.id],
                              )
                            }
                          }
                        }
                      }
                  }
              }
          }
        }
        None ->
          not_found_result(store, identity, "companyContact", ["companyId"])
      }
    Some(_), None ->
      not_found_result(store, identity, "companyContact", ["customerId"])
    _, _ -> not_found_result(store, identity, "companyContact", ["companyId"])
  }
}

@internal
pub fn handle_contact_remove_from_company(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyContactId") {
    Some(id) ->
      case store.get_effective_b2b_company_contact_by_id(store, id) {
        Some(_) -> {
          let #(store, ids) = delete_contact(store, id)
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([]),
              removed_company_contact_id: Some(id),
            ),
            store,
            identity,
            ids,
          )
        }
        None ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([
                user_error(
                  Some(["companyContactId"]),
                  "The company contact doesn't exist.",
                  user_error_code.resource_not_found,
                ),
              ]),
              removed_company_contact_id: None,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([
            user_error(
              Some(["companyContactId"]),
              "The company contact doesn't exist.",
              user_error_code.resource_not_found,
            ),
          ]),
          removed_company_contact_id: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_assign_main_contact(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyId"), read_string(args, "companyContactId") {
    Some(company_id), Some(contact_id) ->
      case
        store.get_effective_b2b_company_by_id(store, company_id),
        store.get_effective_b2b_company_contact_by_id(store, contact_id)
      {
        Some(company), Some(contact) if contact.company_id == company_id -> {
          let updated_company =
            B2BCompanyRecord(..company, main_contact_id: Some(contact.id))
          let #(updated_company, store) = stage_company(store, updated_company)
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([]),
              company: Some(updated_company),
            ),
            store,
            identity,
            [updated_company.id],
          )
        }
        Some(_company), Some(_contact) ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([
                detailed_user_error(
                  Some(["companyContactId"]),
                  "The company contact does not belong to the company.",
                  user_error_code.invalid_input,
                  b2b_types.contact_does_not_match_company_detail,
                ),
              ]),
              company: None,
            ),
            store,
            identity,
            [],
          )
        Some(_company), None ->
          not_found_result(store, identity, "company", ["companyContactId"])
        _, _ -> not_found_result(store, identity, "company", ["companyId"])
      }
    _, _ -> not_found_result(store, identity, "company", ["companyId"])
  }
}

@internal
pub fn handle_revoke_main_contact(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyId") {
    Some(company_id) ->
      case store.get_effective_b2b_company_by_id(store, company_id) {
        Some(company) -> {
          let updated_company =
            B2BCompanyRecord(..company, main_contact_id: None)
          let #(updated_company, store) = stage_company(store, updated_company)
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([]),
              company: Some(updated_company),
            ),
            store,
            identity,
            [updated_company.id],
          )
        }
        None -> not_found_result(store, identity, "company", ["companyId"])
      }
    None -> not_found_result(store, identity, "company", ["companyId"])
  }
}

@internal
pub fn handle_location_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyId") {
    Some(company_id) ->
      case store.get_effective_b2b_company_by_id(store, company_id) {
        Some(company) -> {
          let raw_input = read_object(args, "input")
          let #(input, validation_errors) =
            validate_location_input(raw_input, ["input"])
          let validation_errors =
            validation_errors
            |> list.append(
              validate_duplicate_location_external_id(store, input, None, [
                "input",
              ]),
            )
          let fallback = location_create_fallback_name(company, input)
          case validation_errors {
            [_, ..] ->
              b2b_types.RootResult(
                empty_payload(validation_errors),
                store,
                identity,
                [],
              )
            [] -> {
              let #(location, store, identity) =
                create_location(store, identity, company_id, input, fallback)
              let #(company, store) =
                stage_company(
                  store,
                  B2BCompanyRecord(
                    ..company,
                    location_ids: append_unique(
                      company.location_ids,
                      location.id,
                    ),
                  ),
                )
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([]),
                  company_location: Some(location),
                ),
                store,
                identity,
                [location.id, company.id],
              )
            }
          }
        }
        None ->
          not_found_result(store, identity, "companyLocation", ["companyId"])
      }
    None -> not_found_result(store, identity, "companyLocation", ["companyId"])
  }
}

@internal
pub fn handle_location_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyLocationId") {
    Some(id) ->
      case store.get_effective_b2b_company_location_by_id(store, id) {
        Some(location) -> {
          let raw_input = read_object(args, "input")
          case dict.is_empty(raw_input), has_any_non_null_input(raw_input) {
            True, _ ->
              b2b_types.RootResult(
                empty_payload([location_update_empty_input_error()]),
                store,
                identity,
                [],
              )
            _, False ->
              b2b_types.RootResult(
                empty_payload([no_input_error()]),
                store,
                identity,
                [],
              )
            _, True -> {
              let #(input, validation_errors) =
                validate_location_input(raw_input, ["input"])
              let validation_errors =
                validation_errors
                |> list.append(
                  validate_duplicate_location_external_id(
                    store,
                    input,
                    Some(id),
                    ["input"],
                  ),
                )
              case validation_errors {
                [_, ..] ->
                  b2b_types.RootResult(
                    empty_payload(validation_errors),
                    store,
                    identity,
                    [],
                  )
                [] -> {
                  let #(now, identity) = timestamp(identity)
                  let #(data, identity) =
                    location_data_from_input(
                      identity,
                      input,
                      now,
                      location.data,
                    )
                  let updated = B2BCompanyLocationRecord(..location, data: data)
                  let #(updated, store) =
                    store.upsert_staged_b2b_company_location(store, updated)
                  b2b_types.RootResult(
                    b2b_types.Payload(
                      ..empty_payload([]),
                      company_location: Some(updated),
                    ),
                    store,
                    identity,
                    [updated.id],
                  )
                }
              }
            }
          }
        }
        None ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([
                user_error(
                  Some(["input"]),
                  "The company location doesn't exist",
                  user_error_code.resource_not_found,
                ),
              ]),
              company_location: None,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([
            user_error(
              Some(["input"]),
              "The company location doesn't exist",
              user_error_code.resource_not_found,
            ),
          ]),
          company_location: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn delete_location(
  store: Store,
  location_id: String,
) -> #(Store, List(String)) {
  case store.get_effective_b2b_company_location_by_id(store, location_id) {
    None -> #(store, [])
    Some(location) -> {
      let #(store, cascade_ids) =
        remove_role_assignments_for_location(store, location_id)
      let store = case
        store.get_effective_b2b_company_by_id(store, location.company_id)
      {
        Some(company) -> {
          let #(_, next_store) =
            stage_company(
              store,
              B2BCompanyRecord(
                ..company,
                location_ids: remove_string(company.location_ids, location_id),
              ),
            )
          next_store
        }
        None -> store
      }
      let store = store.delete_staged_b2b_company_location(store, location_id)
      #(store, [location_id, location.company_id] |> list.append(cascade_ids))
    }
  }
}

@internal
pub fn location_delete_blocker(
  store: Store,
  location: B2BCompanyLocationRecord,
) -> Option(LocationDeleteBlocker) {
  case location_has_other_locations(store, location) {
    False -> Some(OnlyLocationOfCompany)
    True ->
      case location_has_associated_orders(store, location) {
        True -> Some(LocationHasOrders)
        False ->
          case location_has_non_zero_store_credit(store, location) {
            True -> Some(LocationHasStoreCredit)
            False -> None
          }
      }
  }
}

@internal
pub fn location_delete_error(
  field: List(String),
  location: B2BCompanyLocationRecord,
  blocker: LocationDeleteBlocker,
) -> b2b_types.UserError {
  let _ = location
  let _ = blocker
  user_error(
    Some(field),
    "Failed to delete the company location.",
    user_error_code.failed_to_delete,
  )
}

@internal
pub fn location_bulk_delete_error(
  field: List(String),
  location: B2BCompanyLocationRecord,
  blocker: LocationDeleteBlocker,
) -> b2b_types.UserError {
  user_error(
    Some(field),
    location_bulk_delete_error_message(location.id, blocker),
    user_error_code.internal_error,
  )
}

fn location_bulk_delete_error_message(
  location_id: String,
  blocker: LocationDeleteBlocker,
) -> String {
  let public_id =
    option.unwrap(resource_ids.shopify_gid_tail(location_id), location_id)
  let prefix = "Failed to delete CompanyLocation " <> public_id <> ": "
  case blocker {
    OnlyLocationOfCompany -> prefix <> "Company must have at least 1 location."
    LocationHasOrders ->
      prefix <> "CompanyLocation has existing Orders/Draft Orders"
    LocationHasStoreCredit ->
      prefix <> "CompanyLocation has non-zero store credit balance"
  }
}

@internal
pub fn location_has_other_locations(
  store: Store,
  location: B2BCompanyLocationRecord,
) -> Bool {
  store.list_effective_b2b_company_locations(store)
  |> list.any(fn(other) {
    other.company_id == location.company_id && other.id != location.id
  })
}

@internal
pub fn location_has_associated_orders(
  store: Store,
  location: B2BCompanyLocationRecord,
) -> Bool {
  location_has_associated_order_marker(location)
  || location_has_staged_order_history(store, location.id)
}

@internal
pub fn location_has_associated_order_marker(
  location: B2BCompanyLocationRecord,
) -> Bool {
  case data_get(location.data, "ordersCount") {
    SrcInt(count) if count > 0 -> True
    _ ->
      case data_get(location.data, "associatedOrdersCount") {
        SrcInt(count) if count > 0 -> True
        _ ->
          case data_get(location.data, "hasAssociatedOrders") {
            SrcBool(True) -> True
            _ ->
              case data_get(location.data, "orders") {
                SrcList([_, ..]) -> True
                _ -> False
              }
          }
      }
  }
}

@internal
pub fn location_has_staged_order_history(
  store: Store,
  location_id: String,
) -> Bool {
  list.any(store.list_effective_orders(store), fn(order) {
    purchasing_entity_location_id(order.data) == Some(location_id)
  })
  || list.any(store.list_effective_draft_orders(store), fn(draft_order) {
    draft_order_references_location(draft_order.data, location_id)
  })
}

@internal
pub fn draft_order_references_location(
  data: CapturedJsonValue,
  location_id: String,
) -> Bool {
  purchasing_entity_location_id(data) == Some(location_id)
  || case captured_object_field(data, "order") {
    Some(order) -> purchasing_entity_location_id(order) == Some(location_id)
    None -> False
  }
}

@internal
pub fn purchasing_entity_location_id(
  data: CapturedJsonValue,
) -> Option(String) {
  data
  |> captured_object_field("purchasingEntity")
  |> option.then(fn(entity) {
    entity
    |> captured_object_field("location")
    |> option.then(fn(location) { captured_string_field(location, "id") })
  })
}

@internal
pub fn location_has_non_zero_store_credit(
  store: Store,
  location: B2BCompanyLocationRecord,
) -> Bool {
  store.list_effective_store_credit_accounts_for_customer(store, location.id)
  |> list.any(fn(account) { money_amount_non_zero(account.balance.amount) })
}

fn money_amount_non_zero(amount: String) -> Bool {
  case float.parse(amount) {
    Ok(value) -> value >. 0.0 || value <. 0.0
    Error(_) -> False
  }
}

@internal
pub fn remove_role_assignments_for_location(
  store: Store,
  location_id: String,
) -> #(Store, List(String)) {
  list.fold(
    store.list_effective_b2b_company_contacts(store),
    #(store, []),
    fn(acc, contact) {
      let #(current_store, staged_ids) = acc
      let current =
        read_object_sources(data_get(contact.data, "roleAssignments"))
      let #(kept, removed_ids) =
        remove_assignments_matching_location(current, location_id)
      case list.length(kept) == list.length(current) {
        True -> acc
        False -> {
          let updated =
            B2BCompanyContactRecord(
              ..contact,
              data: put_source(contact.data, "roleAssignments", SrcList(kept)),
            )
          let #(_, next_store) =
            store.upsert_staged_b2b_company_contact(current_store, updated)
          #(
            next_store,
            staged_ids
              |> list.append([contact.id])
              |> list.append(removed_ids),
          )
        }
      }
    },
  )
}

@internal
pub fn remove_role_assignments_for_contact(
  store: Store,
  contact_id: String,
) -> #(Store, List(String)) {
  list.fold(
    store.list_effective_b2b_company_locations(store),
    #(store, []),
    fn(acc, location) {
      let #(current_store, staged_ids) = acc
      let current =
        read_object_sources(data_get(location.data, "roleAssignments"))
      let #(kept, removed_ids) =
        remove_assignments_matching_contact(current, contact_id)
      case list.length(kept) == list.length(current) {
        True -> acc
        False -> {
          let updated =
            B2BCompanyLocationRecord(
              ..location,
              data: put_source(location.data, "roleAssignments", SrcList(kept)),
            )
          let #(_, next_store) =
            store.upsert_staged_b2b_company_location(current_store, updated)
          #(
            next_store,
            staged_ids
              |> list.append([location.id])
              |> list.append(removed_ids),
          )
        }
      }
    },
  )
}

@internal
pub fn remove_assignments_matching_location(
  assignments: List(SourceValue),
  location_id: String,
) -> #(List(SourceValue), List(String)) {
  list.fold(assignments, #([], []), fn(acc, assignment) {
    let #(kept, removed) = acc
    case assignment_ref(assignment, "companyLocationId") {
      Some(id) if id == location_id -> #(
        kept,
        list.append(removed, [source_id(assignment)]),
      )
      _ -> #(list.append(kept, [assignment]), removed)
    }
  })
}

@internal
pub fn remove_assignments_matching_contact(
  assignments: List(SourceValue),
  contact_id: String,
) -> #(List(SourceValue), List(String)) {
  list.fold(assignments, #([], []), fn(acc, assignment) {
    let #(kept, removed) = acc
    case assignment_ref(assignment, "companyContactId") {
      Some(id) if id == contact_id -> #(
        kept,
        list.append(removed, [source_id(assignment)]),
      )
      _ -> #(list.append(kept, [assignment]), removed)
    }
  })
}

@internal
pub fn handle_location_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyLocationId") {
    Some(id) ->
      case store.get_effective_b2b_company_location_by_id(store, id) {
        Some(location) ->
          case location_delete_blocker(store, location) {
            Some(blocker) ->
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([
                    location_delete_error(
                      ["companyLocationId"],
                      location,
                      blocker,
                    ),
                  ]),
                  deleted_company_location_id: None,
                ),
                store,
                identity,
                [],
              )
            None -> {
              let #(store, ids) = delete_location(store, id)
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([]),
                  deleted_company_location_id: Some(id),
                ),
                store,
                identity,
                ids,
              )
            }
          }
        None ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([
                user_error(
                  Some(["companyLocationId"]),
                  "The company location doesn't exist",
                  user_error_code.resource_not_found,
                ),
              ]),
              deleted_company_location_id: None,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([
            user_error(
              Some(["companyLocationId"]),
              "The company location doesn't exist",
              user_error_code.resource_not_found,
            ),
          ]),
          deleted_company_location_id: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_locations_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let location_ids = read_string_list(args, "companyLocationIds")
  case bulk_action_limit_reached(location_ids) {
    True ->
      b2b_types.RootResult(
        empty_payload([bulk_action_limit_reached_error("companyLocationIds")]),
        store,
        identity,
        [],
      )
    False -> handle_locations_delete_under_limit(store, identity, location_ids)
  }
}

@internal
pub fn handle_locations_delete_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  location_ids: List(String),
) -> b2b_types.RootResult {
  let #(store, deleted, staged, errors) =
    location_ids
    |> list.index_map(fn(id, index) { #(id, index) })
    |> list.fold(#(store, [], [], []), fn(acc, entry) {
      let #(id, index) = entry
      let #(current_store, deleted, staged, errors) = acc
      case store.get_effective_b2b_company_location_by_id(current_store, id) {
        Some(location) ->
          case location_delete_blocker(current_store, location) {
            Some(blocker) -> #(
              current_store,
              deleted,
              staged,
              list.append(errors, [
                location_bulk_delete_error(
                  indexed_field_path("companyLocationIds", index),
                  location,
                  blocker,
                ),
              ]),
            )
            None -> {
              let #(next_store, ids) = delete_location(current_store, id)
              #(
                next_store,
                list.append(deleted, [id]),
                list.append(staged, ids),
                errors,
              )
            }
          }
        None -> #(
          current_store,
          deleted,
          staged,
          list.append(errors, [
            user_error(
              Some(indexed_field_path("companyLocationIds", index)),
              "Resource requested does not exist.",
              user_error_code.resource_not_found,
            ),
          ]),
        )
      }
    })
  b2b_types.RootResult(
    b2b_types.Payload(
      ..empty_payload(errors),
      deleted_company_location_ids: deleted,
    ),
    store,
    identity,
    staged,
  )
}
