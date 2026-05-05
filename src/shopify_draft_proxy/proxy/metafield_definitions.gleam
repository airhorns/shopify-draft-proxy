//// Stateful Gleam port of the metafields/metafield-definitions slice.
////
//// The module intentionally keeps the owner-scoped metafield model narrow:
//// Product, ProductVariant, Collection, and Customer owner IDs are accepted
//// for local metafield staging/reads; broader HasMetafields families stay
//// unsupported until their owning domains have evidence and state.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  ConnectionPageInfoOptions, SerializeConnectionConfig,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, serialize_connection,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, read_optional_string,
  single_root_log_draft,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type MetafieldDefinitionCapabilitiesRecord,
  type MetafieldDefinitionCapabilityRecord,
  type MetafieldDefinitionConstraintValueRecord,
  type MetafieldDefinitionConstraintsRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionTypeRecord, type MetafieldDefinitionValidationRecord,
  type ProductMetafieldRecord, type ProductRecord,
  MetafieldDefinitionCapabilitiesRecord, MetafieldDefinitionCapabilityRecord,
  MetafieldDefinitionConstraintValueRecord, MetafieldDefinitionConstraintsRecord,
  MetafieldDefinitionRecord, MetafieldDefinitionTypeRecord,
  MetafieldDefinitionValidationRecord, ProductMetafieldRecord, ProductRecord,
  ProductSeoRecord,
}

pub type MetafieldDefinitionsError {
  ParseFailed(root_field.RootFieldError)
}

pub type UserError {
  UserError(field: Option(List(String)), message: String, code: String)
}

pub type MetafieldsSetUserError {
  MetafieldsSetUserError(
    field: List(String),
    message: String,
    code: Option(String),
    element_index: Option(Int),
  )
}

type SimpleUserError {
  SimpleUserError(field: List(String), message: String)
}

type DeletedMetafieldIdentifier {
  DeletedMetafieldIdentifier(owner_id: String, namespace: String, key: String)
}

type MetafieldsDeleteResult {
  MetafieldsDeleteResult(
    deleted_metafields: List(Option(DeletedMetafieldIdentifier)),
    user_errors: List(SimpleUserError),
    store: Store,
  )
}

type StandardMetafieldDefinitionTemplate {
  StandardMetafieldDefinitionTemplate(
    id: String,
    namespace: String,
    key: String,
    name: String,
    description: Option(String),
    owner_types: List(String),
    type_: MetafieldDefinitionTypeRecord,
    validations: List(MetafieldDefinitionValidationRecord),
    visible_to_storefront_api: Bool,
  )
}

pub fn is_metafield_definitions_query_root(name: String) -> Bool {
  case name {
    "metafieldDefinition"
    | "metafieldDefinitions"
    | "product"
    | "productVariant"
    | "collection"
    | "customer" -> True
    _ -> False
  }
}

pub fn is_metafield_definitions_mutation_root(name: String) -> Bool {
  case name {
    "metafieldDefinitionCreate"
    | "metafieldDefinitionUpdate"
    | "metafieldDefinitionDelete"
    | "standardMetafieldDefinitionEnable"
    | "metafieldDefinitionPin"
    | "metafieldDefinitionUnpin"
    | "metafieldsSet"
    | "metafieldsDelete"
    | "metafieldDelete" -> True
    _ -> False
  }
}

/// True when the local metafield-definition model must answer reads instead
/// of LiveHybrid passthrough: a lifecycle flow has staged or deleted
/// definitions, or a variable carries a proxy-synthetic definition id.
pub fn local_has_metafield_definition_state(
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
  || !list.is_empty(store.list_effective_metafield_definitions(proxy.store))
  || !dict_is_empty(proxy.store.staged_state.deleted_metafield_definition_ids)
  || !dict_is_empty(proxy.store.base_state.deleted_metafield_definition_ids)
}

/// Pattern 1: cold LiveHybrid definition catalog/detail reads are just
/// upstream reads. Once a local lifecycle has staged or deleted definitions,
/// keep reads local so read-after-write and read-after-delete behavior does
/// not leak back to Shopify.
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  _parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  query: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  case
    proxy.config.read_mode,
    local_has_metafield_definition_state(proxy, variables)
  {
    LiveHybrid, False -> passthrough.passthrough_sync(proxy, request)
    _, _ ->
      respond_local(
        proxy,
        process(proxy.store, query, variables),
        "Failed to handle metafield definitions query",
      )
  }
}

pub fn handle_metafield_definitions_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MetafieldDefinitionsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> Ok(serialize_root_fields(store, fields, variables))
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "metafieldDefinition" ->
              serialize_metafield_definition_root(store, field, variables)
            "metafieldDefinitions" ->
              serialize_metafield_definitions_connection(
                store,
                field,
                variables,
              )
            "product" ->
              serialize_owner_root(store, field, variables, "PRODUCT", "id")
            "productVariant" ->
              serialize_owner_root(
                store,
                field,
                variables,
                "PRODUCTVARIANT",
                "id",
              )
            "collection" ->
              serialize_owner_root(store, field, variables, "COLLECTION", "id")
            "customer" ->
              serialize_owner_root(store, field, variables, "CUSTOMER", "id")
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MetafieldDefinitionsError) {
  use data <- result.try(handle_metafield_definitions_query(
    store,
    document,
    variables,
  ))
  Ok(graphql_helpers.wrap_data(data))
}

pub fn process_mutation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store_in, identity, err)
    Ok(fields) -> {
      let hydrated_store =
        products.hydrate_products_for_live_hybrid_mutation(
          store_in,
          variables,
          upstream,
        )
      handle_mutation_fields(
        hydrated_store,
        identity,
        fields,
        variables,
        upstream,
      )
    }
  }
}

fn handle_mutation_fields(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #(store_in, identity, [], [], [], [])
  let #(store_out, identity_out, entries, drafts, staged_all, top_errors) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        current_store,
        current_identity,
        entries,
        drafts,
        staged_all,
        top_errors,
      ) = acc
      case field {
        Field(name: name, ..) -> {
          let #(payload, next_store, next_identity, staged_ids, field_errors) =
            dispatch_mutation_field(
              name.value,
              current_store,
              current_identity,
              field,
              variables,
              upstream,
            )
          let next_entries = case field_errors {
            [] ->
              list.append(entries, [#(get_field_response_key(field), payload)])
            [_, ..] -> entries
          }
          let next_drafts = case field_errors {
            [] -> {
              let draft =
                single_root_log_draft(
                  name.value,
                  staged_ids,
                  metafield_definitions_status_for(staged_ids),
                  "metafields",
                  "stage-locally",
                  Some(metafield_definitions_notes_for(name.value)),
                )
              list.append(drafts, [draft])
            }
            [_, ..] -> drafts
          }
          let next_staged_all = case field_errors {
            [] -> list.append(staged_all, staged_ids)
            [_, ..] -> staged_all
          }
          #(
            next_store,
            next_identity,
            next_entries,
            next_drafts,
            next_staged_all,
            list.append(top_errors, field_errors),
          )
        }
        _ -> acc
      }
    })
  let envelope = case top_errors {
    [] -> graphql_helpers.wrap_data(json.object(entries))
    [_, ..] -> json.object([#("errors", json.preprocessed_array(top_errors))])
  }
  let staged_resource_ids = case top_errors {
    [] -> staged_all
    [_, ..] -> []
  }
  MutationOutcome(
    data: envelope,
    store: case top_errors {
      [] -> store_out
      [_, ..] -> store_in
    },
    identity: case top_errors {
      [] -> identity_out
      [_, ..] -> identity
    },
    staged_resource_ids: staged_resource_ids,
    log_drafts: case top_errors {
      [] -> drafts
      [_, ..] -> []
    },
  )
}

fn dispatch_mutation_field(
  root_name: String,
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String), List(Json)) {
  case root_name {
    "metafieldDefinitionCreate" ->
      no_top_level_errors(serialize_definition_create_root(
        store_in,
        identity,
        field,
        variables,
      ))
    "metafieldDefinitionUpdate" ->
      no_top_level_errors(serialize_definition_update_root(
        store_in,
        identity,
        field,
        variables,
      ))
    "metafieldDefinitionDelete" ->
      no_top_level_errors(serialize_definition_delete_root(
        store_in,
        identity,
        field,
        variables,
      ))
    "standardMetafieldDefinitionEnable" ->
      no_top_level_errors(
        serialize_standard_metafield_definition_enable_mutation(
          store_in,
          identity,
          field,
          variables,
        ),
      )
    "metafieldDefinitionPin" ->
      no_top_level_errors(serialize_definition_pin_root(
        store_in,
        identity,
        field,
        variables,
        upstream,
      ))
    "metafieldDefinitionUnpin" ->
      no_top_level_errors(serialize_definition_unpin_root(
        store_in,
        identity,
        field,
        variables,
        upstream,
      ))
    "metafieldsSet" ->
      no_top_level_errors(serialize_metafields_set_root(
        store_in,
        identity,
        field,
        variables,
      ))
    "metafieldsDelete" ->
      no_top_level_errors(serialize_metafields_delete_root(
        store_in,
        identity,
        field,
        variables,
      ))
    "metafieldDelete" ->
      no_top_level_errors(serialize_metafield_delete_root(
        store_in,
        identity,
        field,
        variables,
      ))
    _ -> #(json.null(), store_in, identity, [], [])
  }
}

fn respond_local(
  proxy: DraftProxy,
  result: Result(Json, MetafieldDefinitionsError),
  error_message: String,
) -> #(Response, DraftProxy) {
  case result {
    Ok(body) -> #(Response(status: 200, body: body, headers: []), proxy)
    Error(_) -> #(
      Response(
        status: 400,
        body: json.object([#("error", json.string(error_message))]),
        headers: [],
      ),
      proxy,
    )
  }
}

fn no_top_level_errors(
  result: #(Json, Store, SyntheticIdentityRegistry, List(String)),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String), List(Json)) {
  let #(payload, store_out, identity_out, staged_ids) = result
  #(payload, store_out, identity_out, staged_ids, [])
}

fn metafield_definitions_status_for(
  staged_resource_ids: List(String),
) -> store.EntryStatus {
  case staged_resource_ids {
    [] -> store.Failed
    [_, ..] -> store.Staged
  }
}

