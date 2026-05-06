//// Shared internal types and helpers for metafield definitions.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  default_selected_field_options, get_selected_child_fields,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type MetafieldDefinitionCapabilitiesRecord,
  type MetafieldDefinitionCapabilityRecord,
  type MetafieldDefinitionConstraintsRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionTypeRecord, type MetafieldDefinitionValidationRecord,
  type ProductMetafieldRecord, type ProductRecord,
  MetafieldDefinitionCapabilitiesRecord, MetafieldDefinitionCapabilityRecord,
  MetafieldDefinitionConstraintValueRecord, MetafieldDefinitionConstraintsRecord,
  MetafieldDefinitionRecord, MetafieldDefinitionTypeRecord,
  MetafieldDefinitionValidationRecord, ProductMetafieldRecord, ProductRecord,
  ProductSeoRecord,
}

@internal
pub type MetafieldDefinitionsError {
  ParseFailed(root_field.RootFieldError)
}

@internal
pub type UserError {
  UserError(field: Option(List(String)), message: String, code: String)
}

@internal
pub type MetafieldsSetUserError {
  MetafieldsSetUserError(
    field: List(String),
    message: String,
    code: Option(String),
    element_index: Option(Int),
  )
}

@internal
pub type SimpleUserError {
  SimpleUserError(field: List(String), message: String)
}

@internal
pub type DeletedMetafieldIdentifier {
  DeletedMetafieldIdentifier(owner_id: String, namespace: String, key: String)
}

@internal
pub type MetafieldsDeleteResult {
  MetafieldsDeleteResult(
    deleted_metafields: List(Option(DeletedMetafieldIdentifier)),
    user_errors: List(SimpleUserError),
    store: Store,
  )
}

@internal
pub type StandardMetafieldDefinitionTemplate {
  StandardMetafieldDefinitionTemplate(
    id: String,
    namespace: String,
    key: String,
    name: String,
    description: Option(String),
    owner_types: List(String),
    type_: MetafieldDefinitionTypeRecord,
    validations: List(MetafieldDefinitionValidationRecord),
    constraints: MetafieldDefinitionConstraintsRecord,
    visible_to_storefront_api: Bool,
  )
}

@internal
pub const pinned_definition_limit = 20

@internal
pub fn read_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  root_field.get_field_arguments(field, variables)
  |> result.unwrap(dict.new())
}

@internal
pub fn read_optional_string(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  mutation_helpers.read_optional_string(input, key)
}

@internal
pub fn child_field(field: Selection, child_name: String) -> Option(Selection) {
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

@internal
pub fn read_optional_bool(
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
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(value)) -> value
    _ -> dict.new()
  }
}

