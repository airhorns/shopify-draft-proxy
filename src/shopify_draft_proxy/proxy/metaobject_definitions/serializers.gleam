//// Selection and source serializers for metaobject definitions and metaobjects.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{Eq}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Selection, Field, FragmentDefinition, FragmentSpread, InlineFragment,
  NamedType, SelectionSet,
}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionWindow, SelectedFieldOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_type_condition_applies,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/metaobject_definitions/types as metaobject_definition_types
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{
  type Store, find_effective_metaobject_by_handle,
  find_effective_metaobject_definition_by_type, get_effective_metaobject_by_id,
  get_effective_metaobject_definition_by_id,
  list_effective_metaobject_definitions, list_effective_metaobjects,
  list_effective_metaobjects_by_type,
}
import shopify_draft_proxy/state/types.{
  type MetaobjectCapabilitiesRecord, type MetaobjectDefinitionCapabilitiesRecord,
  type MetaobjectDefinitionCapabilityRecord, type MetaobjectDefinitionRecord,
  type MetaobjectDefinitionTypeRecord,
  type MetaobjectFieldDefinitionCapabilitiesRecord,
  type MetaobjectFieldDefinitionRecord,
  type MetaobjectFieldDefinitionReferenceRecord,
  type MetaobjectFieldDefinitionValidationRecord, type MetaobjectFieldRecord,
  type MetaobjectRecord, type MetaobjectStandardTemplateRecord,
  MetaobjectDefinitionCapabilityRecord, MetaobjectOnlineStoreCapabilityRecord,
  MetaobjectPublishableCapabilityRecord, MetaobjectRecord,
  MetaobjectStandardTemplateRecord, MetaobjectString,
}

pub fn wrap_data(data: Json) -> Json {
  graphql_helpers.wrap_data(data)
}

@internal
pub fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  json.object(
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "metaobjectDefinition" ->
              case metaobject_definition_types.read_id_arg(field, variables) {
                Some(id) ->
                  case get_effective_metaobject_definition_by_id(store, id) {
                    Some(definition) ->
                      serialize_definition_selection(
                        definition,
                        field,
                        fragments,
                      )
                    None -> json.null()
                  }
                None -> json.null()
              }
            "metaobjectDefinitionByType" ->
              case
                metaobject_definition_types.read_string_arg(
                  field,
                  variables,
                  "type",
                )
              {
                Some(type_) ->
                  case
                    metaobject_definition_types.find_effective_metaobject_definition_by_normalized_type(
                      store,
                      metaobject_definition_types.normalize_definition_type(
                        type_,
                        requesting_api_client_id,
                      ),
                    )
                  {
                    Some(definition) ->
                      serialize_definition_selection(
                        definition,
                        field,
                        fragments,
                      )
                    None -> json.null()
                  }
                None -> json.null()
              }
            "metaobjectDefinitions" ->
              serialize_definitions_connection(
                store,
                field,
                fragments,
                variables,
                requesting_api_client_id,
              )
            "metaobject" ->
              case metaobject_definition_types.read_id_arg(field, variables) {
                Some(id) ->
                  case get_effective_metaobject_by_id(store, id) {
                    Some(metaobject) ->
                      serialize_metaobject_selection(
                        store,
                        metaobject,
                        field,
                        fragments,
                      )
                    None -> json.null()
                  }
                None -> json.null()
              }
            "metaobjectByHandle" ->
              case
                metaobject_definition_types.read_handle_arg(field, variables)
              {
                #(Some(type_), Some(handle)) ->
                  case
                    find_effective_metaobject_by_handle(store, type_, handle)
                  {
                    Some(metaobject) ->
                      serialize_metaobject_selection(
                        store,
                        metaobject,
                        field,
                        fragments,
                      )
                    None -> json.null()
                  }
                _ -> json.null()
              }
            "metaobjects" ->
              serialize_metaobjects_connection(
                store,
                field,
                fragments,
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

pub fn metaobject_definition_source(
  definition: MetaobjectDefinitionRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("MetaobjectDefinition")),
    #("id", SrcString(definition.id)),
    #("type", SrcString(definition.type_)),
    #("name", graphql_helpers.option_string_source(definition.name)),
    #(
      "description",
      graphql_helpers.option_string_source(definition.description),
    ),
    #(
      "displayNameKey",
      graphql_helpers.option_string_source(definition.display_name_key),
    ),
    #("access", access_source(definition.access)),
    #("capabilities", definition_capabilities_source(definition.capabilities)),
    #(
      "fieldDefinitions",
      SrcList(list.map(definition.field_definitions, field_definition_source)),
    ),
    #(
      "hasThumbnailField",
      graphql_helpers.option_bool_source(definition.has_thumbnail_field),
    ),
    #(
      "metaobjectsCount",
      graphql_helpers.option_int_source(definition.metaobjects_count),
    ),
    #(
      "standardTemplate",
      standard_template_source(definition.standard_template),
    ),
    #("enabledByShopify", SrcBool(definition.enabled_by_shopify)),
    #(
      "enabledByShopifyAt",
      graphql_helpers.option_string_source(definition.enabled_by_shopify_at),
    ),
    #("createdAt", graphql_helpers.option_string_source(definition.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(definition.updated_at)),
  ])
}