fn metafield_definitions_notes_for(root_field_name: String) -> String {
  case root_field_name {
    "metafieldsSet" ->
      "Staged locally in the in-memory owner-scoped metafield draft store."
    "metafieldsDelete" | "metafieldDelete" ->
      "Staged owner-scoped metafield deletions locally in the in-memory draft store."
    _ -> "Staged locally in the in-memory metafield definition draft store."
  }
}

fn read_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  root_field.get_field_arguments(field, variables)
  |> result.unwrap(dict.new())
}

fn child_field(field: Selection, child_name: String) -> Option(Selection) {
  list.find(
    get_selected_child_fields(field, default_selected_field_options()),
    fn(child) {
      case child {
        Field(name: name, ..) -> name.value == child_name
        _ -> False
      }
    },
  )
  |> option.from_result
}

fn read_optional_bool(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(input, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_object(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(value)) -> value
    _ -> dict.new()
  }
}

fn has_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(input, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

fn read_input_objects(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          root_field.ObjectVal(obj) -> Ok(obj)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_definition_identifier(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(#(String, String, String)) {
  let identifier = read_object(args, "identifier")
  let owner_type = read_optional_string(identifier, "ownerType")
  let namespace = read_optional_string(identifier, "namespace")
  let key = read_optional_string(identifier, "key")
  case owner_type, namespace, key {
    Some(owner), Some(ns), Some(k) -> Some(#(owner, ns, k))
    _, _, _ -> None
  }
}

fn find_definition_from_args(
  store_in: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> Option(MetafieldDefinitionRecord) {
  let id =
    read_optional_string(args, "definitionId")
    |> option.or(read_optional_string(args, "id"))
  case id {
    Some(definition_id) ->
      store.get_effective_metafield_definition_by_id(store_in, definition_id)
    None ->
      case read_definition_identifier(args) {
        Some(#(owner_type, namespace, key)) ->
          store.find_effective_metafield_definition(
            store_in,
            owner_type,
            namespace,
            key,
          )
        None -> None
      }
  }
}

fn definition_reference_field(
  args: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case read_optional_string(args, "definitionId") {
    Some(_) -> ["definitionId"]
    None ->
      case read_optional_string(args, "id") {
        Some(_) -> ["id"]
        None -> ["identifier"]
      }
  }
}

fn serialize_metafield_definition_root(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = read_args(field, variables)
  let id = read_optional_string(args, "id")
  let definition = case id {
    Some(definition_id) ->
      store.get_effective_metafield_definition_by_id(store_in, definition_id)
    None ->
      case read_definition_identifier(args) {
        Some(#(owner_type, namespace, key)) ->
          store.find_effective_metafield_definition(
            store_in,
            owner_type,
            namespace,
            key,
          )
        None -> None
      }
  }
  case definition {
    Some(record) ->
      serialize_definition_selection(store_in, record, field, variables)
    None -> json.null()
  }
}

fn serialize_metafield_definitions_connection(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = read_args(field, variables)
  let definitions =
    store.list_effective_metafield_definitions(store_in)
    |> apply_definition_filters(args)
    |> sort_definitions(
      read_optional_string(args, "sortKey"),
      read_optional_bool(args, "reverse") |> option.unwrap(False),
    )
  let window =
    paginate_connection_items(
      definitions,
      field,
      variables,
      fn(definition, _index) { definition.id },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(definition, _index) { definition.id },
      serialize_node: fn(definition, node_field, _index) {
        serialize_definition_selection(
          store_in,
          definition,
          node_field,
          variables,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn apply_definition_filters(
  definitions: List(MetafieldDefinitionRecord),
  args: Dict(String, root_field.ResolvedValue),
) -> List(MetafieldDefinitionRecord) {
  let owner_type = read_optional_string(args, "ownerType")
  let namespace = read_optional_string(args, "namespace")
  let key = read_optional_string(args, "key")
  let pinned_status =
    read_optional_string(args, "pinnedStatus")
    |> option.unwrap("ANY")
  let query = read_optional_string(args, "query")
  definitions
  |> list.filter(fn(definition) {
    option_matches(owner_type, definition.owner_type)
    && option_matches(namespace, definition.namespace)
    && option_matches(key, definition.key)
    && pinned_status_matches(pinned_status, definition)
    && definition_query_matches(query, definition)
  })
}

fn option_matches(expected: Option(String), actual: String) -> Bool {
  case expected {
    Some(value) -> value == actual
    None -> True
  }
}

fn pinned_status_matches(
  status: String,
  definition: MetafieldDefinitionRecord,
) -> Bool {
  case status, definition.pinned_position {
    "PINNED", Some(_) -> True
    "PINNED", None -> False
    "UNPINNED", None -> True
    "UNPINNED", Some(_) -> False
    _, _ -> True
  }
}

fn definition_query_matches(
  raw_query: Option(String),
  definition: MetafieldDefinitionRecord,
) -> Bool {
  case raw_query {
    Some("key:" <> expected_key) -> definition.key == expected_key
    Some(query) ->
      string.contains(
        string.lowercase(definition.name),
        string.lowercase(query),
      )
      || string.contains(
        string.lowercase(definition.namespace),
        string.lowercase(query),
      )
      || string.contains(
        string.lowercase(definition.key),
        string.lowercase(query),
      )
    None -> True
  }
}

fn sort_definitions(
  definitions: List(MetafieldDefinitionRecord),
  sort_key: Option(String),
  reverse: Bool,
) -> List(MetafieldDefinitionRecord) {
  let sorted =
    definitions
    |> list.sort(fn(left, right) {
      case sort_key {
        Some("NAME") ->
          case string.compare(left.name, right.name) {
            order.Eq -> string.compare(left.id, right.id)
            other -> other
          }
        Some("PINNED_POSITION") -> {
          let left_position = option.unwrap(left.pinned_position, -1)
          let right_position = option.unwrap(right.pinned_position, -1)
          case int.compare(right_position, left_position) {
            order.Eq -> string.compare(right.id, left.id)
            other -> other
          }
        }
        _ -> string.compare(left.id, right.id)
      }
    })
  case reverse {
    True -> list.reverse(sorted)
    False -> sorted
  }
}

fn serialize_definition_selection(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "id" -> json.string(definition.id)
            "name" -> json.string(definition.name)
            "namespace" -> json.string(definition.namespace)
            "key" -> json.string(definition.key)
            "ownerType" -> json.string(definition.owner_type)
            "type" -> serialize_definition_type(definition.type_, selection)
            "description" -> optional_string(definition.description)
            "validations" ->
              json.array(definition.validations, fn(validation) {
                serialize_validation(validation, selection)
              })
            "access" -> serialize_json_object(definition.access, selection)
            "capabilities" ->
              serialize_capabilities(definition.capabilities, selection)
            "constraints" ->
              serialize_constraints(
                definition.constraints,
                selection,
                variables,
              )
            "pinnedPosition" -> optional_int(definition.pinned_position)
            "validationStatus" -> json.string(definition.validation_status)
            "metafieldsCount" ->
              json.int(
                get_product_metafields_for_definition(store_in, definition)
                |> list.length,
              )
            "metafields" ->
              serialize_definition_metafields_connection(
                store_in,
                definition,
                selection,
                variables,
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn serialize_definition_type(
  type_record: MetafieldDefinitionTypeRecord,
  field: Selection,
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "name" -> json.string(type_record.name)
            "category" -> optional_string(type_record.category)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn serialize_validation(
  validation: MetafieldDefinitionValidationRecord,
  field: Selection,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "name" -> json.string(validation.name)
              "value" -> optional_string(validation.value)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_json_object(values: Dict(String, Json), field: Selection) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            dict.get(values, name.value) |> result.unwrap(json.null())
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_capabilities(
  capabilities: MetafieldDefinitionCapabilitiesRecord,
  field: Selection,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let capability = case selection {
          Field(name: name, ..) ->
            case name.value {
              "adminFilterable" -> Some(capabilities.admin_filterable)
              "smartCollectionCondition" ->
                Some(capabilities.smart_collection_condition)
              "uniqueValues" -> Some(capabilities.unique_values)
              _ -> None
            }
          _ -> None
        }
        #(key, case capability {
          Some(c) -> serialize_capability(c, selection)
          None -> json.null()
        })
      },
    ),
  )
}

fn serialize_capability(
  capability: MetafieldDefinitionCapabilityRecord,
  field: Selection,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "enabled" -> json.bool(capability.enabled)
              "eligible" -> json.bool(capability.eligible)
              "status" -> optional_string(capability.status)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_constraints(
  constraints: Option(MetafieldDefinitionConstraintsRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case constraints {
    None -> json.null()
    Some(record) ->
      json.object(
        list.map(
          get_selected_child_fields(field, default_selected_field_options()),
          fn(selection) {
            let key = get_field_response_key(selection)
            let value = case selection {
              Field(name: name, ..) ->
                case name.value {
                  "key" -> optional_string(record.key)
                  "values" ->
                    serialize_constraint_values_connection(
                      record.values,
                      selection,
                      variables,
                    )
                  _ -> json.null()
                }
              _ -> json.null()
            }
            #(key, value)
          },
        ),
      )
  }
}

fn serialize_constraint_values_connection(
  values: List(MetafieldDefinitionConstraintValueRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let window =
    paginate_connection_items(
      values,
      field,
      variables,
      fn(value, _index) { value.value },
      default_connection_window_options(),
    )
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: False,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(value, _index) { value.value },
      serialize_node: fn(value, node_field, _index) {
        json.object(
          list.map(
            get_selected_child_fields(
              node_field,
              default_selected_field_options(),
            ),
            fn(selection) {
              let key = get_field_response_key(selection)
              let output = case selection {
                Field(name: name, ..) ->
                  case name.value {
                    "value" -> json.string(value.value)
                    _ -> json.null()
                  }
                _ -> json.null()
              }
              #(key, output)
            },
          ),
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: page_info_options,
    ),
  )
}

fn get_product_metafields_for_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> List(ProductMetafieldRecord) {
  case definition.owner_type {
    "PRODUCT" ->
      store_in
      |> all_effective_metafields()
      |> list.filter(fn(metafield) {
        metafield.owner_type == Some("PRODUCT")
        && metafield.namespace == definition.namespace
        && metafield.key == definition.key
      })
      |> list.sort(fn(left, right) { string.compare(left.id, right.id) })
    _ -> []
  }
}

fn product_metafield_owner_ids_for_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> List(String) {
  get_product_metafields_for_definition(store_in, definition)
  |> list.map(fn(metafield) { metafield.owner_id })
  |> dedupe_strings
}

fn all_effective_metafields(store_in: Store) -> List(ProductMetafieldRecord) {
  let owner_ids =
    list.append(
      dict.values(store_in.base_state.product_metafields),
      dict.values(store_in.staged_state.product_metafields),
    )
    |> list.map(fn(metafield) { metafield.owner_id })
    |> dedupe_strings
  list.flat_map(owner_ids, fn(owner_id) {
    store.get_effective_metafields_by_owner_id(store_in, owner_id)
  })
}

fn serialize_definition_metafields_connection(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = read_args(field, variables)
  let records = get_product_metafields_for_definition(store_in, definition)
  let ordered = case read_optional_bool(args, "reverse") {
    Some(True) -> list.reverse(records)
    _ -> records
  }
  metafields.serialize_metafields_connection(
    list.map(ordered, product_metafield_to_core),
    field,
    variables,
    default_selected_field_options(),
  )
}

fn product_metafield_to_core(
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

fn serialize_owner_root(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  owner_type: String,
  id_arg_name: String,
) -> Json {
  let args = read_args(field, variables)
  case read_optional_string(args, id_arg_name) {
    None -> json.null()
    Some(owner_id) ->
      serialize_owner_selection(
        store_in,
        field,
        variables,
        owner_id,
        owner_type,
      )
  }
}

fn serialize_owner_selection(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  owner_id: String,
  _owner_type: String,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "id" -> json.string(owner_id)
              "title" -> json.null()
              "handle" -> json.null()
              "metafield" ->
                serialize_owner_metafield(
                  store_in,
                  owner_id,
                  selection,
                  variables,
                )
              "metafields" ->
                serialize_owner_metafields_connection(
                  store_in,
                  owner_id,
                  selection,
                  variables,
                )
              "variants" ->
                serialize_product_variants_from_metafields(
                  store_in,
                  selection,
                  variables,
                )
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_owner_metafield(
  store_in: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = read_args(field, variables)
  let namespace = read_optional_string(args, "namespace")
  let key = read_optional_string(args, "key")
  let found =
    store.get_effective_metafields_by_owner_id(store_in, owner_id)
    |> list.find(fn(metafield) {
      metafield.namespace == option.unwrap(namespace, "")
      && metafield.key == option.unwrap(key, "")
    })
    |> option.from_result
  case found {
    Some(metafield) ->
      metafields.serialize_metafield_selection(
        product_metafield_to_core(metafield),
        field,
        default_selected_field_options(),
      )
    None -> json.null()
  }
}

fn serialize_owner_metafields_connection(
  store_in: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = read_args(field, variables)
  let namespace = read_optional_string(args, "namespace")
  let records =
    store.get_effective_metafields_by_owner_id(store_in, owner_id)
    |> list.filter(fn(metafield) {
      case namespace {
        Some(ns) -> metafield.namespace == ns
        None -> True
      }
    })
    |> list.map(product_metafield_to_core)
  metafields.serialize_metafields_connection(
    records,
    field,
    variables,
    default_selected_field_options(),
  )
}

fn serialize_product_variants_from_metafields(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let variant_ids =
    all_effective_metafields(store_in)
    |> list.filter(fn(metafield) {
      metafield.owner_type == Some("PRODUCTVARIANT")
    })
    |> list.map(fn(metafield) { metafield.owner_id })
    |> dedupe_strings
  let window =
    paginate_connection_items(
      variant_ids,
      field,
      variables,
      fn(id, _index) { id },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(id, _index) { id },
      serialize_node: fn(id, node_field, _index) {
        serialize_owner_selection(
          store_in,
          node_field,
          variables,
          id,
          "PRODUCTVARIANT",
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn standard_templates() -> List(StandardMetafieldDefinitionTemplate) {
  [
    StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/1",
      namespace: "descriptors",
      key: "subtitle",
      name: "Product subtitle",
      description: Some("Used as a shorthand for a product name"),
      owner_types: ["PRODUCT", "PRODUCTVARIANT"],
      type_: MetafieldDefinitionTypeRecord(
        name: "single_line_text_field",
        category: Some("TEXT"),
      ),
      validations: [
        MetafieldDefinitionValidationRecord(name: "max", value: Some("70")),
      ],
      visible_to_storefront_api: True,
    ),
    StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/2",
      namespace: "descriptors",
      key: "care_guide",
      name: "Care guide",
      description: Some("Instructions for taking care of a product or apparel"),
      owner_types: ["PRODUCT", "PRODUCTVARIANT"],
      type_: MetafieldDefinitionTypeRecord(
        name: "multi_line_text_field",
        category: Some("TEXT"),
      ),
      validations: [
        MetafieldDefinitionValidationRecord(name: "max", value: Some("500")),
      ],
      visible_to_storefront_api: True,
    ),
    StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/3",
      namespace: "facts",
      key: "isbn",
      name: "ISBN",
      description: Some("International Standard Book Number"),
      owner_types: ["PRODUCT", "PRODUCTVARIANT"],
      type_: MetafieldDefinitionTypeRecord(
        name: "single_line_text_field",
        category: Some("TEXT"),
      ),
      validations: [
        MetafieldDefinitionValidationRecord(
          name: "regex",
          value: Some(
            "^((\\d{3})?([\\-\\s])?(\\d{1,5})([\\-\\s])?(\\d{1,7})([\\-\\s])?(\\d{6})([\\-\\s])?(\\d{1}))$",
          ),
        ),
      ],
      visible_to_storefront_api: True,
    ),
  ]
}

fn find_standard_template(
  args: Dict(String, root_field.ResolvedValue),
) -> #(Option(StandardMetafieldDefinitionTemplate), List(UserError)) {
  let owner_type = read_optional_string(args, "ownerType")
  let id = read_optional_string(args, "id")
  let namespace = read_optional_string(args, "namespace")
  let key = read_optional_string(args, "key")
  case owner_type, id, namespace, key {
    None, _, _, _ | Some(_), None, None, _ | Some(_), None, _, None -> #(None, [
      UserError(
        field: None,
        message: "A namespace and key or standard metafield definition template id must be provided.",
        code: "TEMPLATE_NOT_FOUND",
      ),
    ])
    Some(owner), Some(template_id), _, _ -> {
      let found =
        standard_templates()
        |> list.find(fn(template) {
          template.id == template_id
          && list.contains(template.owner_types, owner)
        })
        |> option.from_result
      case found {
        Some(template) -> #(Some(template), [])
        None -> #(None, [
          UserError(
            field: Some(["id"]),
            message: "Id is not a valid standard metafield definition template id",
            code: "TEMPLATE_NOT_FOUND",
          ),
        ])
      }
    }
    Some(owner), None, Some(ns), Some(k) -> {
      let found =
        standard_templates()
        |> list.find(fn(template) {
          list.contains(template.owner_types, owner)
          && template.namespace == ns
          && template.key == k
        })
        |> option.from_result
      case found {
        Some(template) -> #(Some(template), [])
        None -> #(None, [
          UserError(
            field: None,
            message: "A standard definition wasn't found for the specified owner type, namespace, and key.",
            code: "TEMPLATE_NOT_FOUND",
          ),
        ])
      }
    }
  }
}

fn serialize_standard_metafield_definition_enable_mutation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  let #(template, user_errors) = find_standard_template(args)
  case template {
    None -> #(
      serialize_standard_enable_payload(
        store_in,
        field,
        variables,
        None,
        user_errors,
      ),
      store_in,
      identity,
      [],
    )
    Some(template_record) -> {
      let #(definition, next_identity) =
        build_enabled_standard_definition(
          store_in,
          identity,
          args,
          template_record,
        )
      let next_store =
        store.upsert_staged_metafield_definitions(store_in, [definition])
      #(
        serialize_standard_enable_payload(
          store_in,
          field,
          variables,
          Some(definition),
          [],
        ),
        next_store,
        next_identity,
        [definition.id],
      )
    }
  }
}

fn build_enabled_standard_definition(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
  template: StandardMetafieldDefinitionTemplate,
) -> #(MetafieldDefinitionRecord, SyntheticIdentityRegistry) {
  let owner_type =
    read_optional_string(args, "ownerType")
    |> option.unwrap(
      list.first(template.owner_types) |> result.unwrap("PRODUCT"),
    )
  let existing =
    store.find_effective_metafield_definition(
      store_in,
      owner_type,
      template.namespace,
      template.key,
    )
  let #(id, next_identity) = case existing {
    Some(definition) -> #(definition.id, identity)
    None ->
      synthetic_identity.make_synthetic_gid(identity, "MetafieldDefinition")
  }
  let pinned_position = case read_optional_bool(args, "pin") {
    Some(True) -> Some(next_pinned_position(store_in, owner_type, existing))
    _ -> None
  }
  #(
    MetafieldDefinitionRecord(
      id: id,
      name: template.name,
      namespace: template.namespace,
      key: template.key,
      owner_type: owner_type,
      type_: template.type_,
      description: template.description,
      validations: template.validations,
      access: build_standard_access(args, template),
      capabilities: build_definition_capabilities(read_object(
        args,
        "capabilities",
      )),
      constraints: Some(
        MetafieldDefinitionConstraintsRecord(key: None, values: []),
      ),
      pinned_position: pinned_position,
      validation_status: "ALL_VALID",
    ),
    next_identity,
  )
}

fn build_standard_access(
  args: Dict(String, root_field.ResolvedValue),
  template: StandardMetafieldDefinitionTemplate,
) -> Dict(String, Json) {
  let access = read_object(args, "access")
  dict.from_list([
    #(
      "admin",
      json.string(
        read_optional_string(access, "admin")
        |> option.unwrap("PUBLIC_READ_WRITE"),
      ),
    ),
    #(
      "storefront",
      json.string(
        read_optional_string(access, "storefront")
        |> option.unwrap(case template.visible_to_storefront_api {
          True -> "PUBLIC_READ"
          False -> "NONE"
        }),
      ),
    ),
    #(
      "customerAccount",
      json.string(
        read_optional_string(access, "customerAccount") |> option.unwrap("NONE"),
      ),
    ),
  ])
}

