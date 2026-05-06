//// Shared internal metaobject definition domain helpers.

import gleam/dict.{type Dict}
import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode
import gleam/float
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type SourceValue, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull, SrcObject,
  SrcString, source_to_json,
}
import shopify_draft_proxy/proxy/metafield_values
import shopify_draft_proxy/proxy/metaobject_standard_templates_data as standard_templates
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, read_optional_string, single_root_log_draft,
}
import shopify_draft_proxy/state/store.{
  type Store, find_effective_metaobject_by_handle,
  find_effective_metaobject_definition_by_type,
  list_effective_metaobject_definitions, list_effective_metaobjects_by_type,
  upsert_staged_metaobject_definition,
}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type MetaobjectCapabilitiesRecord, type MetaobjectDefinitionCapabilitiesRecord,
  type MetaobjectDefinitionCapabilityRecord, type MetaobjectDefinitionRecord,
  type MetaobjectFieldDefinitionRecord,
  type MetaobjectFieldDefinitionReferenceRecord,
  type MetaobjectFieldDefinitionValidationRecord, type MetaobjectFieldRecord,
  type MetaobjectJsonValue, type MetaobjectRecord, MetaobjectBool,
  MetaobjectCapabilitiesRecord, MetaobjectDefinitionCapabilitiesRecord,
  MetaobjectDefinitionCapabilityRecord, MetaobjectDefinitionRecord,
  MetaobjectDefinitionTypeRecord, MetaobjectFieldDefinitionRecord,
  MetaobjectFieldDefinitionReferenceRecord,
  MetaobjectFieldDefinitionValidationRecord, MetaobjectFieldRecord,
  MetaobjectFloat, MetaobjectInt, MetaobjectList, MetaobjectNull,
  MetaobjectObject, MetaobjectOnlineStoreCapabilityRecord,
  MetaobjectPublishableCapabilityRecord, MetaobjectRecord,
  MetaobjectStandardTemplateRecord, MetaobjectString,
}

const domain_name = "metaobjects"

const execution_name = "stage-locally"

const definition_column_size_limit = 255

@internal
pub type UserError {
  UserError(
    field: Option(List(String)),
    message: String,
    code: String,
    element_key: Option(String),
    element_index: Option(Int),
  )
}

@internal
pub type BulkDeleteJob {
  BulkDeleteJob(id: String, done: Bool)
}

@internal
pub type BulkDeleteWhere {
  BulkDeleteByIds(List(String))
  BulkDeleteByType(String)
  BulkDeleteNoSelector
}

@internal
pub type FieldOperation {
  FieldCreate(Dict(String, root_field.ResolvedValue))
  FieldUpdate(Dict(String, root_field.ResolvedValue))
  FieldDelete(String)
  FieldUpsert(Dict(String, root_field.ResolvedValue))
}

@internal
pub fn normalize_definition_type(
  type_: String,
  requesting_api_client_id: Option(String),
) -> String {
  let resolved = case string.starts_with(type_, "$app:") {
    True ->
      case requesting_api_client_id {
        Some(api_client_id) ->
          "app--"
          <> api_client_id
          <> "--"
          <> string.drop_start(type_, string.length("$app:"))
        None -> type_
      }
    False -> type_
  }
  string.lowercase(resolved)
}

@internal
pub fn normalized_definition_type_from_input(
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Option(String) {
  read_string(input, "type")
  |> option.map(fn(type_) {
    normalize_definition_type(type_, requesting_api_client_id)
  })
}

@internal
pub fn is_app_reserved_definition_type_input(type_: String) -> Bool {
  string.starts_with(type_, "$app:")
}

@internal
pub fn find_effective_metaobject_definition_by_normalized_type(
  store: Store,
  normalized_type: String,
) -> Option(MetaobjectDefinitionRecord) {
  list_effective_metaobject_definitions(store)
  |> list.find(fn(definition) {
    string.lowercase(definition.type_) == normalized_type
  })
  |> option.from_result
}

@internal
pub fn find_effective_metaobject_definition_by_input_type(
  store: Store,
  type_: String,
  requesting_api_client_id: Option(String),
) -> Option(MetaobjectDefinitionRecord) {
  find_effective_metaobject_definition_by_normalized_type(
    store,
    normalize_definition_type(type_, requesting_api_client_id),
  )
}

@internal
pub fn resolved_value_strings(value: root_field.ResolvedValue) -> List(String) {
  case value {
    root_field.StringVal(value) -> [value]
    root_field.ListVal(values) -> list.flat_map(values, resolved_value_strings)
    root_field.ObjectVal(fields) ->
      dict.values(fields) |> list.flat_map(resolved_value_strings)
    _ -> []
  }
}

@internal
pub fn default_definition_access() -> Dict(String, Option(String)) {
  dict.from_list([
    #("admin", Some("PUBLIC_READ_WRITE")),
    #("storefront", Some("NONE")),
  ])
}

@internal
pub fn default_definition_capabilities() -> MetaobjectDefinitionCapabilitiesRecord {
  MetaobjectDefinitionCapabilitiesRecord(
    publishable: Some(MetaobjectDefinitionCapabilityRecord(False)),
    translatable: Some(MetaobjectDefinitionCapabilityRecord(False)),
    renderable: Some(MetaobjectDefinitionCapabilityRecord(False)),
    online_store: Some(MetaobjectDefinitionCapabilityRecord(False)),
  )
}

@internal
pub fn build_create_definition_validation_errors(
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  let type_ = read_string(input, "type")
  let name = read_string(input, "name")
  let description = read_string(input, "description")
  let access = read_object(input, "access")
  []
  |> append_if(
    is_missing_definition_type(type_),
    UserError(
      Some(["definition", "type"]),
      "Type can't be blank",
      "BLANK",
      None,
      None,
    ),
  )
  |> append_definition_name_validation_errors(name)
  |> append_definition_description_validation_errors(description)
  |> append_if(
    case type_, access {
      Some(t), Some(a) ->
        !is_app_reserved_definition_type_input(t) && dict.has_key(a, "admin")
      _, _ -> False
    },
    UserError(
      Some(["definition", "access", "admin"]),
      "Admin access can only be specified on metaobject definitions that have an app-reserved type.",
      "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
      None,
      None,
    ),
  )
  |> append_definition_type_validation_errors(type_, requesting_api_client_id)
  |> append_create_field_definition_key_errors(read_list(
    input,
    "fieldDefinitions",
  ))
}

@internal
pub fn build_create_definition_uniqueness_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  []
  |> append_if(
    case
      normalized_definition_type_from_input(input, requesting_api_client_id)
    {
      Some(t) ->
        case find_effective_metaobject_definition_by_normalized_type(store, t) {
          Some(_) -> True
          None -> False
        }
      None -> False
    },
    UserError(
      Some(["definition", "type"]),
      "Type has already been taken",
      "TAKEN",
      None,
      None,
    ),
  )
}

@internal
pub fn append_definition_name_validation_errors(
  errors: List(UserError),
  name: Option(String),
) -> List(UserError) {
  errors
  |> append_if(
    is_missing_definition_name(name),
    UserError(
      Some(["definition", "name"]),
      "Name can't be blank",
      "BLANK",
      None,
      None,
    ),
  )
  |> append_if(
    string_option_length(name) > definition_column_size_limit,
    UserError(
      Some(["definition", "name"]),
      "Name is too long (maximum is 255 characters)",
      "TOO_LONG",
      None,
      None,
    ),
  )
}

@internal
pub fn append_definition_description_validation_errors(
  errors: List(UserError),
  description: Option(String),
) -> List(UserError) {
  errors
  |> append_if(
    string_option_length(description) > definition_column_size_limit,
    UserError(
      Some(["definition", "description"]),
      "Description is too long (maximum is 255 characters)",
      "TOO_LONG",
      None,
      None,
    ),
  )
}

@internal
pub fn is_missing_definition_name(name: Option(String)) -> Bool {
  case name {
    None -> True
    Some(value) -> string.trim(value) == ""
  }
}

@internal
pub fn is_missing_definition_type(type_: Option(String)) -> Bool {
  case type_ {
    None -> True
    Some(value) -> string.trim(value) == ""
  }
}

@internal
pub fn string_option_length(value: Option(String)) -> Int {
  case value {
    Some(value) -> string.length(value)
    None -> 0
  }
}

@internal
pub fn append_definition_type_validation_errors(
  errors: List(UserError),
  type_: Option(String),
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  case type_ {
    None -> errors
    Some(raw) ->
      case string.trim(raw) {
        "" -> errors
        _ -> {
          let type_ = normalize_definition_type(raw, requesting_api_client_id)
          let length = string.length(type_)
          errors
          |> append_if(
            length < 3,
            UserError(
              Some(["definition", "type"]),
              "Type is too short (minimum is 3 characters)",
              "TOO_SHORT",
              None,
              None,
            ),
          )
          |> append_if(
            length > 255,
            UserError(
              Some(["definition", "type"]),
              "Type is too long (maximum is 255 characters)",
              "TOO_LONG",
              None,
              None,
            ),
          )
          |> append_if(
            !is_valid_definition_type(type_),
            UserError(
              Some(["definition", "type"]),
              "Type contains one or more invalid characters. Only alphanumeric characters, underscores, and dashes are allowed.",
              "INVALID",
              None,
              None,
            ),
          )
        }
      }
  }
}