@internal
pub fn serialize_definition_selection(
  definition: MetaobjectDefinitionRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_selection(metaobject_definition_source(definition), field, fragments)
}

@internal
pub fn serialize_definitions_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let items = case metaobject_definition_types.read_string(args, "type") {
    Some(type_) -> {
      let normalized_type =
        metaobject_definition_types.normalize_definition_type(
          type_,
          requesting_api_client_id,
        )
      list.filter(list_effective_metaobject_definitions(store), fn(defn) {
        string.lowercase(defn.type_) == normalized_type
      })
    }
    None -> list_effective_metaobject_definitions(store)
  }
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      fn(item, _index) { item.id },
      default_connection_window_options(),
    )
  let ConnectionWindow(items: page_items, has_next_page:, has_previous_page:) =
    window
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: page_items,
      has_next_page: has_next_page,
      has_previous_page: has_previous_page,
      get_cursor_value: fn(item, _index) { item.id },
      serialize_node: fn(item, node_field, _index) {
        serialize_definition_selection(item, node_field, fragments)
      },
      selected_field_options: SelectedFieldOptions(
        include_inline_fragments: False,
      ),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

pub fn metaobject_source(
  store: Store,
  metaobject: MetaobjectRecord,
) -> SourceValue {
  let projected =
    metaobject_definition_types.project_metaobject_through_definition(
      store,
      metaobject,
    )
  metaobject_source_from_projected(store, projected)
}

fn metaobject_source_from_projected(
  store: Store,
  projected: MetaobjectRecord,
) -> SourceValue {
  let definition =
    find_effective_metaobject_definition_by_type(store, projected.type_)
  src_object([
    #("__typename", SrcString("Metaobject")),
    #("id", SrcString(projected.id)),
    #("handle", SrcString(projected.handle)),
    #("type", SrcString(projected.type_)),
    #(
      "displayName",
      graphql_helpers.option_string_source(projected.display_name),
    ),
    #("createdAt", graphql_helpers.option_string_source(projected.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(projected.updated_at)),
    #(
      "capabilities",
      metaobject_capabilities_source(projected.capabilities, definition),
    ),
    #("fields", SrcList(list.map(projected.fields, metaobject_field_source))),
    #("definition", case definition {
      Some(defn) -> metaobject_definition_source(defn)
      None -> SrcNull
    }),
  ])
}

@internal
pub fn serialize_metaobject_selection(
  store: Store,
  metaobject: MetaobjectRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let projected =
    metaobject_definition_types.project_metaobject_through_definition(
      store,
      metaobject,
    )
  let source = metaobject_source(store, projected)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      json.object(
        list.flat_map(selections, fn(selection) {
          project_metaobject_selection(
            store,
            projected,
            source,
            selection,
            fragments,
          )
        }),
      )
    _ -> source_to_json(source)
  }
}

