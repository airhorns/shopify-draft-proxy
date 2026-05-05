//// B2B company domain port.
////
//// Mirrors the local-staging slice from `src/proxy/b2b.ts`: company,
//// contact, location, role, role-assignment, address, staff-assignment, and
//// tax-setting lifecycle roots stage in normalized in-memory state. Welcome
//// email delivery remains outside local support because it has external
//// Shopify side effects.

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

const domain = "b2b"

const default_string_max_length = 255

const notes_max_length = 5000

const external_id_max_length = 64

const external_id_invalid_chars_detail = "external_id_contains_invalid_chars"

const external_id_invalid_chars_message = "External Id can only contain numbers, letters, and some special characters, including !@#$%^&*(){}[]\\/?<>_-~,.;:'`\""

const company_contact_maximum_cap = 10_000

pub type B2BError {
  ParseFailed(root_field.RootFieldError)
}

type UserError {
  UserError(
    field: Option(List(String)),
    message: String,
    code: user_error_code.Code,
    detail: Option(String),
  )
}

type Payload {
  Payload(
    company: Option(B2BCompanyRecord),
    company_contact: Option(B2BCompanyContactRecord),
    company_location: Option(B2BCompanyLocationRecord),
    company_contact_role_assignment: Option(SourceValue),
    role_assignments: List(SourceValue),
    addresses: List(SourceValue),
    company_location_staff_member_assignments: List(SourceValue),
    deleted_company_id: Option(String),
    deleted_company_ids: List(String),
    deleted_company_contact_id: Option(String),
    deleted_company_contact_ids: List(String),
    deleted_company_location_id: Option(String),
    deleted_company_location_ids: List(String),
    deleted_address_id: Option(String),
    revoked_company_contact_role_assignment_id: Option(String),
    revoked_role_assignment_ids: List(String),
    deleted_company_location_staff_member_assignment_ids: List(String),
    removed_company_contact_id: Option(String),
    user_errors: List(UserError),
  )
}

type RootResult {
  RootResult(
    payload: Payload,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_ids: List(String),
  )
}

pub fn is_b2b_query_root(name: String) -> Bool {
  case name {
    "companies"
    | "companiesCount"
    | "company"
    | "companyContact"
    | "companyContactRole"
    | "companyLocation"
    | "companyLocations" -> True
    _ -> False
  }
}

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
/// flows stay on the in-memory B2B model.
pub fn local_has_b2b_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id) || local_b2b_id_known(proxy.store, id)
      _ -> False
    }
  })
}

fn local_b2b_id_known(store: Store, id: String) -> Bool {
  case store.get_effective_b2b_company_by_id(store, id) {
    Some(_) -> True
    None ->
      case store.get_effective_b2b_company_contact_by_id(store, id) {
        Some(_) -> True
        None ->
          case store.get_effective_b2b_company_contact_role_by_id(store, id) {
            Some(_) -> True
            None ->
              case store.get_effective_b2b_company_location_by_id(store, id) {
                Some(_) -> True
                None -> local_b2b_id_deleted(store, id)
              }
          }
      }
  }
}

fn local_b2b_id_deleted(store: Store, id: String) -> Bool {
  dict.has_key(store.staged_state.deleted_b2b_company_ids, id)
  || dict.has_key(store.staged_state.deleted_b2b_company_contact_ids, id)
  || dict.has_key(store.staged_state.deleted_b2b_company_contact_role_ids, id)
  || dict.has_key(store.staged_state.deleted_b2b_company_location_ids, id)
}

/// True iff any B2B record or deletion has been staged locally, or any
/// variable carries a proxy-synthetic gid. Connection and aggregate
/// reads must stay local once a B2B lifecycle scenario has staged state.
pub fn local_has_staged_b2b(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let has_synthetic =
    dict.values(variables)
    |> list.any(fn(value) {
      case value {
        root_field.StringVal(s) -> is_proxy_synthetic_gid(s)
        _ -> False
      }
    })
  has_synthetic
  || dict.size(proxy.store.staged_state.b2b_companies) > 0
  || dict.size(proxy.store.staged_state.deleted_b2b_company_ids) > 0
  || dict.size(proxy.store.staged_state.b2b_company_contacts) > 0
  || dict.size(proxy.store.staged_state.deleted_b2b_company_contact_ids) > 0
  || dict.size(proxy.store.staged_state.b2b_company_contact_roles) > 0
  || dict.size(proxy.store.staged_state.deleted_b2b_company_contact_role_ids)
  > 0
  || dict.size(proxy.store.staged_state.b2b_company_locations) > 0
  || dict.size(proxy.store.staged_state.deleted_b2b_company_location_ids) > 0
}

/// Pattern 1: cold LiveHybrid B2B reads forward upstream verbatim because
/// the local handler has no base-state catalog to merge. Snapshot mode and
/// any request touching local/synthetic B2B state continue through the
/// in-memory handler.
fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "company" ->
      !local_has_b2b_id(proxy, variables)
    parse_operation.QueryOperation, "companyContact" ->
      !local_has_b2b_id(proxy, variables)
    parse_operation.QueryOperation, "companyContactRole" ->
      !local_has_b2b_id(proxy, variables)
    parse_operation.QueryOperation, "companyLocation" ->
      !local_has_b2b_id(proxy, variables)
    parse_operation.QueryOperation, "companies" ->
      !local_has_staged_b2b(proxy, variables)
    parse_operation.QueryOperation, "companiesCount" ->
      !local_has_staged_b2b(proxy, variables)
    parse_operation.QueryOperation, "companyLocations" ->
      !local_has_staged_b2b(proxy, variables)
    _, _ -> False
  }
}

pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case process(proxy.store, document, variables) {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #("message", json.string("Failed to handle B2B query")),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, B2BError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let data =
    fields
    |> list.map(fn(field) {
      let key = get_field_response_key(field)
      #(key, query_field(store, field, fragments, variables))
    })
    |> json.object
  Ok(json.object([#("data", data)]))
}

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
                  domain,
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

fn empty_payload(errors: List(UserError)) -> Payload {
  Payload(
    company: None,
    company_contact: None,
    company_location: None,
    company_contact_role_assignment: None,
    role_assignments: [],
    addresses: [],
    company_location_staff_member_assignments: [],
    deleted_company_id: None,
    deleted_company_ids: [],
    deleted_company_contact_id: None,
    deleted_company_contact_ids: [],
    deleted_company_location_id: None,
    deleted_company_location_ids: [],
    deleted_address_id: None,
    revoked_company_contact_role_assignment_id: None,
    revoked_role_assignment_ids: [],
    deleted_company_location_staff_member_assignment_ids: [],
    removed_company_contact_id: None,
    user_errors: errors,
  )
}

fn status_for(result: RootResult) -> store.EntryStatus {
  case result.payload.user_errors, result.staged_ids {
    [], [_, ..] -> store.Staged
    [], [] -> store.Staged
    _, _ -> store.Failed
  }
}

fn should_log_result(result: RootResult) -> Bool {
  !is_empty_input_result(result)
}

fn is_empty_input_result(result: RootResult) -> Bool {
  case result.staged_ids, result.payload.user_errors {
    [], [error] ->
      error.code == user_error_code.no_input
      || error == company_update_empty_input_error()
    _, _ -> False
  }
}

fn query_field(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "companies" ->
          serialize_company_connection(
            store,
            field,
            store.list_effective_b2b_companies(store),
            fragments,
            variables,
          )
        "companiesCount" ->
          serialize_count(
            field,
            list.length(store.list_effective_b2b_companies(store)),
          )
        "company" ->
          case read_id_arg(field, variables) {
            Some(id) ->
              case store.get_effective_b2b_company_by_id(store, id) {
                Some(company) ->
                  serialize_company(store, company, field, fragments, variables)
                None -> json.null()
              }
            None -> json.null()
          }
        "companyContact" ->
          case read_id_arg(field, variables) {
            Some(id) ->
              case store.get_effective_b2b_company_contact_by_id(store, id) {
                Some(contact) ->
                  serialize_contact(store, contact, field, fragments)
                None -> json.null()
              }
            None -> json.null()
          }
        "companyContactRole" ->
          case read_id_arg(field, variables) {
            Some(id) ->
              case
                store.get_effective_b2b_company_contact_role_by_id(store, id)
              {
                Some(role) ->
                  project_source(role_source(role), field, fragments)
                None -> json.null()
              }
            None -> json.null()
          }
        "companyLocation" ->
          case read_id_arg(field, variables) {
            Some(id) ->
              case store.get_effective_b2b_company_location_by_id(store, id) {
                Some(location) ->
                  serialize_location(store, location, field, fragments)
                None -> json.null()
              }
            None -> json.null()
          }
        "companyLocations" ->
          serialize_location_connection(
            store,
            field,
            store.list_effective_b2b_company_locations(store),
            fragments,
            variables,
          )
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn read_id_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  read_string(graphql_helpers.field_args(field, variables), "id")
}

fn read_string(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(args, key) {
    Ok(root_field.StringVal(value)) ->
      case value {
        "" -> None
        _ -> Some(value)
      }
    _ -> None
  }
}

fn read_bool(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(args, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
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

fn read_object(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(args, key) {
    Ok(root_field.ObjectVal(value)) -> value
    _ -> dict.new()
  }
}

fn read_object_list(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, key) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.ObjectVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn selected_children(field: Selection) -> List(Selection) {
  get_selected_child_fields(field, default_selected_field_options())
}

fn project_source(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(source, selected_children(field), fragments)
}

fn value_to_source(value: StorePropertyValue) -> SourceValue {
  case value {
    StorePropertyNull -> SrcNull
    StorePropertyString(value) -> SrcString(value)
    StorePropertyBool(value) -> SrcBool(value)
    StorePropertyInt(value) -> SrcInt(value)
    StorePropertyFloat(value) -> SrcFloat(value)
    StorePropertyList(values) -> SrcList(list.map(values, value_to_source))
    StorePropertyObject(fields) -> data_to_source(fields)
  }
}

fn source_to_value(value: SourceValue) -> StorePropertyValue {
  case value {
    SrcNull -> StorePropertyNull
    SrcString(value) -> StorePropertyString(value)
    SrcBool(value) -> StorePropertyBool(value)
    SrcInt(value) -> StorePropertyInt(value)
    SrcFloat(value) -> StorePropertyFloat(value)
    SrcList(values) -> StorePropertyList(list.map(values, source_to_value))
    SrcObject(fields) ->
      StorePropertyObject(
        dict.to_list(fields)
        |> list.map(fn(pair) { #(pair.0, source_to_value(pair.1)) })
        |> dict.from_list,
      )
  }
}

fn data_to_source(data: Dict(String, StorePropertyValue)) -> SourceValue {
  SrcObject(
    dict.to_list(data)
    |> list.map(fn(pair) { #(pair.0, value_to_source(pair.1)) })
    |> dict.from_list,
  )
}

fn data_get(
  data: Dict(String, StorePropertyValue),
  key: String,
) -> SourceValue {
  case dict.get(data, key) {
    Ok(value) -> value_to_source(value)
    Error(_) -> SrcNull
  }
}

fn put_source(
  data: Dict(String, StorePropertyValue),
  key: String,
  value: SourceValue,
) -> Dict(String, StorePropertyValue) {
  dict.insert(data, key, source_to_value(value))
}

fn maybe_put_string(
  data: Dict(String, StorePropertyValue),
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, StorePropertyValue) {
  case dict.get(args, key) {
    Ok(root_field.StringVal(value)) ->
      dict.insert(data, key, StorePropertyString(value))
    Ok(root_field.NullVal) -> dict.insert(data, key, StorePropertyNull)
    _ -> data
  }
}

fn maybe_put_bool(
  data: Dict(String, StorePropertyValue),
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, StorePropertyValue) {
  case dict.get(args, key) {
    Ok(root_field.BoolVal(value)) ->
      dict.insert(data, key, StorePropertyBool(value))
    Ok(root_field.NullVal) -> dict.insert(data, key, StorePropertyNull)
    _ -> data
  }
}

fn record_source(
  typename: String,
  id: String,
  data: Dict(String, StorePropertyValue),
) -> SourceValue {
  case data_to_source(data) {
    SrcObject(fields) ->
      SrcObject(
        fields
        |> dict.insert("__typename", SrcString(typename))
        |> dict.insert("id", SrcString(id)),
      )
    other -> other
  }
}

fn company_source(company: B2BCompanyRecord) -> SourceValue {
  record_source("Company", company.id, company.data)
}

fn contact_source(contact: B2BCompanyContactRecord) -> SourceValue {
  record_source("CompanyContact", contact.id, contact.data)
}

fn contact_source_with_main_flag(
  store: Store,
  contact: B2BCompanyContactRecord,
) -> SourceValue {
  case contact_source(contact) {
    SrcObject(fields) ->
      SrcObject(dict.insert(
        fields,
        "isMainContact",
        SrcBool(contact_is_main_contact(store, contact)),
      ))
    source -> source
  }
}

fn role_source(role: B2BCompanyContactRoleRecord) -> SourceValue {
  record_source("CompanyContactRole", role.id, role.data)
}

fn location_source(location: B2BCompanyLocationRecord) -> SourceValue {
  record_source("CompanyLocation", location.id, location.data)
}

fn source_field(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(field)
  case field {
    Field(
      name: name,
      selection_set: Some(SelectionSet(selections: selections, ..)),
      ..,
    ) ->
      case source {
        SrcObject(fields) -> #(
          key,
          project_graphql_value(
            dict.get(fields, name.value) |> result.unwrap(SrcNull),
            selections,
            fragments,
          ),
        )
        _ -> #(key, json.null())
      }
    Field(name: name, ..) ->
      case source {
        SrcObject(fields) -> #(
          key,
          source_to_json(dict.get(fields, name.value) |> result.unwrap(SrcNull)),
        )
        _ -> #(key, json.null())
      }
    _ -> #(key, json.null())
  }
}

fn serialize_count(field: Selection, count: Int) -> Json {
  json.object(
    selected_children(field)
    |> list.map(fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "count" -> #(key, json.int(count))
            "precision" -> #(key, json.string("EXACT"))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_company(
  store: Store,
  company: B2BCompanyRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let contacts = company_contacts(store, company)
  let locations = company_locations(store, company)
  let roles = company_roles(store, company)
  let source = company_source(company)
  json.object(
    selected_children(field)
    |> list.map(fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "contacts" -> #(
              key,
              serialize_contact_connection(
                store,
                child,
                contacts,
                fragments,
                variables,
              ),
            )
            "locations" -> #(
              key,
              serialize_location_connection(
                store,
                child,
                locations,
                fragments,
                variables,
              ),
            )
            "contactRoles" -> #(
              key,
              serialize_role_connection(child, roles, fragments, variables),
            )
            "contactsCount" -> #(
              key,
              serialize_count(child, list.length(contacts)),
            )
            "locationsCount" -> #(
              key,
              serialize_count(child, list.length(locations)),
            )
            "mainContact" -> #(key, case company.main_contact_id {
              Some(contact_id) ->
                case
                  store.get_effective_b2b_company_contact_by_id(
                    store,
                    contact_id,
                  )
                {
                  Some(contact) if contact.company_id == company.id ->
                    serialize_contact(store, contact, child, fragments)
                  _ -> json.null()
                }
              None -> json.null()
            })
            "defaultRole" -> #(key, case roles {
              [role, ..] -> project_source(role_source(role), child, fragments)
              [] -> json.null()
            })
            "orders" | "draftOrders" | "events" -> #(
              key,
              serialize_empty_connection(
                child,
                default_selected_field_options(),
              ),
            )
            "metafields" -> #(
              key,
              serialize_company_metafields_connection(
                store,
                company.id,
                child,
                variables,
              ),
            )
            "ordersCount" -> #(key, serialize_count(child, 0))
            "metafield" -> #(
              key,
              serialize_company_metafield(store, company.id, child, variables),
            )
            _ -> source_field(source, child, fragments)
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_company_metafield(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let namespace = read_string(args, "namespace")
  let key = read_string(args, "key")
  let found =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.find(fn(metafield) {
      metafield.namespace == option.unwrap(namespace, "")
      && metafield.key == option.unwrap(key, "")
    })
    |> option.from_result
  case found {
    Some(metafield) ->
      metafields.serialize_metafield_selection(
        company_metafield_to_core(metafield),
        field,
        default_selected_field_options(),
      )
    None -> json.null()
  }
}

fn serialize_company_metafields_connection(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let namespace = read_string(args, "namespace")
  let records =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.filter(fn(metafield) {
      case namespace {
        Some(ns) -> metafield.namespace == ns
        None -> True
      }
    })
    |> list.map(company_metafield_to_core)
  metafields.serialize_metafields_connection(
    records,
    field,
    variables,
    default_selected_field_options(),
  )
}

fn company_metafield_to_core(
  record: ProductMetafieldRecord,
) -> metafields.MetafieldRecordCore {
  metafields.MetafieldRecordCore(
    id: record.id,
    namespace: record.namespace,
    key: record.key,
    type_: record.type_,
    value: record.value,
    compare_digest: record.compare_digest,
    json_value: record.json_value,
    created_at: record.created_at,
    updated_at: record.updated_at,
    owner_type: record.owner_type,
  )
}

fn serialize_contact(
  store: Store,
  contact: B2BCompanyContactRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = contact_source_with_main_flag(store, contact)
  json.object(
    selected_children(field)
    |> list.map(fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "company" -> #(
              key,
              case
                store.get_effective_b2b_company_by_id(store, contact.company_id)
              {
                Some(company) ->
                  serialize_company(
                    store,
                    company,
                    child,
                    fragments,
                    dict.new(),
                  )
                None -> json.null()
              },
            )
            "roleAssignments" -> #(
              key,
              serialize_source_connection(
                child,
                read_object_sources(data_get(contact.data, "roleAssignments")),
                dict.new(),
                fn(item, node_field, _index) {
                  serialize_role_assignment(store, item, node_field, fragments)
                },
              ),
            )
            "orders" | "draftOrders" -> #(
              key,
              serialize_empty_connection(
                child,
                default_selected_field_options(),
              ),
            )
            "customer" -> #(
              key,
              project_graphql_value(
                data_get(contact.data, "customer"),
                selected_children(child),
                fragments,
              ),
            )
            "isMainContact" -> #(
              key,
              json.bool(contact_is_main_contact(store, contact)),
            )
            "note" ->
              source_field(
                src_object([#("note", contact_notes_source(contact))]),
                child,
                fragments,
              )
            "notes" ->
              source_field(
                src_object([#("notes", contact_notes_source(contact))]),
                child,
                fragments,
              )
            _ -> source_field(source, child, fragments)
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn contact_notes_source(contact: B2BCompanyContactRecord) -> SourceValue {
  case data_get(contact.data, "notes") {
    SrcNull -> data_get(contact.data, "note")
    other -> other
  }
}

fn serialize_location(
  store: Store,
  location: B2BCompanyLocationRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = location_source(location)
  json.object(
    selected_children(field)
    |> list.map(fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "company" -> #(
              key,
              case
                store.get_effective_b2b_company_by_id(
                  store,
                  location.company_id,
                )
              {
                Some(company) ->
                  serialize_company(
                    store,
                    company,
                    child,
                    fragments,
                    dict.new(),
                  )
                None -> json.null()
              },
            )
            "roleAssignments" -> #(
              key,
              serialize_source_connection(
                child,
                read_object_sources(data_get(location.data, "roleAssignments")),
                dict.new(),
                fn(item, node_field, _index) {
                  serialize_role_assignment(store, item, node_field, fragments)
                },
              ),
            )
            "staffMemberAssignments" -> #(
              key,
              serialize_source_connection(
                child,
                read_object_sources(data_get(
                  location.data,
                  "staffMemberAssignments",
                )),
                dict.new(),
                fn(item, node_field, _index) {
                  project_graphql_value(
                    item,
                    selected_children(node_field),
                    fragments,
                  )
                },
              ),
            )
            "orders" | "draftOrders" | "events" | "catalogs" | "metafields" -> #(
              key,
              serialize_empty_connection(
                child,
                default_selected_field_options(),
              ),
            )
            "catalogsCount" | "ordersCount" -> #(key, serialize_count(child, 0))
            "billingAddress" | "shippingAddress" -> #(
              key,
              project_graphql_value(
                data_get(location.data, name.value),
                selected_children(child),
                fragments,
              ),
            )
            "taxSettings" -> #(
              key,
              serialize_tax_settings(location, child, fragments),
            )
            "metafield" -> #(key, json.null())
            _ -> source_field(source, child, fragments)
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_tax_settings(
  location: B2BCompanyLocationRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let tax_settings = case data_get(location.data, "taxSettings") {
    SrcObject(fields) -> fields
    _ -> dict.new()
  }
  let source =
    src_object([
      #("__typename", SrcString("CompanyLocationTaxSettings")),
      #(
        "taxRegistrationId",
        dict.get(tax_settings, "taxRegistrationId")
          |> result.unwrap(data_get(location.data, "taxRegistrationId")),
      ),
      #(
        "taxExempt",
        dict.get(tax_settings, "taxExempt")
          |> result.unwrap(data_get(location.data, "taxExempt")),
      ),
      #(
        "taxExemptions",
        dict.get(tax_settings, "taxExemptions")
          |> result.unwrap(data_get(location.data, "taxExemptions")),
      ),
    ])
  project_source(source, field, fragments)
}