@internal
pub fn is_valid_definition_type(type_: String) -> Bool {
  type_ != ""
  && list.all(string.to_utf_codepoints(type_), fn(char) {
    is_definition_type_codepoint(string.utf_codepoint_to_int(char))
  })
}

@internal
pub fn is_definition_type_codepoint(codepoint: Int) -> Bool {
  is_ascii_lowercase_letter(codepoint)
  || is_ascii_uppercase_letter(codepoint)
  || is_ascii_digit(codepoint)
  || codepoint == 45
  || codepoint == 95
}

@internal
pub fn is_valid_field_key(key: String) -> Bool {
  key != ""
  && list.all(string.to_utf_codepoints(key), fn(char) {
    let codepoint = string.utf_codepoint_to_int(char)
    is_ascii_lowercase_letter(codepoint)
    || is_ascii_digit(codepoint)
    || codepoint == 95
  })
}

@internal
pub fn is_ascii_lowercase_letter(codepoint: Int) -> Bool {
  codepoint >= 97 && codepoint <= 122
}

@internal
pub fn is_ascii_uppercase_letter(codepoint: Int) -> Bool {
  codepoint >= 65 && codepoint <= 90
}

@internal
pub fn is_ascii_digit(codepoint: Int) -> Bool {
  codepoint >= 48 && codepoint <= 57
}

@internal
pub fn append_create_field_definition_key_errors(
  errors: List(UserError),
  values: List(root_field.ResolvedValue),
) -> List(UserError) {
  list.fold(enumerate_values(values), errors, fn(acc, pair) {
    let #(index, value) = pair
    case value {
      root_field.ObjectVal(input) ->
        append_field_key_validation_error(acc, read_string(input, "key"), index)
      _ -> acc
    }
  })
}

@internal
pub fn append_field_key_validation_error(
  errors: List(UserError),
  key: Option(String),
  index: Int,
) -> List(UserError) {
  case key {
    Some(k) ->
      append_if(
        errors,
        !is_valid_field_key(k),
        invalid_field_key_user_error(index, k),
      )
    None -> errors
  }
}

@internal
pub fn invalid_field_key_user_error(index: Int, key: String) -> UserError {
  UserError(
    Some(["definition", "fieldDefinitions", int.to_string(index), "key"]),
    "is invalid",
    "INVALID",
    Some(key),
    Some(index),
  )
}

@internal
pub fn build_definition_from_create_input(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MetaobjectDefinitionRecord, SyntheticIdentityRegistry) {
  let #(id, after_id) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity,
      "MetaobjectDefinition",
    )
  let #(now, after_time) = synthetic_identity.make_synthetic_timestamp(after_id)
  let type_ =
    normalized_definition_type_from_input(input, requesting_api_client_id)
    |> option.unwrap("metaobject_definition")
  #(
    MetaobjectDefinitionRecord(
      id: id,
      type_: type_,
      name: read_string(input, "name"),
      description: read_string(input, "description"),
      display_name_key: read_string(input, "displayNameKey"),
      access: build_definition_access(
        read_object(input, "access"),
        default_definition_access(),
      ),
      capabilities: normalize_definition_capabilities(
        read_object(input, "capabilities"),
        default_definition_capabilities(),
      ),
      field_definitions: read_field_definitions(read_list(
        input,
        "fieldDefinitions",
      )),
      has_thumbnail_field: Some(False),
      metaobjects_count: Some(0),
      standard_template: None,
      created_at: Some(now),
      updated_at: Some(now),
    ),
    after_time,
  )
}

@internal
pub fn apply_definition_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  existing: MetaobjectDefinitionRecord,
  input: Dict(String, root_field.ResolvedValue),
  reset_field_order: Bool,
  requesting_api_client_id: Option(String),
) -> #(MetaobjectDefinitionRecord, SyntheticIdentityRegistry, List(UserError)) {
  let #(now, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(fields, user_errors, ordered_keys) =
    apply_field_definition_operations(
      existing.field_definitions,
      read_list(input, "fieldDefinitions"),
    )
  let next_fields = case reset_field_order {
    True -> reorder_field_definitions(fields, ordered_keys)
    False -> fields
  }
  let type_ =
    normalized_definition_type_from_input(input, requesting_api_client_id)
    |> option.unwrap(existing.type_)
  let type_errors =
    build_update_definition_type_user_errors(
      store,
      existing.id,
      input,
      requesting_api_client_id,
    )
  let access_errors = build_update_definition_access_user_errors(input, type_)
  let name = read_string_if_present(input, "name", existing.name)
  let description =
    read_string_if_present(input, "description", existing.description)
  let scalar_errors =
    []
    |> append_definition_name_validation_errors(name)
    |> append_definition_description_validation_errors(description)
  let updated =
    MetaobjectDefinitionRecord(
      ..existing,
      type_: type_,
      name: name,
      description: description,
      display_name_key: read_string_if_present(
        input,
        "displayNameKey",
        existing.display_name_key,
      ),
      access: case read_object(input, "access") {
        Some(access) -> build_definition_access(Some(access), existing.access)
        None -> existing.access
      },
      capabilities: case read_object(input, "capabilities") {
        Some(capabilities) ->
          normalize_definition_capabilities(
            Some(capabilities),
            existing.capabilities,
          )
        None -> existing.capabilities
      },
      field_definitions: next_fields,
      updated_at: Some(now),
    )
  #(
    updated,
    next_identity,
    list.flatten([
      type_errors,
      scalar_errors,
      access_errors,
      user_errors,
    ]),
  )
}

@internal
pub fn build_update_definition_type_user_errors(
  store: Store,
  existing_id: String,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  let type_ = read_string(input, "type")
  let validation_errors =
    []
    |> append_definition_type_validation_errors(type_, requesting_api_client_id)
  case
    validation_errors,
    normalized_definition_type_from_input(input, requesting_api_client_id)
  {
    [], Some(normalized_type) ->
      []
      |> append_if(
        case
          find_effective_metaobject_definition_by_normalized_type(
            store,
            normalized_type,
          )
        {
          Some(definition) -> definition.id != existing_id
          None -> False
        },
        UserError(
          Some(["definition", "type"]),
          "Type has already been taken",
          "TAKEN",
          None,
          None,
        ),
      )
    [_, ..], _ -> validation_errors
    [], None -> []
  }
}

@internal
pub fn build_update_definition_access_user_errors(
  input: Dict(String, root_field.ResolvedValue),
  next_type: String,
) -> List(UserError) {
  []
  |> append_if(
    case read_object(input, "access") {
      Some(access) ->
        dict.has_key(access, "admin")
        && !{
          case read_string(input, "type") {
            Some(raw_type) -> is_app_reserved_definition_type_input(raw_type)
            None -> is_app_reserved_resolved_definition_type(next_type)
          }
        }
      None -> False
    },
    UserError(
      Some(["definition", "access", "admin"]),
      "Admin access can only be specified on metaobject definitions that have an app-reserved type.",
      "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
      None,
      None,
    ),
  )
}

@internal
pub fn is_app_reserved_resolved_definition_type(type_: String) -> Bool {
  case string.split(type_, "--") {
    ["app", api_client_id, rest, ..] -> api_client_id != "" && rest != ""
    _ -> False
  }
}

@internal
pub fn standard_template(
  type_: String,
) -> Option(standard_templates.StandardMetaobjectTemplate) {
  case
    standard_templates.templates()
    |> list.find(fn(template) { template.type_ == type_ })
  {
    Ok(template) -> Some(template)
    Error(_) -> None
  }
}

@internal
pub fn build_standard_definition(
  identity: SyntheticIdentityRegistry,
  template: standard_templates.StandardMetaobjectTemplate,
) -> #(MetaobjectDefinitionRecord, SyntheticIdentityRegistry) {
  let #(id, after_id) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity,
      "MetaobjectDefinition",
    )
  let #(now, after_time) = synthetic_identity.make_synthetic_timestamp(after_id)
  #(
    MetaobjectDefinitionRecord(
      id: id,
      type_: template.type_,
      name: Some(template.name),
      description: template.description,
      display_name_key: Some(template.display_name_key),
      access: template.access,
      capabilities: template.capabilities,
      field_definitions: template.field_definitions,
      has_thumbnail_field: template.has_thumbnail_field,
      metaobjects_count: Some(0),
      standard_template: Some(MetaobjectStandardTemplateRecord(
        Some(template.type_),
        Some(template.name),
      )),
      created_at: Some(now),
      updated_at: Some(now),
    ),
    after_time,
  )
}

