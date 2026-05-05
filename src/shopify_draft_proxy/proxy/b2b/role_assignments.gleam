//// B2B contact role-assignment mutation helpers.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/b2b/serializers.{
  address_from_input, address_id, append_unique, append_unique_list,
  bulk_action_limit_reached, bulk_action_limit_reached_error,
  company_contact_cap_error, company_contact_cap_reached,
  company_contact_does_not_exist_at, company_contact_mutation_error,
  company_contacts, company_location_not_found_at, company_locations,
  company_metafield_to_core, company_role_does_not_exist_at,
  company_role_not_found_at, company_roles, company_source,
  company_update_empty_input_error, contact_create_empty_input_error,
  contact_customer_id, contact_is_main_contact, contact_notes_source,
  contact_source, contact_source_with_main_flag,
  contact_update_empty_input_error, contains_html_tag_loop, contains_html_tags,
  contains_tag_close, customer_contact_source, customer_email, data_get,
  data_to_source, detailed_user_error, empty_payload, existing_orders_error,
  existing_orders_error_at, external_id_char_allowed, field_path,
  filter_companies_by_query, find_company_contact_by_customer_id,
  has_any_non_null_input, has_duplicate_strings, has_duplicate_strings_loop,
  has_explicit_null_field, has_non_empty_object_field, indexed_field_path,
  indexed_nested_field_path, is_html_tag_start, location_source,
  location_update_empty_input_error, make_gid, maybe_put_bool, maybe_put_string,
  no_input_error, one_role_already_assigned_at, option_from_result,
  optional_json_string, optional_src_string, paginate_records, project_source,
  put_source, read_bool, read_id_arg, read_object, read_object_list,
  read_object_sources, read_string, read_string_list, record_source,
  remove_string, resource_not_found, role_source, sanitize_name_field,
  selected_children, serialize_company, serialize_company_address_node_by_id,
  serialize_company_connection,
  serialize_company_contact_role_assignment_node_by_id,
  serialize_company_metafield, serialize_company_metafields_connection,
  serialize_contact, serialize_contact_connection, serialize_count,
  serialize_location, serialize_location_connection, serialize_mutation_payload,
  serialize_role_connection, serialize_source_connection, serialize_tax_settings,
  serialize_user_error, source_field, source_id, source_string, source_to_value,
  strip_html, strip_html_loop, timestamp, user_error,
  validate_billing_same_as_shipping, validate_company_input,
  validate_contact_input, validate_external_id_charset,
  validate_external_id_field, validate_external_id_length, validate_html,
  validate_length, validate_location_input, validate_tax_exempt_input,
  validate_text_field, value_is_present, value_to_source,
}
import shopify_draft_proxy/proxy/b2b/types as b2b_types
import shopify_draft_proxy/proxy/b2b_user_error_codes as user_error_code
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionWindow, type FragmentMap, type SourceValue,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_value, serialize_connection,
  serialize_empty_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type B2BCompanyContactRecord, type B2BCompanyContactRoleRecord,
  type B2BCompanyLocationRecord, type B2BCompanyRecord, type CapturedJsonValue,
  type CustomerRecord, type ProductMetafieldRecord, type StorePropertyValue,
  B2BCompanyContactRecord, B2BCompanyContactRoleRecord, B2BCompanyLocationRecord,
  B2BCompanyRecord, CapturedObject, CapturedString, StorePropertyBool,
  StorePropertyFloat, StorePropertyInt, StorePropertyList, StorePropertyNull,
  StorePropertyObject, StorePropertyString,
}

@internal
pub fn build_role_assignment(
  identity: SyntheticIdentityRegistry,
  contact: B2BCompanyContactRecord,
  role: B2BCompanyContactRoleRecord,
  location: B2BCompanyLocationRecord,
) -> #(SourceValue, SyntheticIdentityRegistry) {
  let #(id, identity) = make_gid(identity, "CompanyContactRoleAssignment")
  #(
    src_object([
      #("__typename", SrcString("CompanyContactRoleAssignment")),
      #("id", SrcString(id)),
      #("companyContactId", SrcString(contact.id)),
      #("companyContactRoleId", SrcString(role.id)),
      #("companyLocationId", SrcString(location.id)),
      #("companyContact", contact_source(contact)),
      #("role", role_source(role)),
      #("companyLocation", location_source(location)),
    ]),
    identity,
  )
}

