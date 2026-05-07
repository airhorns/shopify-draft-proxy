//// B2B address, staff-assignment, and tax-setting mutation helpers.

import gleam/dict.{type Dict}
import gleam/int

import gleam/list
import gleam/option.{None, Some}

import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/b2b/role_assignments.{
  filter_removed_assignments,
}

import shopify_draft_proxy/proxy/b2b/serializers.{
  address_from_input, address_id, bulk_action_limit_reached,
  bulk_action_limit_reached_error, data_get, empty_payload,
  has_duplicate_strings, location_source, make_gid, maybe_put_bool,
  maybe_put_string, put_source, read_object, read_object_sources, read_string,
  read_string_list, resource_not_found, source_id, timestamp, user_error,
  validate_address_input,
}

import shopify_draft_proxy/proxy/b2b/types as b2b_types
import shopify_draft_proxy/proxy/b2b_user_error_codes as user_error_code
import shopify_draft_proxy/proxy/graphql_helpers.{
  type SourceValue, SrcBool, SrcList, SrcNull, SrcString, src_object,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  B2BCompanyLocationRecord, StorePropertyBool, StorePropertyList,
  StorePropertyString,
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
              let address_input = read_object(args, "address")
              let validation_errors =
                validate_address_input(address_input, ["address"])
              case validation_errors {
                [_, ..] ->
                  b2b_types.RootResult(
                    empty_payload(validation_errors),
                    store,
                    identity,
                    [],
                  )
                [] -> {
                  let #(data, addresses, identity) =
                    list.fold(
                      address_types,
                      #(location.data, [], identity),
                      fn(acc, typ) {
                        let #(data, addresses, identity) = acc
                        case typ {
                          "BILLING" -> {
                            let existing_id =
                              address_id(data_get(data, "billingAddress"))
                            let #(address, identity) =
                              address_from_input(
                                identity,
                                address_input,
                                existing_id,
                              )
                            #(
                              put_source(data, "billingAddress", address),
                              list.append(addresses, [address]),
                              identity,
                            )
                          }
                          "SHIPPING" -> {
                            let existing_id =
                              address_id(data_get(data, "shippingAddress"))
                            let #(address, identity) =
                              address_from_input(
                                identity,
                                address_input,
                                existing_id,
                              )
                            #(
                              put_source(data, "shippingAddress", address),
                              list.append(addresses, [address]),
                              identity,
                            )
                          }
                          _ -> acc
                        }
                      },
                    )
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
                      let updated =
                        B2BCompanyLocationRecord(..location, data: data)
                      let #(updated, store) =
                        store.upsert_staged_b2b_company_location(store, updated)
                      b2b_types.RootResult(
                        b2b_types.Payload(
                          ..empty_payload([]),
                          addresses: addresses,
                        ),
                        store,
                        identity,
                        list.append(
                          [updated.id],
                          list.map(addresses, source_id),
                        ),
                      )
                    }
                  }
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