@internal
pub fn read_field_definitions(
  values: List(root_field.ResolvedValue),
) -> List(MetaobjectFieldDefinitionRecord) {
  list.filter_map(values, fn(value) {
    case value {
      root_field.ObjectVal(obj) ->
        case read_field_definition_input(obj) {
          Some(field) -> Ok(field)
          None -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

@internal
pub fn read_field_definition_input(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(MetaobjectFieldDefinitionRecord) {
  case read_string(input, "key"), read_type_name(input) {
    Some(key), Some(type_name) ->
      Some(MetaobjectFieldDefinitionRecord(
        key: key,
        name: read_string(input, "name"),
        description: read_string(input, "description"),
        required: Some(read_bool(input, "required") |> option.unwrap(False)),
        type_: MetaobjectDefinitionTypeRecord(
          type_name,
          infer_field_type_category(type_name),
        ),
        validations: read_validation_inputs(read_list(input, "validations")),
      ))
    _, _ -> None
  }
}

@internal
pub fn apply_field_definition_operations(
  existing: List(MetaobjectFieldDefinitionRecord),
  operations: List(root_field.ResolvedValue),
) -> #(List(MetaobjectFieldDefinitionRecord), List(UserError), List(String)) {
  list.fold(enumerate_values(operations), #(existing, [], []), fn(acc, pair) {
    let #(fields, errors, ordered_keys) = acc
    let #(index, value) = pair
    case read_field_operation(value) {
      None -> #(fields, errors, ordered_keys)
      Some(operation) -> {
        let key = field_operation_key(operation)
        case key {
          None -> #(
            fields,
            list.append(errors, [
              UserError(
                Some([
                  "definition",
                  "fieldDefinitions",
                  int.to_string(index),
                  "key",
                ]),
                "Key can't be blank",
                "BLANK",
                None,
                Some(index),
              ),
            ]),
            ordered_keys,
          )
          Some(k) ->
            case is_valid_field_key(k) {
              False -> #(
                fields,
                list.append(errors, [invalid_field_key_user_error(index, k)]),
                ordered_keys,
              )
              True ->
                apply_field_operation(
                  fields,
                  errors,
                  list.append(ordered_keys, [k]),
                  operation,
                  k,
                  index,
                )
            }
        }
      }
    }
  })
}

@internal
pub fn apply_field_operation(
  fields: List(MetaobjectFieldDefinitionRecord),
  errors: List(UserError),
  ordered_keys: List(String),
  operation: FieldOperation,
  key: String,
  index: Int,
) -> #(List(MetaobjectFieldDefinitionRecord), List(UserError), List(String)) {
  let existing = find_field_definition(fields, key)
  case operation {
    FieldDelete(_) ->
      case existing {
        None -> #(
          fields,
          list.append(errors, [
            UserError(
              Some([
                "definition",
                "fieldDefinitions",
                int.to_string(index),
                "delete",
              ]),
              "Field definition not found.",
              "NOT_FOUND",
              Some(key),
              Some(index),
            ),
          ]),
          ordered_keys,
        )
        Some(_) -> #(
          list.filter(fields, fn(field) { field.key != key }),
          errors,
          ordered_keys,
        )
      }
    FieldCreate(input) ->
      case existing {
        Some(_) -> #(
          fields,
          list.append(errors, [
            UserError(
              Some([
                "definition",
                "fieldDefinitions",
                int.to_string(index),
                "create",
              ]),
              "Field definition already exists.",
              "TAKEN",
              Some(key),
              Some(index),
            ),
          ]),
          ordered_keys,
        )
        None ->
          case read_field_definition_input(input) {
            Some(field) -> #(list.append(fields, [field]), errors, ordered_keys)
            None -> #(fields, errors, ordered_keys)
          }
      }
    FieldUpdate(input) ->
      case existing {
        None -> #(
          fields,
          list.append(errors, [
            UserError(
              Some([
                "definition",
                "fieldDefinitions",
                int.to_string(index),
                "update",
              ]),
              "Field definition not found.",
              "NOT_FOUND",
              Some(key),
              Some(index),
            ),
          ]),
          ordered_keys,
        )
        Some(field) -> #(
          replace_field_definition(fields, merge_field_definition(field, input)),
          errors,
          ordered_keys,
        )
      }
    FieldUpsert(input) ->
      case existing {
        Some(field) -> #(
          replace_field_definition(fields, merge_field_definition(field, input)),
          errors,
          ordered_keys,
        )
        None ->
          case read_field_definition_input(input) {
            Some(field) -> #(list.append(fields, [field]), errors, ordered_keys)
            None -> #(fields, errors, ordered_keys)
          }
      }
  }
}

@internal
pub fn read_field_operation(
  value: root_field.ResolvedValue,
) -> Option(FieldOperation) {
  case value {
    root_field.ObjectVal(obj) ->
      case read_object(obj, "create") {
        Some(payload) -> Some(FieldCreate(payload))
        None ->
          case read_object(obj, "update") {
            Some(payload) -> Some(FieldUpdate(payload))
            None ->
              case dict.get(obj, "delete") {
                Ok(root_field.StringVal(key)) -> Some(FieldDelete(key))
                Ok(root_field.ObjectVal(payload)) ->
                  Some(FieldDelete(
                    read_string(payload, "key") |> option.unwrap(""),
                  ))
                _ -> Some(FieldUpsert(obj))
              }
          }
      }
    _ -> None
  }
}

@internal
pub fn field_operation_key(operation: FieldOperation) -> Option(String) {
  case operation {
    FieldDelete(key) ->
      case key {
        "" -> None
        _ -> Some(key)
      }
    FieldCreate(input) | FieldUpdate(input) | FieldUpsert(input) ->
      read_string(input, "key")
  }
}

@internal
pub fn merge_field_definition(
  existing: MetaobjectFieldDefinitionRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> MetaobjectFieldDefinitionRecord {
  let type_name = read_type_name(input)
  MetaobjectFieldDefinitionRecord(
    key: read_string(input, "key") |> option.unwrap(existing.key),
    name: read_string_if_present(input, "name", existing.name),
    description: read_string_if_present(
      input,
      "description",
      existing.description,
    ),
    required: case dict.get(input, "required") {
      Ok(root_field.BoolVal(value)) -> Some(value)
      Ok(root_field.NullVal) -> None
      _ -> existing.required
    },
    type_: case type_name {
      Some(name) ->
        MetaobjectDefinitionTypeRecord(
          name,
          infer_field_type_category(name) |> option.or(existing.type_.category),
        )
      None -> existing.type_
    },
    validations: case dict.get(input, "validations") {
      Ok(root_field.ListVal(values)) -> read_validation_inputs(values)
      _ -> existing.validations
    },
  )
}

@internal
pub fn reorder_field_definitions(
  fields: List(MetaobjectFieldDefinitionRecord),
  ordered_keys: List(String),
) -> List(MetaobjectFieldDefinitionRecord) {
  let ordered =
    list.filter_map(ordered_keys |> dedupe_strings(), fn(key) {
      case find_field_definition(fields, key) {
        Some(field) -> Ok(field)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_keys)
  list.append(
    ordered,
    list.filter(fields, fn(field) { !dict.has_key(ordered_set, field.key) }),
  )
}

// ---------------------------------------------------------------------------
// Metaobject construction/update
// ---------------------------------------------------------------------------

@internal
pub fn build_create_metaobject_user_errors(
  type_: Option(String),
  definition: Option(MetaobjectDefinitionRecord),
) -> List(UserError) {
  []
  |> append_if(
    option.is_none(type_),
    UserError(
      Some(["metaobject", "type"]),
      "Type can't be blank",
      "BLANK",
      None,
      None,
    ),
  )
  |> append_if(
    case type_, definition {
      Some(_), None -> True
      _, _ -> False
    },
    UserError(
      Some(["metaobject", "type"]),
      "No metaobject definition exists for type \""
        <> option.unwrap(type_, "")
        <> "\"",
      "UNDEFINED_OBJECT_TYPE",
      None,
      None,
    ),
  )
}

@internal
pub fn build_metaobject_from_create_input(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
) -> #(Option(MetaobjectRecord), SyntheticIdentityRegistry, List(UserError)) {
  let capability_errors =
    build_metaobject_capability_user_errors(input, definition)
  let #(fields, errors) = case capability_errors {
    [_, ..] -> #([], capability_errors)
    [] ->
      build_metaobject_fields_from_input(
        store,
        input,
        definition,
        [],
        True,
        True,
        True,
      )
  }
  case errors {
    [_, ..] -> #(None, identity, errors)
    [] -> {
      let display_name = metaobject_display_name(definition, fields, None)
      let preferred =
        read_non_blank_string(input, "handle")
        |> option.or(display_name)
        |> option.unwrap(definition.type_)
      let handle =
        make_unique_metaobject_handle(store, definition.type_, preferred)
      let #(id, after_id) =
        synthetic_identity.make_proxy_synthetic_gid(identity, "Metaobject")
      let #(now, after_time) =
        synthetic_identity.make_synthetic_timestamp(after_id)
      #(
        Some(MetaobjectRecord(
          id: id,
          handle: handle,
          type_: definition.type_,
          display_name: display_name,
          fields: fields,
          capabilities: build_metaobject_capabilities(input, definition, None),
          created_at: Some(now),
          updated_at: Some(now),
        )),
        after_time,
        [],
      )
    }
  }
}