fn build_input_access(
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, Json) {
  let access = read_object(input, "access")
  dict.from_list([
    #(
      "admin",
      json.string(
        read_optional_string(access, "admin")
        |> option.unwrap("PUBLIC_READ_WRITE"),
      ),
    ),
    #(
      "storefront",
      json.string(
        read_optional_string(access, "storefront") |> option.unwrap("NONE"),
      ),
    ),
    #(
      "customerAccount",
      json.string(
        read_optional_string(access, "customerAccount") |> option.unwrap("NONE"),
      ),
    ),
  ])
}

fn build_definition_capabilities(
  capabilities: Dict(String, root_field.ResolvedValue),
) -> MetafieldDefinitionCapabilitiesRecord {
  let admin_filterable = capability_enabled(capabilities, "adminFilterable")
  let smart_collection =
    capability_enabled(capabilities, "smartCollectionCondition")
  let unique_values = capability_enabled(capabilities, "uniqueValues")
  MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: MetafieldDefinitionCapabilityRecord(
      enabled: admin_filterable,
      eligible: True,
      status: Some(case admin_filterable {
        True -> "FILTERABLE"
        False -> "NOT_FILTERABLE"
      }),
    ),
    smart_collection_condition: MetafieldDefinitionCapabilityRecord(
      enabled: smart_collection,
      eligible: True,
      status: None,
    ),
    unique_values: MetafieldDefinitionCapabilityRecord(
      enabled: unique_values,
      eligible: True,
      status: None,
    ),
  )
}