@internal
pub fn serialize_metaobject_mutation_selection(
  store: Store,
  metaobject: MetaobjectRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let projected =
    metaobject_definition_types.project_metaobject_through_definition(
      store,
      metaobject,
    )
  let projected =
    MetaobjectRecord(..projected, display_name: metaobject.display_name)
  let source = metaobject_source_from_projected(store, projected)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      json.object(
        list.flat_map(selections, fn(selection) {
          project_metaobject_selection(
            store,
            projected,
            source,
            selection,
            fragments,
          )
        }),
      )
    _ -> source_to_json(source)
  }
}

@internal
pub fn project_metaobject_selection(
  store: Store,
  metaobject: MetaobjectRecord,
  source: SourceValue,
  selection: Selection,
  fragments: FragmentMap,
) -> List(#(String, Json)) {
  case selection {
    Field(name: name, ..) -> {
      let key = get_field_response_key(selection)
      case name.value {
        "field" -> {
          let args = graphql_helpers.field_args(selection, dict.new())
          let selected = case
            metaobject_definition_types.read_string(args, "key")
          {
            Some(field_key) ->
              list.find(metaobject.fields, fn(f) { f.key == field_key })
              |> option.from_result
            None -> None
          }
          [
            #(key, case selected {
              Some(meta_field) ->
                serialize_metaobject_field_selection(
                  store,
                  meta_field,
                  selection,
                  fragments,
                )
              None -> json.null()
            }),
          ]
        }
        "fields" -> [
          #(
            key,
            json.array(metaobject.fields, fn(meta_field) {
              serialize_metaobject_field_selection(
                store,
                meta_field,
                selection,
                fragments,
              )
            }),
          ),
        ]
        "referencedBy" -> [
          #(
            key,
            serialize_referenced_by_connection(
              store,
              metaobject.id,
              selection,
              fragments,
            ),
          ),
        ]
        _ ->
          case source {
            SrcObject(fields) -> [
              project_source_field(fields, selection, fragments),
            ]
            _ -> [#(key, json.null())]
          }
      }
    }
    InlineFragment(type_condition: tc, selection_set: ss, ..) ->
      case source {
        SrcObject(fields) -> {
          let cond = case tc {
            Some(NamedType(name: name, ..)) -> Some(name.value)
            _ -> None
          }
          case default_type_condition_applies(fields, cond) {
            True -> {
              let SelectionSet(selections: inner, ..) = ss
              list.flat_map(inner, fn(child) {
                project_metaobject_selection(
                  store,
                  metaobject,
                  source,
                  child,
                  fragments,
                )
              })
            }
            False -> []
          }
        }
        _ -> []
      }
    FragmentSpread(name: name, ..) ->
      case dict.get(fragments, name.value), source {
        Ok(FragmentDefinition(
          type_condition: NamedType(name: cond_name, ..),
          selection_set: SelectionSet(selections: inner, ..),
          ..,
        )),
          SrcObject(fields)
        ->
          case default_type_condition_applies(fields, Some(cond_name.value)) {
            True ->
              list.flat_map(inner, fn(child) {
                project_metaobject_selection(
                  store,
                  metaobject,
                  source,
                  child,
                  fragments,
                )
              })
            False -> []
          }
        _, _ -> []
      }
  }
}