@internal
pub fn apply_metaobject_update_input(
  store: Store,
  identity: SyntheticIdentityRegistry,
  existing: MetaobjectRecord,
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
) -> #(Option(MetaobjectRecord), SyntheticIdentityRegistry, List(UserError)) {
  let requested_handle = case dict.get(input, "handle") {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.NullVal) -> None
    _ -> Some(existing.handle)
  }
  case requested_handle {
    None | Some("") -> #(None, identity, [
      UserError(
        Some(["metaobject", "handle"]),
        "Handle can't be blank",
        "BLANK",
        None,
        None,
      ),
    ])
    Some(handle) -> {
      case find_effective_metaobject_by_handle(store, existing.type_, handle) {
        Some(owner) if owner.id != existing.id -> #(None, identity, [
          UserError(
            Some(["metaobject", "handle"]),
            "Handle has already been taken",
            "TAKEN",
            None,
            None,
          ),
        ])
        _ -> {
          let capability_errors =
            build_metaobject_capability_user_errors(input, definition)
          let #(fields_from_input, errors) = case capability_errors {
            [_, ..] -> #(existing.fields, capability_errors)
            [] ->
              build_metaobject_fields_from_input(
                store,
                input,
                definition,
                existing.fields,
                False,
                True,
                False,
              )
          }
          case errors {
            [_, ..] -> #(None, identity, errors)
            [] -> {
              let fields = case dict.get(input, "fields") {
                Ok(_) -> fields_from_input
                Error(_) -> existing.fields
              }
              let display_name = case
                should_recompute_metaobject_display_name(
                  definition,
                  existing.fields,
                  fields,
                  input,
                )
              {
                True ->
                  metaobject_display_name(definition, fields, Some(handle))
                False -> existing.display_name
              }
              let #(now, next_identity) =
                synthetic_identity.make_synthetic_timestamp(identity)
              #(
                Some(
                  MetaobjectRecord(
                    ..existing,
                    handle: handle,
                    display_name: display_name,
                    fields: fields,
                    capabilities: build_metaobject_capabilities(
                      input,
                      definition,
                      Some(existing.capabilities),
                    ),
                    updated_at: Some(now),
                  ),
                ),
                next_identity,
                [],
              )
            }
          }
        }
      }
    }
  }
}

@internal
pub fn build_metaobject_fields_from_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
  existing_fields: List(MetaobjectFieldRecord),
  include_missing: Bool,
  require_required: Bool,
  allow_scalar_boolean_coercion: Bool,
) -> #(List(MetaobjectFieldRecord), List(UserError)) {
  let existing_by_key =
    list.fold(existing_fields, dict.new(), fn(acc, field) {
      dict.insert(acc, field.key, field)
    })
  let definitions_by_key =
    list.fold(definition.field_definitions, dict.new(), fn(acc, field) {
      dict.insert(acc, field.key, field)
    })
  let #(fields_by_key, errors, provided_keys, duplicate_indices_by_key) =
    list.fold(
      enumerate_values(read_list(input, "fields")),
      #(existing_by_key, [], [], dict.new()),
      fn(acc, pair) {
        let #(by_key, errs, provided, duplicate_indices) = acc
        let #(index, value) = pair
        case value {
          root_field.ObjectVal(raw_field) ->
            case read_string(raw_field, "key") {
              None -> #(
                by_key,
                list.append(errs, [
                  UserError(
                    Some(["metaobject", "fields", int.to_string(index), "key"]),
                    "Key can't be blank",
                    "BLANK",
                    None,
                    Some(index),
                  ),
                ]),
                provided,
                duplicate_indices,
              )
              Some(key) ->
                case list.contains(provided, key) {
                  True -> #(
                    dict.delete(by_key, key),
                    list.append(errs, [
                      UserError(
                        Some(["metaobject", "fields", int.to_string(index)]),
                        "Field \"" <> key <> "\" duplicates other inputs",
                        "DUPLICATE_FIELD_INPUT",
                        Some(key),
                        None,
                      ),
                    ]),
                    list.append(provided, [key]),
                    dict.insert(duplicate_indices, key, index),
                  )
                  False ->
                    case dict.get(definitions_by_key, key) {
                      Error(_) -> #(
                        by_key,
                        list.append(errs, [
                          UserError(
                            Some(["metaobject", "fields", int.to_string(index)]),
                            "Field definition \"" <> key <> "\" does not exist",
                            "UNDEFINED_OBJECT_FIELD",
                            Some(key),
                            None,
                          ),
                        ]),
                        list.append(provided, [key]),
                        duplicate_indices,
                      )
                      Ok(field_definition) -> {
                        let value_errors =
                          validate_metaobject_field_input_value(
                            store,
                            raw_field,
                            field_definition,
                            index,
                            allow_scalar_boolean_coercion,
                          )
                        case value_errors {
                          [_, ..] -> #(
                            by_key,
                            list.append(errs, value_errors),
                            list.append(provided, [key]),
                            duplicate_indices,
                          )
                          [] -> #(
                            dict.insert(
                              by_key,
                              key,
                              build_metaobject_field_from_input(
                                raw_field,
                                field_definition,
                              ),
                            ),
                            errs,
                            list.append(provided, [key]),
                            duplicate_indices,
                          )
                        }
                      }
                    }
                }
            }
          _ -> #(by_key, errs, provided, duplicate_indices)
        }
      },
    )
  let required_errors = case require_required {
    False -> []
    True ->
      list.filter_map(definition.field_definitions, fn(field_definition) {
        let has_field = dict.has_key(fields_by_key, field_definition.key)
        let provided = list.contains(provided_keys, field_definition.key)
        let duplicate_index =
          dict.get(duplicate_indices_by_key, field_definition.key)
        case
          field_definition.required == Some(True)
          && !has_field
          && { !provided || result.is_ok(duplicate_index) }
        {
          True -> {
            let field_path = case duplicate_index {
              Ok(index) -> ["metaobject", "fields", int.to_string(index)]
              Error(_) -> ["metaobject"]
            }
            Ok(UserError(
              Some(field_path),
              option.unwrap(field_definition.name, field_definition.key)
                <> " can't be blank",
              "OBJECT_FIELD_REQUIRED",
              Some(field_definition.key),
              None,
            ))
          }
          False -> Error(Nil)
        }
      })
  }
  let all_errors = list.append(errors, required_errors)
  let fields =
    list.filter_map(definition.field_definitions, fn(field_definition) {
      case dict.get(fields_by_key, field_definition.key) {
        Ok(field) -> Ok(field)
        Error(_) ->
          case include_missing {
            True -> Ok(empty_metaobject_field(field_definition))
            False -> Error(Nil)
          }
      }
    })
  #(fields, all_errors)
}

fn should_recompute_metaobject_display_name(
  definition: MetaobjectDefinitionRecord,
  existing_fields: List(MetaobjectFieldRecord),
  updated_fields: List(MetaobjectFieldRecord),
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case definition.display_name_key {
    None -> False
    Some(display_key) ->
      case input_has_field_key(input, display_key) {
        False -> False
        True ->
          case
            find_metaobject_field(existing_fields, display_key),
            find_metaobject_field(updated_fields, display_key)
          {
            Some(existing), Some(updated) ->
              existing.value != updated.value
              || existing.json_value != updated.json_value
            None, Some(_) -> True
            _, _ -> False
          }
      }
  }
}

fn input_has_field_key(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  read_list(input, "fields")
  |> list.any(fn(value) {
    case value {
      root_field.ObjectVal(raw_field) ->
        read_string(raw_field, "key") == Some(key)
      _ -> False
    }
  })
}

fn find_metaobject_field(
  fields: List(MetaobjectFieldRecord),
  key: String,
) -> Option(MetaobjectFieldRecord) {
  list.find(fields, fn(field) { field.key == key }) |> option.from_result
}

@internal
pub fn validate_metaobject_field_input_value(
  store: Store,
  raw_field: Dict(String, root_field.ResolvedValue),
  field_definition: MetaobjectFieldDefinitionRecord,
  index: Int,
  allow_scalar_boolean_coercion: Bool,
) -> List(UserError) {
  let value = read_string(raw_field, "value")
  let json_error = case value, field_definition.type_.name {
    Some(v), "json" ->
      case json.parse(v, decode.dynamic) {
        Ok(_) -> []
        Error(_) -> [
          UserError(
            Some(["metaobject", "fields", int.to_string(index)]),
            build_invalid_json_message(v),
            "INVALID_VALUE",
            Some(field_definition.key),
            None,
          ),
        ]
      }
    _, _ -> []
  }
  let coercion_errors =
    metafield_values.validate_metaobject_value(
      store,
      field_definition.type_.name,
      value,
      field_definition.validations,
      allow_scalar_boolean_coercion,
    )
    |> list.map(fn(error) {
      let metafield_values.ValidationError(message:, element_index:) = error
      UserError(
        Some(["metaobject", "fields", int.to_string(index)]),
        message,
        "INVALID_VALUE",
        Some(field_definition.key),
        element_index,
      )
    })
  list.append(json_error, coercion_errors)
}

@internal
pub fn build_metaobject_field_from_input(
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectFieldDefinitionRecord,
) -> MetaobjectFieldRecord {
  let value =
    normalize_metaobject_value(
      definition.type_.name,
      read_string(input, "value"),
    )
  MetaobjectFieldRecord(
    key: definition.key,
    type_: Some(definition.type_.name),
    value: value,
    json_value: read_metaobject_json_value(definition.type_.name, value),
    definition: Some(field_definition_reference(definition)),
  )
}