@internal
pub fn stage_role_assignments(
  store: Store,
  assignments: List(SourceValue),
) -> #(Store, List(String)) {
  list.fold(assignments, #(store, []), fn(acc, assignment) {
    let #(current_store, staged_ids) = acc
    let contact_id = assignment_ref(assignment, "companyContactId")
    let location_id = assignment_ref(assignment, "companyLocationId")
    let #(store_after_contact, staged_ids) = case contact_id {
      Some(id) ->
        case store.get_effective_b2b_company_contact_by_id(current_store, id) {
          Some(contact) -> {
            let current =
              read_object_sources(data_get(contact.data, "roleAssignments"))
            let updated =
              B2BCompanyContactRecord(
                ..contact,
                data: put_source(
                  contact.data,
                  "roleAssignments",
                  SrcList(list.append(current, [assignment])),
                ),
              )
            let #(_, next_store) =
              store.upsert_staged_b2b_company_contact(current_store, updated)
            #(next_store, list.append(staged_ids, [contact.id]))
          }
          None -> #(current_store, staged_ids)
        }
      None -> #(current_store, staged_ids)
    }
    case location_id {
      Some(id) ->
        case
          store.get_effective_b2b_company_location_by_id(
            store_after_contact,
            id,
          )
        {
          Some(location) -> {
            let current =
              read_object_sources(data_get(location.data, "roleAssignments"))
            let updated =
              B2BCompanyLocationRecord(
                ..location,
                data: put_source(
                  location.data,
                  "roleAssignments",
                  SrcList(list.append(current, [assignment])),
                ),
              )
            let #(_, next_store) =
              store.upsert_staged_b2b_company_location(
                store_after_contact,
                updated,
              )
            #(next_store, list.append(staged_ids, [location.id]))
          }
          None -> #(store_after_contact, staged_ids)
        }
      None -> #(store_after_contact, staged_ids)
    }
  })
}

@internal
pub fn assignment_ref(assignment: SourceValue, key: String) -> Option(String) {
  case assignment {
    SrcObject(fields) ->
      case dict.get(fields, key) {
        Ok(SrcString(value)) -> Some(value)
        _ -> None
      }
    _ -> None
  }
}

@internal
pub fn assignment_matches_filters(
  assignment: SourceValue,
  contact_filter: Option(String),
  location_filter: Option(String),
) -> Bool {
  let contact_matches = case contact_filter {
    Some(id) -> assignment_ref(assignment, "companyContactId") == Some(id)
    None -> True
  }
  let location_matches = case location_filter {
    Some(id) -> assignment_ref(assignment, "companyLocationId") == Some(id)
    None -> True
  }
  contact_matches && location_matches
}

@internal
pub fn append_unique_assignment(
  assignments: List(SourceValue),
  assignment: SourceValue,
) -> List(SourceValue) {
  let id = source_id(assignment)
  case list.any(assignments, fn(item) { source_id(item) == id }) {
    True -> assignments
    False -> list.append(assignments, [assignment])
  }
}

@internal
pub fn list_effective_role_assignments(store: Store) -> List(SourceValue) {
  let from_contacts =
    store.list_effective_b2b_company_contacts(store)
    |> list.fold([], fn(assignments, contact) {
      read_object_sources(data_get(contact.data, "roleAssignments"))
      |> list.fold(assignments, append_unique_assignment)
    })
  store.list_effective_b2b_company_locations(store)
  |> list.fold(from_contacts, fn(assignments, location) {
    read_object_sources(data_get(location.data, "roleAssignments"))
    |> list.fold(assignments, append_unique_assignment)
  })
}

@internal
pub fn get_effective_contact(
  id: Option(String),
  store: Store,
) -> Option(B2BCompanyContactRecord) {
  case id {
    Some(id) -> store.get_effective_b2b_company_contact_by_id(store, id)
    None -> None
  }
}