@internal
pub fn serialize_metaobjects_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let items = case metaobject_definition_types.read_string(args, "type") {
    Some(type_) -> list_effective_metaobjects_by_type(store, type_)
    None -> list_effective_metaobjects(store)
  }
  let items =
    items
    |> list.filter(fn(item) { is_metaobject_visible_in_catalog(store, item) })
    |> sort_metaobjects_for_connection(
      metaobject_definition_types.read_string(args, "sortKey"),
      option.unwrap(
        metaobject_definition_types.read_bool(args, "reverse"),
        False,
      ),
    )
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      fn(item, _index) { item.id },
      default_connection_window_options(),
    )
  let ConnectionWindow(items: page_items, has_next_page:, has_previous_page:) =
    window
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: page_items,
      has_next_page: has_next_page,
      has_previous_page: has_previous_page,
      get_cursor_value: fn(item, _index) { item.id },
      serialize_node: fn(item, node_field, _index) {
        serialize_metaobject_selection(store, item, node_field, fragments)
      },
      selected_field_options: SelectedFieldOptions(
        include_inline_fragments: False,
      ),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn sort_metaobjects_for_connection(
  items: List(MetaobjectRecord),
  sort_key: Option(String),
  reverse: Bool,
) -> List(MetaobjectRecord) {
  let normalized = option.unwrap(sort_key, "id") |> string.lowercase
  let sorted =
    list.sort(items, fn(left, right) {
      case normalized {
        "display_name" -> {
          let primary =
            resource_ids.compare_nullable_strings(
              left.display_name,
              right.display_name,
            )
          case primary {
            Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
            _ -> primary
          }
        }
        "type" -> {
          let primary = string.compare(left.type_, right.type_)
          case primary {
            Eq -> {
              let secondary = string.compare(left.handle, right.handle)
              case secondary {
                Eq ->
                  resource_ids.compare_shopify_resource_ids(left.id, right.id)
                _ -> secondary
              }
            }
            _ -> primary
          }
        }
        "updated_at" -> {
          let primary =
            resource_ids.compare_nullable_strings(
              left.updated_at,
              right.updated_at,
            )
          case primary {
            Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
            _ -> primary
          }
        }
        _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
      }
    })
  case reverse {
    True -> list.reverse(sorted)
    False -> sorted
  }
}

@internal
pub fn is_metaobject_visible_in_catalog(
  store: Store,
  metaobject: MetaobjectRecord,
) -> Bool {
  case find_effective_metaobject_definition_by_type(store, metaobject.type_) {
    None -> True
    Some(definition) ->
      metaobject_has_required_field_values(metaobject, definition)
      && metaobject_publishable_visible(metaobject, definition)
  }
}

@internal
pub fn metaobject_has_required_field_values(
  metaobject: MetaobjectRecord,
  definition: MetaobjectDefinitionRecord,
) -> Bool {
  list.all(definition.field_definitions, fn(field_definition) {
    case field_definition.required {
      Some(True) ->
        case
          list.find(metaobject.fields, fn(field) {
            field.key == field_definition.key
          })
        {
          Ok(field) ->
            case field.value {
              Some(value) -> value != ""
              None -> False
            }
          Error(_) -> False
        }
      _ -> True
    }
  })
}

@internal
pub fn metaobject_publishable_visible(
  metaobject: MetaobjectRecord,
  definition: MetaobjectDefinitionRecord,
) -> Bool {
  case
    definition.capabilities.publishable,
    metaobject.capabilities.publishable
  {
    Some(MetaobjectDefinitionCapabilityRecord(enabled: False)), None -> False
    _, _ -> True
  }
}

@internal
pub fn metaobject_field_source(field: MetaobjectFieldRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("MetaobjectField")),
    #("key", SrcString(field.key)),
    #("type", graphql_helpers.option_string_source(field.type_)),
    #("value", graphql_helpers.option_string_source(field.value)),
    #("jsonValue", metaobject_field_json_value_source(field)),
    #("definition", case field.definition {
      Some(defn) -> field_definition_reference_source(defn)
      None -> SrcNull
    }),
  ])
}

@internal
pub fn metaobject_field_json_value_source(
  field: MetaobjectFieldRecord,
) -> SourceValue {
  case field.type_, field.value {
    Some(type_), Some(raw) ->
      case measurement_json_value_source(type_, raw) {
        Some(source) -> source
        None -> metaobject_field_stored_json_value_source(field)
      }
    _, _ -> metaobject_field_stored_json_value_source(field)
  }
}

@internal
pub fn metaobject_field_stored_json_value_source(
  field: MetaobjectFieldRecord,
) -> SourceValue {
  case field.json_value, field.type_ {
    MetaobjectString(raw), Some(type_) ->
      case
        metaobject_definition_types.should_parse_metaobject_json_value(type_)
      {
        True ->
          metaobject_definition_types.metaobject_json_value_to_source(
            metaobject_definition_types.read_metaobject_json_value(
              type_,
              Some(raw),
            ),
          )
        False ->
          metaobject_definition_types.metaobject_json_value_to_source(
            field.json_value,
          )
      }
    _, _ ->
      metaobject_definition_types.metaobject_json_value_to_source(
        field.json_value,
      )
  }
}

