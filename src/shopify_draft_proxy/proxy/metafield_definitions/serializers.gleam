//// Selection and payload serializers for metafield definitions.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  ConnectionPageInfoOptions, SerializeConnectionConfig,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, paginate_connection_items, serialize_connection,
}
import shopify_draft_proxy/proxy/metafield_definitions/types as definition_types
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type MetafieldDefinitionCapabilitiesRecord,
  type MetafieldDefinitionCapabilityRecord,
  type MetafieldDefinitionConstraintValueRecord,
  type MetafieldDefinitionConstraintsRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionTypeRecord, type MetafieldDefinitionValidationRecord,
  type ProductMetafieldRecord,
}

@internal
pub fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  serialize_root_fields_with_requesting_api_client_id(
    store,
    fields,
    variables,
    None,
  )
}

@internal
pub fn serialize_root_fields_with_requesting_api_client_id(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "metafieldDefinition" ->
              serialize_metafield_definition_root_with_requesting_api_client_id(
                store,
                field,
                variables,
                requesting_api_client_id,
              )
            "metafieldDefinitions" ->
              serialize_metafield_definitions_connection(
                store,
                field,
                variables,
                requesting_api_client_id,
              )
            "product" ->
              serialize_owner_root(
                store,
                field,
                variables,
                "PRODUCT",
                "id",
                requesting_api_client_id,
              )
            "productVariant" ->
              serialize_owner_root(
                store,
                field,
                variables,
                "PRODUCTVARIANT",
                "id",
                requesting_api_client_id,
              )
            "collection" ->
              serialize_owner_root(
                store,
                field,
                variables,
                "COLLECTION",
                "id",
                requesting_api_client_id,
              )
            "customer" ->
              serialize_owner_root(
                store,
                field,
                variables,
                "CUSTOMER",
                "id",
                requesting_api_client_id,
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

@internal
pub fn serialize_metafield_definition_root(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  serialize_metafield_definition_root_with_requesting_api_client_id(
    store_in,
    field,
    variables,
    None,
  )
}

@internal
pub fn serialize_metafield_definition_root_with_requesting_api_client_id(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  let args = definition_types.read_args(field, variables)
  let id = definition_types.read_optional_string(args, "id")
  let definition = case id {
    Some(definition_id) ->
      store.get_effective_metafield_definition_by_id(store_in, definition_id)
    None ->
      case
        definition_types.read_definition_identifier_with_requesting_api_client_id(
          args,
          requesting_api_client_id,
        )
      {
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

@internal
pub fn serialize_metafield_definitions_connection(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  let args = definition_types.read_args(field, variables)
  let definitions =
    store.list_effective_metafield_definitions(store_in)
    |> apply_definition_filters(args, requesting_api_client_id)
    |> sort_definitions(
      definition_types.read_optional_string(args, "sortKey"),
      definition_types.read_optional_bool(args, "reverse")
        |> option.unwrap(False),
    )
  serialize_definition_records_connection(
    store_in,
    definitions,
    field,
    variables,
  )
}

@internal
pub fn serialize_definition_records_connection(
  store_in: Store,
  definitions: List(MetafieldDefinitionRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
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

@internal
pub fn apply_definition_filters(
  definitions: List(MetafieldDefinitionRecord),
  args: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> List(MetafieldDefinitionRecord) {
  let owner_type = definition_types.read_optional_string(args, "ownerType")
  let namespace =
    definition_types.read_optional_string(args, "namespace")
    |> option.map(fn(namespace) {
      definition_types.resolve_app_namespace(
        namespace,
        requesting_api_client_id,
      )
    })
  let key = definition_types.read_optional_string(args, "key")
  let pinned_status =
    definition_types.read_optional_string(args, "pinnedStatus")
    |> option.unwrap("ANY")
  let query = definition_types.read_optional_string(args, "query")
  definitions
  |> list.filter(fn(definition) {
    option_matches(owner_type, definition.owner_type)
    && option_matches(namespace, definition.namespace)
    && option_matches(key, definition.key)
    && pinned_status_matches(pinned_status, definition)
    && definition_query_matches(query, definition)
  })
}

@internal
pub fn option_matches(expected: Option(String), actual: String) -> Bool {
  case expected {
    Some(value) -> value == actual
    None -> True
  }
}

@internal
pub fn pinned_status_matches(
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

@internal
pub fn definition_query_matches(
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

@internal
pub fn sort_definitions(
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

@internal
pub fn serialize_definition_selection(
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
                definition_types.get_product_metafields_for_definition(
                  store_in,
                  definition,
                )
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

@internal
pub fn serialize_definition_type(
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

@internal
pub fn serialize_validation(
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

@internal
pub fn serialize_json_object(
  values: Dict(String, Json),
  field: Selection,
) -> Json {
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

@internal
pub fn serialize_capabilities(
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

@internal
pub fn serialize_capability(
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

@internal
pub fn serialize_constraints(
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

@internal
pub fn serialize_constraint_values_connection(
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

@internal
pub fn get_product_metafields_for_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> List(ProductMetafieldRecord) {
  store_in
  |> definition_types.all_effective_metafields()
  |> list.filter(fn(metafield) {
    metafield.owner_type == Some(definition.owner_type)
    && metafield.namespace == definition.namespace
    && metafield.key == definition.key
  })
  |> list.sort(fn(left, right) { string.compare(left.id, right.id) })
}

@internal
pub fn product_metafield_owner_ids_for_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> List(String) {
  definition_types.get_product_metafields_for_definition(store_in, definition)
  |> list.map(fn(metafield) { metafield.owner_id })
  |> definition_types.dedupe_strings
}

@internal
pub fn all_effective_metafields(
  store_in: Store,
) -> List(ProductMetafieldRecord) {
  let owner_ids =
    list.append(
      dict.values(store_in.base_state.product_metafields),
      dict.values(store_in.staged_state.product_metafields),
    )
    |> list.map(fn(metafield) { metafield.owner_id })
    |> definition_types.dedupe_strings
  list.flat_map(owner_ids, fn(owner_id) {
    store.get_effective_metafields_by_owner_id(store_in, owner_id)
  })
}

@internal
pub fn serialize_definition_metafields_connection(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = definition_types.read_args(field, variables)
  let records =
    definition_types.get_product_metafields_for_definition(store_in, definition)
  let ordered = case definition_types.read_optional_bool(args, "reverse") {
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

@internal
pub fn product_metafield_to_core(
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
pub fn serialize_owner_root(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  owner_type: String,
  id_arg_name: String,
  requesting_api_client_id: Option(String),
) -> Json {
  let args = definition_types.read_args(field, variables)
  case definition_types.read_optional_string(args, id_arg_name) {
    None -> json.null()
    Some(owner_id) ->
      serialize_owner_selection(
        store_in,
        field,
        variables,
        owner_id,
        owner_type,
        requesting_api_client_id,
      )
  }
}

@internal
pub fn serialize_owner_selection(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  owner_id: String,
  owner_type: String,
  requesting_api_client_id: Option(String),
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
              "metafieldDefinitions" ->
                serialize_owner_metafield_definitions_connection(
                  store_in,
                  owner_type,
                  selection,
                  variables,
                  requesting_api_client_id,
                )
              "variants" ->
                serialize_product_variants_from_metafields(
                  store_in,
                  selection,
                  variables,
                  requesting_api_client_id,
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

@internal
pub fn serialize_owner_metafield_definitions_connection(
  store_in: Store,
  owner_type: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  let args = definition_types.read_args(field, variables)
  let definitions =
    store.list_effective_metafield_definitions(store_in)
    |> list.filter(fn(definition) { definition.owner_type == owner_type })
    |> apply_definition_filters(args, requesting_api_client_id)
    |> sort_definitions(
      definition_types.read_optional_string(args, "sortKey"),
      definition_types.read_optional_bool(args, "reverse")
        |> option.unwrap(False),
    )
  serialize_definition_records_connection(
    store_in,
    definitions,
    field,
    variables,
  )
}

@internal
pub fn serialize_owner_metafield(
  store_in: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = definition_types.read_args(field, variables)
  let namespace = definition_types.read_optional_string(args, "namespace")
  let key = definition_types.read_optional_string(args, "key")
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

@internal
pub fn serialize_owner_metafields_connection(
  store_in: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = definition_types.read_args(field, variables)
  let namespace = definition_types.read_optional_string(args, "namespace")
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

@internal
pub fn serialize_product_variants_from_metafields(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  let variant_ids =
    definition_types.all_effective_metafields(store_in)
    |> list.filter(fn(metafield) {
      metafield.owner_type == Some("PRODUCTVARIANT")
    })
    |> list.map(fn(metafield) { metafield.owner_id })
    |> definition_types.dedupe_strings
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
          requesting_api_client_id,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn serialize_standard_enable_payload(
  store_in: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  created_definition: Option(MetafieldDefinitionRecord),
  user_errors: List(definition_types.UserError),
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

@internal
pub fn serialize_standard_user_error(
  error: definition_types.UserError,
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

@internal
pub fn serialize_definition_mutation_payload(
  store_in: Store,
  definition_field_name: String,
  definition: Option(MetafieldDefinitionRecord),
  user_errors: List(definition_types.UserError),
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

@internal
pub fn serialize_definition_delete_payload(
  deleted_definition: Option(MetafieldDefinitionRecord),
  user_errors: List(definition_types.UserError),
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

@internal
pub fn serialize_deleted_definition_identifier(
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

@internal
pub fn serialize_pinning_payload(
  store_in: Store,
  payload_field_name: String,
  definition: Option(MetafieldDefinitionRecord),
  user_errors: List(definition_types.UserError),
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

@internal
pub fn serialize_definition_user_error(
  error: definition_types.UserError,
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

@internal
pub fn serialize_metafields_delete_payload(
  result: definition_types.MetafieldsDeleteResult,
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

@internal
pub fn serialize_metafield_delete_payload(
  deleted_id: Option(String),
  errors: List(definition_types.SimpleUserError),
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

@internal
pub fn serialize_deleted_metafield_identifiers(
  identifiers: List(Option(definition_types.DeletedMetafieldIdentifier)),
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

@internal
pub fn serialize_simple_user_errors(
  field: Selection,
  errors: List(definition_types.SimpleUserError),
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

@internal
pub fn deleted_identifiers_to_stage_ids(
  identifiers: List(Option(definition_types.DeletedMetafieldIdentifier)),
) -> List(String) {
  identifiers
  |> list.filter_map(fn(identifier) {
    case identifier {
      Some(record) -> Ok(record.owner_id)
      None -> Error(Nil)
    }
  })
  |> definition_types.dedupe_strings
}

@internal
pub fn serialize_metafields_set_payload(
  records: List(ProductMetafieldRecord),
  errors: List(definition_types.MetafieldsSetUserError),
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

@internal
pub fn serialize_metafields_set_user_errors(
  field: Selection,
  errors: List(definition_types.MetafieldsSetUserError),
) -> Json {
  case definition_types.child_field(field, "userErrors") {
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

@internal
pub fn serialize_metafield_payload(
  records: List(ProductMetafieldRecord),
  field: Selection,
) -> Json {
  case definition_types.child_field(field, "metafields") {
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

@internal
pub fn optional_string(value: Option(String)) -> Json {
  case value {
    Some(s) -> json.string(s)
    None -> json.null()
  }
}

@internal
pub fn optional_int(value: Option(Int)) -> Json {
  case value {
    Some(i) -> json.int(i)
    None -> json.null()
  }
}

@internal
pub fn optional_string_list(value: Option(List(String))) -> Json {
  case value {
    Some(items) -> json.array(items, json.string)
    None -> json.null()
  }
}