@internal
pub fn get_effective_location(
  id: Option(String),
  store: Store,
) -> Option(B2BCompanyLocationRecord) {
  case id {
    Some(id) -> store.get_effective_b2b_company_location_by_id(store, id)
    None -> None
  }
}

@internal
pub fn resolve_role_assignments(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  contact_fallback: Option(String),
  location_fallback: Option(String),
  input_field: Option(String),
) -> #(List(SourceValue), List(b2b_types.UserError), SyntheticIdentityRegistry) {
  inputs
  |> list.index_map(fn(input, index) { #(input, index) })
  |> list.fold(#([], [], identity, []), fn(acc, entry) {
    let #(input, index) = entry
    let #(assignments, errors, current_identity, planned_pairs) = acc
    let input_contact_id = read_string(input, "companyContactId")
    let contact_id = input_contact_id |> option_or(contact_fallback)
    let role_id = read_string(input, "companyContactRoleId")
    let input_location_id = read_string(input, "companyLocationId")
    let location_id = input_location_id |> option_or(location_fallback)
    let contact_field = case input_contact_id {
      Some(_) -> role_assignment_field(input_field, index, "companyContactId")
      None -> ["companyContactId"]
    }
    let role_field =
      role_assignment_field(input_field, index, "companyContactRoleId")
    let location_field = case input_location_id {
      Some(_) -> role_assignment_field(input_field, index, "companyLocationId")
      None -> ["companyLocationId"]
    }
    case contact_id, role_id, location_id {
      Some(contact_id), Some(role_id), Some(location_id) -> {
        let contact =
          store.get_effective_b2b_company_contact_by_id(store, contact_id)
        let role =
          store.get_effective_b2b_company_contact_role_by_id(store, role_id)
        let location =
          store.get_effective_b2b_company_location_by_id(store, location_id)
        let lookup_errors =
          role_assignment_lookup_errors(
            contact,
            role,
            location,
            contact_field,
            role_field,
            location_field,
            role_assignment_missing_field(input_field, index),
            input_field,
            contact_fallback,
            location_fallback,
          )
        case lookup_errors, contact, role, location {
          [_, ..], _, _, _ -> #(
            assignments,
            list.append(errors, lookup_errors),
            current_identity,
            planned_pairs,
          )
          [], Some(contact), Some(role), Some(location) -> {
            let pair = #(contact.id, location.id)
            case
              contact_has_role_assignment_for_location(contact, location.id)
              || list.contains(planned_pairs, pair)
            {
              True -> #(
                assignments,
                list.append(errors, [
                  one_role_already_assigned_at(role_assignment_item_field(
                    input_field,
                    index,
                  )),
                ]),
                current_identity,
                planned_pairs,
              )
              False -> {
                let #(assignment, next_identity) =
                  build_role_assignment(
                    current_identity,
                    contact,
                    role,
                    location,
                  )
                #(
                  list.append(assignments, [assignment]),
                  errors,
                  next_identity,
                  list.append(planned_pairs, [pair]),
                )
              }
            }
          }
          _, _, _, _ -> #(assignments, errors, current_identity, planned_pairs)
        }
      }
      _, _, _ -> #(
        assignments,
        list.append(errors, [
          resource_not_found(role_assignment_missing_field(input_field, index)),
        ]),
        current_identity,
        planned_pairs,
      )
    }
  })
  |> fn(result) {
    let #(assignments, errors, identity, _) = result
    #(assignments, errors, identity)
  }
}

@internal
pub fn role_assignment_lookup_errors(
  contact: Option(B2BCompanyContactRecord),
  role: Option(B2BCompanyContactRoleRecord),
  location: Option(B2BCompanyLocationRecord),
  contact_field: List(String),
  role_field: List(String),
  location_field: List(String),
  item_field: List(String),
  input_field: Option(String),
  contact_fallback: Option(String),
  location_fallback: Option(String),
) -> List(b2b_types.UserError) {
  case input_field, contact_fallback, location_fallback {
    Some(_), Some(_), None ->
      bulk_contact_role_assignment_lookup_errors(
        contact,
        role,
        location,
        contact_field,
        role_field,
        location_field,
      )
    Some(_), None, Some(_) ->
      bulk_location_role_assignment_lookup_errors(
        contact,
        role,
        location,
        item_field,
      )
    _, _, _ ->
      single_role_assignment_lookup_errors(
        contact,
        role,
        location,
        contact_field,
        role_field,
        location_field,
      )
  }
}