@internal
pub fn measurement_json_value_source(
  type_: String,
  raw: String,
) -> Option(SourceValue) {
  case string.starts_with(type_, "list.") {
    True -> {
      let base_type = string.drop_start(type_, 5)
      case
        metaobject_definition_types.is_measurement_metaobject_type(base_type)
      {
        True -> parse_single_item_measurement_list_source(raw, base_type)
        False -> None
      }
    }
    False ->
      case metaobject_definition_types.is_measurement_metaobject_type(type_) {
        True -> parse_measurement_source(raw)
        False -> None
      }
  }
}

@internal
pub fn parse_single_item_measurement_list_source(
  raw: String,
  type_: String,
) -> Option(SourceValue) {
  case string.starts_with(raw, "[") && string.ends_with(raw, "]") {
    True ->
      parse_measurement_list_item_source(
        string.drop_start(raw, 1) |> string.drop_end(1),
        type_,
      )
      |> option.map(fn(item) { SrcList([item]) })
    False -> None
  }
}

@internal
pub fn parse_measurement_list_item_source(
  raw: String,
  type_: String,
) -> Option(SourceValue) {
  case parse_measurement_source(raw) {
    Some(SrcObject(fields)) ->
      case dict.get(fields, "unit") {
        Ok(SrcString(unit)) ->
          Some(
            SrcObject(dict.insert(
              fields,
              "unit",
              SrcString(
                metaobject_definition_types.normalize_measurement_list_json_unit(
                  type_,
                  unit,
                ),
              ),
            )),
          )
        _ -> Some(SrcObject(fields))
      }
    other -> other
  }
}