@internal
pub fn empty_metaobject_field(
  definition: MetaobjectFieldDefinitionRecord,
) -> MetaobjectFieldRecord {
  MetaobjectFieldRecord(
    key: definition.key,
    type_: Some(definition.type_.name),
    value: None,
    json_value: MetaobjectNull,
    definition: Some(field_definition_reference(definition)),
  )
}

@internal
pub fn project_metaobject_fields_through_definition(
  metaobject: MetaobjectRecord,
  definition: Option(MetaobjectDefinitionRecord),
) -> List(MetaobjectFieldRecord) {
  case definition {
    None -> metaobject.fields
    Some(defn) ->
      case defn.field_definitions {
        [] -> metaobject.fields
        definitions -> {
          let fields_by_key =
            list.fold(metaobject.fields, dict.new(), fn(acc, field) {
              dict.insert(acc, field.key, field)
            })
          list.map(definitions, fn(field_definition) {
            case dict.get(fields_by_key, field_definition.key) {
              Ok(field) ->
                MetaobjectFieldRecord(
                  ..field,
                  type_: Some(field_definition.type_.name),
                  json_value: read_metaobject_json_value(
                    field_definition.type_.name,
                    field.value,
                  ),
                  definition: Some(field_definition_reference(field_definition)),
                )
              Error(_) -> empty_metaobject_field(field_definition)
            }
          })
        }
      }
  }
}

@internal
pub fn project_metaobject_through_definition(
  store: Store,
  metaobject: MetaobjectRecord,
) -> MetaobjectRecord {
  let definition =
    find_effective_metaobject_definition_by_type(store, metaobject.type_)
  let fields =
    project_metaobject_fields_through_definition(metaobject, definition)
  let display_name = case definition {
    Some(defn) ->
      case list.is_empty(defn.field_definitions) {
        True -> metaobject.display_name
        False -> metaobject_display_name(defn, fields, Some(metaobject.handle))
      }
    _ -> metaobject.display_name
  }
  MetaobjectRecord(..metaobject, display_name: display_name, fields: fields)
}

@internal
pub fn metaobject_display_name(
  definition: MetaobjectDefinitionRecord,
  fields: List(MetaobjectFieldRecord),
  handle: Option(String),
) -> Option(String) {
  case definition.display_name_key {
    None -> None
    Some(key) ->
      case list.find(fields, fn(field) { field.key == key }) {
        Ok(field) ->
          case field.type_ {
            Some(type_) ->
              case
                is_display_measurement_metaobject_type(type_),
                field.json_value
              {
                True, MetaobjectNull -> field_value_or_handle(field, handle)
                True, json_value ->
                  Some(measurement_display_json_value_to_string(json_value))
                _, _ -> field_value_or_handle(field, handle)
              }
            None -> field_value_or_handle(field, handle)
          }
        Error(_) -> option.map(handle, metaobject_handle_display_name)
      }
  }
}

@internal
pub fn is_display_measurement_metaobject_type(type_: String) -> Bool {
  case string.starts_with(type_, "list.") {
    True -> is_measurement_metaobject_type(string.drop_start(type_, 5))
    False -> is_measurement_metaobject_type(type_)
  }
}

@internal
pub fn field_value_or_handle(
  field: MetaobjectFieldRecord,
  handle: Option(String),
) -> Option(String) {
  case field.value {
    Some(value) -> Some(value)
    None -> option.map(handle, metaobject_handle_display_name)
  }
}

@internal
pub fn measurement_display_json_value_to_string(
  value: MetaobjectJsonValue,
) -> String {
  case value {
    MetaobjectList(items) ->
      "["
      <> string.join(
        list.map(items, measurement_display_json_value_to_string),
        ",",
      )
      <> "]"
    MetaobjectObject(fields) -> {
      let normalized_fields = case dict.get(fields, "unit") {
        Ok(MetaobjectString(unit)) ->
          dict.insert(fields, "unit", MetaobjectString(string.lowercase(unit)))
        _ -> fields
      }
      measurement_object_to_compact_string(normalized_fields)
    }
    _ -> metaobject_json_value_to_compact_string(value)
  }
}

@internal
pub fn measurement_object_to_compact_string(
  fields: Dict(String, MetaobjectJsonValue),
) -> String {
  let value = case dict.get(fields, "value") {
    Ok(value) -> measurement_display_scalar_to_string(value)
    Error(_) -> "null"
  }
  let unit = case dict.get(fields, "unit") {
    Ok(MetaobjectString(unit)) -> json_string_literal(unit)
    Ok(value) -> metaobject_json_value_to_compact_string(value)
    Error(_) -> "null"
  }
  "{\"value\":" <> value <> ",\"unit\":" <> unit <> "}"
}

@internal
pub fn measurement_display_scalar_to_string(
  value: MetaobjectJsonValue,
) -> String {
  case value {
    MetaobjectFloat(float_value) -> {
      let rendered = float.to_string(float_value)
      case string.ends_with(rendered, ".0") {
        True -> string.drop_end(rendered, 2)
        False -> rendered
      }
    }
    _ -> metaobject_json_value_to_compact_string(value)
  }
}

@internal
pub fn json_string_literal(value: String) -> String {
  json.string(value) |> json.to_string
}

@internal
pub fn metaobject_handle_display_name(handle: String) -> String {
  handle
  |> string.replace("-", " ")
  |> string.replace("_", " ")
  |> string.split(" ")
  |> list.filter(fn(part) { part != "" })
  |> list.map(capitalise_handle_part)
  |> string.join(" ")
}

@internal
pub fn capitalise_handle_part(part: String) -> String {
  case string.pop_grapheme(part) {
    Ok(#(first, rest)) -> string.uppercase(first) <> rest
    Error(_) -> part
  }
}

@internal
pub fn make_unique_metaobject_handle(
  store: Store,
  type_: String,
  preferred: String,
) -> String {
  let base = normalize_metaobject_handle(preferred)
  let base = case base {
    "" -> normalize_metaobject_handle(type_)
    other -> other
  }
  unique_handle_loop(
    store,
    type_,
    case base {
      "" -> "metaobject"
      other -> other
    },
    case base {
      "" -> "metaobject"
      other -> other
    },
    1,
  )
}

@internal
pub fn unique_handle_loop(
  store: Store,
  type_: String,
  base: String,
  handle: String,
  suffix: Int,
) -> String {
  case find_effective_metaobject_by_handle(store, type_, handle) {
    None -> handle
    Some(_) ->
      unique_handle_loop(
        store,
        type_,
        base,
        base <> "-" <> int.to_string(suffix + 1),
        suffix + 1,
      )
  }
}

@internal
pub fn normalize_metaobject_handle(value: String) -> String {
  value
  |> string.trim
  |> string.lowercase
  |> string.replace(" ", "-")
  |> string.replace("_", "-")
}

@internal
pub fn build_metaobject_capabilities(
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
  existing: Option(MetaobjectCapabilitiesRecord),
) -> MetaobjectCapabilitiesRecord {
  let raw = read_object(input, "capabilities")
  let existing_record =
    option.unwrap(existing, MetaobjectCapabilitiesRecord(None, None))
  let publishable = case raw {
    Some(capabilities) ->
      case read_object(capabilities, "publishable") {
        Some(publishable) ->
          case read_string(publishable, "status") {
            Some(status) ->
              Some(MetaobjectPublishableCapabilityRecord(Some(status)))
            None -> existing_record.publishable
          }
        None -> existing_record.publishable
      }
    None -> existing_record.publishable
  }
  let publishable = case publishable, existing {
    None, None ->
      case definition.capabilities.publishable {
        Some(MetaobjectDefinitionCapabilityRecord(enabled: True)) ->
          Some(MetaobjectPublishableCapabilityRecord(Some("DRAFT")))
        _ -> None
      }
    _, _ -> publishable
  }
  let online_store = case raw {
    Some(capabilities) ->
      case dict.get(capabilities, "onlineStore") {
        Ok(root_field.NullVal) -> None
        Ok(root_field.ObjectVal(obj)) ->
          Some(
            MetaobjectOnlineStoreCapabilityRecord(read_string(
              obj,
              "templateSuffix",
            )),
          )
        _ ->
          case existing {
            Some(_) -> existing_record.online_store
            None -> None
          }
      }
    None ->
      case existing {
        Some(_) -> existing_record.online_store
        None -> None
      }
  }
  MetaobjectCapabilitiesRecord(
    publishable: publishable,
    online_store: online_store,
  )
}

@internal
pub fn build_metaobject_capability_user_errors(
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
) -> List(UserError) {
  case read_object(input, "capabilities") {
    None -> []
    Some(capabilities) -> {
      []
      |> append_if(
        dict.has_key(capabilities, "publishable")
          && !definition_capability_enabled(definition.capabilities.publishable),
        metaobject_capability_not_enabled_user_error("publishable"),
      )
      |> append_if(
        dict.has_key(capabilities, "onlineStore")
          && !definition_capability_enabled(
          definition.capabilities.online_store,
        ),
        metaobject_capability_not_enabled_user_error("onlineStore"),
      )
    }
  }
}

fn definition_capability_enabled(
  capability: Option(MetaobjectDefinitionCapabilityRecord),
) -> Bool {
  case capability {
    Some(MetaobjectDefinitionCapabilityRecord(enabled: True)) -> True
    _ -> False
  }
}

fn metaobject_capability_not_enabled_user_error(
  capability_key: String,
) -> UserError {
  UserError(
    Some(["capabilities", capability_key]),
    "Capability is not enabled on this definition",
    "CAPABILITY_NOT_ENABLED",
    None,
    None,
  )
}

@internal
pub fn adjust_definition_count(
  store: Store,
  identity: SyntheticIdentityRegistry,
  type_: String,
  delta: Int,
) -> #(Store, SyntheticIdentityRegistry) {
  case find_effective_metaobject_definition_by_type(store, type_) {
    None -> #(store, identity)
    Some(definition) -> {
      let #(now, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let count = option.unwrap(definition.metaobjects_count, 0) + delta
      let next_count = case count < 0 {
        True -> 0
        False -> count
      }
      let updated =
        MetaobjectDefinitionRecord(
          ..definition,
          metaobjects_count: Some(next_count),
          updated_at: Some(now),
        )
      let #(_, next_store) = upsert_staged_metaobject_definition(store, updated)
      #(next_store, next_identity)
    }
  }
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

@internal
pub fn field_definition_reference(
  definition: MetaobjectFieldDefinitionRecord,
) -> MetaobjectFieldDefinitionReferenceRecord {
  MetaobjectFieldDefinitionReferenceRecord(
    key: definition.key,
    name: definition.name,
    required: definition.required,
    type_: definition.type_,
  )
}

@internal
pub fn metaobject_json_value_to_source(
  value: MetaobjectJsonValue,
) -> SourceValue {
  case value {
    MetaobjectNull -> SrcNull
    MetaobjectString(value) -> SrcString(value)
    MetaobjectBool(value) -> SrcBool(value)
    MetaobjectInt(value) -> SrcInt(value)
    MetaobjectFloat(value) -> SrcFloat(value)
    MetaobjectList(items) ->
      SrcList(list.map(items, metaobject_json_value_to_source))
    MetaobjectObject(fields) ->
      SrcObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          #(pair.0, metaobject_json_value_to_source(pair.1))
        })
        |> dict.from_list,
      )
  }
}