@internal
pub fn single_role_assignment_lookup_errors(
  contact: Option(B2BCompanyContactRecord),
  role: Option(B2BCompanyContactRoleRecord),
  location: Option(B2BCompanyLocationRecord),
  contact_field: List(String),
  role_field: List(String),
  location_field: List(String),
) -> List(b2b_types.UserError) {
  let contact_errors = case contact {
    Some(_) -> []
    None -> [resource_not_found(contact_field)]
  }
  let role_errors = case role {
    Some(role) ->
      case contact {
        Some(contact) if role.company_id != contact.company_id -> [
          company_role_not_found_at(role_field),
        ]
        _ -> []
      }
    None -> [company_role_not_found_at(role_field)]
  }
  let location_errors = case location {
    Some(location) ->
      case contact {
        Some(contact) if location.company_id != contact.company_id -> [
          company_location_not_found_at(location_field),
        ]
        _ -> []
      }
    None -> [company_location_not_found_at(location_field)]
  }
  list.append(list.append(contact_errors, role_errors), location_errors)
}

@internal
pub fn bulk_contact_role_assignment_lookup_errors(
  contact: Option(B2BCompanyContactRecord),
  role: Option(B2BCompanyContactRoleRecord),
  location: Option(B2BCompanyLocationRecord),
  contact_field: List(String),
  role_field: List(String),
  location_field: List(String),
) -> List(b2b_types.UserError) {
  case contact, location, role {
    None, _, _ -> [resource_not_found(contact_field)]
    Some(contact), Some(location), _
      if location.company_id != contact.company_id
    -> [resource_not_found(location_field)]
    Some(contact), Some(_), Some(role)
      if role.company_id != contact.company_id
    -> [resource_not_found(role_field)]
    Some(_), Some(_), Some(_) -> []
    Some(_), None, _ -> [resource_not_found(location_field)]
    Some(_), Some(_), None -> [resource_not_found(role_field)]
  }
}

@internal
pub fn bulk_location_role_assignment_lookup_errors(
  contact: Option(B2BCompanyContactRecord),
  role: Option(B2BCompanyContactRoleRecord),
  location: Option(B2BCompanyLocationRecord),
  item_field: List(String),
) -> List(b2b_types.UserError) {
  case location, contact, role {
    None, _, _ -> [resource_not_found(["companyLocationId"])]
    Some(location), Some(contact), _
      if contact.company_id != location.company_id
    -> [company_contact_does_not_exist_at(item_field)]
    Some(location), Some(_), Some(role)
      if role.company_id != location.company_id
    -> [company_role_does_not_exist_at(item_field)]
    Some(_), Some(_), Some(_) -> []
    Some(_), None, _ -> [company_contact_does_not_exist_at(item_field)]
    Some(_), Some(_), None -> [company_role_does_not_exist_at(item_field)]
  }
}

@internal
pub fn role_assignment_field(
  input_field: Option(String),
  index: Int,
  field: String,
) -> List(String) {
  case input_field {
    Some(list_field) -> indexed_nested_field_path(list_field, index, field)
    None -> [field]
  }
}

@internal
pub fn role_assignment_item_field(
  input_field: Option(String),
  index: Int,
) -> Option(List(String)) {
  case input_field {
    Some(list_field) -> Some(indexed_field_path(list_field, index))
    None -> None
  }
}

@internal
pub fn role_assignment_missing_field(
  input_field: Option(String),
  index: Int,
) -> List(String) {
  case input_field {
    Some(list_field) -> indexed_field_path(list_field, index)
    None -> ["rolesToAssign"]
  }
}