@internal
pub fn parse_measurement_source(raw: String) -> Option(SourceValue) {
  case string.split(raw, on: ",\"unit\":\"") {
    [left, right] -> {
      let value_raw = string.drop_start(left, string.length("{\"value\":"))
      let unit = string.drop_end(right, 2)
      case measurement_number_source(value_raw) {
        Some(value) ->
          Some(src_object([#("value", value), #("unit", SrcString(unit))]))
        None -> None
      }
    }
    _ -> None
  }
}

@internal
pub fn measurement_number_source(raw: String) -> Option(SourceValue) {
  case string.contains(raw, ".") {
    True ->
      case float.parse(raw) {
        Ok(value) -> Some(whole_float_to_number_source(value))
        Error(_) -> None
      }
    False ->
      case int.parse(raw) {
        Ok(value) -> Some(SrcInt(value))
        Error(_) ->
          case float.parse(raw) {
            Ok(value) -> Some(whole_float_to_number_source(value))
            Error(_) -> None
          }
      }
  }
}

@internal
pub fn whole_float_to_number_source(value: Float) -> SourceValue {
  let truncated = float.truncate(value)
  case int.to_float(truncated) == value {
    True -> SrcInt(truncated)
    False -> SrcFloat(value)
  }
}

@internal
pub fn serialize_metaobject_field_selection(
  store: Store,
  meta_field: MetaobjectFieldRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = metaobject_field_source(meta_field)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      json.object(
        list.flat_map(selections, fn(selection) {
          project_metaobject_field_selection(
            store,
            meta_field,
            source,
            selection,
            fragments,
          )
        }),
      )
    _ -> source_to_json(source)
  }
}

@internal
pub fn project_metaobject_field_selection(
  store: Store,
  meta_field: MetaobjectFieldRecord,
  source: SourceValue,
  selection: Selection,
  fragments: FragmentMap,
) -> List(#(String, Json)) {
  case selection {
    Field(name: name, ..) -> {
      let key = get_field_response_key(selection)
      case name.value {
        "reference" -> [
          #(
            key,
            serialize_single_reference(store, meta_field, selection, fragments),
          ),
        ]
        "references" -> [
          #(
            key,
            serialize_field_references_connection(
              store,
              meta_field,
              selection,
              fragments,
            ),
          ),
        ]
        _ ->
          case source {
            SrcObject(fields) -> [
              project_source_field(fields, selection, fragments),
            ]
            _ -> [#(key, json.null())]
          }
      }
    }
    InlineFragment(type_condition: tc, selection_set: ss, ..) ->
      case source {
        SrcObject(fields) -> {
          let cond = case tc {
            Some(NamedType(name: name, ..)) -> Some(name.value)
            _ -> None
          }
          case default_type_condition_applies(fields, cond) {
            True -> {
              let SelectionSet(selections: inner, ..) = ss
              list.flat_map(inner, fn(child) {
                project_metaobject_field_selection(
                  store,
                  meta_field,
                  source,
                  child,
                  fragments,
                )
              })
            }
            False -> []
          }
        }
        _ -> []
      }
    FragmentSpread(name: name, ..) ->
      case dict.get(fragments, name.value), source {
        Ok(FragmentDefinition(
          type_condition: NamedType(name: cond_name, ..),
          selection_set: SelectionSet(selections: inner, ..),
          ..,
        )),
          SrcObject(fields)
        ->
          case default_type_condition_applies(fields, Some(cond_name.value)) {
            True ->
              list.flat_map(inner, fn(child) {
                project_metaobject_field_selection(
                  store,
                  meta_field,
                  source,
                  child,
                  fragments,
                )
              })
            False -> []
          }
        _, _ -> []
      }
  }
}

@internal
pub fn serialize_single_reference(
  store: Store,
  field: MetaobjectFieldRecord,
  selection: Selection,
  fragments: FragmentMap,
) -> Json {
  case field.type_, field.value {
    Some("metaobject_reference"), Some(id) ->
      case get_effective_metaobject_by_id(store, id) {
        Some(metaobject) ->
          serialize_metaobject_selection(
            store,
            metaobject,
            selection,
            fragments,
          )
        None -> json.null()
      }
    _, _ -> json.null()
  }
}

@internal
pub fn serialize_field_references_connection(
  store: Store,
  field_record: MetaobjectFieldRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field_record.type_ {
    Some("list.metaobject_reference") -> {
      let refs =
        metaobject_definition_types.read_metaobject_reference_ids_from_field(
          field_record,
        )
        |> list.filter_map(fn(id) {
          case get_effective_metaobject_by_id(store, id) {
            Some(record) -> Ok(record)
            None -> Error(Nil)
          }
        })
      let window =
        paginate_connection_items(
          refs,
          field,
          dict.new(),
          fn(item, _index) { item.id },
          default_connection_window_options(),
        )
      let ConnectionWindow(
        items: page_items,
        has_next_page:,
        has_previous_page:,
      ) = window
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: page_items,
          has_next_page: has_next_page,
          has_previous_page: has_previous_page,
          get_cursor_value: fn(item, _index) { item.id },
          serialize_node: fn(item, node_field, _index) {
            serialize_metaobject_selection(store, item, node_field, fragments)
          },
          selected_field_options: SelectedFieldOptions(False),
          page_info_options: default_connection_page_info_options(),
        ),
      )
    }
    _ -> json.null()
  }
}

@internal
pub fn serialize_referenced_by_connection(
  store: Store,
  target_id: String,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let relations =
    list.flat_map(list_effective_metaobjects(store), fn(referencer) {
      let projected =
        metaobject_definition_types.project_metaobject_through_definition(
          store,
          referencer,
        )
      list.filter_map(projected.fields, fn(meta_field) {
        case
          list.contains(
            metaobject_definition_types.read_metaobject_reference_ids_from_field(
              meta_field,
            ),
            target_id,
          )
        {
          True -> Ok(#(meta_field, projected))
          False -> Error(Nil)
        }
      })
    })
  let window =
    paginate_connection_items(
      relations,
      field,
      dict.new(),
      fn(item, _index) {
        let #(meta_field, referencer) = item
        referencer.id <> ":" <> meta_field.key
      },
      default_connection_window_options(),
    )
  let ConnectionWindow(items: page_items, has_next_page:, has_previous_page:) =
    window
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: page_items,
      has_next_page: has_next_page,
      has_previous_page: has_previous_page,
      get_cursor_value: fn(item, _index) {
        let #(meta_field, referencer) = item
        referencer.id <> ":" <> meta_field.key
      },
      serialize_node: fn(item, node_field, _index) {
        let #(meta_field, referencer) = item
        let relation_source =
          src_object([
            #("__typename", SrcString("MetaobjectFieldReference")),
            #("key", SrcString(meta_field.key)),
            #(
              "name",
              graphql_helpers.option_string_source(case meta_field.definition {
                Some(defn) -> defn.name
                None -> None
              }),
            ),
            #("namespace", SrcString(referencer.type_)),
            #("referencer", metaobject_source(store, referencer)),
          ])
        project_selection(relation_source, node_field, fragments)
      },
      selected_field_options: SelectedFieldOptions(False),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn project_selection(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> source_to_json(source)
  }
}

