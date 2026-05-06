//// Mutation handling for the metafield definitions domain.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{get_field_response_key}
import shopify_draft_proxy/proxy/metafield_definitions/serializers
import shopify_draft_proxy/proxy/metafield_definitions/types as definition_types
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type MetafieldDefinitionCapabilitiesRecord,
  type MetafieldDefinitionConstraintsRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionValidationRecord,
  MetafieldDefinitionCapabilitiesRecord, MetafieldDefinitionCapabilityRecord,
  MetafieldDefinitionConstraintValueRecord, MetafieldDefinitionConstraintsRecord,
  MetafieldDefinitionRecord, MetafieldDefinitionTypeRecord,
  MetafieldDefinitionValidationRecord,
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

@internal
pub fn handle_mutation_fields(
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

@internal
pub fn dispatch_mutation_field(
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

@internal
pub fn no_top_level_errors(
  result: #(Json, Store, SyntheticIdentityRegistry, List(String)),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String), List(Json)) {
  let #(payload, store_out, identity_out, staged_ids) = result
  #(payload, store_out, identity_out, staged_ids, [])
}

@internal
pub fn metafield_definitions_status_for(
  staged_resource_ids: List(String),
) -> store.EntryStatus {
  case staged_resource_ids {
    [] -> store_types.Failed
    [_, ..] -> store_types.Staged
  }
}

@internal
pub fn metafield_definitions_notes_for(root_field_name: String) -> String {
  case root_field_name {
    "metafieldsSet" ->
      "Staged locally in the in-memory owner-scoped metafield draft store."
    "metafieldsDelete" | "metafieldDelete" ->
      "Staged owner-scoped metafield deletions locally in the in-memory draft store."
    _ -> "Staged locally in the in-memory metafield definition draft store."
  }
}

@internal
pub fn standard_templates() -> List(
  definition_types.StandardMetafieldDefinitionTemplate,
) {
  [
    definition_types.StandardMetafieldDefinitionTemplate(
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
      constraints: empty_definition_constraints(),
      visible_to_storefront_api: True,
    ),
    definition_types.StandardMetafieldDefinitionTemplate(
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
      constraints: empty_definition_constraints(),
      visible_to_storefront_api: True,
    ),
    definition_types.StandardMetafieldDefinitionTemplate(
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
      constraints: empty_definition_constraints(),
      visible_to_storefront_api: True,
    ),
    definition_types.StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/10001",
      namespace: "shopify",
      key: "color-pattern",
      name: "Color",
      description: Some(
        "Defines the primary color or pattern, such as blue or striped",
      ),
      owner_types: ["PRODUCT"],
      type_: MetafieldDefinitionTypeRecord(
        name: "list.metaobject_reference",
        category: Some("REFERENCE"),
      ),
      validations: [],
      constraints: MetafieldDefinitionConstraintsRecord(
        key: Some("category"),
        values: [
          MetafieldDefinitionConstraintValueRecord(value: "ap-2-1-1"),
        ],
      ),
      visible_to_storefront_api: True,
    ),
    definition_types.StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/10004",
      namespace: "shopify",
      key: "material",
      name: "Material",
      description: Some(
        "Defines a product's primary material, such as cotton or wool",
      ),
      owner_types: ["PRODUCT"],
      type_: MetafieldDefinitionTypeRecord(
        name: "list.metaobject_reference",
        category: Some("REFERENCE"),
      ),
      validations: [],
      constraints: MetafieldDefinitionConstraintsRecord(
        key: Some("category"),
        values: [
          MetafieldDefinitionConstraintValueRecord(value: "ap-2-1-1"),
        ],
      ),
      visible_to_storefront_api: True,
    ),
  ]
}

@internal
pub fn empty_definition_constraints() -> MetafieldDefinitionConstraintsRecord {
  MetafieldDefinitionConstraintsRecord(key: None, values: [])
}

