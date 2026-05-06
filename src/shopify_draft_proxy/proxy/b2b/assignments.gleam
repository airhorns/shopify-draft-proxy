//// B2B address, staff-assignment, and tax-setting mutation helpers.

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
import shopify_draft_proxy/proxy/b2b/role_assignments.{
  append_unique_assignment, assignment_matches_filters, assignment_ref,
  build_role_assignment, bulk_contact_role_assignment_lookup_errors,
  bulk_location_role_assignment_lookup_errors,
  contact_has_role_assignment_for_location, filter_removed_assignments,
  filter_removed_role_assignments, get_effective_contact, get_effective_location,
  handle_contact_assign_role, handle_contact_assign_roles,
  handle_contact_assign_roles_under_limit, handle_contact_revoke_role,
  handle_contact_revoke_roles, handle_contact_revoke_roles_under_limit,
  handle_location_assign_roles, handle_location_assign_roles_under_limit,
  handle_location_revoke_roles, handle_location_revoke_roles_under_limit,
  list_effective_role_assignments, option_or, read_arg_or_null,
  resolve_role_assignments, resolve_role_revocations, revoke_role_assignments,
  role_assignment_can_be_revoked, role_assignment_field,
  role_assignment_item_field, role_assignment_lookup_errors,
  role_assignment_missing_field, single_role_assignment_lookup_errors,
  stage_role_assignments,
}
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
pub fn handle_assign_address(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "locationId") {
    Some(location_id) ->
      case store.get_effective_b2b_company_location_by_id(store, location_id) {
        Some(location) -> {
          let address_types = read_string_list(args, "addressTypes")
          case has_duplicate_strings(address_types) {
            True ->
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload([
                    user_error(
                      None,
                      "Invalid input.",
                      user_error_code.invalid_input,
                    ),
                  ]),
                  addresses: [],
                ),
                store,
                identity,
                [],
              )
            False -> {
              let #(address, identity) =
                address_from_input(identity, read_object(args, "address"), None)
              let #(data, addresses) =
                list.fold(address_types, #(location.data, []), fn(acc, typ) {
                  let #(data, addresses) = acc
                  case typ {
                    "BILLING" -> #(
                      put_source(data, "billingAddress", address),
                      list.append(addresses, [address]),
                    )
                    "SHIPPING" -> #(
                      put_source(data, "shippingAddress", address),
                      list.append(addresses, [address]),
                    )
                    _ -> acc
                  }
                })
              case addresses {
                [] ->
                  b2b_types.RootResult(
                    b2b_types.Payload(
                      ..empty_payload([
                        user_error(
                          Some(["addressTypes"]),
                          "Address type is invalid",
                          user_error_code.invalid,
                        ),
                      ]),
                      addresses: [],
                    ),
                    store,
                    identity,
                    [],
                  )
                _ -> {
                  let updated = B2BCompanyLocationRecord(..location, data: data)
                  let #(updated, store) =
                    store.upsert_staged_b2b_company_location(store, updated)
                  b2b_types.RootResult(
                    b2b_types.Payload(..empty_payload([]), addresses: addresses),
                    store,
                    identity,
                    list.append([updated.id], list.map(addresses, source_id)),
                  )
                }
              }
            }
          }
        }
        None ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([resource_not_found(["locationId"])]),
              addresses: [],
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([resource_not_found(["locationId"])]),
          addresses: [],
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_address_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "addressId") {
    Some(target_address_id) -> {
      let #(store, detached_location_ids) =
        detach_address_from_locations(store, target_address_id)
      case detached_location_ids {
        [_, ..] ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([]),
              deleted_address_id: Some(target_address_id),
            ),
            store,
            identity,
            list.append(detached_location_ids, [target_address_id]),
          )
        [] ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([resource_not_found(["addressId"])]),
              deleted_address_id: None,
            ),
            store,
            identity,
            [],
          )
      }
    }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([resource_not_found(["addressId"])]),
          deleted_address_id: None,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn detach_address_from_locations(
  store: Store,
  target_address_id: String,
) -> #(Store, List(String)) {
  list.fold(
    store.list_effective_b2b_company_locations(store),
    #(store, []),
    fn(acc, location) {
      let #(current_store, detached_ids) = acc
      let billing_matches =
        address_id(data_get(location.data, "billingAddress"))
        == Some(target_address_id)
      let shipping_matches =
        address_id(data_get(location.data, "shippingAddress"))
        == Some(target_address_id)
      case billing_matches || shipping_matches {
        False -> acc
        True -> {
          let data = case billing_matches {
            True -> put_source(location.data, "billingAddress", SrcNull)
            False -> location.data
          }
          let data = case shipping_matches {
            True -> put_source(data, "shippingAddress", SrcNull)
            False -> data
          }
          let data = case
            data_get(location.data, "billingSameAsShipping"),
            billing_matches || shipping_matches
          {
            SrcBool(True), True ->
              dict.insert(
                data,
                "billingSameAsShipping",
                StorePropertyBool(False),
              )
            _, _ -> data
          }
          let updated = B2BCompanyLocationRecord(..location, data: data)
          let #(_, next_store) =
            store.upsert_staged_b2b_company_location(current_store, updated)
          #(next_store, list.append(detached_ids, [location.id]))
        }
      }
    },
  )
}