// ---------------------------------------------------------------------------
// Readers and small utilities
// ---------------------------------------------------------------------------

@internal
pub fn read_id_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  read_string(graphql_helpers.field_args(field, variables), "id")
}

@internal
pub fn read_string_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  read_string(graphql_helpers.field_args(field, variables), key)
}

@internal
pub fn read_bool_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  read_bool(graphql_helpers.field_args(field, variables), key)
  |> option.unwrap(False)
}

@internal
pub fn read_handle_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Option(String), Option(String)) {
  read_handle_value(read_object_arg(
    graphql_helpers.field_args(field, variables),
    "handle",
  ))
}

@internal
pub fn read_handle_value(
  input: Dict(String, root_field.ResolvedValue),
) -> #(Option(String), Option(String)) {
  #(read_string(input, "type"), read_string(input, "handle"))
}

@internal
pub fn read_string(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  read_optional_string(input, key)
}

@internal
pub fn read_non_blank_string(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case read_string(input, key) {
    Some(value) ->
      case string.trim(value) {
        "" -> None
        trimmed -> Some(trimmed)
      }
    None -> None
  }
}

@internal
pub fn read_string_if_present(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
  existing: Option(String),
) -> Option(String) {
  case dict.get(input, key) {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.NullVal) -> None
    _ -> existing
  }
}

