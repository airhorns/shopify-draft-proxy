//// B2B source projection and GraphQL serialization helpers.

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
pub fn empty_payload(errors: List(b2b_types.UserError)) -> b2b_types.Payload {
  b2b_types.Payload(
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

@internal
pub fn read_id_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  read_string(graphql_helpers.field_args(field, variables), "id")
}

@internal
pub fn read_string(
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

@internal
pub fn read_bool(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(args, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_string_list(
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

@internal
pub fn read_object(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(args, key) {
    Ok(root_field.ObjectVal(value)) -> value
    _ -> dict.new()
  }
}

@internal
pub fn read_object_list(
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

@internal
pub fn selected_children(field: Selection) -> List(Selection) {
  get_selected_child_fields(field, default_selected_field_options())
}

@internal
pub fn project_source(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(source, selected_children(field), fragments)
}

@internal
pub fn value_to_source(value: StorePropertyValue) -> SourceValue {
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

@internal
pub fn source_to_value(value: SourceValue) -> StorePropertyValue {
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

@internal
pub fn data_to_source(data: Dict(String, StorePropertyValue)) -> SourceValue {
  SrcObject(
    dict.to_list(data)
    |> list.map(fn(pair) { #(pair.0, value_to_source(pair.1)) })
    |> dict.from_list,
  )
}

@internal
pub fn data_get(
  data: Dict(String, StorePropertyValue),
  key: String,
) -> SourceValue {
  case dict.get(data, key) {
    Ok(value) -> value_to_source(value)
    Error(_) -> SrcNull
  }
}

@internal
pub fn put_source(
  data: Dict(String, StorePropertyValue),
  key: String,
  value: SourceValue,
) -> Dict(String, StorePropertyValue) {
  dict.insert(data, key, source_to_value(value))
}

@internal
pub fn maybe_put_string(
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

@internal
pub fn maybe_put_bool(
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

@internal
pub fn record_source(
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

@internal
pub fn company_source(company: B2BCompanyRecord) -> SourceValue {
  record_source("Company", company.id, company.data)
}

@internal
pub fn contact_source(contact: B2BCompanyContactRecord) -> SourceValue {
  record_source("CompanyContact", contact.id, contact.data)
}

@internal
pub fn contact_source_with_main_flag(
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

@internal
pub fn role_source(role: B2BCompanyContactRoleRecord) -> SourceValue {
  record_source("CompanyContactRole", role.id, role.data)
}

@internal
pub fn location_source(location: B2BCompanyLocationRecord) -> SourceValue {
  record_source("CompanyLocation", location.id, location.data)
}

@internal
pub fn source_field(
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

@internal
pub fn serialize_count(field: Selection, count: Int) -> Json {
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

@internal
pub fn serialize_company(
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

@internal
pub fn serialize_company_metafield(
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

@internal
pub fn serialize_company_metafields_connection(
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

@internal
pub fn company_metafield_to_core(
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

@internal
pub fn serialize_contact(
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

@internal
pub fn contact_notes_source(contact: B2BCompanyContactRecord) -> SourceValue {
  case data_get(contact.data, "notes") {
    SrcNull -> data_get(contact.data, "note")
    other -> other
  }
}

@internal
pub fn serialize_location(
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

@internal
pub fn serialize_tax_settings(
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

@internal
pub fn serialize_company_connection(
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

@internal
pub fn serialize_contact_connection(
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

@internal
pub fn serialize_role_connection(
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

@internal
pub fn serialize_location_connection(
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

@internal
pub fn serialize_source_connection(
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

@internal
pub fn serialize_role_assignment(
  store: Store,
  assignment: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_source(hydrate_role_assignment(store, assignment), field, fragments)
}

@internal
pub fn hydrate_role_assignment(
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

@internal
pub fn paginate_records(
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

@internal
pub fn source_id(value: SourceValue) -> String {
  case value {
    SrcObject(fields) ->
      case dict.get(fields, "id") {
        Ok(SrcString(id)) -> id
        _ -> ""
      }
    _ -> ""
  }
}

@internal
pub fn read_object_sources(value: SourceValue) -> List(SourceValue) {
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

@internal
pub fn company_contacts(store: Store, company: B2BCompanyRecord) {
  company.contact_ids
  |> list.filter_map(fn(id) {
    case store.get_effective_b2b_company_contact_by_id(store, id) {
      Some(contact) -> Ok(contact)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn contact_customer_id(contact: B2BCompanyContactRecord) -> Option(String) {
  case dict.get(contact.data, "customerId") {
    Ok(StorePropertyString(customer_id)) -> Some(customer_id)
    _ -> None
  }
}

@internal
pub fn find_company_contact_by_customer_id(
  contacts: List(B2BCompanyContactRecord),
  customer_id: String,
) -> Option(B2BCompanyContactRecord) {
  contacts
  |> list.find(fn(contact) { contact_customer_id(contact) == Some(customer_id) })
  |> option_from_result
}

@internal
pub fn customer_email(customer: CustomerRecord) -> Option(String) {
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

@internal
pub fn customer_contact_source(
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

@internal
pub fn company_contact_cap_reached(company: B2BCompanyRecord) -> Bool {
  list.length(company.contact_ids) >= b2b_types.company_contact_maximum_cap
}

@internal
pub fn company_contact_cap_error() -> b2b_types.UserError {
  detailed_user_error(
    Some(["companyId"]),
    "Company contact maximum cap reached.",
    user_error_code.limit_reached,
    b2b_types.company_contact_max_cap_reached_detail,
  )
}

@internal
pub fn bulk_action_limit_reached_error(field: String) -> b2b_types.UserError {
  user_error(
    Some([field]),
    b2b_types.bulk_action_limit_reached_message,
    user_error_code.limit_reached,
  )
}

@internal
pub fn bulk_action_limit_reached(items: List(a)) -> Bool {
  list.length(items) > b2b_types.bulk_actions_max_size
}

@internal
pub fn company_contact_mutation_error(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: List(String),
  message: String,
  code: user_error_code.Code,
  detail: Option(String),
) -> b2b_types.RootResult {
  let error = case detail {
    Some(detail) -> detailed_user_error(Some(field), message, code, detail)
    None -> user_error(Some(field), message, code)
  }
  b2b_types.RootResult(
    b2b_types.Payload(..empty_payload([error]), company_contact: None),
    store,
    identity,
    [],
  )
}

@internal
pub fn company_locations(store: Store, company: B2BCompanyRecord) {
  company.location_ids
  |> list.filter_map(fn(id) {
    case store.get_effective_b2b_company_location_by_id(store, id) {
      Some(location) -> Ok(location)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn company_roles(store: Store, company: B2BCompanyRecord) {
  company.contact_role_ids
  |> list.filter_map(fn(id) {
    case store.get_effective_b2b_company_contact_role_by_id(store, id) {
      Some(role) -> Ok(role)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn contact_is_main_contact(
  store: Store,
  contact: B2BCompanyContactRecord,
) -> Bool {
  case store.get_effective_b2b_company_by_id(store, contact.company_id) {
    Some(company) -> company.main_contact_id == Some(contact.id)
    None -> False
  }
}

@internal
pub fn option_from_result(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(x) -> Some(x)
    Error(_) -> None
  }
}

@internal
pub fn append_unique(items: List(String), value: String) -> List(String) {
  case list.contains(items, value) {
    True -> items
    False -> list.append(items, [value])
  }
}

@internal
pub fn append_unique_list(
  items: List(String),
  values: List(String),
) -> List(String) {
  list.fold(values, items, fn(acc, value) { append_unique(acc, value) })
}

@internal
pub fn has_duplicate_strings(items: List(String)) -> Bool {
  has_duplicate_strings_loop(items, [])
}

@internal
pub fn has_duplicate_strings_loop(
  items: List(String),
  seen: List(String),
) -> Bool {
  case items {
    [] -> False
    [first, ..rest] ->
      case list.contains(seen, first) {
        True -> True
        False -> has_duplicate_strings_loop(rest, [first, ..seen])
      }
  }
}

@internal
pub fn remove_string(items: List(String), value: String) -> List(String) {
  list.filter(items, fn(item) { item != value })
}

@internal
pub fn filter_companies_by_query(
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

@internal
pub fn source_string(value: SourceValue) -> String {
  case value {
    SrcString(value) -> value
    _ -> ""
  }
}

@internal
pub fn user_error(
  field: Option(List(String)),
  message: String,
  code: user_error_code.Code,
) {
  b2b_types.UserError(field: field, message: message, code: code, detail: None)
}

@internal
pub fn detailed_user_error(
  field: Option(List(String)),
  message: String,
  code: user_error_code.Code,
  detail: String,
) {
  b2b_types.UserError(
    field: field,
    message: message,
    code: code,
    detail: Some(detail),
  )
}

@internal
pub fn field_path(prefix: List(String), field: String) -> List(String) {
  list.append(prefix, [field])
}

@internal
pub fn indexed_field_path(field: String, index: Int) -> List(String) {
  [field, int.to_string(index)]
}

@internal
pub fn indexed_nested_field_path(
  list_field: String,
  index: Int,
  field: String,
) -> List(String) {
  [list_field, int.to_string(index), field]
}

@internal
pub fn validate_length(
  value: String,
  field: String,
  prefix: List(String),
  label: String,
  max: Int,
) -> List(b2b_types.UserError) {
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

@internal
pub fn validate_html(
  value: String,
  field: String,
  prefix: List(String),
  label: String,
) -> List(b2b_types.UserError) {
  case contains_html_tags(value) {
    True -> [
      detailed_user_error(
        Some(field_path(prefix, field)),
        label <> " contains HTML tags",
        user_error_code.invalid,
        b2b_types.contains_html_tags_detail,
      ),
    ]
    False -> []
  }
}

@internal
pub fn validate_text_field(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  error_field: String,
  prefix: List(String),
  label: String,
  max: Int,
  reject_html: Bool,
) -> List(b2b_types.UserError) {
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

@internal
pub fn validate_external_id_field(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> List(b2b_types.UserError) {
  case read_string(input, "externalId") {
    Some(value) -> {
      validate_external_id_length(value, prefix)
      |> list.append(validate_external_id_charset(value, prefix))
    }
    None -> []
  }
}

@internal
pub fn validate_external_id_length(
  value: String,
  prefix: List(String),
) -> List(b2b_types.UserError) {
  case string.length(value) > b2b_types.external_id_max_length {
    True -> [
      user_error(
        Some(field_path(prefix, "externalId")),
        "External Id must be "
          <> int.to_string(b2b_types.external_id_max_length)
          <> " characters or less.",
        user_error_code.too_long,
      ),
    ]
    False -> []
  }
}

@internal
pub fn validate_external_id_charset(
  value: String,
  prefix: List(String),
) -> List(b2b_types.UserError) {
  case value |> string.to_graphemes |> list.all(external_id_char_allowed) {
    True -> []
    False -> [
      detailed_user_error(
        Some(field_path(prefix, "externalId")),
        b2b_types.external_id_invalid_chars_message,
        user_error_code.invalid,
        b2b_types.external_id_invalid_chars_detail,
      ),
    ]
  }
}

@internal
pub fn external_id_char_allowed(char: String) -> Bool {
  string.contains(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*(){}[]\\/?<>_-~.,;:'\"`",
    char,
  )
}

@internal
pub fn value_is_present(value: root_field.ResolvedValue) -> Bool {
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

@internal
pub fn has_non_empty_object_field(
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

@internal
pub fn has_explicit_null_field(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Bool {
  case dict.get(input, field) {
    Ok(root_field.NullVal) -> True
    _ -> False
  }
}

@internal
pub fn has_any_non_null_input(
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

@internal
pub fn no_input_error() -> b2b_types.UserError {
  user_error(Some(["input"]), "No input provided.", user_error_code.no_input)
}

@internal
pub fn contact_create_empty_input_error() -> b2b_types.UserError {
  user_error(
    None,
    "Company contact create input is empty.",
    user_error_code.no_input,
  )
}

@internal
pub fn company_update_empty_input_error() -> b2b_types.UserError {
  user_error(
    Some(["input"]),
    "At least one attribute to change must be present",
    user_error_code.invalid,
  )
}

@internal
pub fn contact_update_empty_input_error() -> b2b_types.UserError {
  user_error(
    None,
    "Company contact update input is empty.",
    user_error_code.no_input,
  )
}

@internal
pub fn location_update_empty_input_error() -> b2b_types.UserError {
  user_error(
    None,
    "Company location update input is empty.",
    user_error_code.no_input,
  )
}

@internal
pub fn validate_billing_same_as_shipping(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> List(b2b_types.UserError) {
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

@internal
pub fn validate_tax_exempt_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> List(b2b_types.UserError) {
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

@internal
pub fn sanitize_name_field(
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case read_string(input, "name") {
    Some(value) ->
      dict.insert(input, "name", root_field.StringVal(strip_html(value)))
    None -> input
  }
}

@internal
pub fn validate_company_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> #(Dict(String, root_field.ResolvedValue), List(b2b_types.UserError)) {
  let input = sanitize_name_field(input)
  let errors =
    validate_text_field(
      input,
      "name",
      "name",
      prefix,
      "Name",
      b2b_types.default_string_max_length,
      False,
    )
    |> list.append(validate_text_field(
      input,
      "note",
      "notes",
      prefix,
      "Notes",
      b2b_types.notes_max_length,
      True,
    ))
    |> list.append(validate_external_id_field(input, prefix))
  #(input, errors)
}

@internal
pub fn validate_contact_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> #(Dict(String, root_field.ResolvedValue), List(b2b_types.UserError)) {
  let errors =
    validate_text_field(
      input,
      "title",
      "title",
      prefix,
      "Title",
      b2b_types.default_string_max_length,
      True,
    )
    |> list.append(validate_text_field(
      input,
      "note",
      "notes",
      prefix,
      "Notes",
      b2b_types.notes_max_length,
      True,
    ))
    |> list.append(validate_text_field(
      input,
      "notes",
      "notes",
      prefix,
      "Notes",
      b2b_types.notes_max_length,
      True,
    ))
  #(input, errors)
}

@internal
pub fn validate_location_input(
  input: Dict(String, root_field.ResolvedValue),
  prefix: List(String),
) -> #(Dict(String, root_field.ResolvedValue), List(b2b_types.UserError)) {
  let input = sanitize_name_field(input)
  let errors =
    validate_text_field(
      input,
      "name",
      "name",
      prefix,
      "Name",
      b2b_types.default_string_max_length,
      False,
    )
    |> list.append(validate_text_field(
      input,
      "note",
      "notes",
      prefix,
      "Notes",
      b2b_types.notes_max_length,
      True,
    ))
    |> list.append(validate_billing_same_as_shipping(input, prefix))
    |> list.append(validate_tax_exempt_input(input, prefix))
    |> list.append(validate_external_id_field(input, prefix))
  #(input, errors)
}

@internal
pub fn contains_html_tags(value: String) -> Bool {
  contains_html_tag_loop(string.to_graphemes(value))
}

@internal
pub fn contains_html_tag_loop(graphemes: List(String)) -> Bool {
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

@internal
pub fn contains_tag_close(graphemes: List(String)) -> Bool {
  case graphemes {
    [] -> False
    ["<", ..] -> False
    [">", ..] -> True
    [_, ..rest] -> contains_tag_close(rest)
  }
}

@internal
pub fn is_html_tag_start(value: String) -> Bool {
  value == "/"
  || value == "!"
  || value == "?"
  || string.contains(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    value,
  )
}

@internal
pub fn strip_html(value: String) -> String {
  strip_html_loop(string.to_graphemes(value), False, [])
}

@internal
pub fn strip_html_loop(
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

@internal
pub fn resource_not_found(field: List(String)) {
  user_error(
    Some(field),
    "Resource requested does not exist.",
    user_error_code.resource_not_found,
  )
}

@internal
pub fn company_role_not_found_at(field: List(String)) {
  user_error(
    Some(field),
    "The company contact role doesn't exist.",
    user_error_code.resource_not_found,
  )
}

@internal
pub fn company_location_not_found_at(field: List(String)) {
  user_error(
    Some(field),
    "The company location doesn't exist.",
    user_error_code.resource_not_found,
  )
}

@internal
pub fn company_contact_does_not_exist_at(field: List(String)) {
  user_error(
    Some(field),
    "Company contact does not exist.",
    user_error_code.resource_not_found,
  )
}

@internal
pub fn company_role_does_not_exist_at(field: List(String)) {
  user_error(
    Some(field),
    "Company role does not exist.",
    user_error_code.resource_not_found,
  )
}

@internal
pub fn one_role_already_assigned_at(field: Option(List(String))) {
  detailed_user_error(
    field,
    "Company contact has already been assigned a role in that company location.",
    user_error_code.limit_reached,
    b2b_types.one_role_already_assigned_detail,
  )
}

@internal
pub fn existing_orders_error() {
  existing_orders_error_at(["companyContactId"])
}

@internal
pub fn existing_orders_error_at(field: List(String)) {
  detailed_user_error(
    Some(field),
    "Cannot delete a company contact with existing orders or draft orders.",
    user_error_code.failed_to_delete,
    b2b_types.existing_orders_detail,
  )
}

@internal
pub fn make_gid(
  identity: SyntheticIdentityRegistry,
  typename: String,
) -> #(String, SyntheticIdentityRegistry) {
  synthetic_identity.make_proxy_synthetic_gid(identity, typename)
}

@internal
pub fn timestamp(
  identity: SyntheticIdentityRegistry,
) -> #(String, SyntheticIdentityRegistry) {
  synthetic_identity.make_synthetic_timestamp(identity)
}

@internal
pub fn address_from_input(
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

@internal
pub fn optional_src_string(value: Option(String)) -> SourceValue {
  case value {
    Some(value) -> SrcString(value)
    None -> SrcNull
  }
}

@internal
pub fn address_id(value: SourceValue) -> Option(String) {
  case value {
    SrcObject(fields) ->
      case dict.get(fields, "id") {
        Ok(SrcString(id)) -> Some(id)
        _ -> None
      }
    _ -> None
  }
}

@internal
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

@internal
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

@internal
pub fn serialize_mutation_payload(
  store: Store,
  payload: b2b_types.Payload,
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

@internal
pub fn serialize_user_error(
  error: b2b_types.UserError,
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

@internal
pub fn optional_json_string(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}