fn capability_enabled(
  capabilities: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(capabilities, key) {
    Ok(root_field.ObjectVal(value)) ->
      read_optional_bool(value, "enabled") |> option.unwrap(False)
    _ -> False
  }
}

fn serialize_standard_enable_payload(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  created_definition: Option(MetafieldDefinitionRecord),
  user_errors: List(UserError),
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "createdDefinition" ->
                case created_definition {
                  Some(definition) ->
                    serialize_definition_selection(
                      store_in,
                      definition,
                      selection,
                      variables,
                    )
                  None -> json.null()
                }
              "userErrors" ->
                json.array(user_errors, fn(error) {
                  serialize_standard_user_error(error, selection)
                })
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_standard_user_error(error: UserError, field: Selection) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "__typename" ->
                json.string("StandardMetafieldDefinitionEnableUserError")
              "field" -> optional_string_list(error.field)
              "message" -> json.string(error.message)
              "code" -> json.string(error.code)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_definition_create_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  let input = read_object(args, "definition")
  let errors = validate_product_owner_definition_input(input, True)
  case errors {
    [_, ..] -> #(
      serialize_definition_mutation_payload(
        store_in,
        "createdDefinition",
        None,
        errors,
        field,
        variables,
      ),
      store_in,
      identity,
      [],
    )
    [] -> {
      let existing =
        store.find_effective_metafield_definition(
          store_in,
          read_optional_string(input, "ownerType") |> option.unwrap("PRODUCT"),
          read_optional_string(input, "namespace") |> option.unwrap(""),
          read_optional_string(input, "key") |> option.unwrap(""),
        )
      case existing {
        Some(_) -> #(
          serialize_definition_mutation_payload(
            store_in,
            "createdDefinition",
            None,
            [
              UserError(
                field: Some(["definition"]),
                message: "A metafield definition already exists for this owner type, namespace, and key.",
                code: "TAKEN",
              ),
            ],
            field,
            variables,
          ),
          store_in,
          identity,
          [],
        )
        None -> {
          let #(definition, next_identity) =
            build_definition_from_input(store_in, identity, input)
          let next_store =
            store.upsert_staged_metafield_definitions(store_in, [definition])
          #(
            serialize_definition_mutation_payload(
              store_in,
              "createdDefinition",
              Some(definition),
              [],
              field,
              variables,
            ),
            next_store,
            next_identity,
            [definition.id],
          )
        }
      }
    }
  }
}

fn serialize_definition_update_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  let input = read_object(args, "definition")
  let errors = validate_product_owner_definition_input(input, False)
  case errors {
    [_, ..] -> #(
      serialize_definition_mutation_payload(
        store_in,
        "updatedDefinition",
        None,
        errors,
        field,
        variables,
      ),
      store_in,
      identity,
      [],
    )
    [] -> {
      let existing =
        store.find_effective_metafield_definition(
          store_in,
          read_optional_string(input, "ownerType") |> option.unwrap("PRODUCT"),
          read_optional_string(input, "namespace") |> option.unwrap(""),
          read_optional_string(input, "key") |> option.unwrap(""),
        )
      case existing {
        None -> #(
          serialize_definition_mutation_payload(
            store_in,
            "updatedDefinition",
            None,
            [
              UserError(
                field: Some(["definition"]),
                message: "Definition not found.",
                code: "NOT_FOUND",
              ),
            ],
            field,
            variables,
          ),
          store_in,
          identity,
          [],
        )
        Some(definition) -> {
          let requested_type = read_optional_string(input, "type")
          case requested_type {
            Some(type_name) ->
              case type_name != definition.type_.name {
                True -> #(
                  serialize_definition_mutation_payload(
                    store_in,
                    "updatedDefinition",
                    None,
                    [
                      UserError(
                        field: Some(["definition", "type"]),
                        message: "Type can't be changed.",
                        code: "IMMUTABLE",
                      ),
                    ],
                    field,
                    variables,
                  ),
                  store_in,
                  identity,
                  [],
                )
                False ->
                  update_definition_success(
                    store_in,
                    identity,
                    field,
                    variables,
                    input,
                    definition,
                  )
              }
            _ -> {
              update_definition_success(
                store_in,
                identity,
                field,
                variables,
                input,
                definition,
              )
            }
          }
        }
      }
    }
  }
}

fn serialize_definition_delete_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  let definition = find_definition_from_args(store_in, args)
  case definition {
    None -> #(
      serialize_definition_delete_payload(
        None,
        [
          UserError(
            field: Some(definition_reference_field(args)),
            message: "Definition not found.",
            code: "NOT_FOUND",
          ),
        ],
        field,
      ),
      store_in,
      identity,
      [],
    )
    Some(record) -> {
      let associated_product_owner_ids =
        product_metafield_owner_ids_for_definition(store_in, record)
      let store_after_metafields = case
        read_optional_bool(args, "deleteAllAssociatedMetafields")
      {
        Some(True) ->
          store.delete_product_metafields_for_definition(store_in, record)
        _ -> store_in
      }
      let next_store =
        stage_delete_definition(store_after_metafields, record)
        |> ensure_product_shells(associated_product_owner_ids)
      #(
        serialize_definition_delete_payload(Some(record), [], field),
        next_store,
        identity,
        [record.id],
      )
    }
  }
}

fn update_definition_success(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  input: Dict(String, root_field.ResolvedValue),
  definition: MetafieldDefinitionRecord,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let updated =
    MetafieldDefinitionRecord(
      ..definition,
      name: read_optional_string(input, "name")
        |> option.unwrap(definition.name),
      description: case has_field(input, "description") {
        True -> read_optional_string(input, "description")
        False -> definition.description
      },
      validations: case has_field(input, "validations") {
        True -> read_validation_records(input)
        False -> definition.validations
      },
      access: case has_field(input, "access") {
        True -> build_input_access(input)
        False -> definition.access
      },
      capabilities: case has_field(input, "capabilities") {
        True ->
          build_definition_capabilities(read_object(input, "capabilities"))
        False -> definition.capabilities
      },
    )
  let next_store =
    store.upsert_staged_metafield_definitions(store_in, [updated])
  #(
    serialize_definition_mutation_payload(
      store_in,
      "updatedDefinition",
      Some(updated),
      [],
      field,
      variables,
    ),
    next_store,
    identity,
    [updated.id],
  )
}

fn serialize_definition_pin_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  // Pattern 2: pinning is a supported local mutation, but a cold LiveHybrid
  // request may target an upstream definition. Hydrate the definition catalog
  // first, then stage only the pin effect locally. Snapshot/no-transport mode
  // keeps the current local not-found behavior.
  let store_in = maybe_hydrate_definition_for_args(store_in, args, upstream)
  case find_definition_from_args(store_in, args) {
    None -> #(
      serialize_pinning_payload(
        store_in,
        "pinnedDefinition",
        None,
        [
          UserError(
            field: Some(definition_reference_field(args)),
            message: "Definition not found.",
            code: "NOT_FOUND",
          ),
        ],
        field,
        variables,
      ),
      store_in,
      identity,
      [],
    )
    Some(definition) -> {
      let pinned = pin_definition(store_in, definition)
      let next_store =
        store.upsert_staged_metafield_definitions(store_in, [pinned])
      #(
        serialize_pinning_payload(
          store_in,
          "pinnedDefinition",
          Some(pinned),
          [],
          field,
          variables,
        ),
        next_store,
        identity,
        [pinned.id],
      )
    }
  }
}

fn serialize_definition_unpin_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  // Pattern 2: unpinning mirrors pinning — hydrate any upstream definition
  // before applying the local stage so downstream reads observe local state.
  let store_in = maybe_hydrate_definition_for_args(store_in, args, upstream)
  case find_definition_from_args(store_in, args) {
    None -> #(
      serialize_pinning_payload(
        store_in,
        "unpinnedDefinition",
        None,
        [
          UserError(
            field: Some(definition_reference_field(args)),
            message: "Definition not found.",
            code: "NOT_FOUND",
          ),
        ],
        field,
        variables,
      ),
      store_in,
      identity,
      [],
    )
    Some(definition) -> {
      let #(unpinned, compacted) = unpin_definition(store_in, definition)
      let next_store =
        store.upsert_staged_metafield_definitions(store_in, compacted)
      #(
        serialize_pinning_payload(
          store_in,
          "unpinnedDefinition",
          Some(unpinned),
          [],
          field,
          variables,
        ),
        next_store,
        identity,
        [unpinned.id],
      )
    }
  }
}

fn validate_product_owner_definition_input(
  input: Dict(String, root_field.ResolvedValue),
  create: Bool,
) -> List(UserError) {
  let required = case create {
    True -> ["namespace", "key", "ownerType", "name", "type"]
    False -> ["namespace", "key", "ownerType"]
  }
  let errors =
    list.filter_map(required, fn(field_name) {
      case read_optional_string(input, field_name) {
        Some(value) ->
          case string.trim(value) {
            "" -> Ok(blank_definition_error(field_name))
            _ -> Error(Nil)
          }
        None -> Ok(blank_definition_error(field_name))
      }
    })
  let owner_type_error = case read_optional_string(input, "ownerType") {
    Some("PRODUCT") | None -> []
    Some(_) -> [
      UserError(
        field: Some(["definition", "ownerType"]),
        message: "Only PRODUCT metafield definitions are supported locally.",
        code: "UNSUPPORTED_OWNER_TYPE",
      ),
    ]
  }
  list.append(errors, owner_type_error)
}