@internal
pub fn project_selection_with_metaobject(
  store: Store,
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case source, field {
    SrcObject(fields),
      Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..)
    ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          case selection {
            Field(name: name, ..) if name.value == "metaobject" ->
              case dict.get(fields, "metaobject") {
                Ok(SrcObject(meta_fields)) ->
                  project_source_field_with_metaobject(
                    store,
                    meta_fields,
                    selection,
                    fragments,
                  )
                Ok(SrcNull) -> #(key, json.null())
                _ -> #(key, json.null())
              }
            _ -> project_source_field(fields, selection, fragments)
          }
        }),
      )
    _, _ -> project_selection(source, field, fragments)
  }
}

@internal
pub fn project_source_field_with_metaobject(
  store: Store,
  meta_fields: Dict(String, SourceValue),
  selection: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(selection)
  let id = case dict.get(meta_fields, "id") {
    Ok(SrcString(value)) -> Some(value)
    _ -> None
  }
  case id {
    Some(metaobject_id) ->
      case get_effective_metaobject_by_id(store, metaobject_id) {
        Some(record) -> #(
          key,
          serialize_metaobject_selection(store, record, selection, fragments),
        )
        None -> #(
          key,
          project_graphql_value(
            SrcObject(meta_fields),
            graphql_helpers.field_raw_selections(selection),
            fragments,
          ),
        )
      }
    None -> #(
      key,
      project_graphql_value(
        SrcObject(meta_fields),
        graphql_helpers.field_raw_selections(selection),
        fragments,
      ),
    )
  }
}

@internal
pub fn project_source_field(
  source: Dict(String, SourceValue),
  selection: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(selection)
  case selection {
    Field(name: name, ..) ->
      case name.value {
        "__typename" -> #(key, case dict.get(source, "__typename") {
          Ok(value) -> source_to_json(value)
          Error(_) -> json.null()
        })
        field_name -> {
          let value = dict.get(source, field_name) |> result.unwrap(SrcNull)
          let selections = graphql_helpers.field_raw_selections(selection)
          case selections {
            [] -> #(key, source_to_json(value))
            _ -> #(key, project_graphql_value(value, selections, fragments))
          }
        }
      }
    _ -> #(key, json.null())
  }
}

@internal
pub fn access_source(access: Dict(String, Option(String))) -> SourceValue {
  SrcObject(
    dict.to_list(access)
    |> list.map(fn(pair) {
      let #(key, value) = pair
      #(key, graphql_helpers.option_string_source(value))
    })
    |> dict.from_list,
  )
}

@internal
pub fn definition_capabilities_source(
  capabilities: MetaobjectDefinitionCapabilitiesRecord,
) -> SourceValue {
  src_object([
    #("publishable", definition_capability_source(capabilities.publishable)),
    #("translatable", definition_capability_source(capabilities.translatable)),
    #("renderable", definition_capability_source(capabilities.renderable)),
    #("onlineStore", definition_capability_source(capabilities.online_store)),
  ])
}