fn serialize_company_connection(
  store: Store,
  field: Selection,
  companies: List(B2BCompanyRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let window =
    paginate_connection_items(
      filter_companies_by_query(
        companies,
        graphql_helpers.field_args(field, variables),
      ),
      field,
      variables,
      fn(company, _index) {
        case company.cursor {
          Some(cursor) -> cursor
          None -> company.id
        }
      },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(company, _index) {
        case company.cursor {
          Some(cursor) -> cursor
          None -> company.id
        }
      },
      serialize_node: fn(company, node_field, _index) {
        serialize_company(store, company, node_field, fragments, variables)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_contact_connection(
  store: Store,
  field: Selection,
  contacts: List(B2BCompanyContactRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let window = paginate_records(contacts, field, variables, fn(c) { c.id })
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(contact, _index) { contact.id },
      serialize_node: fn(contact, node_field, _index) {
        serialize_contact(store, contact, node_field, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_role_connection(
  field: Selection,
  roles: List(B2BCompanyContactRoleRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let window = paginate_records(roles, field, variables, fn(r) { r.id })
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(role, _index) { role.id },
      serialize_node: fn(role, node_field, _index) {
        project_source(role_source(role), node_field, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_location_connection(
  store: Store,
  field: Selection,
  locations: List(B2BCompanyLocationRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let window = paginate_records(locations, field, variables, fn(l) { l.id })
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(location, _index) { location.id },
      serialize_node: fn(location, node_field, _index) {
        serialize_location(store, location, node_field, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_source_connection(
  field: Selection,
  items: List(SourceValue),
  variables: Dict(String, root_field.ResolvedValue),
  serialize_node: fn(SourceValue, Selection, Int) -> Json,
) -> Json {
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      fn(item, _index) { source_id(item) },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(item, _index) { source_id(item) },
      serialize_node: serialize_node,
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn paginate_records(
  records: List(a),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  get_id: fn(a) -> String,
) -> ConnectionWindow(a) {
  paginate_connection_items(
    records,
    field,
    variables,
    fn(record, _index) { get_id(record) },
    default_connection_window_options(),
  )
}

fn source_id(value: SourceValue) -> String {
  case value {
    SrcObject(fields) ->
      case dict.get(fields, "id") {
        Ok(SrcString(id)) -> id
        _ -> ""
      }
    _ -> ""
  }
}

fn read_object_sources(value: SourceValue) -> List(SourceValue) {
  case value {
    SrcList(items) ->
      list.filter(items, fn(item) {
        case item {
          SrcObject(_) -> True
          _ -> False
        }
      })
    _ -> []
  }
}

fn company_contacts(store: Store, company: B2BCompanyRecord) {
  company.contact_ids
  |> list.filter_map(fn(id) {
    case store.get_effective_b2b_company_contact_by_id(store, id) {
      Some(contact) -> Ok(contact)
      None -> Error(Nil)
    }
  })
}

fn contact_customer_id(contact: B2BCompanyContactRecord) -> Option(String) {
  case dict.get(contact.data, "customerId") {
    Ok(StorePropertyString(customer_id)) -> Some(customer_id)
    _ -> None
  }
}

fn find_company_contact_by_customer_id(
  contacts: List(B2BCompanyContactRecord),
  customer_id: String,
) -> Option(B2BCompanyContactRecord) {
  contacts
  |> list.find(fn(contact) { contact_customer_id(contact) == Some(customer_id) })
  |> option_from_result
}

fn customer_email(customer: CustomerRecord) -> Option(String) {
  case customer.email {
    Some(email) -> {
      let trimmed = string.trim(email)
      case trimmed == "" {
        True -> None
        False -> Some(email)
      }
    }
    None -> None
  }
}

fn customer_contact_source(
  customer: CustomerRecord,
  email: String,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("Customer")),
    #("id", SrcString(customer.id)),
    #("email", SrcString(email)),
    #("firstName", customer.first_name |> optional_src_string),
    #("lastName", customer.last_name |> optional_src_string),
  ])
}

fn company_contact_cap_reached(company: B2BCompanyRecord) -> Bool {
  list.length(company.contact_ids) >= company_contact_maximum_cap
}

fn company_contact_cap_error() -> UserError {
  user_error(
    Some(["companyId"]),
    "Company contact maximum cap reached.",
    user_error_code.company_contact_max_cap_reached,
  )
}

fn company_contact_mutation_error(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: List(String),
  message: String,
  code: user_error_code.Code,
) -> RootResult {
  RootResult(
    Payload(
      ..empty_payload([
        user_error(Some(field), message, code),
      ]),
      company_contact: None,
    ),
    store,
    identity,
    [],
  )
}

fn company_locations(store: Store, company: B2BCompanyRecord) {
  company.location_ids
  |> list.filter_map(fn(id) {
    case store.get_effective_b2b_company_location_by_id(store, id) {
      Some(location) -> Ok(location)
      None -> Error(Nil)
    }
  })
}

fn company_roles(store: Store, company: B2BCompanyRecord) {
  company.contact_role_ids
  |> list.filter_map(fn(id) {
    case store.get_effective_b2b_company_contact_role_by_id(store, id) {
      Some(role) -> Ok(role)
      None -> Error(Nil)
    }
  })
}

fn contact_is_main_contact(
  store: Store,
  contact: B2BCompanyContactRecord,
) -> Bool {
  case store.get_effective_b2b_company_by_id(store, contact.company_id) {
    Some(company) -> company.main_contact_id == Some(contact.id)
    None -> False
  }
}

fn option_from_result(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(x) -> Some(x)
    Error(_) -> None
  }
}

fn append_unique(items: List(String), value: String) -> List(String) {
  case list.contains(items, value) {
    True -> items
    False -> list.append(items, [value])
  }
}

fn has_duplicate_strings(items: List(String)) -> Bool {
  has_duplicate_strings_loop(items, [])
}

fn has_duplicate_strings_loop(items: List(String), seen: List(String)) -> Bool {
  case items {
    [] -> False
    [first, ..rest] ->
      case list.contains(seen, first) {
        True -> True
        False -> has_duplicate_strings_loop(rest, [first, ..seen])
      }
  }
}

fn remove_string(items: List(String), value: String) -> List(String) {
  list.filter(items, fn(item) { item != value })
}

fn filter_companies_by_query(
  companies: List(B2BCompanyRecord),
  args: Dict(String, root_field.ResolvedValue),
) -> List(B2BCompanyRecord) {
  case read_string(args, "query") {
    None -> companies
    Some(raw) -> {
      let q = string.lowercase(raw)
      companies
      |> list.filter(fn(company) {
        let name =
          source_string(data_get(company.data, "name")) |> string.lowercase
        let external_id =
          source_string(data_get(company.data, "externalId"))
          |> string.lowercase
        string.contains(name, q)
        || string.contains(external_id, q)
        || string.contains(string.lowercase(company.id), q)
      })
    }
  }
}

fn source_string(value: SourceValue) -> String {
  case value {
    SrcString(value) -> value
    _ -> ""
  }
}

fn user_error(
  field: Option(List(String)),
  message: String,
  code: user_error_code.Code,
) {
  UserError(field: field, message: message, code: code, detail: None)
}

fn detailed_user_error(
  field: Option(List(String)),
  message: String,
  code: user_error_code.Code,
  detail: String,
) {
  UserError(field: field, message: message, code: code, detail: Some(detail))
}

fn field_path(prefix: List(String), field: String) -> List(String) {
  list.append(prefix, [field])
}

fn indexed_field_path(field: String, index: Int) -> List(String) {
  [field, int.to_string(index)]
}

fn indexed_nested_field_path(
  list_field: String,
  index: Int,
  field: String,
) -> List(String) {
  [list_field, int.to_string(index), field]
}

fn validate_length(
  value: String,
  field: String,
  prefix: List(String),
  label: String,
  max: Int,
) -> List(UserError) {
  case string.length(value) > max {
    True -> [
      user_error(
        Some(field_path(prefix, field)),
        label
          <> " is too long (maximum is "
          <> int.to_string(max)
          <> " characters)",
        user_error_code.too_long,
      ),
    ]
    False -> []
  }
}

fn validate_html(
  value: String,
  field: String,
  prefix: List(String),
  label: String,
) -> List(UserError) {
  case contains_html_tags(value) {
    True -> [
      user_error(
        Some(field_path(prefix, field)),
        label <> " contains HTML tags",
        user_error_code.contains_html_tags,
      ),
    ]
    False -> []
  }
}

fn validate_text_field(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  error_field: String,
  prefix: List(String),
  label: String,
  max: Int,
  reject_html: Bool,
) -> List(UserError) {
  case read_string(input, field) {
    Some(value) -> {
      let html_errors = case reject_html {
        True -> validate_html(value, error_field, prefix, label)
        False -> []
      }
      html_errors
      |> list.append(validate_length(value, error_field, prefix, label, max))
    }
    None -> []
  }
}

fn validate_external_id_field(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> List(UserError) {
  case read_string(input, "externalId") {
    Some(value) -> {
      validate_external_id_length(value, prefix)
      |> list.append(validate_external_id_charset(value, prefix))
    }
    None -> []
  }
}

fn validate_external_id_length(
  value: String,
  prefix: List(String),
) -> List(UserError) {
  case string.length(value) > external_id_max_length {
    True -> [
      user_error(
        Some(field_path(prefix, "externalId")),
        "External Id must be "
          <> int.to_string(external_id_max_length)
          <> " characters or less.",
        user_error_code.too_long,
      ),
    ]
    False -> []
  }
}

fn validate_external_id_charset(
  value: String,
  prefix: List(String),
) -> List(UserError) {
  case value |> string.to_graphemes |> list.all(external_id_char_allowed) {
    True -> []
    False -> [
      detailed_user_error(
        Some(field_path(prefix, "externalId")),
        external_id_invalid_chars_message,
        user_error_code.invalid,
        external_id_invalid_chars_detail,
      ),
    ]
  }
}

fn external_id_char_allowed(char: String) -> Bool {
  string.contains(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*(){}[]\\/?<>_-~.,;:'\"`",
    char,
  )
}

fn value_is_present(value: root_field.ResolvedValue) -> Bool {
  case value {
    root_field.NullVal -> False
    root_field.StringVal(value) -> string.trim(value) != ""
    root_field.ListVal(items) -> list.any(items, value_is_present)
    root_field.ObjectVal(fields) ->
      fields
      |> dict.to_list
      |> list.any(fn(entry) { value_is_present(entry.1) })
    _ -> True
  }
}

fn has_non_empty_object_field(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Bool {
  case dict.get(input, field) {
    Ok(root_field.ObjectVal(fields)) ->
      fields
      |> dict.to_list
      |> list.any(fn(entry) { value_is_present(entry.1) })
    _ -> False
  }
}

fn has_explicit_null_field(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Bool {
  case dict.get(input, field) {
    Ok(root_field.NullVal) -> True
    _ -> False
  }
}

fn has_any_non_null_input(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  input
  |> dict.to_list
  |> list.any(fn(entry) {
    case entry.1 {
      root_field.NullVal -> False
      _ -> True
    }
  })
}

fn no_input_error() -> UserError {
  user_error(Some(["input"]), "No input provided.", user_error_code.no_input)
}

fn contact_create_empty_input_error() -> UserError {
  user_error(
    None,
    "Company contact create input is empty.",
    user_error_code.no_input,
  )
}

fn company_update_empty_input_error() -> UserError {
  user_error(
    Some(["input"]),
    "At least one attribute to change must be present",
    user_error_code.invalid,
  )
}

fn contact_update_empty_input_error() -> UserError {
  user_error(
    None,
    "Company contact update input is empty.",
    user_error_code.no_input,
  )
}

fn location_update_empty_input_error() -> UserError {
  user_error(
    None,
    "Company location update input is empty.",
    user_error_code.no_input,
  )
}

fn validate_billing_same_as_shipping(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> List(UserError) {
  let billing_address_present =
    has_non_empty_object_field(input, "billingAddress")
  case read_bool(input, "billingSameAsShipping") {
    Some(True) if billing_address_present -> [
      user_error(
        Some(field_path(prefix, "billingAddress")),
        "Invalid input.",
        user_error_code.invalid_input,
      ),
    ]
    Some(False) if !billing_address_present -> [
      user_error(
        Some(field_path(prefix, "billingAddress")),
        "Billing address can't be blank when billingSameAsShipping is false",
        user_error_code.invalid_input,
      ),
    ]
    _ -> []
  }
}

fn validate_tax_exempt_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> List(UserError) {
  case has_explicit_null_field(input, "taxExempt") {
    True -> [
      user_error(
        Some(field_path(prefix, "taxExempt")),
        "Invalid input.",
        user_error_code.invalid_input,
      ),
    ]
    False -> []
  }
}

fn sanitize_name_field(
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case read_string(input, "name") {
    Some(value) ->
      dict.insert(input, "name", root_field.StringVal(strip_html(value)))
    None -> input
  }
}

fn validate_company_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> #(Dict(String, root_field.ResolvedValue), List(UserError)) {
  let input = sanitize_name_field(input)
  let errors =
    validate_text_field(
      input,
      "name",
      "name",
      prefix,
      "Name",
      default_string_max_length,
      False,
    )
    |> list.append(validate_text_field(
      input,
      "note",
      "notes",
      prefix,
      "Notes",
      notes_max_length,
      True,
    ))
    |> list.append(validate_external_id_field(input, prefix))
  #(input, errors)
}

fn validate_contact_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> #(Dict(String, root_field.ResolvedValue), List(UserError)) {
  let errors =
    validate_text_field(
      input,
      "title",
      "title",
      prefix,
      "Title",
      default_string_max_length,
      True,
    )
    |> list.append(validate_text_field(
      input,
      "note",
      "notes",
      prefix,
      "Notes",
      notes_max_length,
      True,
    ))
    |> list.append(validate_text_field(
      input,
      "notes",
      "notes",
      prefix,
      "Notes",
      notes_max_length,
      True,
    ))
  #(input, errors)
}

fn validate_location_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> #(Dict(String, root_field.ResolvedValue), List(UserError)) {
  let input = sanitize_name_field(input)
  let errors =
    validate_text_field(
      input,
      "name",
      "name",
      prefix,
      "Name",
      default_string_max_length,
      False,
    )
    |> list.append(validate_text_field(
      input,
      "note",
      "notes",
      prefix,
      "Notes",
      notes_max_length,
      True,
    ))
    |> list.append(validate_billing_same_as_shipping(input, prefix))
    |> list.append(validate_tax_exempt_input(input, prefix))
    |> list.append(validate_external_id_field(input, prefix))
  #(input, errors)
}

fn contains_html_tags(value: String) -> Bool {
  contains_html_tag_loop(string.to_graphemes(value))
}

fn contains_html_tag_loop(graphemes: List(String)) -> Bool {
  case graphemes {
    [] -> False
    ["<", next, ..rest] ->
      case is_html_tag_start(next) && contains_tag_close(rest) {
        True -> True
        False -> contains_html_tag_loop([next, ..rest])
      }
    [_, ..rest] -> contains_html_tag_loop(rest)
  }
}

fn contains_tag_close(graphemes: List(String)) -> Bool {
  case graphemes {
    [] -> False
    ["<", ..] -> False
    [">", ..] -> True
    [_, ..rest] -> contains_tag_close(rest)
  }
}

fn is_html_tag_start(value: String) -> Bool {
  value == "/"
  || value == "!"
  || value == "?"
  || string.contains(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    value,
  )
}

fn strip_html(value: String) -> String {
  strip_html_loop(string.to_graphemes(value), False, [])
}

fn strip_html_loop(
  graphemes: List(String),
  in_tag: Bool,
  acc: List(String),
) -> String {
  case graphemes {
    [] -> string.concat(list.reverse(acc))
    [first, ..rest] ->
      case in_tag, first {
        True, ">" -> strip_html_loop(rest, False, acc)
        True, _ -> strip_html_loop(rest, True, acc)
        False, "<" -> {
          case rest {
            [next, ..after_next] ->
              case is_html_tag_start(next) && contains_tag_close(after_next) {
                True -> strip_html_loop(after_next, True, acc)
                False -> strip_html_loop(rest, False, [first, ..acc])
              }
            _ -> strip_html_loop(rest, False, [first, ..acc])
          }
        }
        False, _ -> strip_html_loop(rest, False, [first, ..acc])
      }
  }
}

fn resource_not_found(field: List(String)) {
  user_error(
    Some(field),
    "Resource requested does not exist.",
    user_error_code.resource_not_found,
  )
}

fn company_role_not_found_at(field: List(String)) {
  user_error(
    Some(field),
    "The company contact role doesn't exist.",
    user_error_code.resource_not_found,
  )
}

fn company_location_not_found_at(field: List(String)) {
  user_error(
    Some(field),
    "The company location doesn't exist.",
    user_error_code.resource_not_found,
  )
}

fn company_contact_does_not_exist_at(field: List(String)) {
  user_error(
    Some(field),
    "Company contact does not exist.",
    user_error_code.resource_not_found,
  )
}

fn company_role_does_not_exist_at(field: List(String)) {
  user_error(
    Some(field),
    "Company role does not exist.",
    user_error_code.resource_not_found,
  )
}

fn one_role_already_assigned_at(field: Option(List(String))) {
  user_error(
    field,
    "Company contact has already been assigned a role in that company location.",
    user_error_code.limit_reached,
  )
}

fn existing_orders_error() {
  existing_orders_error_at(["companyContactId"])
}

fn existing_orders_error_at(field: List(String)) {
  user_error(
    Some(field),
    "Cannot delete a company contact with existing orders or draft orders.",
    user_error_code.failed_to_delete,
  )
}

fn dispatch_mutation_root(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> RootResult {
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
    _ -> RootResult(empty_payload([]), store, identity, [])
  }
}

fn make_gid(
  identity: SyntheticIdentityRegistry,
  typename: String,
) -> #(String, SyntheticIdentityRegistry) {
  synthetic_identity.make_proxy_synthetic_gid(identity, typename)
}

fn timestamp(
  identity: SyntheticIdentityRegistry,
) -> #(String, SyntheticIdentityRegistry) {
  synthetic_identity.make_synthetic_timestamp(identity)
}

fn company_data_from_input(
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

fn contact_data_from_input(
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

fn prepare_contact_create_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> #(Dict(String, root_field.ResolvedValue), List(UserError)) {
  prepare_contact_input(store, ensure_contact_locale(store, input), None, True)
}

fn prepare_contact_update_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  contact_id: String,
) -> #(Dict(String, root_field.ResolvedValue), List(UserError)) {
  prepare_contact_input(store, input, Some(contact_id), False)
}

fn prepare_contact_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_contact_id: Option(String),
  default_locale: Bool,
) -> #(Dict(String, root_field.ResolvedValue), List(UserError)) {
  let input = case default_locale {
    True -> ensure_contact_locale(store, input)
    False -> input
  }
  let input = rename_contact_note_input(input)
  let #(input, phone_errors) = normalize_contact_phone_input(store, input)
  let errors =
    []
    |> list.append(phone_errors)
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

fn ensure_contact_locale(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case dict.has_key(input, "locale") {
    True -> input
    False ->
      dict.insert(input, "locale", root_field.StringVal(primary_locale(store)))
  }
}

fn primary_locale(store: Store) -> String {
  store.list_effective_shop_locales(store, None)
  |> list.find(fn(locale) { locale.primary })
  |> result.map(fn(locale) { locale.locale })
  |> result.unwrap("en")
}

fn rename_contact_note_input(
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(input, "note") {
    Ok(value) -> input |> dict.delete("note") |> dict.insert("notes", value)
    Error(_) -> input
  }
}

fn normalize_contact_phone_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> #(Dict(String, root_field.ResolvedValue), List(UserError)) {
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

fn validate_contact_locale_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case dict.get(input, "locale") {
    Ok(root_field.StringVal(value)) ->
      case valid_locale_format(value) {
        True -> []
        False -> [
          user_error(
            Some(["input", "locale"]),
            "Invalid locale format.",
            user_error_code.invalid,
          ),
        ]
      }
    _ -> []
  }
}

fn validate_contact_notes_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case dict.get(input, "notes") {
    Ok(root_field.StringVal(value)) ->
      case contains_html_tag(value) {
        True -> [
          user_error(
            Some(["input", "note"]),
            "Notes cannot contain HTML tags",
            user_error_code.contains_html_tags,
          ),
        ]
        False -> []
      }
    _ -> []
  }
}

fn validate_contact_duplicate_email(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_contact_id: Option(String),
) -> List(UserError) {
  case read_string(input, "email") {
    Some(email) ->
      case contact_email_exists(store, email, exclude_contact_id) {
        True -> [
          user_error(
            Some(["input", "email"]),
            "Email address has already been taken.",
            user_error_code.taken,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn validate_contact_duplicate_phone(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_contact_id: Option(String),
) -> List(UserError) {
  case read_string(input, "phone") {
    Some(phone) ->
      case contact_phone_exists(store, phone, exclude_contact_id) {
        True -> [
          user_error(
            Some(["input", "phone"]),
            "Phone number has already been taken.",
            user_error_code.taken,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn validate_duplicate_company_external_id(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_company_id: Option(String),
  prefix: List(String),
) -> List(UserError) {
  case read_string(input, "externalId") {
    Some(external_id) ->
      case company_external_id_exists(store, external_id, exclude_company_id) {
        True -> [
          user_error(
            Some(field_path(prefix, "externalId")),
            "External id has already been taken.",
            user_error_code.taken,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn validate_duplicate_location_external_id(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  exclude_location_id: Option(String),
  prefix: List(String),
) -> List(UserError) {
  case read_string(input, "externalId") {
    Some(external_id) ->
      case
        location_external_id_exists(store, external_id, exclude_location_id)
      {
        True -> [
          user_error(
            Some(field_path(prefix, "externalId")),
            "External id has already been taken.",
            user_error_code.taken,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn company_external_id_exists(
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

fn location_external_id_exists(
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

fn contact_email_exists(
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

fn contact_phone_exists(
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

fn normalize_phone(store: Store, phone: String) -> Result(String, Nil) {
  let trimmed = string.trim(phone)
  let digits = digits_only(trimmed)
  case string.starts_with(trimmed, "+") {
    True -> validate_e164_digits(digits)
    False -> {
      let calling_code = country_calling_code(shop_country_code(store))
      let local_digits = case
        string.starts_with(digits, calling_code) && string.length(digits) > 10
      {
        True -> digits
        False -> calling_code <> digits
      }
      validate_e164_digits(local_digits)
    }
  }
}

fn validate_e164_digits(digits: String) -> Result(String, Nil) {
  let length = string.length(digits)
  case length >= 8 && length <= 15 && all_digits(digits) {
    True -> Ok("+" <> digits)
    False -> Error(Nil)
  }
}

fn shop_country_code(store: Store) -> String {
  case store.get_effective_shop(store) {
    Some(shop) ->
      shop.shop_address.country_code_v2
      |> option.map(string.uppercase)
      |> option.unwrap("US")
    None -> "US"
  }
}

fn country_calling_code(country_code: String) -> String {
  case country_code {
    "US" | "CA" -> "1"
    "GB" | "GG" | "IM" | "JE" -> "44"
    "AU" -> "61"
    "NZ" -> "64"
    "FR" -> "33"
    "DE" -> "49"
    "ES" -> "34"
    "IT" -> "39"
    "NL" -> "31"
    "BE" -> "32"
    "CH" -> "41"
    "AT" -> "43"
    "DK" -> "45"
    "FI" -> "358"
    "IE" -> "353"
    "NO" -> "47"
    "SE" -> "46"
    "BR" -> "55"
    "MX" -> "52"
    "JP" -> "81"
    "SG" -> "65"
    _ -> "1"
  }
}

fn digits_only(value: String) -> String {
  case string.pop_grapheme(value) {
    Error(_) -> ""
    Ok(#(grapheme, rest)) ->
      case is_digit_string(grapheme) {
        True -> grapheme <> digits_only(rest)
        False -> digits_only(rest)
      }
  }
}

fn all_digits(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) -> is_digit_string(grapheme) && all_digits(rest)
  }
}

fn is_digit_string(grapheme: String) -> Bool {
  string.contains("0123456789", grapheme)
}

fn valid_locale_format(locale: String) -> Bool {
  case string.split(locale, on: "-") {
    [language, ..subtags] ->
      valid_locale_language(language) && list.all(subtags, valid_locale_subtag)
    _ -> False
  }
}

fn valid_locale_language(language: String) -> Bool {
  let length = string.length(language)
  case length >= 2 && length <= 3 {
    True -> all_alpha(language)
    False -> False
  }
}

fn valid_locale_subtag(subtag: String) -> Bool {
  let length = string.length(subtag)
  length >= 1 && length <= 8 && all_alphanumeric(subtag)
}

fn all_alpha(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) -> is_alpha(grapheme) && all_alpha(rest)
  }
}

fn all_alphanumeric(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Error(_) -> True
    Ok(#(grapheme, rest)) ->
      { is_alpha(grapheme) || is_digit_string(grapheme) }
      && all_alphanumeric(rest)
  }
}

fn is_alpha(grapheme: String) -> Bool {
  string.contains(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    grapheme,
  )
}

fn contains_html_tag(value: String) -> Bool {
  string.contains(value, "<") && string.contains(value, ">")
}

fn address_from_input(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  existing_id: Option(String),
) -> #(SourceValue, SyntheticIdentityRegistry) {
  let #(id, identity) = case existing_id {
    Some(id) -> #(id, identity)
    None -> make_gid(identity, "CompanyAddress")
  }
  #(
    src_object([
      #("__typename", SrcString("CompanyAddress")),
      #("id", SrcString(id)),
      #("address1", read_string(input, "address1") |> optional_src_string),
      #("address2", read_string(input, "address2") |> optional_src_string),
      #("city", read_string(input, "city") |> optional_src_string),
      #("zip", read_string(input, "zip") |> optional_src_string),
      #("recipient", read_string(input, "recipient") |> optional_src_string),
      #("firstName", read_string(input, "firstName") |> optional_src_string),
      #("lastName", read_string(input, "lastName") |> optional_src_string),
      #("phone", read_string(input, "phone") |> optional_src_string),
      #("zoneCode", read_string(input, "zoneCode") |> optional_src_string),
      #("countryCode", read_string(input, "countryCode") |> optional_src_string),
    ]),
    identity,
  )
}

fn optional_src_string(value: Option(String)) -> SourceValue {
  case value {
    Some(value) -> SrcString(value)
    None -> SrcNull
  }
}

fn location_data_from_input(
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

fn address_id(value: SourceValue) -> Option(String) {
  case value {
    SrcObject(fields) ->
      case dict.get(fields, "id") {
        Ok(SrcString(id)) -> Some(id)
        _ -> None
      }
    _ -> None
  }
}

fn refresh_company_counts(company: B2BCompanyRecord) -> B2BCompanyRecord {
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

fn stage_company(
  store: Store,
  company: B2BCompanyRecord,
) -> #(B2BCompanyRecord, Store) {
  store.upsert_staged_b2b_company(store, refresh_company_counts(company))
}

fn create_default_roles(
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

fn create_contact(
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

fn create_location(
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

fn location_create_fallback_name(
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

fn handle_company_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
) -> RootResult {
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
        prepare_contact_create_input(store, raw_contact_input)
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
      RootResult(empty_payload(validation_errors), store, identity, [])
    [], "" ->
      RootResult(
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
      let payload = Payload(..empty_payload([]), company: Some(company))
      RootResult(
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

fn option_string(value: Option(String), fallback: String) -> String {
  case value {
    Some(value) -> value
    None -> fallback
  }
}

fn handle_company_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyId") {
    Some(company_id) ->
      case store.get_effective_b2b_company_by_id(store, company_id) {
        Some(company) -> {
          let raw_input = read_object(args, "input")
          case dict.is_empty(raw_input), has_any_non_null_input(raw_input) {
            True, _ ->
              RootResult(
                empty_payload([company_update_empty_input_error()]),
                store,
                identity,
                [],
              )
            _, False ->
              RootResult(empty_payload([no_input_error()]), store, identity, [])
            _, True -> {
              case reject_customer_since_update(raw_input) {
                [_, ..] as errors ->
                  RootResult(empty_payload(errors), store, identity, [])
                [] -> {
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
                      RootResult(
                        empty_payload(validation_errors),
                        store,
                        identity,
                        [],
                      )
                    [], "" ->
                      RootResult(
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
                      RootResult(
                        Payload(..empty_payload([]), company: Some(updated)),
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

fn reject_customer_since_update(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
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

fn not_found_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field_name: String,
  field_path: List(String),
) -> RootResult {
  let payload = case field_name {
    "company" ->
      Payload(..empty_payload([resource_not_found(field_path)]), company: None)
    "companyContact" ->
      Payload(
        ..empty_payload([resource_not_found(field_path)]),
        company_contact: None,
      )
    "companyLocation" ->
      Payload(
        ..empty_payload([resource_not_found(field_path)]),
        company_location: None,
      )
    _ -> empty_payload([resource_not_found(field_path)])
  }
  RootResult(payload, store, identity, [])
}

fn delete_company_tree(
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

fn handle_company_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_b2b_company_by_id(store, id) {
        Some(_) -> {
          let #(store, ids) = delete_company_tree(store, id)
          RootResult(
            Payload(..empty_payload([]), deleted_company_id: Some(id)),
            store,
            identity,
            ids,
          )
        }
        None ->
          RootResult(
            Payload(
              ..empty_payload([resource_not_found(["id"])]),
              deleted_company_id: None,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      RootResult(
        Payload(
          ..empty_payload([resource_not_found(["id"])]),
          deleted_company_id: None,
        ),
        store,
        identity,
        [],
      )
  }
}

fn handle_companies_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  let #(store, deleted, staged, errors) =
    read_string_list(args, "companyIds")
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
  RootResult(
    Payload(..empty_payload(errors), deleted_company_ids: deleted),
    store,
    identity,
    staged,
  )
}

fn handle_contact_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyId") {
    Some(company_id) ->
      case store.get_effective_b2b_company_by_id(store, company_id) {
        Some(company) -> {
          case company_contact_cap_reached(company) {
            True ->
              RootResult(
                Payload(
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
                  RootResult(
                    Payload(
                      ..empty_payload([contact_create_empty_input_error()]),
                      company_contact: None,
                    ),
                    store,
                    identity,
                    [],
                  )
                _, False ->
                  RootResult(
                    Payload(
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
                      RootResult(
                        Payload(..empty_payload(errors), company_contact: None),
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
                      RootResult(
                        Payload(
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

fn handle_contact_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyContactId") {
    Some(contact_id) ->
      case store.get_effective_b2b_company_contact_by_id(store, contact_id) {
        Some(contact) -> {
          let raw_input = read_object(args, "input")
          case dict.is_empty(raw_input), has_any_non_null_input(raw_input) {
            True, _ ->
              RootResult(
                Payload(
                  ..empty_payload([contact_update_empty_input_error()]),
                  company_contact: None,
                ),
                store,
                identity,
                [],
              )
            _, False ->
              RootResult(
                Payload(
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
                  RootResult(
                    Payload(..empty_payload(errors), company_contact: None),
                    store,
                    identity,
                    [],
                  )
                [] -> {
                  let #(now, identity) = timestamp(identity)
                  let updated =
                    B2BCompanyContactRecord(
                      ..contact,
                      data: contact_data_from_input(input, now, contact.data),
                    )
                  let #(updated, store) =
                    store.upsert_staged_b2b_company_contact(store, updated)
                  RootResult(
                    Payload(..empty_payload([]), company_contact: Some(updated)),
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
          RootResult(
            Payload(
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
      RootResult(
        Payload(
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

fn delete_contact(store: Store, contact_id: String) -> #(Store, List(String)) {
  case store.get_effective_b2b_company_contact_by_id(store, contact_id) {
    None -> #(store, [])
    Some(contact) -> {
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
      #(store, [contact_id, contact.company_id])
    }
  }
}

fn contact_has_associated_orders(
  store: Store,
  contact: B2BCompanyContactRecord,
) -> Bool {
  contact_has_associated_order_marker(contact)
  || contact_has_staged_order_history(store, contact.id)
}

fn contact_has_associated_order_marker(
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

fn contact_has_staged_order_history(store: Store, contact_id: String) -> Bool {
  list.any(store.list_effective_orders(store), fn(order) {
    purchasing_entity_contact_id(order.data) == Some(contact_id)
  })
  || list.any(store.list_effective_draft_orders(store), fn(draft_order) {
    completed_draft_order_references_contact(draft_order.data, contact_id)
  })
}

fn completed_draft_order_references_contact(
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

fn purchasing_entity_contact_id(data: CapturedJsonValue) -> Option(String) {
  data
  |> captured_object_field("purchasingEntity")
  |> option.then(fn(entity) {
    entity
    |> captured_object_field("contact")
    |> option.then(fn(contact) { captured_string_field(contact, "id") })
  })
}

fn captured_object_field(
  data: CapturedJsonValue,
  field: String,
) -> Option(CapturedJsonValue) {
  case data {
    CapturedObject(fields) -> captured_field(fields, field)
    _ -> None
  }
}

fn captured_string_field(
  data: CapturedJsonValue,
  field: String,
) -> Option(String) {
  case captured_object_field(data, field) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

fn captured_field(
  fields: List(#(String, CapturedJsonValue)),
  field: String,
) -> Option(CapturedJsonValue) {
  case fields {
    [] -> None
    [#(key, value), ..] if key == field -> Some(value)
    [_, ..rest] -> captured_field(rest, field)
  }
}

fn handle_contact_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyContactId") {
    Some(id) ->
      case store.get_effective_b2b_company_contact_by_id(store, id) {
        Some(contact) ->
          case contact_has_associated_orders(store, contact) {
            True ->
              RootResult(
                Payload(
                  ..empty_payload([existing_orders_error()]),
                  deleted_company_contact_id: None,
                ),
                store,
                identity,
                [],
              )
            False -> {
              let #(store, ids) = delete_contact(store, id)
              RootResult(
                Payload(
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
          RootResult(
            Payload(
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
      RootResult(
        Payload(
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

fn handle_contacts_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  let #(store, deleted, staged, errors) =
    read_string_list(args, "companyContactIds")
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
  RootResult(
    Payload(..empty_payload(errors), deleted_company_contact_ids: deleted),
    store,
    identity,
    staged,
  )
}

fn handle_assign_customer_as_contact(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
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
                user_error_code.customer_not_found,
              )
            Some(customer) ->
              case find_company_contact_by_customer_id(contacts, customer_id) {
                Some(_) ->
                  company_contact_mutation_error(
                    store,
                    identity,
                    ["companyId"],
                    "Customer is already associated with a company contact.",
                    user_error_code.customer_already_a_contact,
                  )
                None ->
                  case customer_email(customer) {
                    None ->
                      company_contact_mutation_error(
                        store,
                        identity,
                        ["companyId"],
                        "Customer must have an email address.",
                        user_error_code.customer_email_must_exist,
                      )
                    Some(email) ->
                      case company_contact_cap_reached(company) {
                        True ->
                          RootResult(
                            Payload(
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
                              RootResult(
                                Payload(
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
                              RootResult(
                                Payload(
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

fn handle_contact_remove_from_company(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyContactId") {
    Some(id) ->
      case store.get_effective_b2b_company_contact_by_id(store, id) {
        Some(_) -> {
          let #(store, ids) = delete_contact(store, id)
          RootResult(
            Payload(..empty_payload([]), removed_company_contact_id: Some(id)),
            store,
            identity,
            ids,
          )
        }
        None ->
          RootResult(
            Payload(
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
      RootResult(
        Payload(
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

fn handle_assign_main_contact(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
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
          RootResult(
            Payload(..empty_payload([]), company: Some(updated_company)),
            store,
            identity,
            [updated_company.id],
          )
        }
        Some(_company), Some(_contact) ->
          RootResult(
            Payload(
              ..empty_payload([
                user_error(
                  Some(["companyContactId"]),
                  "The company contact does not belong to the company.",
                  user_error_code.invalid_input,
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

fn handle_revoke_main_contact(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyId") {
    Some(company_id) ->
      case store.get_effective_b2b_company_by_id(store, company_id) {
        Some(company) -> {
          let updated_company =
            B2BCompanyRecord(..company, main_contact_id: None)
          let #(updated_company, store) = stage_company(store, updated_company)
          RootResult(
            Payload(..empty_payload([]), company: Some(updated_company)),
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

fn handle_location_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
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
              RootResult(empty_payload(validation_errors), store, identity, [])
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
              RootResult(
                Payload(..empty_payload([]), company_location: Some(location)),
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

fn handle_location_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyLocationId") {
    Some(id) ->
      case store.get_effective_b2b_company_location_by_id(store, id) {
        Some(location) -> {
          let raw_input = read_object(args, "input")
          case dict.is_empty(raw_input), has_any_non_null_input(raw_input) {
            True, _ ->
              RootResult(
                empty_payload([location_update_empty_input_error()]),
                store,
                identity,
                [],
              )
            _, False ->
              RootResult(empty_payload([no_input_error()]), store, identity, [])
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
                  RootResult(
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
                  RootResult(
                    Payload(
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
          RootResult(
            Payload(
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
      RootResult(
        Payload(
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

fn delete_location(
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

fn remove_role_assignments_for_location(
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

fn remove_assignments_matching_location(
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

fn handle_location_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyLocationId") {
    Some(id) ->
      case store.get_effective_b2b_company_location_by_id(store, id) {
        Some(_) -> {
          let #(store, ids) = delete_location(store, id)
          RootResult(
            Payload(..empty_payload([]), deleted_company_location_id: Some(id)),
            store,
            identity,
            ids,
          )
        }
        None ->
          RootResult(
            Payload(
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
      RootResult(
        Payload(
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

fn handle_locations_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  let #(store, deleted, staged, errors) =
    read_string_list(args, "companyLocationIds")
    |> list.index_map(fn(id, index) { #(id, index) })
    |> list.fold(#(store, [], [], []), fn(acc, entry) {
      let #(id, index) = entry
      let #(current_store, deleted, staged, errors) = acc
      case store.get_effective_b2b_company_location_by_id(current_store, id) {
        Some(_) -> {
          let #(next_store, ids) = delete_location(current_store, id)
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
            resource_not_found(indexed_field_path("companyLocationIds", index)),
          ]),
        )
      }
    })
  RootResult(
    Payload(..empty_payload(errors), deleted_company_location_ids: deleted),
    store,
    identity,
    staged,
  )
}

fn build_role_assignment(
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

fn serialize_role_assignment(
  store: Store,
  assignment: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_source(hydrate_role_assignment(store, assignment), field, fragments)
}

fn hydrate_role_assignment(
  store: Store,
  assignment: SourceValue,
) -> SourceValue {
  case assignment {
    SrcObject(fields) -> {
      let contact_id =
        source_string(
          dict.get(fields, "companyContactId") |> result.unwrap(SrcNull),
        )
      let role_id =
        source_string(
          dict.get(fields, "companyContactRoleId") |> result.unwrap(SrcNull),
        )
      let location_id =
        source_string(
          dict.get(fields, "companyLocationId") |> result.unwrap(SrcNull),
        )
      let with_contact = case
        store.get_effective_b2b_company_contact_by_id(store, contact_id)
      {
        Some(contact) ->
          dict.insert(
            fields,
            "companyContact",
            contact_source_with_main_flag(store, contact),
          )
        None -> fields
      }
      let with_role = case
        store.get_effective_b2b_company_contact_role_by_id(store, role_id)
      {
        Some(role) -> dict.insert(with_contact, "role", role_source(role))
        None -> with_contact
      }
      let with_location = case
        store.get_effective_b2b_company_location_by_id(store, location_id)
      {
        Some(location) ->
          dict.insert(with_role, "companyLocation", location_source(location))
        None -> with_role
      }
      SrcObject(with_location)
    }
    _ -> assignment
  }
}

fn stage_role_assignments(
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

fn assignment_ref(assignment: SourceValue, key: String) -> Option(String) {
  case assignment {
    SrcObject(fields) ->
      case dict.get(fields, key) {
        Ok(SrcString(value)) -> Some(value)
        _ -> None
      }
    _ -> None
  }
}

fn resolve_role_assignments(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  contact_fallback: Option(String),
  location_fallback: Option(String),
  input_field: Option(String),
) -> #(List(SourceValue), List(UserError), SyntheticIdentityRegistry) {
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

fn role_assignment_lookup_errors(
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
) -> List(UserError) {
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

fn single_role_assignment_lookup_errors(
  contact: Option(B2BCompanyContactRecord),
  role: Option(B2BCompanyContactRoleRecord),
  location: Option(B2BCompanyLocationRecord),
  contact_field: List(String),
  role_field: List(String),
  location_field: List(String),
) -> List(UserError) {
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

fn bulk_contact_role_assignment_lookup_errors(
  contact: Option(B2BCompanyContactRecord),
  role: Option(B2BCompanyContactRoleRecord),
  location: Option(B2BCompanyLocationRecord),
  contact_field: List(String),
  role_field: List(String),
  location_field: List(String),
) -> List(UserError) {
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

fn bulk_location_role_assignment_lookup_errors(
  contact: Option(B2BCompanyContactRecord),
  role: Option(B2BCompanyContactRoleRecord),
  location: Option(B2BCompanyLocationRecord),
  item_field: List(String),
) -> List(UserError) {
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

fn role_assignment_field(
  input_field: Option(String),
  index: Int,
  field: String,
) -> List(String) {
  case input_field {
    Some(list_field) -> indexed_nested_field_path(list_field, index, field)
    None -> [field]
  }
}

fn role_assignment_item_field(
  input_field: Option(String),
  index: Int,
) -> Option(List(String)) {
  case input_field {
    Some(list_field) -> Some(indexed_field_path(list_field, index))
    None -> None
  }
}

fn role_assignment_missing_field(
  input_field: Option(String),
  index: Int,
) -> List(String) {
  case input_field {
    Some(list_field) -> indexed_field_path(list_field, index)
    None -> ["rolesToAssign"]
  }
}

fn contact_has_role_assignment_for_location(
  contact: B2BCompanyContactRecord,
  location_id: String,
) -> Bool {
  read_object_sources(data_get(contact.data, "roleAssignments"))
  |> list.any(fn(assignment) {
    assignment_ref(assignment, "companyLocationId") == Some(location_id)
  })
}

fn option_or(value: Option(a), fallback: Option(a)) -> Option(a) {
  case value {
    Some(_) -> value
    None -> fallback
  }
}

fn handle_contact_assign_role(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
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
  RootResult(
    Payload(
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

fn read_arg_or_null(args: Dict(String, root_field.ResolvedValue), key: String) {
  dict.get(args, key) |> result.unwrap(root_field.NullVal)
}

fn handle_contact_assign_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  let #(assignments, errors, identity) =
    resolve_role_assignments(
      store,
      identity,
      read_object_list(args, "rolesToAssign"),
      read_string(args, "companyContactId"),
      None,
      Some("rolesToAssign"),
    )
  let #(store, staged) = case errors {
    [] -> stage_role_assignments(store, assignments)
    _ -> #(store, [])
  }
  RootResult(
    Payload(..empty_payload(errors), role_assignments: case errors {
      [] -> assignments
      _ -> []
    }),
    store,
    identity,
    staged,
  )
}

fn handle_location_assign_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  let #(assignments, errors, identity) =
    resolve_role_assignments(
      store,
      identity,
      read_object_list(args, "rolesToAssign"),
      None,
      read_string(args, "companyLocationId"),
      Some("rolesToAssign"),
    )
  let #(store, staged) = case errors {
    [] -> stage_role_assignments(store, assignments)
    _ -> #(store, [])
  }
  RootResult(
    Payload(..empty_payload(errors), role_assignments: case errors {
      [] -> assignments
      _ -> []
    }),
    store,
    identity,
    staged,
  )
}

fn revoke_role_assignments(
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
              filter_removed_assignments(current, assignment_ids, revoke_all)
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
                #(next_store, list.append(removed, removed_here))
              }
            }
          }
        }
      },
    )
  let #(store, removed) =
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
              filter_removed_assignments(current, assignment_ids, revoke_all)
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
                  store.upsert_staged_b2b_company_location(
                    current_store,
                    updated,
                  )
                #(next_store, list.append(removed, removed_here))
              }
            }
          }
        }
      },
    )
  #(store, list.unique(removed))
}

fn filter_removed_assignments(
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

fn missing_indexed_id_errors(
  requested_ids: List(String),
  found_ids: List(String),
  field: String,
) -> List(UserError) {
  requested_ids
  |> list.index_map(fn(id, index) { #(id, index) })
  |> list.fold([], fn(errors, entry) {
    let #(id, index) = entry
    case list.contains(found_ids, id) {
      True -> errors
      False ->
        list.append(errors, [
          resource_not_found(indexed_field_path(field, index)),
        ])
    }
  })
}

fn handle_contact_revoke_role(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
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
  RootResult(
    Payload(
      ..empty_payload(errors),
      revoked_company_contact_role_assignment_id: list.first(revoked)
        |> option_from_result,
    ),
    store,
    identity,
    revoked,
  )
}

fn handle_contact_revoke_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  let revoke_all = case read_bool(args, "revokeAll") {
    Some(True) -> True
    _ -> False
  }
  let role_assignment_ids = read_string_list(args, "roleAssignmentIds")
  let #(store, revoked) =
    revoke_role_assignments(
      store,
      role_assignment_ids,
      read_string(args, "companyContactId"),
      None,
      revoke_all,
    )
  let errors = case revoke_all {
    True -> []
    False ->
      missing_indexed_id_errors(
        role_assignment_ids,
        revoked,
        "roleAssignmentIds",
      )
  }
  RootResult(
    Payload(..empty_payload(errors), revoked_role_assignment_ids: revoked),
    store,
    identity,
    revoked,
  )
}

fn handle_location_revoke_roles(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  let roles_to_revoke = read_string_list(args, "rolesToRevoke")
  let #(store, revoked) =
    revoke_role_assignments(
      store,
      roles_to_revoke,
      None,
      read_string(args, "companyLocationId"),
      False,
    )
  let errors =
    missing_indexed_id_errors(roles_to_revoke, revoked, "rolesToRevoke")
  RootResult(
    Payload(..empty_payload(errors), revoked_role_assignment_ids: revoked),
    store,
    identity,
    revoked,
  )
}

fn handle_assign_address(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "locationId") {
    Some(location_id) ->
      case store.get_effective_b2b_company_location_by_id(store, location_id) {
        Some(location) -> {
          let address_types = read_string_list(args, "addressTypes")
          case has_duplicate_strings(address_types) {
            True ->
              RootResult(
                Payload(
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
                  RootResult(
                    Payload(
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
                  RootResult(
                    Payload(..empty_payload([]), addresses: addresses),
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
          RootResult(
            Payload(
              ..empty_payload([resource_not_found(["locationId"])]),
              addresses: [],
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      RootResult(
        Payload(
          ..empty_payload([resource_not_found(["locationId"])]),
          addresses: [],
        ),
        store,
        identity,
        [],
      )
  }
}

fn handle_address_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "addressId") {
    Some(target_address_id) -> {
      let #(store, detached_location_ids) =
        detach_address_from_locations(store, target_address_id)
      case detached_location_ids {
        [_, ..] ->
          RootResult(
            Payload(
              ..empty_payload([]),
              deleted_address_id: Some(target_address_id),
            ),
            store,
            identity,
            list.append(detached_location_ids, [target_address_id]),
          )
        [] ->
          RootResult(
            Payload(
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
      RootResult(
        Payload(
          ..empty_payload([resource_not_found(["addressId"])]),
          deleted_address_id: None,
        ),
        store,
        identity,
        [],
      )
  }
}

fn detach_address_from_locations(
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

fn handle_assign_staff(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyLocationId") {
    Some(location_id) ->
      case store.get_effective_b2b_company_location_by_id(store, location_id) {
        Some(location) -> {
          let staff_member_ids = read_string_list(args, "staffMemberIds")
          let errors = invalid_staff_member_id_errors(staff_member_ids)
          case errors {
            [_, ..] ->
              RootResult(
                Payload(
                  ..empty_payload(errors),
                  company_location_staff_member_assignments: [],
                ),
                store,
                identity,
                [],
              )
            [] -> {
              let #(assignments, identity) =
                list.fold(staff_member_ids, #([], identity), fn(acc, staff_id) {
                  let #(items, current_identity) = acc
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
                  #(list.append(items, [assignment]), next_identity)
                })
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
              RootResult(
                Payload(
                  ..empty_payload([]),
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
          RootResult(
            Payload(
              ..empty_payload([resource_not_found(["companyLocationId"])]),
              company_location_staff_member_assignments: [],
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      RootResult(
        Payload(
          ..empty_payload([resource_not_found(["companyLocationId"])]),
          company_location_staff_member_assignments: [],
        ),
        store,
        identity,
        [],
      )
  }
}

fn invalid_staff_member_id_errors(ids: List(String)) -> List(UserError) {
  ids
  |> list.index_map(fn(id, index) { #(id, index) })
  |> list.fold([], fn(errors, entry) {
    let #(id, index) = entry
    case valid_staff_member_id(id) {
      True -> errors
      False ->
        list.append(errors, [
          resource_not_found(indexed_field_path("staffMemberIds", index)),
        ])
    }
  })
}

fn valid_staff_member_id(id: String) -> Bool {
  valid_shopify_gid_type(id, "StaffMember")
  && !string.ends_with(id, "/999999999999")
}

fn valid_shopify_gid_type(id: String, resource_type: String) -> Bool {
  string.starts_with(id, "gid://shopify/" <> resource_type <> "/")
}

fn handle_remove_staff(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  let ids = read_string_list(args, "companyLocationStaffMemberAssignmentIds")
  let #(store, removed, staged) =
    list.fold(
      store.list_effective_b2b_company_locations(store),
      #(store, [], []),
      fn(acc, location) {
        let #(current_store, removed, staged) = acc
        let current =
          read_object_sources(data_get(location.data, "staffMemberAssignments"))
        let #(next, removed_here) =
          filter_removed_assignments(current, ids, False)
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
  let errors =
    missing_indexed_id_errors(
      ids,
      removed,
      "companyLocationStaffMemberAssignmentIds",
    )
  RootResult(
    Payload(
      ..empty_payload(errors),
      deleted_company_location_staff_member_assignment_ids: removed,
    ),
    store,
    identity,
    list.append(staged, removed),
  )
}

fn handle_tax_settings_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args,
) -> RootResult {
  case read_string(args, "companyLocationId") {
    Some(location_id) ->
      case store.get_effective_b2b_company_location_by_id(store, location_id) {
        Some(location) -> {
          let errors = validate_tax_settings_update_args(args)
          case errors {
            [_, ..] ->
              RootResult(
                Payload(..empty_payload(errors), company_location: None),
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
              RootResult(
                Payload(..empty_payload([]), company_location: Some(updated)),
                store,
                identity,
                [updated.id],
              )
            }
          }
        }
        None ->
          RootResult(
            Payload(
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
      RootResult(
        Payload(
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

fn validate_tax_settings_update_args(
  args: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
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

fn has_any_tax_settings_update_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.has_key(args, "taxRegistrationId")
  || dict.has_key(args, "taxExempt")
  || dict.has_key(args, "exemptionsToAssign")
  || dict.has_key(args, "exemptionsToRemove")
}

fn read_string_values(value: SourceValue) -> List(String) {
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

pub fn serialize_company_address_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let found =
    list.find(store.list_effective_b2b_company_locations(store), fn(location) {
      address_id(data_get(location.data, "billingAddress")) == Some(id)
      || address_id(data_get(location.data, "shippingAddress")) == Some(id)
    })
  case found {
    Ok(location) -> {
      let address = case address_id(data_get(location.data, "billingAddress")) {
        Some(billing_id) if billing_id == id ->
          data_get(location.data, "billingAddress")
        _ -> data_get(location.data, "shippingAddress")
      }
      project_graphql_value(address, selections, fragments)
    }
    Error(_) -> json.null()
  }
}

pub fn serialize_company_contact_role_assignment_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let from_contacts =
    store.list_effective_b2b_company_contacts(store)
    |> list.flat_map(fn(contact) {
      read_object_sources(data_get(contact.data, "roleAssignments"))
    })
  let from_locations =
    store.list_effective_b2b_company_locations(store)
    |> list.flat_map(fn(location) {
      read_object_sources(data_get(location.data, "roleAssignments"))
    })
  case
    list.find(list.append(from_contacts, from_locations), fn(assignment) {
      source_id(assignment) == id
    })
  {
    Ok(assignment) ->
      project_graphql_value(
        hydrate_role_assignment(store, assignment),
        selections,
        fragments,
      )
    Error(_) -> json.null()
  }
}

fn serialize_mutation_payload(
  store: Store,
  payload: Payload,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  json.object(
    selected_children(field)
    |> list.map(fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "company" -> #(key, case payload.company {
              Some(company) ->
                serialize_company(store, company, child, fragments, variables)
              None -> json.null()
            })
            "companyContact" -> #(key, case payload.company_contact {
              Some(contact) ->
                serialize_contact(store, contact, child, fragments)
              None -> json.null()
            })
            "companyLocation" -> #(key, case payload.company_location {
              Some(location) ->
                serialize_location(store, location, child, fragments)
              None -> json.null()
            })
            "companyContactRoleAssignment" -> #(
              key,
              case payload.company_contact_role_assignment {
                Some(assignment) ->
                  serialize_role_assignment(store, assignment, child, fragments)
                None -> json.null()
              },
            )
            "roleAssignments" -> #(
              key,
              json.array(payload.role_assignments, fn(item) {
                serialize_role_assignment(store, item, child, fragments)
              }),
            )
            "addresses" -> #(key, case payload.user_errors, payload.addresses {
              [_, ..], [] -> json.null()
              _, _ ->
                json.array(payload.addresses, fn(item) {
                  project_graphql_value(
                    item,
                    selected_children(child),
                    fragments,
                  )
                })
            })
            "companyLocationStaffMemberAssignments" -> #(
              key,
              case
                payload.user_errors,
                payload.company_location_staff_member_assignments
              {
                [_, ..], [] -> json.null()
                _, _ ->
                  json.array(
                    payload.company_location_staff_member_assignments,
                    fn(item) {
                      project_graphql_value(
                        item,
                        selected_children(child),
                        fragments,
                      )
                    },
                  )
              },
            )
            "userErrors" -> #(
              key,
              json.array(payload.user_errors, fn(error) {
                serialize_user_error(error, child, fragments)
              }),
            )
            "deletedCompanyId" -> #(
              key,
              optional_json_string(payload.deleted_company_id),
            )
            "deletedCompanyIds" -> #(
              key,
              json.array(payload.deleted_company_ids, json.string),
            )
            "deletedCompanyContactId" -> #(
              key,
              optional_json_string(payload.deleted_company_contact_id),
            )
            "deletedCompanyContactIds" -> #(
              key,
              json.array(payload.deleted_company_contact_ids, json.string),
            )
            "deletedCompanyLocationId" -> #(
              key,
              optional_json_string(payload.deleted_company_location_id),
            )
            "deletedCompanyLocationIds" -> #(
              key,
              json.array(payload.deleted_company_location_ids, json.string),
            )
            "deletedAddressId" -> #(
              key,
              optional_json_string(payload.deleted_address_id),
            )
            "revokedCompanyContactRoleAssignmentId" -> #(
              key,
              optional_json_string(
                payload.revoked_company_contact_role_assignment_id,
              ),
            )
            "revokedRoleAssignmentIds" -> #(
              key,
              json.array(payload.revoked_role_assignment_ids, json.string),
            )
            "deletedCompanyLocationStaffMemberAssignmentIds" -> #(
              key,
              case
                payload.user_errors,
                payload.deleted_company_location_staff_member_assignment_ids
              {
                [_, ..], [] -> json.null()
                _, _ ->
                  json.array(
                    payload.deleted_company_location_staff_member_assignment_ids,
                    json.string,
                  )
              },
            )
            "removedCompanyContactId" -> #(
              key,
              optional_json_string(payload.removed_company_contact_id),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_user_error(
  error: UserError,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source =
    src_object([
      #("field", case error.field {
        Some(fields) -> SrcList(list.map(fields, SrcString))
        None -> SrcNull
      }),
      #("message", SrcString(error.message)),
      #("code", SrcString(user_error_code.value(error.code))),
      #("detail", case error.detail {
        Some(detail) -> SrcString(detail)
        None -> SrcNull
      }),
    ])
  project_source(source, field, fragments)
}

fn optional_json_string(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}