@internal
pub fn handle_assign_staff(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let staff_member_ids = read_string_list(args, "staffMemberIds")
  case bulk_action_limit_reached(staff_member_ids) {
    True ->
      b2b_types.RootResult(
        empty_payload([bulk_action_limit_reached_error("staffMemberIds")]),
        store,
        identity,
        [],
      )
    False ->
      handle_assign_staff_under_limit(store, identity, args, staff_member_ids)
  }
}

@internal
pub fn handle_assign_staff_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
  staff_member_ids: List(String),
) -> b2b_types.RootResult {
  case read_string(args, "companyLocationId") {
    Some(location_id) ->
      case store.get_effective_b2b_company_location_by_id(store, location_id) {
        Some(location) -> {
          let #(assignments, errors, identity) =
            list.index_fold(
              staff_member_ids,
              #([], [], identity),
              fn(acc, staff_id, index) {
                let #(items, errors, current_identity) = acc
                case staff_member_exists(store, staff_id) {
                  True -> {
                    let #(id, next_identity) =
                      make_gid(
                        current_identity,
                        "CompanyLocationStaffMemberAssignment",
                      )
                    let assignment =
                      src_object([
                        #(
                          "__typename",
                          SrcString("CompanyLocationStaffMemberAssignment"),
                        ),
                        #("id", SrcString(id)),
                        #("staffMemberId", SrcString(staff_id)),
                        #("companyLocationId", SrcString(location.id)),
                        #(
                          "staffMember",
                          src_object([
                            #("__typename", SrcString("StaffMember")),
                            #("id", SrcString(staff_id)),
                          ]),
                        ),
                        #("companyLocation", location_source(location)),
                      ])
                    #(list.append(items, [assignment]), errors, next_identity)
                  }
                  False -> #(
                    items,
                    list.append(errors, [
                      resource_not_found([
                        "staffMemberIds",
                        int.to_string(index),
                      ]),
                    ]),
                    current_identity,
                  )
                }
              },
            )
          case assignments {
            [] ->
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload(errors),
                  company_location_staff_member_assignments: [],
                ),
                store,
                identity,
                [],
              )
            [_, ..] -> {
              let current =
                read_object_sources(data_get(
                  location.data,
                  "staffMemberAssignments",
                ))
              let updated =
                B2BCompanyLocationRecord(
                  ..location,
                  data: put_source(
                    location.data,
                    "staffMemberAssignments",
                    SrcList(list.append(current, assignments)),
                  ),
                )
              let #(updated, store) =
                store.upsert_staged_b2b_company_location(store, updated)
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload(errors),
                  company_location_staff_member_assignments: assignments,
                ),
                store,
                identity,
                list.append([updated.id], list.map(assignments, source_id)),
              )
            }
          }
        }
        None ->
          b2b_types.RootResult(
            b2b_types.Payload(
              ..empty_payload([resource_not_found(["companyLocationId"])]),
              company_location_staff_member_assignments: [],
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      b2b_types.RootResult(
        b2b_types.Payload(
          ..empty_payload([resource_not_found(["companyLocationId"])]),
          company_location_staff_member_assignments: [],
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn staff_member_exists(store: Store, staff_id: String) -> Bool {
  case store.get_effective_admin_platform_generic_node_by_id(store, staff_id) {
    Some(record) -> record.typename == "StaffMember"
    None -> False
  }
}

@internal
pub fn handle_remove_staff(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  let ids = read_string_list(args, "companyLocationStaffMemberAssignmentIds")
  case bulk_action_limit_reached(ids) {
    True ->
      b2b_types.RootResult(
        empty_payload([
          bulk_action_limit_reached_error(
            "companyLocationStaffMemberAssignmentIds",
          ),
        ]),
        store,
        identity,
        [],
      )
    False -> handle_remove_staff_under_limit(store, identity, ids)
  }
}

@internal
pub fn handle_remove_staff_under_limit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  ids: List(String),
) -> b2b_types.RootResult {
  let existing_ids = effective_staff_assignment_ids(store)
  let valid_ids = list.filter(ids, fn(id) { list.contains(existing_ids, id) })
  let errors =
    list.index_fold(ids, [], fn(errors, id, index) {
      case list.contains(existing_ids, id) {
        True -> errors
        False ->
          list.append(errors, [
            resource_not_found([
              "companyLocationStaffMemberAssignmentIds",
              int.to_string(index),
            ]),
          ])
      }
    })
  let #(store, removed, staged) =
    list.fold(
      store.list_effective_b2b_company_locations(store),
      #(store, [], []),
      fn(acc, location) {
        let #(current_store, removed, staged) = acc
        let current =
          read_object_sources(data_get(location.data, "staffMemberAssignments"))
        let #(next, removed_here) =
          filter_removed_assignments(current, valid_ids, False)
        case list.length(next) == list.length(current) {
          True -> acc
          False -> {
            let updated =
              B2BCompanyLocationRecord(
                ..location,
                data: put_source(
                  location.data,
                  "staffMemberAssignments",
                  SrcList(next),
                ),
              )
            let #(_, next_store) =
              store.upsert_staged_b2b_company_location(current_store, updated)
            #(
              next_store,
              list.append(removed, removed_here),
              list.append(staged, [location.id]),
            )
          }
        }
      },
    )
  b2b_types.RootResult(
    b2b_types.Payload(
      ..empty_payload(errors),
      deleted_company_location_staff_member_assignment_ids: removed,
    ),
    store,
    identity,
    list.append(staged, removed),
  )
}

@internal
pub fn effective_staff_assignment_ids(store: Store) -> List(String) {
  store.list_effective_b2b_company_locations(store)
  |> list.flat_map(fn(location) {
    read_object_sources(data_get(location.data, "staffMemberAssignments"))
    |> list.map(source_id)
  })
}

@internal
pub fn handle_tax_settings_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> b2b_types.RootResult {
  case read_string(args, "companyLocationId") {
    Some(location_id) ->
      case store.get_effective_b2b_company_location_by_id(store, location_id) {
        Some(location) -> {
          let errors = validate_tax_settings_update_args(args)
          case errors {
            [_, ..] ->
              b2b_types.RootResult(
                b2b_types.Payload(
                  ..empty_payload(errors),
                  company_location: None,
                ),
                store,
                identity,
                [],
              )
            [] -> {
              let current_exemptions =
                read_string_values(data_get(location.data, "taxExemptions"))
                |> list.append(read_string_list(args, "exemptionsToAssign"))
                |> list.filter(fn(item) {
                  !list.contains(
                    read_string_list(args, "exemptionsToRemove"),
                    item,
                  )
                })
              let #(now, identity) = timestamp(identity)
              let data =
                location.data
                |> dict.insert(
                  "taxExemptions",
                  StorePropertyList(list.map(
                    current_exemptions,
                    StorePropertyString,
                  )),
                )
                |> dict.insert("updatedAt", StorePropertyString(now))
                |> maybe_put_string(args, "taxRegistrationId")
                |> maybe_put_bool(args, "taxExempt")
              let tax_settings =
                src_object([
                  #("taxRegistrationId", data_get(data, "taxRegistrationId")),
                  #("taxExempt", data_get(data, "taxExempt")),
                  #("taxExemptions", data_get(data, "taxExemptions")),
                ])
              let data = put_source(data, "taxSettings", tax_settings)
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
              Some(["companyLocationId"]),
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
pub fn validate_tax_settings_update_args(
  args: Dict(String, root_field.ResolvedValue),
) -> List(b2b_types.UserError) {
  case dict.get(args, "taxExempt") {
    Ok(root_field.NullVal) -> [
      user_error(
        Some(["taxExempt"]),
        "Invalid input.",
        user_error_code.invalid_input,
      ),
    ]
    _ ->
      case has_any_tax_settings_update_input(args) {
        True -> []
        False -> [
          user_error(
            Some(["companyLocationId"]),
            "No input provided.",
            user_error_code.no_input,
          ),
        ]
      }
  }
}

@internal
pub fn has_any_tax_settings_update_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.has_key(args, "taxRegistrationId")
  || dict.has_key(args, "taxExempt")
  || dict.has_key(args, "exemptionsToAssign")
  || dict.has_key(args, "exemptionsToRemove")
}

@internal
pub fn read_string_values(value: SourceValue) -> List(String) {
  case value {
    SrcList(items) ->
      list.filter_map(items, fn(item) {
        case item {
          SrcString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}