fn blank_definition_error(field_name: String) -> UserError {
  UserError(
    field: Some(["definition", field_name]),
    message: field_name <> " is required.",
    code: "BLANK",
  )
}

fn build_definition_from_input(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MetafieldDefinitionRecord, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "MetafieldDefinition")
  let type_name =
    read_optional_string(input, "type")
    |> option.unwrap("single_line_text_field")
  let owner_type =
    read_optional_string(input, "ownerType") |> option.unwrap("PRODUCT")
  #(
    MetafieldDefinitionRecord(
      id: id,
      name: read_optional_string(input, "name") |> option.unwrap(""),
      namespace: read_optional_string(input, "namespace") |> option.unwrap(""),
      key: read_optional_string(input, "key") |> option.unwrap(""),
      owner_type: owner_type,
      type_: MetafieldDefinitionTypeRecord(
        name: type_name,
        category: infer_definition_type_category(type_name),
      ),
      description: read_optional_string(input, "description"),
      validations: read_validation_records(input),
      access: build_input_access(input),
      capabilities: build_definition_capabilities(read_object(
        input,
        "capabilities",
      )),
      constraints: Some(
        MetafieldDefinitionConstraintsRecord(key: None, values: []),
      ),
      pinned_position: case read_optional_bool(input, "pin") {
        Some(True) -> Some(next_pinned_position(store_in, owner_type, None))
        _ -> None
      },
      validation_status: "ALL_VALID",
    ),
    next_identity,
  )
}

fn infer_definition_type_category(type_name: String) -> Option(String) {
  case type_name {
    "url" | "color" -> Some("TEXT")
    "rating" | "boolean" -> Some("NUMBER")
    "dimension" | "volume" | "weight" -> Some("MEASUREMENT")
    "date" | "date_time" -> Some("DATE_TIME")
    _ ->
      case string.contains(type_name, "text") {
        True -> Some("TEXT")
        False ->
          case string.starts_with(type_name, "number_") {
            True -> Some("NUMBER")
            False ->
              case string.contains(type_name, "reference") {
                True -> Some("REFERENCE")
                False -> None
              }
          }
      }
  }
}

fn read_validation_records(
  input: Dict(String, root_field.ResolvedValue),
) -> List(MetafieldDefinitionValidationRecord) {
  read_input_objects(input, "validations")
  |> list.filter_map(fn(record) {
    case read_optional_string(record, "name") {
      Some(name) ->
        case name {
          "" -> Error(Nil)
          _ ->
            Ok(MetafieldDefinitionValidationRecord(
              name: name,
              value: read_optional_string(record, "value"),
            ))
        }
      None -> Error(Nil)
    }
  })
}

fn serialize_definition_mutation_payload(
  store_in: Store,
  definition_field_name: String,
  definition: Option(MetafieldDefinitionRecord),
  user_errors: List(UserError),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value == definition_field_name, name.value {
              True, _ ->
                case definition {
                  Some(record) ->
                    serialize_definition_selection(
                      store_in,
                      record,
                      selection,
                      variables,
                    )
                  None -> json.null()
                }
              False, "userErrors" ->
                json.array(user_errors, fn(error) {
                  serialize_definition_user_error(error, selection)
                })
              False, "validationJob" -> json.null()
              False, _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_definition_delete_payload(
  deleted_definition: Option(MetafieldDefinitionRecord),
  user_errors: List(UserError),
  field: Selection,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "deletedDefinitionId" ->
                case deleted_definition {
                  Some(record) -> json.string(record.id)
                  None -> json.null()
                }
              "deletedDefinition" ->
                case deleted_definition {
                  Some(record) ->
                    serialize_deleted_definition_identifier(record, selection)
                  None -> json.null()
                }
              "userErrors" ->
                json.array(user_errors, fn(error) {
                  serialize_definition_user_error(error, selection)
                })
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_deleted_definition_identifier(
  definition: MetafieldDefinitionRecord,
  field: Selection,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "ownerType" -> json.string(definition.owner_type)
              "namespace" -> json.string(definition.namespace)
              "key" -> json.string(definition.key)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_pinning_payload(
  store_in: Store,
  payload_field_name: String,
  definition: Option(MetafieldDefinitionRecord),
  user_errors: List(UserError),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value == payload_field_name, name.value {
              True, _ ->
                case definition {
                  Some(record) ->
                    serialize_definition_selection(
                      store_in,
                      record,
                      selection,
                      variables,
                    )
                  None -> json.null()
                }
              False, "userErrors" ->
                json.array(user_errors, fn(error) {
                  serialize_definition_user_error(error, selection)
                })
              False, _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_definition_user_error(error: UserError, field: Selection) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "field" -> optional_string_list(error.field)
              "message" -> json.string(error.message)
              "code" -> json.string(error.code)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn list_pinned_definitions(
  store_in: Store,
  owner_type: String,
) -> List(MetafieldDefinitionRecord) {
  store.list_effective_metafield_definitions(store_in)
  |> list.filter(fn(definition) {
    definition.owner_type == owner_type && definition.pinned_position != None
  })
}

fn next_pinned_position(
  store_in: Store,
  owner_type: String,
  existing: Option(MetafieldDefinitionRecord),
) -> Int {
  case existing {
    Some(definition) ->
      case definition.pinned_position {
        Some(pos) -> pos
        None ->
          highest_pinned_position(store_in, owner_type, Some(definition.id)) + 1
      }
    None -> highest_pinned_position(store_in, owner_type, None) + 1
  }
}

fn highest_pinned_position(
  store_in: Store,
  owner_type: String,
  skip_id: Option(String),
) -> Int {
  list.fold(
    list_pinned_definitions(store_in, owner_type),
    0,
    fn(highest, definition) {
      case skip_id == Some(definition.id) {
        True -> highest
        False -> int.max(highest, option.unwrap(definition.pinned_position, 0))
      }
    },
  )
}

fn pin_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> MetafieldDefinitionRecord {
  case definition.pinned_position {
    Some(_) -> definition
    None ->
      MetafieldDefinitionRecord(
        ..definition,
        pinned_position: Some(next_pinned_position(
          store_in,
          definition.owner_type,
          None,
        )),
      )
  }
}

fn unpin_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> #(MetafieldDefinitionRecord, List(MetafieldDefinitionRecord)) {
  case definition.pinned_position {
    None -> #(definition, [definition])
    Some(removed_position) -> {
      let unpinned =
        MetafieldDefinitionRecord(..definition, pinned_position: None)
      let compacted =
        list_pinned_definitions(store_in, definition.owner_type)
        |> list.filter(fn(candidate) {
          candidate.id != definition.id
          && option.unwrap(candidate.pinned_position, 0) > removed_position
        })
        |> list.map(fn(candidate) {
          MetafieldDefinitionRecord(
            ..candidate,
            pinned_position: Some(
              option.unwrap(candidate.pinned_position, 1) - 1,
            ),
          )
        })
      #(unpinned, [unpinned, ..compacted])
    }
  }
}

fn stage_delete_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> Store {
  let compacted_store = case definition.pinned_position {
    None -> store_in
    Some(removed_position) -> {
      let compacted =
        list_pinned_definitions(store_in, definition.owner_type)
        |> list.filter(fn(candidate) {
          candidate.id != definition.id
          && option.unwrap(candidate.pinned_position, 0) > removed_position
        })
        |> list.map(fn(candidate) {
          MetafieldDefinitionRecord(
            ..candidate,
            pinned_position: Some(
              option.unwrap(candidate.pinned_position, 1) - 1,
            ),
          )
        })
      store.upsert_staged_metafield_definitions(store_in, compacted)
    }
  }
  store.stage_delete_metafield_definition(compacted_store, definition.id)
}

fn ensure_product_shells(store_in: Store, product_ids: List(String)) -> Store {
  list.fold(product_ids, store_in, fn(current, product_id) {
    case store.get_effective_product_by_id(current, product_id) {
      Some(_) -> current
      None -> {
        let #(_, next_store) =
          store.upsert_staged_product(
            current,
            minimal_product_shell(product_id),
          )
        next_store
      }
    }
  })
}

fn minimal_product_shell(product_id: String) -> ProductRecord {
  ProductRecord(
    id: product_id,
    legacy_resource_id: None,
    title: "",
    handle: "",
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: [],
    price_range_min: None,
    price_range_max: None,
    total_variants: None,
    has_only_default_variant: None,
    has_out_of_stock_variants: None,
    total_inventory: None,
    tracks_inventory: None,
    created_at: None,
    updated_at: None,
    published_at: None,
    description_html: "",
    online_store_preview_url: None,
    template_suffix: None,
    seo: ProductSeoRecord(title: None, description: None),
    category: None,
    publication_ids: [],
    contextual_pricing: None,
    cursor: None,
  )
}

fn maybe_hydrate_definition_for_args(
  store_in: Store,
  args: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  case find_definition_from_args(store_in, args) {
    Some(_) -> store_in
    None ->
      case read_definition_identifier(args) {
        Some(#(owner_type, namespace, _key)) ->
          hydrate_definitions_by_namespace(
            store_in,
            owner_type,
            namespace,
            upstream,
          )
        None ->
          case read_definition_id_arg(args) {
            Some(id) -> hydrate_definition_by_id(store_in, id, upstream)
            None -> store_in
          }
      }
  }
}

fn read_definition_id_arg(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  read_optional_string(args, "definitionId")
  |> option.or(read_optional_string(args, "id"))
}

fn hydrate_definitions_by_namespace(
  store_in: Store,
  owner_type: String,
  namespace: String,
  upstream: UpstreamContext,
) -> Store {
  let query =
    "query MetafieldDefinitionsHydrateByNamespace($ownerType: MetafieldOwnerType!, $namespace: String!) {
  metafieldDefinitions(ownerType: $ownerType, first: 50, namespace: $namespace, sortKey: PINNED_POSITION) {
"
    <> metafield_definition_hydrate_selection("    ")
    <> "  }\n"
    <> "}\n"
  let variables =
    json.object([
      #("ownerType", json.string(owner_type)),
      #("namespace", json.string(namespace)),
    ])
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "MetafieldDefinitionsHydrateByNamespace",
      query,
      variables,
    )
  {
    Ok(value) ->
      metafield_definitions_from_hydrate_response(value)
      |> upsert_hydrated_definitions(store_in)
    Error(_) -> store_in
  }
}

fn hydrate_definition_by_id(
  store_in: Store,
  id: String,
  upstream: UpstreamContext,
) -> Store {
  let query = "query MetafieldDefinitionHydrateById($id: ID!) {
  metafieldDefinition(id: $id) {
" <> metafield_definition_node_hydrate_selection("    ") <> "  }\n" <> "}\n"
  let variables = json.object([#("id", json.string(id))])
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "MetafieldDefinitionHydrateById",
      query,
      variables,
    )
  {
    Ok(value) ->
      metafield_definitions_from_hydrate_response(value)
      |> upsert_hydrated_definitions(store_in)
    Error(_) -> store_in
  }
}

fn upsert_hydrated_definitions(
  definitions: List(MetafieldDefinitionRecord),
  store_in: Store,
) -> Store {
  case definitions {
    [] -> store_in
    [_, ..] -> store.upsert_base_metafield_definitions(store_in, definitions)
  }
}

fn metafield_definition_hydrate_selection(indent: String) -> String {
  indent
  <> "nodes {\n"
  <> metafield_definition_node_hydrate_selection(indent <> "  ")
  <> indent
  <> "}\n"
}

fn metafield_definition_node_hydrate_selection(indent: String) -> String {
  indent
  <> "id\n"
  <> indent
  <> "name\n"
  <> indent
  <> "namespace\n"
  <> indent
  <> "key\n"
  <> indent
  <> "ownerType\n"
  <> indent
  <> "type { name category }\n"
  <> indent
  <> "description\n"
  <> indent
  <> "validations { name value }\n"
  <> indent
  <> "access { admin storefront customerAccount }\n"
  <> indent
  <> "capabilities {\n"
  <> indent
  <> "  adminFilterable { enabled eligible status }\n"
  <> indent
  <> "  smartCollectionCondition { enabled eligible }\n"
  <> indent
  <> "  uniqueValues { enabled eligible }\n"
  <> indent
  <> "}\n"
  <> indent
  <> "constraints { key values(first: 10) { nodes { value } } }\n"
  <> indent
  <> "pinnedPosition\n"
  <> indent
  <> "validationStatus\n"
}

fn serialize_metafields_delete_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  let inputs = read_input_objects(args, "metafields")
  let result = delete_metafields_by_identifiers(store_in, inputs)
  #(
    serialize_metafields_delete_payload(result, field),
    result.store,
    identity,
    deleted_identifiers_to_stage_ids(result.deleted_metafields),
  )
}

fn serialize_metafield_delete_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  let input = read_object(args, "input")
  case read_optional_string(input, "id") {
    None -> #(
      serialize_metafield_delete_payload(
        None,
        [SimpleUserError(["input", "id"], "Metafield id is required")],
        field,
      ),
      store_in,
      identity,
      [],
    )
    Some(metafield_id) -> {
      case store.find_effective_metafield_by_id(store_in, metafield_id) {
        None -> #(
          serialize_metafield_delete_payload(Some(metafield_id), [], field),
          store_in,
          identity,
          [metafield_id],
        )
        Some(record) -> {
          let result =
            delete_metafields_by_identifiers(store_in, [
              dict.from_list([
                #("ownerId", root_field.StringVal(record.owner_id)),
                #("namespace", root_field.StringVal(record.namespace)),
                #("key", root_field.StringVal(record.key)),
              ]),
            ])
          let deleted_id = case result.user_errors {
            [] -> Some(metafield_id)
            [_, ..] -> None
          }
          let stage_ids = case deleted_id {
            Some(id) -> [id]
            None -> []
          }
          #(
            serialize_metafield_delete_payload(
              deleted_id,
              result.user_errors,
              field,
            ),
            result.store,
            identity,
            stage_ids,
          )
        }
      }
    }
  }
}