@internal
pub fn read_bool(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(input, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_object(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_object_arg(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  read_object(input, key) |> option.unwrap(dict.new())
}

@internal
pub fn read_list(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(root_field.ResolvedValue) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(values)) -> values
    _ -> []
  }
}

@internal
pub fn read_type_name(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case read_string(input, "type") {
    Some(value) -> Some(value)
    None ->
      case read_object(input, "type") {
        Some(type_obj) -> read_string(type_obj, "name")
        None -> None
      }
  }
}

@internal
pub fn read_validation_inputs(
  values: List(root_field.ResolvedValue),
) -> List(MetaobjectFieldDefinitionValidationRecord) {
  list.filter_map(values, fn(value) {
    case value {
      root_field.ObjectVal(obj) ->
        case read_string(obj, "name") {
          Some(name) ->
            Ok(MetaobjectFieldDefinitionValidationRecord(
              name,
              read_string(obj, "value"),
            ))
          None -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

@internal
pub fn normalize_definition_capabilities(
  raw: Option(Dict(String, root_field.ResolvedValue)),
  base: MetaobjectDefinitionCapabilitiesRecord,
) -> MetaobjectDefinitionCapabilitiesRecord {
  case raw {
    None -> base
    Some(capabilities) ->
      MetaobjectDefinitionCapabilitiesRecord(
        publishable: merge_definition_capability(
          capabilities,
          "publishable",
          base.publishable,
        ),
        translatable: merge_definition_capability(
          capabilities,
          "translatable",
          base.translatable,
        ),
        renderable: merge_definition_capability(
          capabilities,
          "renderable",
          base.renderable,
        ),
        online_store: merge_definition_capability(
          capabilities,
          "onlineStore",
          base.online_store,
        ),
      )
  }
}

@internal
pub fn merge_definition_capability(
  raw: Dict(String, root_field.ResolvedValue),
  key: String,
  base: Option(MetaobjectDefinitionCapabilityRecord),
) -> Option(MetaobjectDefinitionCapabilityRecord) {
  case read_object(raw, key) {
    Some(capability) ->
      case read_bool(capability, "enabled") {
        Some(enabled) -> Some(MetaobjectDefinitionCapabilityRecord(enabled))
        None -> base
      }
    None -> base
  }
}

@internal
pub fn build_definition_access(
  raw: Option(Dict(String, root_field.ResolvedValue)),
  base: Dict(String, Option(String)),
) -> Dict(String, Option(String)) {
  case raw {
    None -> base
    Some(access) ->
      list.fold(dict.to_list(access), base, fn(acc, pair) {
        let #(key, value) = pair
        case value {
          root_field.StringVal(text) -> dict.insert(acc, key, Some(text))
          root_field.NullVal -> dict.insert(acc, key, None)
          _ -> acc
        }
      })
  }
}

@internal
pub fn infer_field_type_category(type_name: String) -> Option(String) {
  case
    string.contains(type_name, "text")
    || type_name == "url"
    || type_name == "color"
  {
    True -> Some("TEXT")
    False ->
      case
        string.contains(type_name, "number")
        || type_name == "rating"
        || type_name == "volume"
        || type_name == "weight"
      {
        True -> Some("NUMBER")
        False ->
          case string.contains(type_name, "reference") {
            True -> Some("REFERENCE")
            False ->
              case type_name {
                "boolean" -> Some("TRUE_FALSE")
                "date" | "date_time" -> Some("DATE_TIME")
                "json" -> Some("JSON")
                _ -> None
              }
          }
      }
  }
}

@internal
pub fn normalize_metaobject_value(
  type_name: String,
  value: Option(String),
) -> Option(String) {
  case value {
    None -> None
    Some(raw) ->
      case type_name {
        "boolean" -> Some(normalize_boolean_value(raw))
        "number_integer" -> Some(normalize_integer_value(raw))
        "date_time" -> Some(normalize_date_time_value(raw))
        "rating" -> Some(normalize_rating_value_string(raw))
        _ ->
          case string.starts_with(type_name, "list.") {
            True ->
              normalize_list_metaobject_value_string(
                string.drop_start(type_name, 5),
                raw,
              )
            False ->
              case is_measurement_metaobject_type(type_name) {
                True -> normalize_measurement_value_string(raw)
                False -> Some(raw)
              }
          }
      }
  }
}

@internal
pub fn normalize_boolean_value(raw: String) -> String {
  case raw {
    "false" -> "false"
    _ -> "true"
  }
}

@internal
pub fn normalize_integer_value(raw: String) -> String {
  case int.parse(raw) {
    Ok(value) -> int.to_string(value)
    Error(_) ->
      case float.parse(raw) {
        Ok(value) -> int.to_string(float.truncate(value))
        Error(_) -> "0"
      }
  }
}

@internal
pub fn normalize_date_time_value(value: String) -> String {
  let lower = string.lowercase(value)
  case string.ends_with(lower, "z") {
    True -> string.drop_end(value, 1) <> "+00:00"
    False ->
      case has_timezone_offset(value) {
        True -> value
        False -> value <> "+00:00"
      }
  }
}

@internal
pub fn has_timezone_offset(value: String) -> Bool {
  let length = string.length(value)
  case length >= 6 {
    False -> False
    True -> {
      let sign = string.slice(value, length - 6, 1)
      let colon = string.slice(value, length - 3, 1)
      case sign, colon {
        "+", ":" -> True
        "-", ":" -> True
        _, _ -> False
      }
    }
  }
}

@internal
pub fn normalize_list_metaobject_value_string(
  type_name: String,
  raw: String,
) -> Option(String) {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      case decode.run(dynamic, decode.list(decode.dynamic)) {
        Ok(items) ->
          case type_name {
            "number_decimal" | "float" ->
              Some(normalize_decimal_list_string(raw))
            "date_time" ->
              items
              |> list.try_map(fn(item) {
                case decode.run(item, decode.string) {
                  Ok(value) ->
                    Ok(MetaobjectString(normalize_date_time_value(value)))
                  Error(_) -> dynamic_to_metaobject_json(item)
                }
              })
              |> result.map(metaobject_json_list_to_string)
              |> option.from_result
            "rating" -> Some(normalize_rating_list_string(raw))
            _ ->
              case is_measurement_metaobject_type(type_name) {
                True ->
                  items
                  |> list.try_map(normalize_measurement_value_dynamic_to_string)
                  |> result.map(fn(parts) {
                    "[" <> string.join(parts, ",") <> "]"
                  })
                  |> option.from_result
                False -> Some(raw)
              }
          }
        Error(_) -> Some(raw)
      }
    Error(_) -> Some(raw)
  }
}

@internal
pub fn is_measurement_metaobject_type(type_name: String) -> Bool {
  case type_name {
    "antenna_gain"
    | "area"
    | "battery_charge_capacity"
    | "battery_energy_capacity"
    | "capacitance"
    | "concentration"
    | "data_storage_capacity"
    | "data_transfer_rate"
    | "dimension"
    | "display_density"
    | "distance"
    | "duration"
    | "electric_current"
    | "electrical_resistance"
    | "energy"
    | "frequency"
    | "illuminance"
    | "inductance"
    | "luminous_flux"
    | "mass_flow_rate"
    | "power"
    | "pressure"
    | "resolution"
    | "rotational_speed"
    | "sound_level"
    | "speed"
    | "temperature"
    | "thermal_power"
    | "voltage"
    | "volume"
    | "volumetric_flow_rate"
    | "weight" -> True
    _ -> False
  }
}

@internal
pub fn normalize_decimal_list_string(raw: String) -> String {
  case string.starts_with(raw, "[") && string.ends_with(raw, "]") {
    True -> {
      let inner = string.drop_start(raw, 1) |> string.drop_end(1)
      "[\"" <> inner <> "\"]"
    }
    False -> raw
  }
}

@internal
pub fn should_parse_metaobject_json_value(type_name: String) -> Bool {
  case type_name {
    "json" | "json_string" | "link" | "money" | "rating" | "rich_text_field" ->
      True
    _ ->
      is_measurement_metaobject_type(type_name)
      || string.starts_with(type_name, "list.")
  }
}

@internal
pub fn normalize_measurement_value_string(raw: String) -> Option(String) {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      normalize_measurement_value_dynamic_to_string(dynamic)
      |> option.from_result
      |> option.or(Some(raw))
    Error(_) -> Some(raw)
  }
}

@internal
pub fn normalize_measurement_value_dynamic_to_string(
  dynamic: Dynamic,
) -> Result(String, Nil) {
  use fields <- result.try(
    decode.run(dynamic, decode.dict(decode.string, decode.dynamic))
    |> result.replace_error(Nil),
  )
  use value <- result.try(
    dict.get(fields, "value") |> result.replace_error(Nil),
  )
  use unit <- result.try(dict.get(fields, "unit") |> result.replace_error(Nil))
  use value_string <- result.try(read_measurement_number_string(value))
  use unit_string <- result.try(
    decode.run(unit, decode.string) |> result.replace_error(Nil),
  )
  Ok(
    "{\"value\":"
    <> value_string
    <> ",\"unit\":\""
    <> string.uppercase(unit_string)
    <> "\"}",
  )
}

@internal
pub fn read_measurement_number_string(dynamic: Dynamic) -> Result(String, Nil) {
  case decode.run(dynamic, decode.int) {
    Ok(value) -> Ok(int.to_string(value) <> ".0")
    Error(_) ->
      case decode.run(dynamic, decode.float) {
        Ok(value) -> Ok(float.to_string(value))
        Error(_) ->
          case decode.run(dynamic, decode.string) {
            Ok(value) ->
              case int.parse(value) {
                Ok(parsed) -> Ok(int.to_string(parsed) <> ".0")
                Error(_) ->
                  case float.parse(value) {
                    Ok(parsed) -> Ok(float.to_string(parsed))
                    Error(_) -> Error(Nil)
                  }
              }
            Error(_) -> Error(Nil)
          }
      }
  }
}

@internal
pub fn normalize_rating_value_string(raw: String) -> String {
  case rating_parts(raw) {
    Some(parts) -> rating_parts_to_string(parts)
    None -> raw
  }
}

@internal
pub fn normalize_rating_list_string(raw: String) -> String {
  case string.starts_with(raw, "[") && string.ends_with(raw, "]") {
    True -> {
      let inner = string.drop_start(raw, 1) |> string.drop_end(1)
      case rating_parts(inner) {
        Some(parts) -> "[" <> rating_parts_to_string(parts) <> "]"
        None -> raw
      }
    }
    False -> raw
  }
}

@internal
pub fn rating_parts_to_string(parts: #(String, String, String)) -> String {
  let #(scale_min, scale_max, value) = parts
  "{\"scale_min\":\""
  <> scale_min
  <> "\",\"scale_max\":\""
  <> scale_max
  <> "\",\"value\":\""
  <> value
  <> "\"}"
}

@internal
pub fn rating_parts(raw: String) -> Option(#(String, String, String)) {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      case normalize_rating_dynamic(dynamic) {
        Ok(MetaobjectObject(fields)) ->
          case
            dict.get(fields, "scale_min"),
            dict.get(fields, "scale_max"),
            dict.get(fields, "value")
          {
            Ok(MetaobjectString(min)),
              Ok(MetaobjectString(max)),
              Ok(MetaobjectString(value))
            -> Some(#(min, max, value))
            _, _, _ -> None
          }
        _ -> None
      }
    Error(_) -> None
  }
}

@internal
pub fn normalize_rating_dynamic(
  dynamic: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  use fields <- result.try(
    decode.run(dynamic, decode.dict(decode.string, decode.dynamic))
    |> result.replace_error(Nil),
  )
  use scale_min <- result.try(read_dynamic_string_field(fields, "scale_min"))
  use scale_max <- result.try(read_dynamic_string_field(fields, "scale_max"))
  use value <- result.try(read_dynamic_string_field(fields, "value"))
  Ok(
    MetaobjectObject(
      dict.from_list([
        #("scale_min", MetaobjectString(scale_min)),
        #("scale_max", MetaobjectString(scale_max)),
        #("value", MetaobjectString(value)),
      ]),
    ),
  )
}

@internal
pub fn read_dynamic_string_field(
  fields: Dict(String, Dynamic),
  key: String,
) -> Result(String, Nil) {
  use value <- result.try(dict.get(fields, key) |> result.replace_error(Nil))
  decode.run(value, decode.string) |> result.replace_error(Nil)
}

@internal
pub fn metaobject_json_list_to_string(
  items: List(MetaobjectJsonValue),
) -> String {
  "["
  <> string.join(list.map(items, metaobject_json_value_to_compact_string), ",")
  <> "]"
}

@internal
pub fn metaobject_json_value_to_compact_string(
  value: MetaobjectJsonValue,
) -> String {
  source_to_json(metaobject_json_value_to_source(value))
  |> json.to_string
}

@internal
pub fn read_metaobject_json_value(
  type_name: String,
  value: Option(String),
) -> MetaobjectJsonValue {
  case value {
    None -> MetaobjectNull
    Some(raw) ->
      case type_name {
        "date_time" -> MetaobjectString(normalize_date_time_value(raw))
        "boolean" ->
          case raw {
            "true" -> MetaobjectBool(True)
            "false" -> MetaobjectBool(False)
            _ -> MetaobjectString(raw)
          }
        "number_integer" ->
          case int.parse(raw) {
            Ok(value) -> MetaobjectInt(value)
            Error(_) -> MetaobjectString(raw)
          }
        "number_decimal" | "float" -> MetaobjectString(raw)
        "rating" -> parse_rating_json_value(raw)
        _ ->
          case string.starts_with(type_name, "list.") {
            True ->
              case
                is_measurement_metaobject_type(string.drop_start(type_name, 5))
              {
                True ->
                  parse_measurement_list_json_value(
                    raw,
                    string.drop_start(type_name, 5),
                  )
                False -> parse_json_value(raw)
              }
            False ->
              case is_measurement_metaobject_type(type_name) {
                True -> parse_measurement_json_value(raw)
                False ->
                  case should_parse_metaobject_json_value(type_name) {
                    True -> parse_json_value(raw)
                    False -> MetaobjectString(raw)
                  }
              }
          }
      }
  }
}

@internal
pub fn parse_measurement_json_value(raw: String) -> MetaobjectJsonValue {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      measurement_dynamic_to_metaobject_json(dynamic)
      |> result.unwrap(parse_json_value(raw))
    Error(_) -> MetaobjectString(raw)
  }
}

@internal
pub fn parse_measurement_list_json_value(
  raw: String,
  type_: String,
) -> MetaobjectJsonValue {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      case decode.run(dynamic, decode.list(decode.dynamic)) {
        Ok(items) ->
          items
          |> list.try_map(fn(item) {
            use value <- result.try(measurement_dynamic_to_metaobject_json(item))
            Ok(normalize_measurement_json_unit_for_list(value, type_))
          })
          |> result.map(MetaobjectList)
          |> result.unwrap(parse_json_value(raw))
        Error(_) -> parse_json_value(raw)
      }
    Error(_) -> MetaobjectString(raw)
  }
}

@internal
pub fn normalize_measurement_json_unit_for_list(
  value: MetaobjectJsonValue,
  type_: String,
) -> MetaobjectJsonValue {
  case value {
    MetaobjectObject(fields) ->
      case dict.get(fields, "unit") {
        Ok(MetaobjectString(unit)) ->
          MetaobjectObject(dict.insert(
            fields,
            "unit",
            MetaobjectString(normalize_measurement_list_json_unit(type_, unit)),
          ))
        _ -> value
      }
    _ -> value
  }
}

@internal
pub fn normalize_measurement_list_json_unit(
  type_: String,
  unit: String,
) -> String {
  let normalized = string.lowercase(unit)
  case type_, normalized {
    "dimension", "centimeters" -> "cm"
    "volume", "milliliters" -> "ml"
    "weight", "kilograms" -> "kg"
    _, _ -> normalized
  }
}

@internal
pub fn measurement_dynamic_to_metaobject_json(
  dynamic: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  use value_dynamic <- result.try(
    decode.run(
      dynamic,
      decode.field("value", decode.dynamic, fn(value) { decode.success(value) }),
    )
    |> result.replace_error(Nil),
  )
  use unit <- result.try(
    decode.run(
      dynamic,
      decode.field("unit", decode.string, fn(value) { decode.success(value) }),
    )
    |> result.replace_error(Nil),
  )
  use value <- result.try(dynamic_number_to_metaobject_json(value_dynamic))
  Ok(
    MetaobjectObject(
      dict.from_list([
        #("value", value),
        #("unit", MetaobjectString(unit)),
      ]),
    ),
  )
}

@internal
pub fn dynamic_number_to_metaobject_json(
  dynamic: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  case decode.run(dynamic, decode.int) {
    Ok(value) -> Ok(MetaobjectInt(value))
    Error(_) ->
      case decode.run(dynamic, decode.float) {
        Ok(value) -> Ok(whole_float_to_metaobject_number(value))
        Error(_) ->
          case decode.run(dynamic, decode.string) {
            Ok(value) ->
              case int.parse(value) {
                Ok(parsed) -> Ok(MetaobjectInt(parsed))
                Error(_) ->
                  case float.parse(value) {
                    Ok(parsed) -> Ok(whole_float_to_metaobject_number(parsed))
                    Error(_) -> Error(Nil)
                  }
              }
            Error(_) -> Error(Nil)
          }
      }
  }
}

@internal
pub fn whole_float_to_metaobject_number(value: Float) -> MetaobjectJsonValue {
  let truncated = float.truncate(value)
  case int.to_float(truncated) == value {
    True -> MetaobjectInt(truncated)
    False -> MetaobjectFloat(value)
  }
}

@internal
pub fn parse_rating_json_value(raw: String) -> MetaobjectJsonValue {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      normalize_rating_dynamic(dynamic)
      |> result.unwrap(parse_json_value(raw))
    Error(_) -> MetaobjectString(raw)
  }
}

