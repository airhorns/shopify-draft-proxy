//// B2B query dispatch and local-read routing.

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

@internal
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

@internal
pub fn local_b2b_id_known(store: Store, id: String) -> Bool {
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

@internal
pub fn local_b2b_id_deleted(store: Store, id: String) -> Bool {
  dict.has_key(store.staged_state.deleted_b2b_company_ids, id)
  || dict.has_key(store.staged_state.deleted_b2b_company_contact_ids, id)
  || dict.has_key(store.staged_state.deleted_b2b_company_contact_role_ids, id)
  || dict.has_key(store.staged_state.deleted_b2b_company_location_ids, id)
}

/// True iff any B2B record or deletion has been staged locally, or any
/// variable carries a proxy-synthetic gid. Connection and aggregate
/// reads must stay local once a B2B lifecycle scenario has staged state.
@internal
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
@internal
pub fn should_passthrough_in_live_hybrid(
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

@internal
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

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use fields <- result.try(root_field.get_root_fields(document))
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

@internal
pub fn query_field(
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