@internal
pub fn definition_capability_source(
  capability: Option(MetaobjectDefinitionCapabilityRecord),
) -> SourceValue {
  case capability {
    Some(MetaobjectDefinitionCapabilityRecord(enabled: enabled)) ->
      src_object([#("enabled", SrcBool(enabled))])
    None -> SrcNull
  }
}

@internal
pub fn field_definition_source(
  definition: MetaobjectFieldDefinitionRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("MetaobjectFieldDefinition")),
    #("key", SrcString(definition.key)),
    #("name", graphql_helpers.option_string_source(definition.name)),
    #(
      "description",
      graphql_helpers.option_string_source(definition.description),
    ),
    #("required", graphql_helpers.option_bool_source(definition.required)),
    #("type", type_source(definition.type_)),
    #(
      "capabilities",
      field_definition_capabilities_source(definition.capabilities),
    ),
    #(
      "validations",
      SrcList(list.map(definition.validations, validation_source)),
    ),
  ])
}

@internal
pub fn field_definition_capabilities_source(
  capabilities: MetaobjectFieldDefinitionCapabilitiesRecord,
) -> SourceValue {
  src_object([
    #(
      "adminFilterable",
      definition_capability_source(capabilities.admin_filterable),
    ),
  ])
}

@internal
pub fn field_definition_reference_source(
  definition: MetaobjectFieldDefinitionReferenceRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("MetaobjectFieldDefinition")),
    #("key", SrcString(definition.key)),
    #("name", graphql_helpers.option_string_source(definition.name)),
    #("required", graphql_helpers.option_bool_source(definition.required)),
    #("type", type_source(definition.type_)),
  ])
}

@internal
pub fn type_source(type_: MetaobjectDefinitionTypeRecord) -> SourceValue {
  src_object([
    #("name", SrcString(type_.name)),
    #("category", graphql_helpers.option_string_source(type_.category)),
  ])
}

@internal
pub fn validation_source(
  validation: MetaobjectFieldDefinitionValidationRecord,
) -> SourceValue {
  src_object([
    #("name", SrcString(validation.name)),
    #("value", graphql_helpers.option_string_source(validation.value)),
  ])
}

@internal
pub fn standard_template_source(
  template: Option(MetaobjectStandardTemplateRecord),
) -> SourceValue {
  case template {
    Some(MetaobjectStandardTemplateRecord(
      type_: type_,
      name: name,
      enabled_by_shopify: enabled_by_shopify,
      enabled_by_shopify_at: enabled_by_shopify_at,
    )) ->
      src_object([
        #("type", graphql_helpers.option_string_source(type_)),
        #("name", graphql_helpers.option_string_source(name)),
        #("enabledByShopify", SrcBool(enabled_by_shopify)),
        #(
          "enabledByShopifyAt",
          graphql_helpers.option_string_source(enabled_by_shopify_at),
        ),
      ])
    None -> SrcNull
  }
}

@internal
pub fn metaobject_capabilities_source(
  capabilities: MetaobjectCapabilitiesRecord,
  definition: Option(MetaobjectDefinitionRecord),
) -> SourceValue {
  let publishable = case definition {
    Some(defn) ->
      case defn.capabilities.publishable {
        Some(MetaobjectDefinitionCapabilityRecord(enabled: False)) -> SrcNull
        _ -> metaobject_publishable_capability_source(capabilities, definition)
      }
    None -> metaobject_publishable_capability_source(capabilities, definition)
  }
  let online_store = case capabilities.online_store {
    Some(MetaobjectOnlineStoreCapabilityRecord(template_suffix: suffix)) ->
      src_object([
        #("templateSuffix", graphql_helpers.option_string_source(suffix)),
      ])
    None -> SrcNull
  }
  src_object([
    #("publishable", publishable),
    #("onlineStore", online_store),
  ])
}

@internal
pub fn metaobject_publishable_capability_source(
  capabilities: MetaobjectCapabilitiesRecord,
  definition: Option(MetaobjectDefinitionRecord),
) -> SourceValue {
  case capabilities.publishable {
    Some(MetaobjectPublishableCapabilityRecord(status: status)) ->
      src_object([#("status", graphql_helpers.option_string_source(status))])
    None ->
      case definition {
        Some(defn) ->
          case defn.capabilities.publishable {
            Some(MetaobjectDefinitionCapabilityRecord(enabled: True)) ->
              src_object([#("status", SrcString("DRAFT"))])
            _ -> SrcNull
          }
        None -> SrcNull
      }
  }
}