@internal
pub fn contact_has_role_assignment_for_location(
  contact: B2BCompanyContactRecord,
  location_id: String,
) -> Bool {
  read_object_sources(data_get(contact.data, "roleAssignments"))
  |> list.any(fn(assignment) {
    assignment_ref(assignment, "companyLocationId") == Some(location_id)
  })
}

@internal
pub fn option_or(value: Option(a), fallback: Option(a)) -> Option(a) {
  case value {
    Some(_) -> value
    None -> fallback
  }
}

@internal
pub fn handle_contact_assign_role(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let #(assignments, errors, identity) =
    resolve_role_assignments(
      store,
      identity,
      [
        dict.from_list([
          #("companyContactId", read_arg_or_null(args, "companyContactId")),
          #(
            "companyContactRoleId",
            read_arg_or_null(args, "companyContactRoleId"),
          ),
          #("companyLocationId", read_arg_or_null(args, "companyLocationId")),
        ]),
      ],
      None,
      None,
      None,
    )
  let #(store, staged) = case errors {
    [] -> stage_role_assignments(store, assignments)
    _ -> #(store, [])
  }
  b2b_types.RootResult(
    b2b_types.Payload(
      ..empty_payload(errors),
      company_contact_role_assignment: case errors {
        [] -> list.first(assignments) |> option_from_result
        _ -> None
      },
    ),
    store,
    identity,
    staged,
  )
}

@internal
pub fn read_arg_or_null(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) {
  dict.get(args, key) |> result.unwrap(root_field.NullVal)
}

@internal
pub fn handle_contact_assign_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let roles_to_assign = read_object_list(args, "rolesToAssign")
  case bulk_action_limit_reached(roles_to_assign) {
    True ->
      b2b_types.RootResult(
        empty_payload([bulk_action_limit_reached_error("rolesToAssign")]),
        store,
        identity,
        [],
      )
    False ->
      handle_contact_assign_roles_under_limit(
        store,
        identity,
        args,
        roles_to_assign,
      )
  }
}

@internal
pub fn handle_contact_assign_roles_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
  roles_to_assign,
) -> b2b_types.RootResult {
  let #(assignments, errors, identity) =
    resolve_role_assignments(
      store,
      identity,
      roles_to_assign,
      read_string(args, "companyContactId"),
      None,
      Some("rolesToAssign"),
    )
  let #(store, staged) = stage_role_assignments(store, assignments)
  let company_contact =
    read_string(args, "companyContactId")
    |> get_effective_contact(store)
  b2b_types.RootResult(
    b2b_types.Payload(
      ..empty_payload(errors),
      company_contact: company_contact,
      role_assignments: assignments,
    ),
    store,
    identity,
    staged,
  )
}

@internal
pub fn handle_location_assign_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let roles_to_assign = read_object_list(args, "rolesToAssign")
  case bulk_action_limit_reached(roles_to_assign) {
    True ->
      b2b_types.RootResult(
        empty_payload([bulk_action_limit_reached_error("rolesToAssign")]),
        store,
        identity,
        [],
      )
    False ->
      handle_location_assign_roles_under_limit(
        store,
        identity,
        args,
        roles_to_assign,
      )
  }
}

@internal
pub fn handle_location_assign_roles_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
  roles_to_assign,
) -> b2b_types.RootResult {
  let #(assignments, errors, identity) =
    resolve_role_assignments(
      store,
      identity,
      roles_to_assign,
      None,
      read_string(args, "companyLocationId"),
      Some("rolesToAssign"),
    )
  let #(store, staged) = stage_role_assignments(store, assignments)
  let company_location =
    read_string(args, "companyLocationId")
    |> get_effective_location(store)
  b2b_types.RootResult(
    b2b_types.Payload(
      ..empty_payload(errors),
      company_location: company_location,
      role_assignments: assignments,
    ),
    store,
    identity,
    staged,
  )
}