@internal
pub fn parse_json_value(raw: String) -> MetaobjectJsonValue {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      dynamic_to_metaobject_json(dynamic)
      |> result.unwrap(MetaobjectString(raw))
    Error(_) -> MetaobjectString(raw)
  }
}

@internal
pub fn dynamic_to_metaobject_json(
  value: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  case decode.run(value, decode.bool) {
    Ok(value) -> Ok(MetaobjectBool(value))
    Error(_) -> dynamic_to_metaobject_json_non_bool(value)
  }
}

@internal
pub fn dynamic_to_metaobject_json_non_bool(
  value: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  case decode.run(value, decode.optional(decode.dynamic)) {
    Ok(None) -> Ok(MetaobjectNull)
    _ -> dynamic_to_metaobject_json_present(value)
  }
}

@internal
pub fn dynamic_to_metaobject_json_present(
  value: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  case decode.run(value, decode.int) {
    Ok(n) -> Ok(MetaobjectInt(n))
    Error(_) ->
      case decode.run(value, decode.float) {
        Ok(n) -> Ok(whole_float_to_metaobject_number(n))
        Error(_) ->
          case decode.run(value, decode.string) {
            Ok(s) -> Ok(MetaobjectString(s))
            Error(_) ->
              case decode.run(value, decode.list(decode.dynamic)) {
                Ok(items) ->
                  items
                  |> list.try_map(dynamic_to_metaobject_json)
                  |> result.map(MetaobjectList)
                Error(_) ->
                  case
                    decode.run(
                      value,
                      decode.dict(decode.string, decode.dynamic),
                    )
                  {
                    Ok(fields) ->
                      fields
                      |> dict.to_list
                      |> list.try_map(fn(pair) {
                        use converted <- result.try(dynamic_to_metaobject_json(
                          pair.1,
                        ))
                        Ok(#(pair.0, converted))
                      })
                      |> result.map(fn(entries) {
                        MetaobjectObject(dict.from_list(entries))
                      })
                    Error(_) -> Error(Nil)
                  }
              }
          }
      }
  }
}

@internal
pub fn read_metaobject_reference_ids_from_field(
  field: MetaobjectFieldRecord,
) -> List(String) {
  case field.type_ {
    Some("metaobject_reference") ->
      case field.value {
        Some(id) -> [id]
        None -> []
      }
    Some("list.metaobject_reference") ->
      case field.json_value {
        MetaobjectList(items) ->
          list.filter_map(items, fn(item) {
            case item {
              MetaobjectString(id) -> Ok(id)
              _ -> Error(Nil)
            }
          })
        _ -> []
      }
    _ -> []
  }
}

@internal
pub fn read_bulk_delete_ids(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case read_bulk_delete_where(args) {
    BulkDeleteByIds(ids) -> list.take(ids, 250)
    BulkDeleteByType(type_) ->
      list.map(list_effective_metaobjects_by_type(store, type_), fn(item) {
        item.id
      })
      |> list.take(250)
    BulkDeleteNoSelector -> []
  }
}

@internal
pub fn read_bulk_delete_where(
  args: Dict(String, root_field.ResolvedValue),
) -> BulkDeleteWhere {
  case read_object(args, "where") {
    Some(where) ->
      case dict.has_key(where, "ids") {
        True -> BulkDeleteByIds(read_string_list(where, "ids"))
        False ->
          case read_string(where, "type") {
            Some(type_) -> BulkDeleteByType(type_)
            None -> BulkDeleteNoSelector
          }
      }
    None -> BulkDeleteNoSelector
  }
}

@internal
pub fn read_string_list(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(String) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn record_not_found_user_error(field: List(String)) -> UserError {
  UserError(Some(field), "Record not found", "RECORD_NOT_FOUND", None, None)
}

@internal
pub fn build_invalid_json_message(value: String) -> String {
  case string.starts_with(string.trim(value), "{") {
    True -> "Value is invalid JSON."
    False -> "Value is invalid JSON."
  }
}

@internal
pub fn find_field_definition(
  fields: List(MetaobjectFieldDefinitionRecord),
  key: String,
) -> Option(MetaobjectFieldDefinitionRecord) {
  list.find(fields, fn(field) { field.key == key }) |> option.from_result
}

@internal
pub fn replace_field_definition(
  fields: List(MetaobjectFieldDefinitionRecord),
  replacement: MetaobjectFieldDefinitionRecord,
) -> List(MetaobjectFieldDefinitionRecord) {
  list.map(fields, fn(field) {
    case field.key == replacement.key {
      True -> replacement
      False -> field
    }
  })
}

@internal
pub fn append_if(items: List(a), condition: Bool, item: a) -> List(a) {
  case condition {
    True -> list.append(items, [item])
    False -> items
  }
}

@internal
pub fn log_draft(root: String, ids: List(String)) -> LogDraft {
  single_root_log_draft(
    root,
    ids,
    store_types.Staged,
    domain_name,
    execution_name,
    None,
  )
}

@internal
pub fn option_string_to_list(value: Option(String)) -> List(String) {
  case value {
    Some(item) -> [item]
    None -> []
  }
}

@internal
pub fn enumerate_values(items: List(a)) -> List(#(Int, a)) {
  enumerate_loop(items, 0, [])
}

@internal
pub fn enumerate_loop(
  items: List(a),
  index: Int,
  acc: List(#(Int, a)),
) -> List(#(Int, a)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] -> enumerate_loop(rest, index + 1, [#(index, first), ..acc])
  }
}

@internal
pub fn dedupe_strings(items: List(String)) -> List(String) {
  dedupe_loop(items, dict.new(), [])
}

@internal
pub fn dedupe_loop(
  items: List(String),
  seen: Dict(String, Bool),
  acc: List(String),
) -> List(String) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      case dict.has_key(seen, first) {
        True -> dedupe_loop(rest, seen, acc)
        False ->
          dedupe_loop(rest, dict.insert(seen, first, True), [first, ..acc])
      }
  }
}

@internal
pub fn list_to_set(items: List(String)) -> Dict(String, Bool) {
  list.fold(items, dict.new(), fn(acc, item) { dict.insert(acc, item, True) })
}