@internal
pub fn find_standard_template(
  args: Dict(String, root_field.ResolvedValue),
) -> #(
  Option(definition_types.StandardMetafieldDefinitionTemplate),
  List(definition_types.UserError),
) {
  let owner_type = definition_types.read_optional_string(args, "ownerType")
  let id = definition_types.read_optional_string(args, "id")
  let namespace = definition_types.read_optional_string(args, "namespace")
  let key = definition_types.read_optional_string(args, "key")
  case owner_type, id, namespace, key {
    None, _, _, _ | Some(_), None, None, _ | Some(_), None, _, None -> #(None, [
      definition_types.UserError(
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
          definition_types.UserError(
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
          definition_types.UserError(
            field: None,
            message: "A standard definition wasn't found for the specified owner type, namespace, and key.",
            code: "TEMPLATE_NOT_FOUND",
          ),
        ])
      }
    }
  }
}

@internal
pub fn serialize_standard_metafield_definition_enable_mutation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let #(template, user_errors) = find_standard_template(args)
  case template {
    None -> #(
      serializers.serialize_standard_enable_payload(
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
      case
        validate_and_maybe_pin_definition(
          store_in,
          definition,
          definition_types.read_optional_bool(args, "pin"),
        )
      {
        Error(user_errors) -> #(
          serializers.serialize_standard_enable_payload(
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
        Ok(definition) -> {
          let next_store =
            store.upsert_staged_metafield_definitions(store_in, [definition])
          #(
            serializers.serialize_standard_enable_payload(
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
  }
}

@internal
pub fn build_enabled_standard_definition(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
  template: definition_types.StandardMetafieldDefinitionTemplate,
) -> #(MetafieldDefinitionRecord, SyntheticIdentityRegistry) {
  let owner_type =
    definition_types.read_optional_string(args, "ownerType")
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
      capabilities: build_definition_capabilities(definition_types.read_object(
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
  template: definition_types.StandardMetafieldDefinitionTemplate,
) -> Dict(String, Json) {
  let access = definition_types.read_object(args, "access")
  dict.from_list([
    #(
      "admin",
      json.string(
        definition_types.read_optional_string(access, "admin")
        |> option.unwrap("PUBLIC_READ_WRITE"),
      ),
    ),
    #(
      "storefront",
      json.string(
        definition_types.read_optional_string(access, "storefront")
        |> option.unwrap(case template.visible_to_storefront_api {
          True -> "PUBLIC_READ"
          False -> "NONE"
        }),
      ),
    ),
    #(
      "customerAccount",
      json.string(
        definition_types.read_optional_string(access, "customerAccount")
        |> option.unwrap("NONE"),
      ),
    ),
  ])
}

@internal
pub fn build_input_access(
  input: Dict(String, root_field.ResolvedValue),
) -> Dict(String, Json) {
  let access = definition_types.read_object(input, "access")
  dict.from_list([
    #(
      "admin",
      json.string(
        definition_types.read_optional_string(access, "admin")
        |> option.unwrap("PUBLIC_READ_WRITE"),
      ),
    ),
    #(
      "storefront",
      json.string(
        definition_types.read_optional_string(access, "storefront")
        |> option.unwrap("NONE"),
      ),
    ),
    #(
      "customerAccount",
      json.string(
        definition_types.read_optional_string(access, "customerAccount")
        |> option.unwrap("NONE"),
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
      definition_types.read_optional_bool(value, "enabled")
      |> option.unwrap(False)
    _ -> False
  }
}

@internal
pub fn serialize_definition_create_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let input = definition_types.read_object(args, "definition")
  let errors = definition_types.validate_definition_input(input, True)
  case errors {
    [_, ..] -> #(
      serializers.serialize_definition_mutation_payload(
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
          definition_types.read_optional_string(input, "ownerType")
            |> option.unwrap("PRODUCT"),
          definition_types.read_optional_string(input, "namespace")
            |> option.unwrap(""),
          definition_types.read_optional_string(input, "key")
            |> option.unwrap(""),
        )
      case existing {
        Some(_) -> #(
          serializers.serialize_definition_mutation_payload(
            store_in,
            "createdDefinition",
            None,
            [
              definition_types.UserError(
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
          case
            validate_and_maybe_pin_definition(
              store_in,
              definition,
              definition_types.read_optional_bool(input, "pin"),
            )
          {
            Error(user_errors) -> #(
              serializers.serialize_definition_mutation_payload(
                store_in,
                "createdDefinition",
                None,
                definition_input_user_errors(user_errors),
                field,
                variables,
              ),
              store_in,
              identity,
              [],
            )
            Ok(definition) -> {
              let next_store =
                store.upsert_staged_metafield_definitions(store_in, [
                  definition,
                ])
              #(
                serializers.serialize_definition_mutation_payload(
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
  }
}

@internal
pub fn validate_and_maybe_pin_definition(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
  pin: Option(Bool),
) -> Result(MetafieldDefinitionRecord, List(definition_types.UserError)) {
  case pin {
    Some(True) ->
      case definition_types.validate_definition_pin(store_in, definition) {
        [_, ..] as user_errors -> Error(user_errors)
        [] -> Ok(definition_types.pin_definition(store_in, definition))
      }
    _ -> Ok(definition)
  }
}

fn definition_input_user_errors(
  user_errors: List(definition_types.UserError),
) -> List(definition_types.UserError) {
  list.map(user_errors, fn(user_error) {
    definition_types.UserError(
      field: Some(["definition"]),
      message: user_error.message,
      code: user_error.code,
    )
  })
}

@internal
pub fn serialize_definition_update_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let input = definition_types.read_object(args, "definition")
  let errors = definition_types.validate_definition_input(input, False)
  case errors {
    [_, ..] -> #(
      serializers.serialize_definition_mutation_payload(
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
          definition_types.read_optional_string(input, "ownerType")
            |> option.unwrap("PRODUCT"),
          definition_types.read_optional_string(input, "namespace")
            |> option.unwrap(""),
          definition_types.read_optional_string(input, "key")
            |> option.unwrap(""),
        )
      case existing {
        None -> #(
          serializers.serialize_definition_mutation_payload(
            store_in,
            "updatedDefinition",
            None,
            [
              definition_types.UserError(
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
          let requested_type =
            definition_types.read_optional_string(input, "type")
          case requested_type {
            Some(type_name) ->
              case type_name != definition.type_.name {
                True -> #(
                  serializers.serialize_definition_mutation_payload(
                    store_in,
                    "updatedDefinition",
                    None,
                    [
                      definition_types.UserError(
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

@internal
pub fn serialize_definition_delete_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let definition = definition_types.find_definition_from_args(store_in, args)
  case definition {
    None -> #(
      serializers.serialize_definition_delete_payload(
        None,
        [
          definition_types.UserError(
            field: Some(definition_types.definition_reference_field(args)),
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
        definition_types.product_metafield_owner_ids_for_definition(
          store_in,
          record,
        )
      let store_after_metafields = case
        definition_types.read_optional_bool(
          args,
          "deleteAllAssociatedMetafields",
        )
      {
        Some(True) ->
          store.delete_product_metafields_for_definition(store_in, record)
        _ -> store_in
      }
      let next_store =
        definition_types.stage_delete_definition(store_after_metafields, record)
        |> definition_types.ensure_product_shells(associated_product_owner_ids)
      #(
        serializers.serialize_definition_delete_payload(Some(record), [], field),
        next_store,
        identity,
        [record.id],
      )
    }
  }
}

@internal
pub fn update_definition_success(
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
      name: definition_types.read_optional_string(input, "name")
        |> option.unwrap(definition.name),
      description: case definition_types.has_field(input, "description") {
        True -> definition_types.read_optional_string(input, "description")
        False -> definition.description
      },
      validations: case definition_types.has_field(input, "validations") {
        True -> read_validation_records(input)
        False -> definition.validations
      },
      access: case definition_types.has_field(input, "access") {
        True -> build_input_access(input)
        False -> definition.access
      },
      capabilities: case definition_types.has_field(input, "capabilities") {
        True ->
          build_definition_capabilities(definition_types.read_object(
            input,
            "capabilities",
          ))
        False -> definition.capabilities
      },
    )
  let next_store =
    store.upsert_staged_metafield_definitions(store_in, [updated])
  #(
    serializers.serialize_definition_mutation_payload(
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

@internal
pub fn serialize_definition_pin_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  // Pattern 2: pinning is a supported local mutation, but a cold LiveHybrid
  // request may target an upstream definition. Hydrate the definition catalog
  // first, then stage only the pin effect locally. Snapshot/no-transport mode
  // keeps the current local not-found behavior.
  let store_in =
    definition_types.maybe_hydrate_definition_for_args(store_in, args, upstream)
  case definition_types.find_definition_from_args(store_in, args) {
    None -> #(
      serializers.serialize_pinning_payload(
        store_in,
        "pinnedDefinition",
        None,
        [
          definition_types.UserError(
            field: Some(definition_types.definition_reference_field(args)),
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
    Some(definition) ->
      case definition_types.validate_definition_pin(store_in, definition) {
        [_, ..] as user_errors -> #(
          serializers.serialize_pinning_payload(
            store_in,
            "pinnedDefinition",
            None,
            user_errors,
            field,
            variables,
          ),
          store_in,
          identity,
          [],
        )
        [] -> {
          let pinned = definition_types.pin_definition(store_in, definition)
          let next_store =
            store.upsert_staged_metafield_definitions(store_in, [pinned])
          #(
            serializers.serialize_pinning_payload(
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
}

@internal
pub fn serialize_definition_unpin_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  // Pattern 2: unpinning mirrors pinning — hydrate any upstream definition
  // before applying the local stage so downstream reads observe local state.
  let store_in =
    definition_types.maybe_hydrate_definition_for_args(store_in, args, upstream)
  case definition_types.find_definition_from_args(store_in, args) {
    None -> #(
      serializers.serialize_pinning_payload(
        store_in,
        "unpinnedDefinition",
        None,
        [
          definition_types.UserError(
            field: Some(definition_types.definition_reference_field(args)),
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
      let #(unpinned, compacted) =
        definition_types.unpin_definition(store_in, definition)
      let next_store =
        store.upsert_staged_metafield_definitions(store_in, compacted)
      #(
        serializers.serialize_pinning_payload(
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

@internal
pub fn validate_definition_input(
  input: Dict(String, root_field.ResolvedValue),
  create: Bool,
) -> List(definition_types.UserError) {
  let required = case create {
    True -> ["namespace", "key", "ownerType", "name", "type"]
    False -> ["namespace", "key", "ownerType"]
  }
  let errors =
    list.filter_map(required, fn(field_name) {
      case definition_types.read_optional_string(input, field_name) {
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
pub fn blank_definition_error(
  field_name: String,
) -> definition_types.UserError {
  definition_types.UserError(
    field: Some(["definition", field_name]),
    message: field_name <> " is required.",
    code: "BLANK",
  )
}

@internal
pub fn validate_definition_namespace(
  input: Dict(String, root_field.ResolvedValue),
) -> List(definition_types.UserError) {
  case definition_types.read_optional_string(input, "namespace") {
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
) -> List(definition_types.UserError) {
  case definition_types.read_optional_string(input, "key") {
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
) -> List(definition_types.UserError) {
  case definition_types.read_optional_string(input, "name") {
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
) -> List(definition_types.UserError) {
  case definition_types.read_optional_string(input, "description") {
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
) -> List(definition_types.UserError) {
  case definition_types.read_optional_string(input, "type") {
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
) -> definition_types.UserError {
  definition_types.UserError(
    field: Some(["definition", field_name]),
    message: message,
    code: code,
  )
}

@internal
pub fn definition_reserved_namespace(namespace: String) -> Bool {
  definition_types.is_protected_write_namespace(namespace)
  || string.starts_with(namespace, "shopify")
}

@internal
pub fn valid_definition_identifier_characters(value: String) -> Bool {
  value
  |> string.to_utf_codepoints
  |> list.all(fn(codepoint) {
    let code = string.utf_codepoint_to_int(codepoint)
    definition_types.is_alpha_numeric_code(code) || code == 45 || code == 95
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
pub fn build_definition_from_input(
  _store_in: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MetafieldDefinitionRecord, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "MetafieldDefinition")
  let type_name =
    definition_types.read_optional_string(input, "type")
    |> option.unwrap("single_line_text_field")
  let owner_type =
    definition_types.read_optional_string(input, "ownerType")
    |> option.unwrap("PRODUCT")
  #(
    MetafieldDefinitionRecord(
      id: id,
      name: definition_types.read_optional_string(input, "name")
        |> option.unwrap(""),
      namespace: definition_types.read_optional_string(input, "namespace")
        |> option.unwrap(""),
      key: definition_types.read_optional_string(input, "key")
        |> option.unwrap(""),
      owner_type: owner_type,
      type_: MetafieldDefinitionTypeRecord(
        name: type_name,
        category: infer_definition_type_category(type_name),
      ),
      description: definition_types.read_optional_string(input, "description"),
      validations: read_validation_records(input),
      access: build_input_access(input),
      capabilities: build_definition_capabilities(definition_types.read_object(
        input,
        "capabilities",
      )),
      constraints: read_definition_constraints(input),
      pinned_position: None,
      validation_status: "ALL_VALID",
    ),
    next_identity,
  )
}

@internal
pub fn read_definition_constraints(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(MetafieldDefinitionConstraintsRecord) {
  case definition_types.has_field(input, "constraints") {
    False -> Some(MetafieldDefinitionConstraintsRecord(key: None, values: []))
    True -> {
      let constraints = definition_types.read_object(input, "constraints")
      Some(MetafieldDefinitionConstraintsRecord(
        key: definition_types.read_optional_string(constraints, "key"),
        values: read_string_list(constraints, "values")
          |> list.map(fn(value) {
            MetafieldDefinitionConstraintValueRecord(value: value)
          }),
      ))
    }
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
pub fn infer_definition_type_category(type_name: String) -> Option(String) {
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

@internal
pub fn read_validation_records(
  input: Dict(String, root_field.ResolvedValue),
) -> List(MetafieldDefinitionValidationRecord) {
  definition_types.read_input_objects(input, "validations")
  |> list.filter_map(fn(record) {
    case definition_types.read_optional_string(record, "name") {
      Some(name) ->
        case name {
          "" -> Error(Nil)
          _ ->
            Ok(MetafieldDefinitionValidationRecord(
              name: name,
              value: definition_types.read_optional_string(record, "value"),
            ))
        }
      None -> Error(Nil)
    }
  })
}

@internal
pub fn serialize_metafields_delete_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let inputs = definition_types.read_input_objects(args, "metafields")
  let result =
    definition_types.delete_metafields_by_identifiers(store_in, inputs)
  #(
    serializers.serialize_metafields_delete_payload(result, field),
    result.store,
    identity,
    serializers.deleted_identifiers_to_stage_ids(result.deleted_metafields),
  )
}

@internal
pub fn serialize_metafield_delete_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let input = definition_types.read_object(args, "input")
  case definition_types.read_optional_string(input, "id") {
    None -> #(
      serializers.serialize_metafield_delete_payload(
        None,
        [
          definition_types.SimpleUserError(
            ["input", "id"],
            "Metafield id is required",
          ),
        ],
        field,
      ),
      store_in,
      identity,
      [],
    )
    Some(metafield_id) -> {
      case store.find_effective_metafield_by_id(store_in, metafield_id) {
        None -> #(
          serializers.serialize_metafield_delete_payload(
            Some(metafield_id),
            [],
            field,
          ),
          store_in,
          identity,
          [metafield_id],
        )
        Some(record) -> {
          let result =
            definition_types.delete_metafields_by_identifiers(store_in, [
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
            serializers.serialize_metafield_delete_payload(
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

@internal
pub fn serialize_metafields_set_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let inputs = definition_types.read_input_objects(args, "metafields")
  let errors = definition_types.validate_metafields_set_inputs(store_in, inputs)
  case errors {
    [_, ..] -> #(
      serializers.serialize_metafields_set_payload([], errors, field),
      store_in,
      identity,
      [],
    )
    [] -> {
      let #(created, next_store, next_identity) =
        definition_types.upsert_metafields_set_inputs(
          store_in,
          identity,
          inputs,
        )
      #(
        serializers.serialize_metafields_set_payload(created, [], field),
        next_store,
        next_identity,
        list.map(created, fn(record) { record.id }),
      )
    }
  }
}