@internal
pub fn revoke_role_assignments(
  store: Store,
  assignment_ids: List(String),
  contact_filter: Option(String),
  location_filter: Option(String),
  revoke_all: Bool,
) -> #(Store, List(String)) {
  let #(store, removed) =
    list.fold(
      store.list_effective_b2b_company_contacts(store),
      #(store, []),
      fn(acc, contact) {
        let #(current_store, removed) = acc
        case contact_filter {
          Some(id) if id != contact.id -> acc
          _ -> {
            let current =
              read_object_sources(data_get(contact.data, "roleAssignments"))
            let #(next, removed_here) =
              filter_removed_role_assignments(
                current,
                assignment_ids,
                contact_filter,
                location_filter,
                revoke_all,
              )
            case list.length(next) == list.length(current) {
              True -> acc
              False -> {
                let updated =
                  B2BCompanyContactRecord(
                    ..contact,
                    data: put_source(
                      contact.data,
                      "roleAssignments",
                      SrcList(next),
                    ),
                  )
                let #(_, next_store) =
                  store.upsert_staged_b2b_company_contact(
                    current_store,
                    updated,
                  )
                #(next_store, append_unique_list(removed, removed_here))
              }
            }
          }
        }
      },
    )
  list.fold(
    store.list_effective_b2b_company_locations(store),
    #(store, removed),
    fn(acc, location) {
      let #(current_store, removed) = acc
      case location_filter {
        Some(id) if id != location.id -> acc
        _ -> {
          let current =
            read_object_sources(data_get(location.data, "roleAssignments"))
          let #(next, removed_here) =
            filter_removed_role_assignments(
              current,
              assignment_ids,
              contact_filter,
              location_filter,
              revoke_all,
            )
          case list.length(next) == list.length(current) {
            True -> acc
            False -> {
              let updated =
                B2BCompanyLocationRecord(
                  ..location,
                  data: put_source(
                    location.data,
                    "roleAssignments",
                    SrcList(next),
                  ),
                )
              let #(_, next_store) =
                store.upsert_staged_b2b_company_location(current_store, updated)
              #(next_store, append_unique_list(removed, removed_here))
            }
          }
        }
      }
    },
  )
}

@internal
pub fn filter_removed_role_assignments(
  assignments: List(SourceValue),
  ids: List(String),
  contact_filter: Option(String),
  location_filter: Option(String),
  revoke_all: Bool,
) -> #(List(SourceValue), List(String)) {
  list.fold(assignments, #([], []), fn(acc, assignment) {
    let #(kept, removed) = acc
    let id = source_id(assignment)
    let should_remove =
      { revoke_all || list.contains(ids, id) }
      && assignment_matches_filters(assignment, contact_filter, location_filter)
    case should_remove {
      True -> #(kept, append_unique(removed, id))
      False -> #(list.append(kept, [assignment]), removed)
    }
  })
}

@internal
pub fn filter_removed_assignments(
  assignments: List(SourceValue),
  ids: List(String),
  revoke_all: Bool,
) -> #(List(SourceValue), List(String)) {
  list.fold(assignments, #([], []), fn(acc, assignment) {
    let #(kept, removed) = acc
    let id = source_id(assignment)
    let should_remove = revoke_all || list.contains(ids, id)
    case should_remove {
      True -> #(kept, list.append(removed, [id]))
      False -> #(list.append(kept, [assignment]), removed)
    }
  })
}

@internal
pub fn handle_contact_revoke_role(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let ids = case read_string(args, "companyContactRoleAssignmentId") {
    Some(id) -> [id]
    None -> []
  }
  let #(store, revoked) =
    revoke_role_assignments(
      store,
      ids,
      read_string(args, "companyContactId"),
      None,
      False,
    )
  let errors = case revoked {
    [] -> [resource_not_found(["companyContactRoleAssignmentId"])]
    _ -> []
  }
  b2b_types.RootResult(
    b2b_types.Payload(
      ..empty_payload(errors),
      revoked_company_contact_role_assignment_id: list.first(revoked)
        |> option_from_result,
    ),
    store,
    identity,
    revoked,
  )
}