fn delete_metafields_by_identifiers(
  store_in: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> MetafieldsDeleteResult {
  let errors = validate_metafields_delete_inputs(inputs)
  case errors {
    [_, ..] ->
      MetafieldsDeleteResult(
        deleted_metafields: [],
        user_errors: errors,
        store: store_in,
      )
    [] -> {
      let #(store_out, deleted) =
        list.fold(inputs, #(store_in, []), fn(acc, input) {
          let #(current_store, deleted_acc) = acc
          case read_metafield_delete_identifier(input) {
            None -> acc
            Some(#(owner_id, namespace, key)) -> {
              let existing =
                find_owner_metafield(
                  current_store,
                  owner_id,
                  namespace,
                  Some(key),
                )
              case existing {
                None -> #(current_store, list.append(deleted_acc, [None]))
                Some(_) -> {
                  let remaining =
                    store.get_effective_metafields_by_owner_id(
                      current_store,
                      owner_id,
                    )
                    |> list.filter(fn(record) {
                      !{ record.namespace == namespace && record.key == key }
                    })
                  let next_store =
                    store.replace_staged_metafields_for_owner(
                      current_store,
                      owner_id,
                      remaining,
                    )
                  #(
                    next_store,
                    list.append(deleted_acc, [
                      Some(DeletedMetafieldIdentifier(owner_id, namespace, key)),
                    ]),
                  )
                }
              }
            }
          }
        })
      MetafieldsDeleteResult(
        deleted_metafields: deleted,
        user_errors: [],
        store: store_out,
      )
    }
  }
}