@internal
pub fn has_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(input, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

@internal
pub fn read_input_objects(
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

@internal
pub fn read_definition_identifier(
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

@internal
pub fn find_definition_from_args(
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

@internal
pub fn definition_reference_field(
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

@internal
pub fn get_product_metafields_for_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> List(ProductMetafieldRecord) {
  store_in
  |> all_effective_metafields()
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
  get_product_metafields_for_definition(store_in, definition)
  |> list.map(fn(metafield) { metafield.owner_id })
  |> dedupe_strings
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
    |> dedupe_strings
  list.flat_map(owner_ids, fn(owner_id) {
    store.get_effective_metafields_by_owner_id(store_in, owner_id)
  })
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
pub fn build_enabled_standard_definition(
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
      constraints: Some(template.constraints),
      pinned_position: None,
      validation_status: "ALL_VALID",
    ),
    next_identity,
  )
}

@internal
pub fn build_standard_access(
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

@internal
pub fn build_input_access(
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

@internal
pub fn build_definition_capabilities(
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

@internal
pub fn capability_enabled(
  capabilities: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(capabilities, key) {
    Ok(root_field.ObjectVal(value)) ->
      read_optional_bool(value, "enabled") |> option.unwrap(False)
    _ -> False
  }
}

@internal
pub fn validate_definition_input(
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
  errors
  |> list.append(validate_definition_namespace(input))
  |> list.append(validate_definition_key(input))
  |> list.append(validate_definition_name(input))
  |> list.append(validate_definition_description(input))
  |> list.append(validate_definition_type(input, create))
}

@internal
pub fn blank_definition_error(field_name: String) -> UserError {
  UserError(
    field: Some(["definition", field_name]),
    message: field_name <> " is required.",
    code: "BLANK",
  )
}

@internal
pub fn validate_definition_namespace(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_optional_string(input, "namespace") {
    Some(namespace) ->
      case string.trim(namespace) {
        "" -> []
        _ -> {
          let length = string.length(namespace)
          case definition_reserved_namespace(namespace) {
            True -> [
              definition_user_error(
                "namespace",
                "Namespace " <> namespace <> " is reserved.",
                "RESERVED",
              ),
            ]
            False ->
              case length < 3 {
                True -> [
                  definition_user_error(
                    "namespace",
                    "Namespace is too short (minimum is 3 characters)",
                    "TOO_SHORT",
                  ),
                ]
                False ->
                  case length > 255 {
                    True -> [
                      definition_user_error(
                        "namespace",
                        "Namespace is too long (maximum is 255 characters)",
                        "TOO_LONG",
                      ),
                    ]
                    False ->
                      case valid_definition_identifier_characters(namespace) {
                        True -> []
                        False -> [
                          definition_user_error(
                            "namespace",
                            "Namespace contains one or more invalid characters.",
                            "INVALID_CHARACTER",
                          ),
                        ]
                      }
                  }
              }
          }
        }
      }
    None -> []
  }
}

@internal
pub fn validate_definition_key(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_optional_string(input, "key") {
    Some(key) ->
      case string.trim(key) {
        "" -> []
        _ -> {
          let length = string.length(key)
          case length < 2 {
            True -> [
              definition_user_error(
                "key",
                "Key is too short (minimum is 2 characters)",
                "TOO_SHORT",
              ),
            ]
            False ->
              case length > 64 {
                True -> [
                  definition_user_error(
                    "key",
                    "Key is too long (maximum is 64 characters)",
                    "TOO_LONG",
                  ),
                ]
                False ->
                  case valid_definition_identifier_characters(key) {
                    True -> []
                    False -> [
                      definition_user_error(
                        "key",
                        "Key contains one or more invalid characters.",
                        "INVALID_CHARACTER",
                      ),
                    ]
                  }
              }
          }
        }
      }
    None -> []
  }
}

@internal
pub fn validate_definition_name(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_optional_string(input, "name") {
    Some(name) ->
      case string.length(name) > 255 {
        True -> [
          definition_user_error(
            "name",
            "Name is too long (maximum is 255 characters)",
            "TOO_LONG",
          ),
        ]
        False -> []
      }
    _ -> []
  }
}

@internal
pub fn validate_definition_description(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_optional_string(input, "description") {
    Some(description) ->
      case string.length(description) > 255 {
        True -> [
          definition_user_error(
            "description",
            "Description is too long (maximum is 255 characters)",
            "TOO_LONG",
          ),
        ]
        False -> []
      }
    _ -> []
  }
}

@internal
pub fn validate_definition_type(
  input: Dict(String, root_field.ResolvedValue),
  create: Bool,
) -> List(UserError) {
  case read_optional_string(input, "type") {
    Some(type_name) ->
      case string.trim(type_name) {
        "" -> []
        _ ->
          case valid_definition_type_name(type_name) {
            True -> []
            False -> [
              definition_user_error(
                "type",
                invalid_definition_type_message(type_name),
                "INCLUSION",
              ),
            ]
          }
      }
    None ->
      case create {
        True -> []
        False -> []
      }
  }
}

@internal
pub fn definition_user_error(
  field_name: String,
  message: String,
  code: String,
) -> UserError {
  UserError(
    field: Some(["definition", field_name]),
    message: message,
    code: code,
  )
}

@internal
pub fn definition_reserved_namespace(namespace: String) -> Bool {
  is_protected_write_namespace(namespace)
  || string.starts_with(namespace, "shopify")
}

@internal
pub fn valid_definition_identifier_characters(value: String) -> Bool {
  value
  |> string.to_utf_codepoints
  |> list.all(fn(codepoint) {
    let code = string.utf_codepoint_to_int(codepoint)
    is_alpha_numeric_code(code) || code == 45 || code == 95
  })
}

@internal
pub fn valid_definition_type_name(type_name: String) -> Bool {
  list.contains(valid_definition_type_names(), type_name)
}

@internal
pub fn invalid_definition_type_message(type_name: String) -> String {
  "Type name "
  <> type_name
  <> " is not a valid type. Valid types are: "
  <> string.join(valid_definition_type_names(), ", ")
  <> "."
}

@internal
pub fn valid_definition_type_names() -> List(String) {
  [
    "antenna_gain", "area", "battery_charge_capacity", "battery_energy_capacity",
    "boolean", "capacitance", "color", "concentration", "data_storage_capacity",
    "data_transfer_rate", "date_time", "date", "dimension", "display_density",
    "distance", "duration", "electric_current", "electrical_resistance",
    "energy", "frequency", "id", "illuminance", "inductance", "json", "language",
    "link", "list.antenna_gain", "list.area", "list.battery_charge_capacity",
    "list.battery_energy_capacity", "list.capacitance", "list.color",
    "list.concentration", "list.data_storage_capacity",
    "list.data_transfer_rate", "list.date_time", "list.date", "list.dimension",
    "list.display_density", "list.distance", "list.duration",
    "list.electric_current", "list.electrical_resistance", "list.energy",
    "list.frequency", "list.illuminance", "list.inductance", "list.link",
    "list.luminous_flux", "list.mass_flow_rate", "list.number_decimal",
    "list.number_integer", "list.power", "list.pressure", "list.rating",
    "list.resolution", "list.rotational_speed", "list.single_line_text_field",
    "list.sound_level", "list.speed", "list.temperature", "list.thermal_power",
    "list.url", "list.voltage", "list.volume", "list.volumetric_flow_rate",
    "list.weight", "luminous_flux", "mass_flow_rate", "money",
    "multi_line_text_field", "number_decimal", "number_integer", "power",
    "pressure", "rating", "resolution", "rich_text_field", "rotational_speed",
    "single_line_text_field", "sound_level", "speed", "temperature",
    "thermal_power", "url", "voltage", "volume", "volumetric_flow_rate",
    "weight", "company_reference", "list.company_reference",
    "customer_reference", "list.customer_reference", "product_reference",
    "list.product_reference", "collection_reference",
    "list.collection_reference", "variant_reference", "list.variant_reference",
    "file_reference", "list.file_reference", "product_taxonomy_value_reference",
    "list.product_taxonomy_value_reference", "metaobject_reference",
    "list.metaobject_reference", "mixed_reference", "list.mixed_reference",
    "page_reference", "list.page_reference", "article_reference",
    "list.article_reference", "order_reference", "list.order_reference",
  ]
}

@internal
pub fn validate_definition_pin(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> List(UserError) {
  case definition_is_constrained(definition) {
    True -> [
      UserError(
        field: None,
        message: "Constrained metafield definitions do not support pinning.",
        code: "UNSUPPORTED_PINNING",
      ),
    ]
    False ->
      case definition.pinned_position {
        Some(_) -> []
        None ->
          case
            list.length(list_pinned_definitions(store_in, definition.owner_type))
            >= pinned_definition_limit
          {
            True -> [
              UserError(
                field: None,
                message: "Limit of 20 pinned definitions.",
                code: "PINNED_LIMIT_REACHED",
              ),
            ]
            False -> []
          }
      }
  }
}

@internal
pub fn definition_is_constrained(
  definition: MetafieldDefinitionRecord,
) -> Bool {
  case definition.constraints {
    Some(MetafieldDefinitionConstraintsRecord(key: Some(_), ..)) -> True
    Some(MetafieldDefinitionConstraintsRecord(values: [_, ..], ..)) -> True
    _ -> False
  }
}

@internal
pub fn list_pinned_definitions(
  store_in: Store,
  owner_type: String,
) -> List(MetafieldDefinitionRecord) {
  store.list_effective_metafield_definitions(store_in)
  |> list.filter(fn(definition) {
    definition.owner_type == owner_type && definition.pinned_position != None
  })
}

@internal
pub fn next_pinned_position(
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

@internal
pub fn highest_pinned_position(
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

@internal
pub fn pin_definition(
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

@internal
pub fn unpin_definition(
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

@internal
pub fn stage_delete_definition(
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

@internal
pub fn ensure_product_shells(
  store_in: Store,
  product_ids: List(String),
) -> Store {
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

@internal
pub fn minimal_product_shell(product_id: String) -> ProductRecord {
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

@internal
pub fn maybe_hydrate_definition_for_args(
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

@internal
pub fn read_definition_id_arg(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  read_optional_string(args, "definitionId")
  |> option.or(read_optional_string(args, "id"))
}

@internal
pub fn hydrate_definitions_by_namespace(
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

@internal
pub fn hydrate_definition_by_id(
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

@internal
pub fn upsert_hydrated_definitions(
  definitions: List(MetafieldDefinitionRecord),
  store_in: Store,
) -> Store {
  case definitions {
    [] -> store_in
    [_, ..] -> store.upsert_base_metafield_definitions(store_in, definitions)
  }
}

@internal
pub fn metafield_definition_hydrate_selection(indent: String) -> String {
  indent
  <> "nodes {\n"
  <> metafield_definition_node_hydrate_selection(indent <> "  ")
  <> indent
  <> "}\n"
}

@internal
pub fn metafield_definition_node_hydrate_selection(indent: String) -> String {
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

@internal
pub fn delete_metafields_by_identifiers(
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

@internal
pub fn validate_metafields_delete_inputs(
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

@internal
pub fn read_metafield_delete_identifier(
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

@internal
pub fn validate_metafields_set_inputs(
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

@internal
pub fn validate_metafields_set_input(
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
          let errors =
            list.append(
              errors,
              validate_metafields_set_namespace(namespace, index),
            )
          let errors =
            list.append(errors, validate_metafields_set_key(key, index))
          let errors =
            list.append(errors, validate_metafields_set_type(type_, index))
          let errors =
            list.append(
              errors,
              validate_metafields_set_value_presence(value, index),
            )
          let errors =
            list.append(
              errors,
              validate_metafields_set_definition_type(
                definition,
                input_type,
                index,
              ),
            )
          let errors =
            list.append(
              errors,
              validate_metafields_set_value_type(store_in, type_, value, index),
            )
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

@internal
pub fn validate_metafields_set_namespace(
  namespace: String,
  index: Int,
) -> List(MetafieldsSetUserError) {
  let length = string.length(namespace)
  case is_protected_write_namespace(namespace) {
    True -> [
      MetafieldsSetUserError(
        field: ["metafields", int.to_string(index), "namespace"],
        message: "Namespace " <> namespace <> " is a reserved namespace",
        code: None,
        element_index: None,
      ),
    ]
    False ->
      case length < 3 {
        True -> [
          make_metafields_set_user_error(
            Some(index),
            Some("namespace"),
            "Namespace is too short (minimum is 3 characters)",
            "TOO_SHORT",
          ),
        ]
        False ->
          case length > 255 {
            True -> [
              make_metafields_set_user_error(
                Some(index),
                Some("namespace"),
                "Namespace is too long (maximum is 255 characters)",
                "TOO_LONG",
              ),
            ]
            False ->
              case valid_namespace_characters(namespace) {
                True -> []
                False -> [
                  make_metafields_set_user_error(
                    Some(index),
                    Some("namespace"),
                    "Namespace contains invalid characters.",
                    "INVALID",
                  ),
                ]
              }
          }
      }
  }
}

@internal
pub fn validate_metafields_set_key(
  key: Option(String),
  index: Int,
) -> List(MetafieldsSetUserError) {
  case key {
    Some(k) ->
      case string.trim(k) {
        "" -> [
          make_metafields_set_user_error(
            Some(index),
            Some("key"),
            "Key is required.",
            "BLANK",
          ),
        ]
        _ -> {
          let length = string.length(k)
          case length < 2 {
            True -> [
              make_metafields_set_user_error(
                Some(index),
                Some("key"),
                "Key is too short (minimum is 2 characters)",
                "TOO_SHORT",
              ),
            ]
            False ->
              case length > 64 {
                True -> [
                  make_metafields_set_user_error(
                    Some(index),
                    Some("key"),
                    "Key is too long (maximum is 64 characters)",
                    "TOO_LONG",
                  ),
                ]
                False ->
                  case valid_key_characters(k) {
                    True -> []
                    False -> [
                      make_metafields_set_user_error(
                        Some(index),
                        Some("key"),
                        "Key contains invalid characters.",
                        "INVALID",
                      ),
                    ]
                  }
              }
          }
        }
      }
    None -> [
      make_metafields_set_user_error(
        Some(index),
        Some("key"),
        "Key is required.",
        "BLANK",
      ),
    ]
  }
}

@internal
pub fn validate_metafields_set_type(
  type_: Option(String),
  index: Int,
) -> List(MetafieldsSetUserError) {
  case type_ {
    Some(_) -> []
    None -> [
      MetafieldsSetUserError(
        field: ["metafields", int.to_string(index), "type"],
        message: "Type can't be blank",
        code: Some("BLANK"),
        element_index: None,
      ),
    ]
  }
}

@internal
pub fn validate_metafields_set_value_presence(
  value: Option(String),
  index: Int,
) -> List(MetafieldsSetUserError) {
  case value {
    Some(_) -> []
    None -> [
      make_metafields_set_user_error(
        Some(index),
        Some("value"),
        "Value is required.",
        "BLANK",
      ),
    ]
  }
}

@internal
pub fn validate_metafields_set_definition_type(
  definition: Option(MetafieldDefinitionRecord),
  input_type: Option(String),
  index: Int,
) -> List(MetafieldsSetUserError) {
  case definition, input_type {
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
        ]
        False -> []
      }
    _, _ -> []
  }
}

@internal
pub fn is_protected_write_namespace(namespace: String) -> Bool {
  list.contains(
    ["shopify-l10n-fields", "protected", "shopify_standard"],
    namespace,
  )
}

@internal
pub fn valid_namespace_characters(namespace: String) -> Bool {
  namespace
  |> string.to_utf_codepoints
  |> list.all(fn(codepoint) {
    let code = string.utf_codepoint_to_int(codepoint)
    is_alpha_numeric_code(code) || code == 45 || code == 46 || code == 95
  })
}

@internal
pub fn valid_key_characters(key: String) -> Bool {
  key
  |> string.to_utf_codepoints
  |> list.all(fn(codepoint) {
    let code = string.utf_codepoint_to_int(codepoint)
    is_alpha_numeric_code(code) || code == 45 || code == 95
  })
}

@internal
pub fn is_alpha_numeric_code(code: Int) -> Bool {
  { code >= 48 && code <= 57 }
  || { code >= 65 && code <= 90 }
  || { code >= 97 && code <= 122 }
}

@internal
pub fn validate_metafields_set_value_type(
  store_in: Store,
  type_: Option(String),
  value: Option(String),
  index: Int,
) -> List(MetafieldsSetUserError) {
  case type_, value {
    Some(type_name), Some(raw_value) ->
      case valid_metafield_value_for_type(store_in, type_name, raw_value) {
        True -> []
        False -> [
          make_metafields_set_user_error(
            Some(index),
            Some("value"),
            invalid_value_message(type_name),
            "INVALID_VALUE",
          ),
        ]
      }
    _, _ -> []
  }
}

@internal
pub fn valid_metafield_value_for_type(
  store_in: Store,
  type_name: String,
  raw_value: String,
) -> Bool {
  case string.starts_with(type_name, "list.") {
    True ->
      valid_list_metafield_value(
        store_in,
        string.drop_start(type_name, 5),
        raw_value,
      )
    False -> valid_scalar_metafield_value(store_in, type_name, raw_value)
  }
}

@internal
pub fn valid_scalar_metafield_value(
  store_in: Store,
  type_name: String,
  raw_value: String,
) -> Bool {
  case type_name {
    "number_integer" | "integer" -> valid_integer_string(raw_value)
    "number_decimal" | "float" -> valid_decimal_string(raw_value)
    "boolean" -> raw_value == "true" || raw_value == "false"
    "date" -> valid_iso_date(raw_value)
    "date_time" -> valid_iso_date_time(raw_value)
    "url" -> valid_url(raw_value)
    "color" -> valid_color(raw_value)
    "json" | "json_string" | "rich_text_field" -> valid_json_string(raw_value)
    "money" -> valid_money_json_object(raw_value)
    "weight" | "dimension" | "volume" -> valid_value_unit_json_object(raw_value)
    _ ->
      case string.ends_with(type_name, "_reference") {
        True -> valid_reference_value(store_in, type_name, raw_value)
        False -> True
      }
  }
}

@internal
pub fn valid_list_metafield_value(
  store_in: Store,
  type_name: String,
  raw_value: String,
) -> Bool {
  case json.parse(raw_value, commit.json_value_decoder()) {
    Ok(commit.JsonArray(items)) ->
      list.all(items, fn(item) {
        case item_to_metafield_value_string(item) {
          Some(item_raw) ->
            valid_scalar_metafield_value(store_in, type_name, item_raw)
          None -> False
        }
      })
    _ -> False
  }
}

@internal
pub fn item_to_metafield_value_string(
  value: commit.JsonValue,
) -> Option(String) {
  case value {
    commit.JsonString(s) -> Some(s)
    commit.JsonInt(n) -> Some(int.to_string(n))
    commit.JsonFloat(f) -> Some(float.to_string(f))
    commit.JsonBool(b) ->
      case b {
        True -> Some("true")
        False -> Some("false")
      }
    commit.JsonObject(_) | commit.JsonArray(_) ->
      Some(json.to_string(commit.json_value_to_json(value)))
    commit.JsonNull -> None
  }
}

@internal
pub fn valid_integer_string(value: String) -> Bool {
  case int.parse(value) {
    Ok(_) -> True
    Error(_) -> False
  }
}

@internal
pub fn valid_decimal_string(value: String) -> Bool {
  case float.parse(value) {
    Ok(_) -> True
    Error(_) ->
      case int.parse(value) {
        Ok(_) -> True
        Error(_) -> False
      }
  }
}

@internal
pub fn valid_json_string(value: String) -> Bool {
  case json.parse(value, commit.json_value_decoder()) {
    Ok(_) -> True
    Error(_) -> False
  }
}

@internal
pub fn valid_value_unit_json_object(value: String) -> Bool {
  case json.parse(value, commit.json_value_decoder()) {
    Ok(commit.JsonObject(fields)) ->
      json_number_string_field(fields, "value") != None
      && json_string_field(fields, "unit") != None
    _ -> False
  }
}

@internal
pub fn valid_money_json_object(value: String) -> Bool {
  case json.parse(value, commit.json_value_decoder()) {
    Ok(commit.JsonObject(fields)) ->
      {
        json_number_string_field(fields, "value") != None
        && json_string_field(fields, "unit") != None
      }
      || {
        json_number_string_field(fields, "amount") != None
        && json_string_field(fields, "currency_code") != None
      }
    _ -> False
  }
}

@internal
pub fn valid_url(value: String) -> Bool {
  case
    string.starts_with(value, "https://"),
    string.starts_with(value, "http://")
  {
    True, _ -> url_has_host(string.drop_start(value, 8))
    _, True -> url_has_host(string.drop_start(value, 7))
    _, _ -> False
  }
}

@internal
pub fn url_has_host(rest: String) -> Bool {
  case string.split(rest, on: "/") {
    [host, ..] -> host != "" && !string.contains(host, " ")
    _ -> False
  }
}

@internal
pub fn valid_color(value: String) -> Bool {
  case string.length(value) == 7, string.slice(value, 0, 1) {
    True, "#" ->
      value
      |> string.drop_start(1)
      |> string.to_utf_codepoints
      |> list.all(fn(codepoint) {
        let code = string.utf_codepoint_to_int(codepoint)
        { code >= 48 && code <= 57 }
        || { code >= 65 && code <= 70 }
        || { code >= 97 && code <= 102 }
      })
    _, _ -> False
  }
}

@internal
pub fn valid_iso_date(value: String) -> Bool {
  case string.split(value, on: "-") {
    [year, month, day] ->
      string.length(year) == 4
      && string.length(month) == 2
      && string.length(day) == 2
      && valid_date_parts(year, month, day)
    _ -> False
  }
}

@internal
pub fn valid_date_parts(year: String, month: String, day: String) -> Bool {
  case int.parse(year), int.parse(month), int.parse(day) {
    Ok(y), Ok(m), Ok(d) -> {
      let max_day = days_in_month(y, m)
      m >= 1 && m <= 12 && d >= 1 && d <= max_day
    }
    _, _, _ -> False
  }
}

@internal
pub fn days_in_month(year: Int, month: Int) -> Int {
  case month {
    1 | 3 | 5 | 7 | 8 | 10 | 12 -> 31
    4 | 6 | 9 | 11 -> 30
    2 ->
      case is_leap_year(year) {
        True -> 29
        False -> 28
      }
    _ -> 0
  }
}

@internal
pub fn is_leap_year(year: Int) -> Bool {
  year % 400 == 0 || { year % 4 == 0 && year % 100 != 0 }
}

@internal
pub fn valid_iso_date_time(value: String) -> Bool {
  case string.split(value, on: "T") {
    [date, time] -> valid_iso_date(date) && valid_time_with_optional_zone(time)
    _ -> False
  }
}

@internal
pub fn valid_time_with_optional_zone(value: String) -> Bool {
  let time = strip_timezone(value)
  let time = case string.split(time, on: ".") {
    [whole, _fraction] -> whole
    [whole] -> whole
    _ -> ""
  }
  case string.split(time, on: ":") {
    [hour, minute, second] ->
      string.length(hour) == 2
      && string.length(minute) == 2
      && string.length(second) == 2
      && valid_time_parts(hour, minute, second)
    _ -> False
  }
}

@internal
pub fn strip_timezone(value: String) -> String {
  let lowered = string.lowercase(value)
  case string.ends_with(lowered, "z") {
    True -> string.drop_end(value, 1)
    False -> {
      let len = string.length(value)
      case len >= 6 {
        False -> value
        True -> {
          let sign = string.slice(value, len - 6, 1)
          let colon = string.slice(value, len - 3, 1)
          case { sign == "+" || sign == "-" } && colon == ":" {
            True -> string.drop_end(value, 6)
            False -> value
          }
        }
      }
    }
  }
}

@internal
pub fn valid_time_parts(hour: String, minute: String, second: String) -> Bool {
  case int.parse(hour), int.parse(minute), int.parse(second) {
    Ok(h), Ok(m), Ok(s) ->
      h >= 0 && h <= 23 && m >= 0 && m <= 59 && s >= 0 && s <= 60
    _, _, _ -> False
  }
}

@internal
pub fn valid_reference_value(
  store_in: Store,
  type_name: String,
  value: String,
) -> Bool {
  case reference_gid_resource_type(type_name, value) {
    Some(_) -> reference_exists_or_store_is_cold(store_in, type_name, value)
    None -> False
  }
}

@internal
pub fn reference_gid_resource_type(
  type_name: String,
  value: String,
) -> Option(String) {
  case string.split(value, "/") {
    ["gid:", "", "shopify", resource_type, id] ->
      case reference_type_accepts_resource(type_name, resource_type, id) {
        True -> Some(resource_type)
        False -> None
      }
    _ -> None
  }
}

@internal
pub fn reference_type_accepts_resource(
  type_name: String,
  resource_type: String,
  id: String,
) -> Bool {
  case type_name {
    "product_reference" ->
      resource_type == "Product" && valid_numeric_gid_id(id)
    "variant_reference" ->
      resource_type == "ProductVariant" && valid_numeric_gid_id(id)
    "collection_reference" ->
      resource_type == "Collection" && valid_numeric_gid_id(id)
    "customer_reference" ->
      resource_type == "Customer" && valid_numeric_gid_id(id)
    "metaobject_reference" ->
      resource_type == "Metaobject" && string.length(id) > 0
    "file_reference" -> string.length(id) > 0
    "mixed_reference" ->
      string.length(resource_type) > 0 && string.length(id) > 0
    _ -> True
  }
}

@internal
pub fn valid_numeric_gid_id(id: String) -> Bool {
  case int.parse(id) {
    Ok(_) -> True
    Error(_) -> False
  }
}

@internal
pub fn reference_exists_or_store_is_cold(
  store_in: Store,
  type_name: String,
  value: String,
) -> Bool {
  case type_name {
    "product_reference" -> {
      let known_count = list.length(store.list_effective_products(store_in))
      known_count == 0
      || store.get_effective_product_by_id(store_in, value) != None
    }
    "variant_reference" -> {
      let known_count =
        list.length(store.list_effective_product_variants(store_in))
      known_count == 0
      || store.get_effective_variant_by_id(store_in, value) != None
    }
    "collection_reference" -> {
      let known_count = list.length(store.list_effective_collections(store_in))
      known_count == 0
      || store.get_effective_collection_by_id(store_in, value) != None
    }
    "customer_reference" -> {
      let known_count = list.length(store.list_effective_customers(store_in))
      known_count == 0
      || store.get_effective_customer_by_id(store_in, value) != None
    }
    "metaobject_reference" ->
      store.get_effective_metaobject_by_id(store_in, value) != None
    "file_reference" -> store.get_effective_file_by_id(store_in, value) != None
    "mixed_reference" -> True
    _ -> True
  }
}

@internal
pub fn invalid_value_message(type_name: String) -> String {
  case string.starts_with(type_name, "list.") {
    True -> "Value is invalid for " <> type_name <> "."
    False ->
      case type_name {
        "number_integer" | "integer" -> "Value must be an integer."
        "number_decimal" | "float" -> "Value must be a valid decimal."
        "boolean" -> "Value must be true or false."
        "date" -> "Value must be a valid date."
        "date_time" -> "Value must be a valid date time."
        "url" -> "Value must be a valid URL."
        "color" -> "Value must be a hex color code."
        "json" | "json_string" | "rich_text_field" ->
          "Value must be valid JSON."
        _ -> "Value is invalid for " <> type_name <> "."
      }
  }
}

@internal
pub fn validate_definition_value(
  definition: Option(MetafieldDefinitionRecord),
  value: Option(String),
  index: Int,
) -> List(MetafieldsSetUserError) {
  case definition, value {
    Some(def), Some(raw_value) ->
      list.filter_map(def.validations, fn(validation) {
        case validation.name, validation.value {
          "max", Some(max_raw) ->
            case
              definition_max_error(def.type_.name, raw_value, max_raw, index)
            {
              Some(error) -> Ok(error)
              None -> Error(Nil)
            }
          "min", Some(min_raw) ->
            case
              definition_min_error(def.type_.name, raw_value, min_raw, index)
            {
              Some(error) -> Ok(error)
              None -> Error(Nil)
            }
          "regex", Some(pattern) ->
            case value_matches_supported_regex(raw_value, pattern) {
              True -> Error(Nil)
              False ->
                Ok(make_metafields_set_user_error(
                  Some(index),
                  Some("value"),
                  "Value does not match the metafield definition pattern.",
                  "INVALID_VALUE",
                ))
            }
          "allowed_list", Some(allowed_raw) ->
            case value_in_allowed_list(def.type_.name, raw_value, allowed_raw) {
              True -> Error(Nil)
              False ->
                Ok(make_metafields_set_user_error(
                  Some(index),
                  Some("value"),
                  "Value is not included in the metafield definition allowed values.",
                  "INCLUSION",
                ))
            }
          _, _ -> Error(Nil)
        }
      })
    _, _ -> []
  }
}

@internal
pub fn definition_max_error(
  type_name: String,
  raw_value: String,
  max_raw: String,
  index: Int,
) -> Option(MetafieldsSetUserError) {
  case int.parse(max_raw) {
    Ok(max) ->
      case numeric_metafield_type(type_name) {
        True ->
          case parse_metafield_number(raw_value) {
            Some(number) ->
              case number >. int.to_float(max) {
                True ->
                  Some(make_metafields_set_user_error(
                    Some(index),
                    Some("value"),
                    "Value must be less than or equal to "
                      <> int.to_string(max)
                      <> ".",
                    "LESS_THAN_OR_EQUAL_TO",
                  ))
                False -> None
              }
            None -> None
          }
        False ->
          case string.length(raw_value) > max {
            True ->
              Some(make_metafields_set_user_error(
                Some(index),
                Some("value"),
                "Value must be "
                  <> int.to_string(max)
                  <> " characters or fewer for this metafield definition.",
                "LESS_THAN_OR_EQUAL_TO",
              ))
            False -> None
          }
      }
    Error(_) -> None
  }
}

@internal
pub fn definition_min_error(
  type_name: String,
  raw_value: String,
  min_raw: String,
  index: Int,
) -> Option(MetafieldsSetUserError) {
  case int.parse(min_raw) {
    Ok(min) ->
      case numeric_metafield_type(type_name) {
        True ->
          case parse_metafield_number(raw_value) {
            Some(number) ->
              case number <. int.to_float(min) {
                True ->
                  Some(make_metafields_set_user_error(
                    Some(index),
                    Some("value"),
                    "Value must be greater than or equal to "
                      <> int.to_string(min)
                      <> ".",
                    "GREATER_THAN_OR_EQUAL_TO",
                  ))
                False -> None
              }
            None -> None
          }
        False ->
          case string.length(raw_value) < min {
            True ->
              Some(make_metafields_set_user_error(
                Some(index),
                Some("value"),
                "Value must be "
                  <> int.to_string(min)
                  <> " characters or more for this metafield definition.",
                "TOO_SHORT",
              ))
            False -> None
          }
      }
    Error(_) -> None
  }
}

@internal
pub fn numeric_metafield_type(type_name: String) -> Bool {
  case type_name {
    "number_integer" | "integer" | "number_decimal" | "float" -> True
    _ -> False
  }
}

@internal
pub fn parse_metafield_number(raw_value: String) -> Option(Float) {
  case float.parse(raw_value) {
    Ok(value) -> Some(value)
    Error(_) ->
      case int.parse(raw_value) {
        Ok(value) -> Some(int.to_float(value))
        Error(_) -> None
      }
  }
}

@internal
pub fn value_matches_supported_regex(value: String, pattern: String) -> Bool {
  case pattern {
    "^[A-Z]+$" -> all_codepoints_match(value, is_uppercase_code)
    "^[a-z]+$" -> all_codepoints_match(value, is_lowercase_code)
    "^[0-9]+$" -> all_codepoints_match(value, is_digit_code)
    "^[a-zA-Z0-9_-]+$" ->
      all_codepoints_match(value, fn(code) {
        is_alpha_numeric_code(code) || code == 45 || code == 95
      })
    "^#[0-9A-Fa-f]{6}$" -> valid_color(value)
    _ -> True
  }
}

@internal
pub fn all_codepoints_match(value: String, predicate: fn(Int) -> Bool) -> Bool {
  string.length(value) > 0
  && {
    value
    |> string.to_utf_codepoints
    |> list.all(fn(codepoint) {
      predicate(string.utf_codepoint_to_int(codepoint))
    })
  }
}

@internal
pub fn is_uppercase_code(code: Int) -> Bool {
  code >= 65 && code <= 90
}

@internal
pub fn is_lowercase_code(code: Int) -> Bool {
  code >= 97 && code <= 122
}

@internal
pub fn is_digit_code(code: Int) -> Bool {
  code >= 48 && code <= 57
}

@internal
pub fn value_in_allowed_list(
  type_name: String,
  raw_value: String,
  allowed_raw: String,
) -> Bool {
  case json.parse(allowed_raw, commit.json_value_decoder()) {
    Ok(commit.JsonArray(items)) -> {
      let allowed =
        list.filter_map(items, fn(item) {
          case item_to_metafield_value_string(item) {
            Some(item_raw) -> Ok(item_raw)
            None -> Error(Nil)
          }
        })
      case string.starts_with(type_name, "list.") {
        True ->
          case json.parse(raw_value, commit.json_value_decoder()) {
            Ok(commit.JsonArray(values)) ->
              list.all(values, fn(item) {
                case item_to_metafield_value_string(item) {
                  Some(item_raw) -> list.contains(allowed, item_raw)
                  None -> False
                }
              })
            _ -> False
          }
        False -> list.contains(allowed, raw_value)
      }
    }
    _ -> True
  }
}

@internal
pub fn json_number_string_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
) -> Option(String) {
  case lookup_json_field(fields, key) {
    Some(commit.JsonInt(n)) -> Some(int.to_string(n))
    Some(commit.JsonFloat(f)) -> Some(float.to_string(f))
    Some(commit.JsonString(s)) ->
      case valid_decimal_string(s) {
        True -> Some(s)
        False -> None
      }
    _ -> None
  }
}

@internal
pub fn json_string_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
) -> Option(String) {
  case lookup_json_field(fields, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

@internal
pub fn lookup_json_field(
  fields: List(#(String, commit.JsonValue)),
  key: String,
) -> Option(commit.JsonValue) {
  list.find(fields, fn(pair) {
    let #(field_key, _) = pair
    field_key == key
  })
  |> option.from_result
  |> option.map(fn(pair) {
    let #(_, value) = pair
    value
  })
}

@internal
pub fn validate_compare_digest(
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

@internal
pub fn upsert_metafields_set_inputs(
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

@internal
pub fn group_metafields_by_owner(
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

@internal
pub fn append_metafields_set_owner_input(
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

@internal
pub fn upsert_owner_metafields(
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
        market_localizable_content: [],
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

@internal
pub fn replace_metafield_by_identity(
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

@internal
pub fn read_metafields_set_namespace(
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

@internal
pub fn default_app_metafield_namespace() -> String {
  "app--347082227713"
}

@internal
pub fn find_owner_metafield(
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

@internal
pub fn owner_type_from_id(owner_id: String) -> Option(String) {
  case string.split(owner_id, "/") {
    ["gid:", "", "shopify", "Product", _] -> Some("PRODUCT")
    ["gid:", "", "shopify", "ProductVariant", _] -> Some("PRODUCTVARIANT")
    ["gid:", "", "shopify", "Collection", _] -> Some("COLLECTION")
    ["gid:", "", "shopify", "Customer", _] -> Some("CUSTOMER")
    ["gid:", "", "shopify", "Order", _] -> Some("ORDER")
    ["gid:", "", "shopify", "Company", _] -> Some("COMPANY")
    _ -> None
  }
}

@internal
pub fn make_metafields_set_user_error(
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
    element_index: None,
  )
}

@internal
pub fn metafield_definitions_from_hydrate_response(
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

@internal
pub fn metafield_definition_from_json(
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

@internal
pub fn metafield_definition_validation_from_json(
  value: commit.JsonValue,
) -> Result(MetafieldDefinitionValidationRecord, Nil) {
  use name <- result.try(json_get_required_string(value, "name"))
  Ok(MetafieldDefinitionValidationRecord(
    name: name,
    value: json_get_string(value, "value"),
  ))
}

@internal
pub fn definition_access_from_json(
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

@internal
pub fn definition_capabilities_from_json(
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

@internal
pub fn definition_capability_from_json(
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

@internal
pub fn definition_constraints_from_json(
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

@internal
pub fn json_get_required_string(
  value: commit.JsonValue,
  key: String,
) -> Result(String, Nil) {
  case json_get_string(value, key) {
    Some(s) -> Ok(s)
    None -> Error(Nil)
  }
}

@internal
pub fn json_get_array(
  value: commit.JsonValue,
  key: String,
) -> List(commit.JsonValue) {
  case json_get(value, key) {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

@internal
pub fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(b)) -> Some(b)
    _ -> None
  }
}

@internal
pub fn json_get_int(value: commit.JsonValue, key: String) -> Option(Int) {
  case json_get(value, key) {
    Some(commit.JsonInt(i)) -> Some(i)
    _ -> None
  }
}

@internal
pub fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

@internal
pub fn json_get(
  value: commit.JsonValue,
  key: String,
) -> Option(commit.JsonValue) {
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

@internal
pub fn dict_is_empty(values: Dict(String, a)) -> Bool {
  values
  |> dict.to_list
  |> list.is_empty
}

@internal
pub fn dedupe_strings(items: List(String)) -> List(String) {
  dedupe_strings_loop(items, dict.new(), [])
}

@internal
pub fn dedupe_strings_loop(
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

@internal
pub fn enumerate(items: List(a)) -> List(#(Int, a)) {
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