@internal
pub fn handle_contact_revoke_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let revoke_all = case read_bool(args, "revokeAll") {
    Some(True) -> True
    _ -> False
  }
  let role_assignment_ids = read_string_list(args, "roleAssignmentIds")
  case bulk_action_limit_reached(role_assignment_ids) {
    True ->
      b2b_types.RootResult(
        empty_payload([bulk_action_limit_reached_error("roleAssignmentIds")]),
        store,
        identity,
        [],
      )
    False ->
      handle_contact_revoke_roles_under_limit(
        store,
        identity,
        args,
        revoke_all,
        role_assignment_ids,
      )
  }
}

@internal
pub fn handle_contact_revoke_roles_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
  revoke_all: Bool,
  role_assignment_ids: List(String),
) -> b2b_types.RootResult {
  let contact_id = read_string(args, "companyContactId")
  let company_contact = get_effective_contact(contact_id, store)
  case contact_id, company_contact {
    Some(_), Some(_) -> {
      let #(revoked, errors) =
        resolve_role_revocations(
          store,
          role_assignment_ids,
          "roleAssignmentIds",
          contact_id,
          None,
          revoke_all,
        )
      let #(store, _) =
        revoke_role_assignments(store, revoked, contact_id, None, revoke_all)
      let company_contact = get_effective_contact(contact_id, store)
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload(errors),
          company_contact: company_contact,
          revoked_role_assignment_ids: revoked,
        ),
        store,
        identity,
        revoked,
      )
    }
    _, _ ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([resource_not_found(["companyContactId"])]),
          company_contact: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_location_revoke_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let roles_to_revoke = read_string_list(args, "rolesToRevoke")
  case bulk_action_limit_reached(roles_to_revoke) {
    True ->
      b2b_types.RootResult(
        empty_payload([bulk_action_limit_reached_error("rolesToRevoke")]),
        store,
        identity,
        [],
      )
    False ->
      handle_location_revoke_roles_under_limit(
        store,
        identity,
        args,
        roles_to_revoke,
      )
  }
}

@internal
pub fn handle_location_revoke_roles_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
  roles_to_revoke: List(String),
) -> b2b_types.RootResult {
  let location_id = read_string(args, "companyLocationId")
  let company_location = get_effective_location(location_id, store)
  case location_id, company_location {
    Some(_), Some(_) -> {
      let #(revoked, errors) =
        resolve_role_revocations(
          store,
          roles_to_revoke,
          "rolesToRevoke",
          None,
          location_id,
          False,
        )
      let #(store, _) =
        revoke_role_assignments(store, revoked, None, location_id, False)
      let company_location = get_effective_location(location_id, store)
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload(errors),
          company_location: company_location,
          revoked_role_assignment_ids: revoked,
        ),
        store,
        identity,
        revoked,
      )
    }
    _, _ ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([resource_not_found(["companyLocationId"])]),
          company_location: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn resolve_role_revocations(
  store: Store,
  ids: List(String),
  arg_name: String,
  contact_filter: Option(String),
  location_filter: Option(String),
  revoke_all: Bool,
) -> #(List(String), List(b2b_types.UserError)) {
  case revoke_all {
    True -> {
      let revoked =
        list_effective_role_assignments(store)
        |> list.filter(fn(assignment) {
          assignment_matches_filters(
            assignment,
            contact_filter,
            location_filter,
          )
        })
        |> list.map(source_id)
      #(revoked, [])
    }
    False ->
      list.index_fold(ids, #([], []), fn(acc, id, index) {
        let #(revoked, errors) = acc
        case
          list.contains(revoked, id)
          || !role_assignment_can_be_revoked(
            store,
            id,
            contact_filter,
            location_filter,
          )
        {
          True -> #(
            revoked,
            list.append(errors, [
              resource_not_found([arg_name, int.to_string(index)]),
            ]),
          )
          False -> #(list.append(revoked, [id]), errors)
        }
      })
  }
}

@internal
pub fn role_assignment_can_be_revoked(
  store: Store,
  id: String,
  contact_filter: Option(String),
  location_filter: Option(String),
) -> Bool {
  list_effective_role_assignments(store)
  |> list.any(fn(assignment) {
    source_id(assignment) == id
    && assignment_matches_filters(assignment, contact_filter, location_filter)
  })
}
