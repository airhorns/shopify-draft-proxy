//// B2B query dispatch and local-read routing.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import gleam/result

import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/b2b/serializers.{
  project_source, read_id_arg, role_source, serialize_company,
  serialize_company_connection, serialize_contact, serialize_count,
  serialize_location, serialize_location_connection,
}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}

import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}

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