fn validate_metafields_delete_inputs(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(SimpleUserError) {
  inputs
  |> enumerate
  |> list.filter_map(fn(pair) {
    let #(index, input) = pair
    case read_optional_string(input, "ownerId") {
      None ->
        Ok(SimpleUserError(
          ["metafields", int.to_string(index), "ownerId"],
          "Owner id is required",
        ))
      Some(_) ->
        case read_optional_string(input, "namespace") {
          None ->
            Ok(SimpleUserError(
              ["metafields", int.to_string(index), "namespace"],
              "Namespace is required",
            ))
          Some(_) ->
            case read_optional_string(input, "key") {
              None ->
                Ok(SimpleUserError(
                  ["metafields", int.to_string(index), "key"],
                  "Key is required",
                ))
              Some(_) -> Error(Nil)
            }
        }
    }
  })
}

fn read_metafield_delete_identifier(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(#(String, String, String)) {
  case
    read_optional_string(input, "ownerId"),
    read_optional_string(input, "namespace"),
    read_optional_string(input, "key")
  {
    Some(owner_id), Some(namespace), Some(key) ->
      Some(#(owner_id, namespace, key))
    _, _, _ -> None
  }
}

fn serialize_metafields_delete_payload(
  result: MetafieldsDeleteResult,
  field: Selection,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "deletedMetafields" ->
                serialize_deleted_metafield_identifiers(
                  result.deleted_metafields,
                  selection,
                )
              "userErrors" ->
                serialize_simple_user_errors(selection, result.user_errors)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_metafield_delete_payload(
  deleted_id: Option(String),
  errors: List(SimpleUserError),
  field: Selection,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "deletedId" -> optional_string(deleted_id)
              "userErrors" -> serialize_simple_user_errors(selection, errors)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn serialize_deleted_metafield_identifiers(
  identifiers: List(Option(DeletedMetafieldIdentifier)),
  field: Selection,
) -> Json {
  json.array(identifiers, fn(identifier) {
    case identifier {
      None -> json.null()
      Some(record) ->
        json.object(
          list.map(
            get_selected_child_fields(field, default_selected_field_options()),
            fn(selection) {
              let key = get_field_response_key(selection)
              let value = case selection {
                Field(name: name, ..) ->
                  case name.value {
                    "ownerId" -> json.string(record.owner_id)
                    "namespace" -> json.string(record.namespace)
                    "key" -> json.string(record.key)
                    _ -> json.null()
                  }
                _ -> json.null()
              }
              #(key, value)
            },
          ),
        )
    }
  })
}

fn serialize_simple_user_errors(
  field: Selection,
  errors: List(SimpleUserError),
) -> Json {
  json.array(errors, fn(error) {
    json.object(
      list.map(
        get_selected_child_fields(field, default_selected_field_options()),
        fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "field" -> json.array(error.field, json.string)
                "message" -> json.string(error.message)
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        },
      ),
    )
  })
}

fn deleted_identifiers_to_stage_ids(
  identifiers: List(Option(DeletedMetafieldIdentifier)),
) -> List(String) {
  identifiers
  |> list.filter_map(fn(identifier) {
    case identifier {
      Some(record) -> Ok(record.owner_id)
      None -> Error(Nil)
    }
  })
  |> dedupe_strings
}

fn serialize_metafields_set_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = read_args(field, variables)
  let inputs = read_input_objects(args, "metafields")
  let errors = validate_metafields_set_inputs(store_in, inputs)
  case errors {
    [_, ..] -> #(
      serialize_metafields_set_payload([], errors, field),
      store_in,
      identity,
      [],
    )
    [] -> {
      let #(created, next_store, next_identity) =
        upsert_metafields_set_inputs(store_in, identity, inputs)
      #(
        serialize_metafields_set_payload(created, [], field),
        next_store,
        next_identity,
        list.map(created, fn(record) { record.id }),
      )
    }
  }
}

fn serialize_metafields_set_payload(
  records: List(ProductMetafieldRecord),
  errors: List(MetafieldsSetUserError),
  field: Selection,
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "metafields" ->
                case
                  errors != []
                  && list.any(errors, fn(error) {
                    error.code == Some("LESS_THAN_OR_EQUAL_TO")
                  })
                {
                  True -> json.null()
                  False -> serialize_metafield_payload(records, field)
                }
              "userErrors" ->
                serialize_metafields_set_user_errors(field, errors)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

fn validate_metafields_set_inputs(
  store_in: Store,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(MetafieldsSetUserError) {
  let initial = case inputs {
    [] -> [
      make_metafields_set_user_error(
        None,
        None,
        "At least one metafield input is required.",
        "BLANK",
      ),
    ]
    _ -> []
  }
  let initial = case list.length(inputs) > 25 {
    True -> [
      make_metafields_set_user_error(
        None,
        None,
        "Exceeded the maximum metafields input limit of 25.",
        "LESS_THAN_OR_EQUAL_TO",
      ),
      ..initial
    ]
    False -> initial
  }
  let indexed = enumerate(inputs)
  list.fold(indexed, initial, fn(errors, pair) {
    let #(index, input) = pair
    list.append(errors, validate_metafields_set_input(store_in, input, index))
  })
}

fn validate_metafields_set_input(
  store_in: Store,
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(MetafieldsSetUserError) {
  let owner_id = read_optional_string(input, "ownerId")
  let namespace = read_metafields_set_namespace(input)
  let key = read_optional_string(input, "key")
  let value = read_optional_string(input, "value")
  case owner_id {
    None -> [
      make_metafields_set_user_error(
        Some(index),
        Some("ownerId"),
        "Owner id is required.",
        "BLANK",
      ),
    ]
    Some(owner) ->
      case owner_type_from_id(owner) {
        None -> [
          make_metafields_set_user_error(
            Some(index),
            Some("ownerId"),
            "Owner does not exist.",
            "INVALID",
          ),
        ]
        Some(owner_type) -> {
          let existing = find_owner_metafield(store_in, owner, namespace, key)
          let definition = case key {
            Some(k) ->
              store.find_effective_metafield_definition(
                store_in,
                owner_type,
                namespace,
                k,
              )
            None -> None
          }
          let input_type = read_optional_string(input, "type")
          let type_ =
            input_type
            |> option.or(case definition {
              Some(d) -> Some(d.type_.name)
              None -> None
            })
            |> option.or(case existing {
              Some(m) -> m.type_
              None -> None
            })
          let errors = []
          let errors = case key {
            Some(k) ->
              case string.trim(k) {
                "" -> [
                  make_metafields_set_user_error(
                    Some(index),
                    Some("key"),
                    "Key is required.",
                    "BLANK",
                  ),
                  ..errors
                ]
                _ -> errors
              }
            None -> [
              make_metafields_set_user_error(
                Some(index),
                Some("key"),
                "Key is required.",
                "BLANK",
              ),
              ..errors
            ]
          }
          let errors = case type_ {
            Some(_) -> errors
            None -> [
              MetafieldsSetUserError(
                field: ["metafields", int.to_string(index), "type"],
                message: "Type can't be blank",
                code: Some("BLANK"),
                element_index: None,
              ),
              ..errors
            ]
          }
          let errors = case value {
            Some(_) -> errors
            None -> [
              make_metafields_set_user_error(
                Some(index),
                Some("value"),
                "Value is required.",
                "BLANK",
              ),
              ..errors
            ]
          }
          let errors = case definition, input_type {
            Some(def), Some(input_type_name) ->
              case input_type_name != def.type_.name {
                True -> [
                  make_metafields_set_user_error(
                    Some(index),
                    Some("type"),
                    "Type must be "
                      <> def.type_.name
                      <> " for this metafield definition.",
                    "INVALID_TYPE",
                  ),
                  ..errors
                ]
                False -> errors
              }
            _, _ -> errors
          }
          let errors =
            list.append(
              errors,
              validate_definition_value(definition, value, index),
            )
          let errors =
            list.append(errors, validate_compare_digest(input, existing, index))
          errors
        }
      }
  }
}

fn validate_definition_value(
  definition: Option(MetafieldDefinitionRecord),
  value: Option(String),
  index: Int,
) -> List(MetafieldsSetUserError) {
  case definition, value {
    Some(def), Some(raw_value) ->
      list.filter_map(def.validations, fn(validation) {
        case validation.name, validation.value {
          "max", Some(max_raw) ->
            case int.parse(max_raw) {
              Ok(max) ->
                case string.length(raw_value) > max {
                  True ->
                    Ok(make_metafields_set_user_error(
                      Some(index),
                      Some("value"),
                      "Value must be "
                        <> int.to_string(max)
                        <> " characters or fewer for this metafield definition.",
                      "LESS_THAN_OR_EQUAL_TO",
                    ))
                  False -> Error(Nil)
                }
              Error(_) -> Error(Nil)
            }
          _, _ -> Error(Nil)
        }
      })
    _, _ -> []
  }
}

fn validate_compare_digest(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(ProductMetafieldRecord),
  index: Int,
) -> List(MetafieldsSetUserError) {
  case has_field(input, "compareDigest") {
    False -> []
    True -> {
      let provided = case dict.get(input, "compareDigest") {
        Ok(root_field.NullVal) -> Some(None)
        Ok(root_field.StringVal(value)) -> Some(Some(value))
        _ -> None
      }
      case provided {
        None -> [
          make_metafields_set_user_error(
            Some(index),
            Some("compareDigest"),
            "Compare digest is invalid.",
            "INVALID_COMPARE_DIGEST",
          ),
        ]
        Some(value) -> {
          let current = case existing {
            Some(record) ->
              record.compare_digest
              |> option.or(
                Some(
                  metafields.make_metafield_compare_digest(
                    product_metafield_to_core(record),
                  ),
                ),
              )
            None -> None
          }
          case value == current {
            True -> []
            False -> [
              MetafieldsSetUserError(
                field: ["metafields", int.to_string(index)],
                message: "The resource has been updated since it was loaded. Try again with an updated `compareDigest` value.",
                code: Some("STALE_OBJECT"),
                element_index: None,
              ),
            ]
          }
        }
      }
    }
  }
}

fn upsert_metafields_set_inputs(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> #(List(ProductMetafieldRecord), Store, SyntheticIdentityRegistry) {
  let grouped = group_metafields_by_owner(inputs)
  list.fold(grouped, #([], store_in, identity), fn(acc, pair) {
    let #(created_acc, current_store, current_identity) = acc
    let #(owner_id, owner_inputs) = pair
    let owner_type = owner_type_from_id(owner_id) |> option.unwrap("PRODUCT")
    let existing =
      store.get_effective_metafields_by_owner_id(current_store, owner_id)
    let #(metafields_for_owner, created, next_identity) =
      upsert_owner_metafields(
        current_store,
        current_identity,
        owner_id,
        owner_type,
        owner_inputs,
        existing,
      )
    let next_store =
      store.replace_staged_metafields_for_owner(
        current_store,
        owner_id,
        metafields_for_owner,
      )
    #(list.append(created_acc, created), next_store, next_identity)
  })
}

fn group_metafields_by_owner(
  inputs: List(Dict(String, root_field.ResolvedValue)),
) -> List(#(String, List(Dict(String, root_field.ResolvedValue)))) {
  list.fold(inputs, [], fn(groups, input) {
    case read_optional_string(input, "ownerId") {
      Some(owner_id) ->
        append_metafields_set_owner_input(groups, owner_id, input)
      None -> groups
    }
  })
}

fn append_metafields_set_owner_input(
  groups: List(#(String, List(Dict(String, root_field.ResolvedValue)))),
  owner_id: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(String, List(Dict(String, root_field.ResolvedValue)))) {
  case groups {
    [] -> [#(owner_id, [input])]
    [first, ..rest] -> {
      let #(group_owner_id, group_inputs) = first
      case group_owner_id == owner_id {
        True -> [#(group_owner_id, list.append(group_inputs, [input])), ..rest]
        False -> [
          first,
          ..append_metafields_set_owner_input(rest, owner_id, input)
        ]
      }
    }
  }
}

fn upsert_owner_metafields(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  owner_id: String,
  owner_type: String,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  existing: List(ProductMetafieldRecord),
) -> #(
  List(ProductMetafieldRecord),
  List(ProductMetafieldRecord),
  SyntheticIdentityRegistry,
) {
  list.fold(inputs, #(existing, [], identity), fn(acc, input) {
    let #(current, created, current_identity) = acc
    let namespace = read_metafields_set_namespace(input)
    let key = read_optional_string(input, "key") |> option.unwrap("")
    let found =
      current
      |> list.find(fn(metafield) {
        metafield.namespace == namespace && metafield.key == key
      })
      |> option.from_result
    let definition =
      store.find_effective_metafield_definition(
        store_in,
        owner_type,
        namespace,
        key,
      )
    let type_ =
      read_optional_string(input, "type")
      |> option.or(case definition {
        Some(def) -> Some(def.type_.name)
        None -> None
      })
      |> option.or(case found {
        Some(m) -> m.type_
        None -> None
      })
    let raw_value =
      read_optional_string(input, "value")
      |> option.or(case found {
        Some(m) -> m.value
        None -> None
      })
    let value = metafields.normalize_metafield_value(type_, raw_value)
    let #(id, identity_after_id) = case found {
      Some(record) -> #(record.id, current_identity)
      None ->
        synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
    }
    let #(created_at, identity_after_created) = case found {
      Some(record) -> #(
        option.unwrap(record.created_at, "2024-01-01T00:00:00Z"),
        identity_after_id,
      )
      None -> synthetic_identity.make_synthetic_timestamp(identity_after_id)
    }
    let #(updated_at, identity_after_updated) = case found {
      Some(record) ->
        case value == record.value && type_ == record.type_ {
          True -> #(
            option.unwrap(record.updated_at, created_at),
            identity_after_created,
          )
          False ->
            synthetic_identity.make_synthetic_timestamp(identity_after_created)
        }
      None -> #(created_at, identity_after_created)
    }
    let core =
      ProductMetafieldRecord(
        id: id,
        owner_id: owner_id,
        namespace: namespace,
        key: key,
        type_: type_,
        value: value,
        compare_digest: None,
        json_value: metafields.parse_metafield_json_value(type_, value),
        created_at: Some(created_at),
        updated_at: Some(updated_at),
        owner_type: Some(owner_type),
      )
    let record = case found {
      Some(existing_record)
        if value == existing_record.value && type_ == existing_record.type_
      ->
        ProductMetafieldRecord(
          ..core,
          compare_digest: existing_record.compare_digest,
        )
      _ ->
        ProductMetafieldRecord(
          ..core,
          compare_digest: Some(
            metafields.make_metafield_compare_digest(product_metafield_to_core(
              core,
            )),
          ),
        )
    }
    let replaced =
      replace_metafield_by_identity(current, namespace, key, record)
    #(replaced, list.append(created, [record]), identity_after_updated)
  })
}

fn replace_metafield_by_identity(
  records: List(ProductMetafieldRecord),
  namespace: String,
  key: String,
  record: ProductMetafieldRecord,
) -> List(ProductMetafieldRecord) {
  let without =
    list.filter(records, fn(candidate) {
      !{ candidate.namespace == namespace && candidate.key == key }
    })
  list.append(without, [record])
}

fn read_metafields_set_namespace(
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  case read_optional_string(input, "namespace") {
    Some(ns) ->
      case string.trim(ns) {
        "" -> default_app_metafield_namespace()
        _ -> ns
      }
    None -> default_app_metafield_namespace()
  }
}

fn default_app_metafield_namespace() -> String {
  "app--347082227713"
}

fn find_owner_metafield(
  store_in: Store,
  owner_id: String,
  namespace: String,
  key: Option(String),
) -> Option(ProductMetafieldRecord) {
  case key {
    Some(k) ->
      store.get_effective_metafields_by_owner_id(store_in, owner_id)
      |> list.find(fn(metafield) {
        metafield.namespace == namespace && metafield.key == k
      })
      |> option.from_result
    None -> None
  }
}

fn owner_type_from_id(owner_id: String) -> Option(String) {
  case string.split(owner_id, "/") {
    ["gid:", "", "shopify", "Product", _] -> Some("PRODUCT")
    ["gid:", "", "shopify", "ProductVariant", _] -> Some("PRODUCTVARIANT")
    ["gid:", "", "shopify", "Collection", _] -> Some("COLLECTION")
    ["gid:", "", "shopify", "Customer", _] -> Some("CUSTOMER")
    _ -> None
  }
}

fn make_metafields_set_user_error(
  index: Option(Int),
  field_name: Option(String),
  message: String,
  code: String,
) -> MetafieldsSetUserError {
  let field = case index, field_name {
    None, _ -> ["metafields"]
    Some(i), Some(name) -> ["metafields", int.to_string(i), name]
    Some(i), None -> ["metafields", int.to_string(i)]
  }
  MetafieldsSetUserError(
    field: field,
    message: message,
    code: Some(code),
    element_index: index,
  )
}

fn serialize_metafields_set_user_errors(
  field: Selection,
  errors: List(MetafieldsSetUserError),
) -> Json {
  case child_field(field, "userErrors") {
    Some(user_error_field) ->
      json.array(errors, fn(error) {
        json.object(
          list.map(
            get_selected_child_fields(
              user_error_field,
              default_selected_field_options(),
            ),
            fn(selection) {
              let key = get_field_response_key(selection)
              let value = case selection {
                Field(name: name, ..) ->
                  case name.value {
                    "field" -> json.array(error.field, json.string)
                    "message" -> json.string(error.message)
                    "code" -> optional_string(error.code)
                    "elementIndex" -> optional_int(error.element_index)
                    _ -> json.null()
                  }
                _ -> json.null()
              }
              #(key, value)
            },
          ),
        )
      })
    None ->
      json.array(errors, fn(error) {
        json.object([
          #("field", json.array(error.field, json.string)),
          #("message", json.string(error.message)),
        ])
      })
  }
}

fn serialize_metafield_payload(
  records: List(ProductMetafieldRecord),
  field: Selection,
) -> Json {
  case child_field(field, "metafields") {
    Some(metafields_field) ->
      json.array(records, fn(record) {
        metafields.serialize_metafield_selection(
          product_metafield_to_core(record),
          metafields_field,
          default_selected_field_options(),
        )
      })
    None ->
      json.array(records, fn(record) {
        json.object([#("id", json.string(record.id))])
      })
  }
}

fn optional_string(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}

fn optional_int(value: Option(Int)) -> Json {
  case value {
    Some(i) -> json.int(i)
    None -> json.null()
  }
}

fn optional_string_list(value: Option(List(String))) -> Json {
  case value {
    Some(items) -> json.array(items, json.string)
    None -> json.null()
  }
}

fn metafield_definitions_from_hydrate_response(
  value: commit.JsonValue,
) -> List(MetafieldDefinitionRecord) {
  case json_get(value, "data") {
    Some(data) -> {
      let from_connection = case json_get(data, "metafieldDefinitions") {
        Some(connection) ->
          case json_get(connection, "nodes") {
            Some(commit.JsonArray(nodes)) ->
              list.filter_map(nodes, metafield_definition_from_json)
            _ -> []
          }
        None -> []
      }
      let from_singular = case json_get(data, "metafieldDefinition") {
        Some(commit.JsonNull) | None -> []
        Some(node) ->
          case metafield_definition_from_json(node) {
            Ok(definition) -> [definition]
            Error(_) -> []
          }
      }
      list.append(from_connection, from_singular)
    }
    None -> []
  }
}

fn metafield_definition_from_json(
  value: commit.JsonValue,
) -> Result(MetafieldDefinitionRecord, Nil) {
  use id <- result.try(json_get_required_string(value, "id"))
  use name <- result.try(json_get_required_string(value, "name"))
  use namespace <- result.try(json_get_required_string(value, "namespace"))
  use key <- result.try(json_get_required_string(value, "key"))
  use owner_type <- result.try(json_get_required_string(value, "ownerType"))
  let type_node = json_get(value, "type")
  Ok(MetafieldDefinitionRecord(
    id: id,
    name: name,
    namespace: namespace,
    key: key,
    owner_type: owner_type,
    type_: MetafieldDefinitionTypeRecord(
      name: type_node
        |> option.then(fn(node) { json_get_string(node, "name") })
        |> option.unwrap("single_line_text_field"),
      category: type_node
        |> option.then(fn(node) { json_get_string(node, "category") }),
    ),
    description: json_get_string(value, "description"),
    validations: json_get_array(value, "validations")
      |> list.filter_map(metafield_definition_validation_from_json),
    access: definition_access_from_json(json_get(value, "access")),
    capabilities: definition_capabilities_from_json(json_get(
      value,
      "capabilities",
    )),
    constraints: Some(
      definition_constraints_from_json(json_get(value, "constraints")),
    ),
    pinned_position: json_get_int(value, "pinnedPosition"),
    validation_status: json_get_string(value, "validationStatus")
      |> option.unwrap("ALL_VALID"),
  ))
}

fn metafield_definition_validation_from_json(
  value: commit.JsonValue,
) -> Result(MetafieldDefinitionValidationRecord, Nil) {
  use name <- result.try(json_get_required_string(value, "name"))
  Ok(MetafieldDefinitionValidationRecord(
    name: name,
    value: json_get_string(value, "value"),
  ))
}

fn definition_access_from_json(
  value: Option(commit.JsonValue),
) -> Dict(String, Json) {
  case value {
    Some(node) ->
      [
        #("admin", "PUBLIC_READ_WRITE"),
        #("storefront", "NONE"),
        #("customerAccount", "NONE"),
      ]
      |> list.map(fn(pair) {
        let #(key, fallback) = pair
        #(
          key,
          json.string(json_get_string(node, key) |> option.unwrap(fallback)),
        )
      })
      |> dict.from_list
    None ->
      dict.from_list([
        #("admin", json.string("PUBLIC_READ_WRITE")),
        #("storefront", json.string("NONE")),
        #("customerAccount", json.string("NONE")),
      ])
  }
}

fn definition_capabilities_from_json(
  value: Option(commit.JsonValue),
) -> MetafieldDefinitionCapabilitiesRecord {
  let node = option.unwrap(value, commit.JsonObject([]))
  MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: definition_capability_from_json(
      json_get(node, "adminFilterable"),
      Some("NOT_FILTERABLE"),
    ),
    smart_collection_condition: definition_capability_from_json(
      json_get(node, "smartCollectionCondition"),
      None,
    ),
    unique_values: definition_capability_from_json(
      json_get(node, "uniqueValues"),
      None,
    ),
  )
}

fn definition_capability_from_json(
  value: Option(commit.JsonValue),
  default_status: Option(String),
) -> MetafieldDefinitionCapabilityRecord {
  case value {
    Some(node) ->
      MetafieldDefinitionCapabilityRecord(
        enabled: json_get_bool(node, "enabled") |> option.unwrap(False),
        eligible: json_get_bool(node, "eligible") |> option.unwrap(False),
        status: json_get_string(node, "status") |> option.or(default_status),
      )
    None ->
      MetafieldDefinitionCapabilityRecord(
        enabled: False,
        eligible: False,
        status: default_status,
      )
  }
}

fn definition_constraints_from_json(
  value: Option(commit.JsonValue),
) -> MetafieldDefinitionConstraintsRecord {
  let node = option.unwrap(value, commit.JsonObject([]))
  let values = case json_get(node, "values") {
    Some(connection) ->
      json_get_array(connection, "nodes")
      |> list.filter_map(fn(value_node) {
        case json_get_string(value_node, "value") {
          Some(value) -> Ok(MetafieldDefinitionConstraintValueRecord(value))
          None -> Error(Nil)
        }
      })
    None -> []
  }
  MetafieldDefinitionConstraintsRecord(
    key: json_get_string(node, "key"),
    values: values,
  )
}

fn json_get_required_string(
  value: commit.JsonValue,
  key: String,
) -> Result(String, Nil) {
  case json_get_string(value, key) {
    Some(s) -> Ok(s)
    None -> Error(Nil)
  }
}

fn json_get_array(
  value: commit.JsonValue,
  key: String,
) -> List(commit.JsonValue) {
  case json_get(value, key) {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(b)) -> Some(b)
    _ -> None
  }
}

fn json_get_int(value: commit.JsonValue, key: String) -> Option(Int) {
  case json_get(value, key) {
    Some(commit.JsonInt(i)) -> Some(i)
    _ -> None
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(k, v) if k == key -> Ok(v)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn dict_is_empty(values: Dict(String, a)) -> Bool {
  values
  |> dict.to_list
  |> list.is_empty
}

fn dedupe_strings(items: List(String)) -> List(String) {
  dedupe_strings_loop(items, dict.new(), [])
}

fn dedupe_strings_loop(
  remaining: List(String),
  seen: Dict(String, Bool),
  acc: List(String),
) -> List(String) {
  case remaining {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      case dict.get(seen, first) {
        Ok(_) -> dedupe_strings_loop(rest, seen, acc)
        Error(_) ->
          dedupe_strings_loop(rest, dict.insert(seen, first, True), [
            first,
            ..acc
          ])
      }
  }
}

fn enumerate(items: List(a)) -> List(#(Int, a)) {
  enumerate_loop(items, 0, [])
}

fn enumerate_loop(
  items: List(a),
  index: Int,
  acc: List(#(Int, a)),
) -> List(#(Int, a)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] -> enumerate_loop(rest, index + 1, [#(index, first), ..acc])
  }
}
