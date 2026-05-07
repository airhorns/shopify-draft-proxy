//// Mutation handling for the metafield definitions domain.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/regexp
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{get_field_response_key}
import shopify_draft_proxy/proxy/metafield_definitions/serializers
import shopify_draft_proxy/proxy/metafield_definitions/standard_templates_data
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
  type MetafieldDefinitionConstraintValueRecord,
  type MetafieldDefinitionConstraintsRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionValidationRecord,
  MetafieldDefinitionCapabilitiesRecord, MetafieldDefinitionCapabilityRecord,
  MetafieldDefinitionConstraintValueRecord, MetafieldDefinitionConstraintsRecord,
  MetafieldDefinitionRecord, MetafieldDefinitionTypeRecord,
  MetafieldDefinitionValidationRecord,
}

const metafield_definition_resource_type_limit = 256

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
  process_mutation_with_requesting_api_client_id(
    store_in,
    identity,
    document,
    variables,
    upstream,
    app_identity.read_requesting_api_client_id(upstream.headers),
  )
}

@internal
pub fn process_mutation_with_headers(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  request_headers: Dict(String, String),
) -> MutationOutcome {
  process_mutation_with_requesting_api_client_id(
    store_in,
    identity,
    document,
    variables,
    upstream,
    app_identity.read_requesting_api_client_id(request_headers),
  )
}

fn process_mutation_with_requesting_api_client_id(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
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
        requesting_api_client_id,
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
  requesting_api_client_id: Option(String),
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
              requesting_api_client_id,
            )
          let next_entries =
            list.append(entries, [#(get_field_response_key(field), payload)])
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
    [_, ..] ->
      json.object([
        #("errors", json.preprocessed_array(top_errors)),
        #("data", json.object(entries)),
      ])
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
  requesting_api_client_id: Option(String),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String), List(Json)) {
  case root_name {
    "metafieldDefinitionCreate" -> {
      let args = definition_types.read_args(field, variables)
      let input = definition_types.read_object(args, "definition")
      case
        cross_app_namespace_access_errors(
          root_name,
          input,
          requesting_api_client_id,
        )
      {
        [] ->
          no_top_level_errors(
            serialize_definition_create_root_with_requesting_api_client_id(
              store_in,
              identity,
              field,
              variables,
              requesting_api_client_id,
            ),
          )
        errors -> #(json.null(), store_in, identity, [], errors)
      }
    }
    "metafieldDefinitionUpdate" -> {
      let args = definition_types.read_args(field, variables)
      let input = definition_types.read_object(args, "definition")
      case
        cross_app_namespace_access_errors(
          root_name,
          input,
          requesting_api_client_id,
        )
      {
        [] ->
          no_top_level_errors(
            serialize_definition_update_root_with_requesting_api_client_id(
              store_in,
              identity,
              field,
              variables,
              requesting_api_client_id,
            ),
          )
        errors -> #(json.null(), store_in, identity, [], errors)
      }
    }
    "metafieldDefinitionDelete" ->
      no_top_level_errors(
        serialize_definition_delete_root_with_requesting_api_client_id(
          store_in,
          identity,
          field,
          variables,
          requesting_api_client_id,
        ),
      )
    "standardMetafieldDefinitionEnable" ->
      no_top_level_errors(
        serialize_standard_metafield_definition_enable_mutation_with_requesting_api_client_id(
          store_in,
          identity,
          field,
          variables,
          requesting_api_client_id,
        ),
      )
    "metafieldDefinitionPin" ->
      no_top_level_errors(
        serialize_definition_pin_root_with_requesting_api_client_id(
          store_in,
          identity,
          field,
          variables,
          upstream,
          requesting_api_client_id,
        ),
      )
    "metafieldDefinitionUnpin" ->
      no_top_level_errors(
        serialize_definition_unpin_root_with_requesting_api_client_id(
          store_in,
          identity,
          field,
          variables,
          upstream,
          requesting_api_client_id,
        ),
      )
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
pub fn cross_app_namespace_access_errors(
  root_name: String,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> List(Json) {
  case definition_types.read_optional_string(input, "namespace") {
    Some(namespace) ->
      case
        definition_types.namespace_belongs_to_requesting_api_client(
          namespace,
          requesting_api_client_id,
        )
      {
        True -> []
        False -> [metafield_definition_namespace_access_denied_error(root_name)]
      }
    None -> []
  }
}

@internal
pub fn metafield_definition_namespace_access_denied_error(
  root_name: String,
) -> Json {
  let required_access =
    "API client to have access to the namespace and the resource type associated with the metafield definition.\n"
  json.object([
    #(
      "message",
      json.string(
        "Access denied for "
        <> root_name
        <> " field. Required access: "
        <> required_access,
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("ACCESS_DENIED")),
        #(
          "documentation",
          json.string("https://shopify.dev/api/usage/access-scopes"),
        ),
        #("requiredAccess", json.string(required_access)),
      ]),
    ),
    #("path", json.preprocessed_array([json.string(root_name)])),
  ])
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
  standard_templates_data.templates()
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
  find_standard_template_with_requesting_api_client_id(args, None)
}

@internal
pub fn find_standard_template_with_requesting_api_client_id(
  args: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(
  Option(definition_types.StandardMetafieldDefinitionTemplate),
  List(definition_types.UserError),
) {
  let owner_type = definition_types.read_optional_string(args, "ownerType")
  let id = definition_types.read_optional_string(args, "id")
  let namespace =
    definition_types.read_optional_string(args, "namespace")
    |> option.map(fn(namespace) {
      definition_types.resolve_app_namespace(
        namespace,
        requesting_api_client_id,
      )
    })
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
  serialize_standard_metafield_definition_enable_mutation_with_requesting_api_client_id(
    store_in,
    identity,
    field,
    variables,
    None,
  )
}

@internal
pub fn serialize_standard_metafield_definition_enable_mutation_with_requesting_api_client_id(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let raw_args = definition_types.read_args(field, variables)
  let args = translate_standard_enable_deprecated_args(raw_args)
  let #(template, user_errors) =
    find_standard_template_with_requesting_api_client_id(
      args,
      requesting_api_client_id,
    )
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
      let owner_type = standard_definition_owner_type(args, template_record)
      let existing_definition =
        store.find_effective_metafield_definition(
          store_in,
          owner_type,
          template_record.namespace,
          template_record.key,
        )
      let #(definition, next_identity) =
        build_enabled_standard_definition(
          store_in,
          identity,
          args,
          template_record,
        )
      let standard_errors =
        validate_standard_enable_input(
          store_in,
          raw_args,
          args,
          template_record,
          definition,
          existing_definition,
        )
      case standard_errors {
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
        Ok(Nil) -> {
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
                store.upsert_staged_metafield_definitions(store_in, [
                  definition,
                ])
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
  }
}

@internal
pub fn translate_standard_enable_deprecated_args(
  args: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  args
  |> translate_standard_enable_deprecated_capability(
    "useAsCollectionCondition",
    "smartCollectionCondition",
  )
  |> translate_standard_enable_deprecated_capability(
    "useAsAdminFilter",
    "adminFilterable",
  )
  |> translate_standard_enable_deprecated_storefront_access
}

fn translate_standard_enable_deprecated_capability(
  args: Dict(String, root_field.ResolvedValue),
  deprecated_key: String,
  capability_key: String,
) -> Dict(String, root_field.ResolvedValue) {
  case definition_types.read_optional_bool(args, deprecated_key) {
    Some(enabled) -> {
      let capabilities = definition_types.read_object(args, "capabilities")
      case dict.has_key(capabilities, capability_key) {
        True -> args
        False ->
          dict.insert(
            args,
            "capabilities",
            root_field.ObjectVal(dict.insert(
              capabilities,
              capability_key,
              root_field.ObjectVal(
                dict.from_list([
                  #("enabled", root_field.BoolVal(enabled)),
                ]),
              ),
            )),
          )
      }
    }
    None -> args
  }
}

fn translate_standard_enable_deprecated_storefront_access(
  args: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case definition_types.read_optional_bool(args, "visibleToStorefrontApi") {
    Some(visible) -> {
      let access = definition_types.read_object(args, "access")
      case dict.has_key(access, "storefront") {
        True -> args
        False ->
          dict.insert(
            args,
            "access",
            root_field.ObjectVal(dict.insert(
              access,
              "storefront",
              root_field.StringVal(case visible {
                True -> "PUBLIC_READ"
                False -> "NONE"
              }),
            )),
          )
      }
    }
    None -> args
  }
}

@internal
pub fn validate_standard_enable_input(
  store_in: Store,
  raw_args: Dict(String, root_field.ResolvedValue),
  args: Dict(String, root_field.ResolvedValue),
  template: definition_types.StandardMetafieldDefinitionTemplate,
  definition: MetafieldDefinitionRecord,
  existing_definition: Option(MetafieldDefinitionRecord),
) -> Result(Nil, List(definition_types.UserError)) {
  let errors =
    validate_standard_admin_access(raw_args, template)
    |> list.append(validate_standard_unstructured_metafields(
      store_in,
      args,
      definition,
      existing_definition,
    ))
    |> list.append(validate_standard_capability_inputs(
      store_in,
      raw_args,
      args,
      definition,
      existing_definition,
    ))
  case errors {
    [] -> Ok(Nil)
    [_, ..] -> Error(errors)
  }
}

fn validate_standard_admin_access(
  args: Dict(String, root_field.ResolvedValue),
  template: definition_types.StandardMetafieldDefinitionTemplate,
) -> List(definition_types.UserError) {
  let access = definition_types.read_object(args, "access")
  case
    dict.has_key(access, "admin")
    && !standard_template_allows_admin_access(template)
  {
    True -> [
      definition_types.UserError(
        field: Some(["access"]),
        message: "Admin access input is not allowed for this standard metafield definition.",
        code: "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
      ),
    ]
    False -> []
  }
}

fn standard_template_allows_admin_access(
  template: definition_types.StandardMetafieldDefinitionTemplate,
) -> Bool {
  string.starts_with(template.namespace, "app--")
}

fn validate_standard_unstructured_metafields(
  store_in: Store,
  args: Dict(String, root_field.ResolvedValue),
  definition: MetafieldDefinitionRecord,
  existing_definition: Option(MetafieldDefinitionRecord),
) -> List(definition_types.UserError) {
  let force_enable =
    definition_types.read_optional_bool(args, "forceEnable")
    |> option.unwrap(False)
  case force_enable, existing_definition {
    True, _ | False, Some(_) -> []
    False, None -> {
      case
        definition_types.get_product_metafields_for_definition(
          store_in,
          definition,
        )
      {
        [] -> []
        [_, ..] -> [
          definition_types.UserError(
            field: None,
            message: "Unstructured metafields already exist for this owner type, namespace, and key.",
            code: "UNSTRUCTURED_ALREADY_EXISTS",
          ),
        ]
      }
    }
  }
}

fn validate_standard_capability_inputs(
  store_in: Store,
  raw_args: Dict(String, root_field.ResolvedValue),
  args: Dict(String, root_field.ResolvedValue),
  definition: MetafieldDefinitionRecord,
  existing_definition: Option(MetafieldDefinitionRecord),
) -> List(definition_types.UserError) {
  let capabilities = definition_types.read_object(args, "capabilities")
  let deprecated_condition_error =
    definition_types.read_optional_bool(raw_args, "useAsCollectionCondition")
    == Some(True)
    && capability_enabled(capabilities, "smartCollectionCondition")
    && !smart_collection_condition_capability_eligible(
      definition.owner_type,
      definition.type_.name,
    )
  case deprecated_condition_error {
    True -> [
      definition_types.UserError(
        field: None,
        message: "Definition type is not allowed for smart collection conditions.",
        code: "TYPE_NOT_ALLOWED_FOR_CONDITIONS",
      ),
    ]
    False -> {
      validate_capability_inputs(
        store_in,
        definition.owner_type,
        definition.type_.name,
        capabilities,
        None,
        option.map(existing_definition, fn(existing) { existing.id }),
      )
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
  let owner_type = standard_definition_owner_type(args, template)
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
      capabilities: build_definition_capabilities(
        definition_types.read_object(args, "capabilities"),
        owner_type,
        template.type_.name,
      ),
      constraints: Some(template.constraints),
      pinned_position: None,
      validation_status: "ALL_VALID",
    ),
    next_identity,
  )
}

fn standard_definition_owner_type(
  args: Dict(String, root_field.ResolvedValue),
  template: definition_types.StandardMetafieldDefinitionTemplate,
) -> String {
  definition_types.read_optional_string(args, "ownerType")
  |> option.unwrap(list.first(template.owner_types) |> result.unwrap("PRODUCT"))
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
  owner_type: String,
  type_name: String,
) -> MetafieldDefinitionCapabilitiesRecord {
  let admin_filterable = capability_enabled(capabilities, "adminFilterable")
  let smart_collection =
    capability_enabled(capabilities, "smartCollectionCondition")
  let unique_values =
    capability_enabled_or_required(
      capabilities,
      "uniqueValues",
      required_unique_values_capability(type_name),
    )
  let admin_filterable_eligible =
    admin_filterable_capability_eligible(owner_type, type_name)
  let smart_collection_eligible =
    smart_collection_condition_capability_eligible(owner_type, type_name)
  let unique_values_eligible = unique_values_capability_eligible(type_name)
  MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: MetafieldDefinitionCapabilityRecord(
      enabled: admin_filterable,
      eligible: admin_filterable_eligible,
      status: Some(case admin_filterable {
        True -> "FILTERABLE"
        False -> "NOT_FILTERABLE"
      }),
    ),
    smart_collection_condition: MetafieldDefinitionCapabilityRecord(
      enabled: smart_collection,
      eligible: smart_collection_eligible,
      status: None,
    ),
    unique_values: MetafieldDefinitionCapabilityRecord(
      enabled: unique_values,
      eligible: unique_values_eligible,
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

fn capability_enabled_or_required(
  capabilities: Dict(String, root_field.ResolvedValue),
  key: String,
  required: Bool,
) -> Bool {
  case capability_explicitly_disabled(capabilities, key) {
    True -> False
    False ->
      case capability_enabled(capabilities, key) {
        True -> True
        False -> required
      }
  }
}

fn capability_explicitly_disabled(
  capabilities: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(capabilities, key) {
    Ok(root_field.ObjectVal(value)) ->
      definition_types.read_optional_bool(value, "enabled") == Some(False)
    _ -> False
  }
}

fn required_unique_values_capability(type_name: String) -> Bool {
  type_name == "id"
}

fn unique_values_capability_eligible(type_name: String) -> Bool {
  list.contains(
    ["id", "number_integer", "single_line_text_field", "url"],
    type_name,
  )
}

fn smart_collection_condition_capability_eligible(
  owner_type: String,
  type_name: String,
) -> Bool {
  owner_type == "PRODUCT" && type_name == "single_line_text_field"
}

fn admin_filterable_capability_eligible(
  owner_type: String,
  type_name: String,
) -> Bool {
  let supported_types = [
    "boolean", "date", "date_time", "dimension", "id", "money", "number_decimal",
    "number_integer", "rating", "single_line_text_field", "volume", "weight",
  ]
  list.contains(
    ["PRODUCT", "PRODUCTVARIANT", "CUSTOMER", "ORDER", "COMPANY"],
    owner_type,
  )
  && list.contains(supported_types, type_name)
}

fn validate_capability_inputs(
  store_in: Store,
  owner_type: String,
  type_name: String,
  capabilities: Dict(String, root_field.ResolvedValue),
  field: Option(List(String)),
  exclude_definition_id: Option(String),
) -> List(definition_types.UserError) {
  let enabled_capabilities = [
    #(
      "adminFilterable",
      capability_enabled(capabilities, "adminFilterable"),
      admin_filterable_capability_eligible(owner_type, type_name),
    ),
    #(
      "smartCollectionCondition",
      capability_enabled(capabilities, "smartCollectionCondition"),
      smart_collection_condition_capability_eligible(owner_type, type_name),
    ),
    #(
      "uniqueValues",
      capability_enabled(capabilities, "uniqueValues"),
      unique_values_capability_eligible(type_name),
    ),
  ]
  let eligibility_errors =
    enabled_capabilities
    |> list.filter_map(fn(entry) {
      let #(key, enabled, eligible) = entry
      case enabled && !eligible {
        True -> Ok(capability_not_eligible_error(field, key))
        False -> Error(Nil)
      }
    })

  let limit_errors = case eligibility_errors {
    [_, ..] -> []
    [] ->
      case capability_enabled(capabilities, "adminFilterable") {
        True ->
          case
            admin_filterable_owner_type_limit_reached(
              store_in,
              owner_type,
              exclude_definition_id,
            )
          {
            True -> [
              definition_types.UserError(
                field: field,
                message: admin_filterable_owner_type_limit_message(owner_type),
                code: "OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS",
              ),
            ]
            False -> []
          }
        False -> []
      }
  }
  list.append(eligibility_errors, limit_errors)
}

fn capability_not_eligible_error(
  field: Option(List(String)),
  capability_key: String,
) -> definition_types.UserError {
  definition_types.UserError(
    field: field,
    message: "The capability "
      <> capability_error_name(capability_key)
      <> " is not valid for this definition.",
    code: "INVALID_CAPABILITY",
  )
}

fn capability_error_name(capability_key: String) -> String {
  case capability_key {
    "adminFilterable" -> "admin_filterable"
    "smartCollectionCondition" -> "smart_collection_condition"
    "uniqueValues" -> "unique_values"
    _ -> capability_key
  }
}

fn admin_filterable_owner_type_limit_message(owner_type: String) -> String {
  case owner_type {
    "PRODUCT" ->
      "You can only use 50 product metafield definitions to filter the product list. To add a new filter, disable filtering on an existing one."
    _ ->
      "You can only use 50 metafield definitions for this owner type as admin filters. To add a new filter, disable filtering on an existing one."
  }
}

fn admin_filterable_owner_type_limit_reached(
  store_in: Store,
  owner_type: String,
  exclude_definition_id: Option(String),
) -> Bool {
  let count =
    store.list_effective_metafield_definitions(store_in)
    |> list.filter(fn(definition) {
      definition.owner_type == owner_type
      && definition.capabilities.admin_filterable.enabled
      && case exclude_definition_id {
        Some(id) -> definition.id != id
        None -> True
      }
    })
    |> list.length
  count >= 50
}

@internal
pub fn serialize_definition_create_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  serialize_definition_create_root_with_requesting_api_client_id(
    store_in,
    identity,
    field,
    variables,
    None,
  )
}

@internal
pub fn serialize_definition_create_root_with_requesting_api_client_id(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let input =
    definition_types.read_object(args, "definition")
    |> definition_types.resolve_namespace_input(requesting_api_client_id)
  let owner_type =
    definition_types.read_optional_string(input, "ownerType")
    |> option.unwrap("PRODUCT")
  let type_name =
    definition_types.read_optional_string(input, "type")
    |> option.unwrap("single_line_text_field")
  let input_errors = definition_types.validate_definition_input(input, True)
  let validation_errors = case input_errors {
    [] -> validate_definition_validation_records(input, type_name)
    [_, ..] -> []
  }
  let capability_errors = case list.append(input_errors, validation_errors) {
    [] ->
      validate_capability_inputs(
        store_in,
        owner_type,
        type_name,
        definition_types.read_object(input, "capabilities"),
        Some(["definition"]),
        None,
      )
    [_, ..] -> []
  }
  let errors =
    input_errors
    |> list.append(validation_errors)
    |> list.append(capability_errors)
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
          owner_type,
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
          case validate_definition_resource_type_limit(store_in, definition) {
            [_, ..] as user_errors -> #(
              serializers.serialize_definition_mutation_payload(
                store_in,
                "createdDefinition",
                None,
                user_errors,
                field,
                variables,
              ),
              store_in,
              identity,
              [],
            )
            [] ->
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
}

fn validate_definition_resource_type_limit(
  store_in: Store,
  definition: MetafieldDefinitionRecord,
) -> List(definition_types.UserError) {
  let count =
    store.list_effective_metafield_definitions(store_in)
    |> list.filter(fn(existing) {
      existing.owner_type == definition.owner_type
      && !definition_is_standard_template(existing)
      && definition_limit_scope_matches(existing, definition)
    })
    |> list.length
  case count >= metafield_definition_resource_type_limit {
    True -> [
      definition_types.UserError(
        field: Some(["definition"]),
        message: "Stores can only have "
          <> int.to_string(metafield_definition_resource_type_limit)
          <> " definitions for each store resource.",
        code: "RESOURCE_TYPE_LIMIT_EXCEEDED",
      ),
    ]
    False -> []
  }
}

fn definition_limit_scope_matches(
  existing: MetafieldDefinitionRecord,
  candidate: MetafieldDefinitionRecord,
) -> Bool {
  case
    definition_types.namespace_api_client_id(existing.namespace),
    definition_types.namespace_api_client_id(candidate.namespace)
  {
    Some(existing_api_client_id), Some(candidate_api_client_id) ->
      existing_api_client_id == candidate_api_client_id
    None, None -> True
    _, _ -> False
  }
}

fn definition_is_standard_template(
  definition: MetafieldDefinitionRecord,
) -> Bool {
  standard_templates()
  |> list.any(fn(template) {
    list.contains(template.owner_types, definition.owner_type)
    && template.namespace == definition.namespace
    && template.key == definition.key
    && template.type_.name == definition.type_.name
  })
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
  serialize_definition_update_root_with_requesting_api_client_id(
    store_in,
    identity,
    field,
    variables,
    None,
  )
}

@internal
pub fn serialize_definition_update_root_with_requesting_api_client_id(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let input =
    definition_types.read_object(args, "definition")
    |> definition_types.resolve_namespace_input(requesting_api_client_id)
  let errors =
    list.append(
      definition_types.validate_definition_input(input, False),
      constraint_update_input_conflict_errors(input),
    )
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
                  update_definition_success_after_validation(
                    store_in,
                    identity,
                    field,
                    variables,
                    input,
                    definition,
                  )
              }
            _ -> {
              update_definition_success_after_validation(
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

fn update_definition_success_after_validation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  input: Dict(String, root_field.ResolvedValue),
  definition: MetafieldDefinitionRecord,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let validation_errors = case
    definition_types.has_field(input, "validations")
  {
    True ->
      validate_definition_validation_records(input, definition.type_.name)
      |> list.append(validate_metaobject_definition_id_immutability(
        input,
        definition,
      ))
    False -> []
  }
  case validation_errors {
    [_, ..] -> #(
      serializers.serialize_definition_mutation_payload(
        store_in,
        "updatedDefinition",
        None,
        validation_errors,
        field,
        variables,
      ),
      store_in,
      identity,
      [],
    )
    [] ->
      update_definition_success_after_capability_validation(
        store_in,
        identity,
        field,
        variables,
        input,
        definition,
      )
  }
}

fn update_definition_success_after_capability_validation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  input: Dict(String, root_field.ResolvedValue),
  definition: MetafieldDefinitionRecord,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let capability_errors = case
    definition_types.has_field(input, "capabilities")
  {
    True ->
      validate_capability_inputs(
        store_in,
        definition.owner_type,
        definition.type_.name,
        definition_types.read_object(input, "capabilities"),
        Some(["definition"]),
        Some(definition.id),
      )
    False -> []
  }
  case capability_errors {
    [_, ..] -> #(
      serializers.serialize_definition_mutation_payload(
        store_in,
        "updatedDefinition",
        None,
        capability_errors,
        field,
        variables,
      ),
      store_in,
      identity,
      [],
    )
    [] ->
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

fn validations_user_error(
  message: String,
  code: String,
) -> definition_types.UserError {
  definition_types.UserError(
    field: Some(["definition", "validations"]),
    message: message,
    code: code,
  )
}

fn validate_definition_validation_records(
  input: Dict(String, root_field.ResolvedValue),
  type_name: String,
) -> List(definition_types.UserError) {
  case read_validation_input_objects(input) {
    Error(errors) -> errors
    Ok(records) -> {
      case duplicate_validation_name(records) {
        Some(_) -> [
          validations_user_error(
            "Validations cannot contain duplicate \"name\" options.",
            "DUPLICATE_OPTION",
          ),
        ]
        None ->
          records
          |> list.index_map(fn(record, index) {
            validate_validation_record(index, record, type_name)
          })
          |> list.flatten
          |> list.append(validate_required_validation_options(
            records,
            type_name,
          ))
          |> list.append(validate_min_max_validation_options(records, type_name))
      }
    }
  }
}

fn read_validation_input_objects(
  input: Dict(String, root_field.ResolvedValue),
) -> Result(
  List(Dict(String, root_field.ResolvedValue)),
  List(definition_types.UserError),
) {
  case dict.get(input, "validations") {
    Error(_) -> Ok([])
    Ok(root_field.ListVal(values)) -> {
      let errors =
        list.filter_map(values, fn(value) {
          case value {
            root_field.ObjectVal(record) ->
              case valid_validation_record_shape(record) {
                True -> Error(Nil)
                False ->
                  Ok(validations_user_error(
                    "Validations must be an array of objects with exactly name and value fields.",
                    "INVALID",
                  ))
              }
            _ ->
              Ok(validations_user_error(
                "Validations must be an array of objects with exactly name and value fields.",
                "INVALID",
              ))
          }
        })
      case errors {
        [_, ..] -> Error(errors)
        [] ->
          Ok(
            list.filter_map(values, fn(value) {
              case value {
                root_field.ObjectVal(record) -> Ok(record)
                _ -> Error(Nil)
              }
            }),
          )
      }
    }
    _ ->
      Error([
        validations_user_error(
          "Validations must be an array of objects with exactly name and value fields.",
          "INVALID",
        ),
      ])
  }
}

fn valid_validation_record_shape(
  record: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.size(record) == 2
  && definition_types.has_field(record, "name")
  && definition_types.has_field(record, "value")
}

fn duplicate_validation_name(
  records: List(Dict(String, root_field.ResolvedValue)),
) -> Option(String) {
  duplicate_validation_name_loop(records, [])
}

fn duplicate_validation_name_loop(
  records: List(Dict(String, root_field.ResolvedValue)),
  seen: List(String),
) -> Option(String) {
  case records {
    [] -> None
    [record, ..rest] -> {
      let name = definition_types.read_optional_string(record, "name")
      case name {
        Some(name) ->
          case list.contains(seen, name) {
            True -> Some(name)
            False -> duplicate_validation_name_loop(rest, [name, ..seen])
          }
        None -> duplicate_validation_name_loop(rest, seen)
      }
    }
  }
}

fn validate_validation_record(
  index: Int,
  record: Dict(String, root_field.ResolvedValue),
  type_name: String,
) -> List(definition_types.UserError) {
  case definition_types.read_optional_string(record, "name") {
    None -> [
      validations_user_error(
        "Validations must be an array of objects with exactly name and value fields.",
        "INVALID",
      ),
    ]
    Some(name) -> {
      case string.trim(name) {
        "" -> [
          validations_user_error("Validations name is required.", "INVALID"),
        ]
        _ ->
          case list.contains(allowed_validation_option_names(type_name), name) {
            False -> [
              validations_user_error(
                "Validations value for option "
                  <> name
                  <> " contains an invalid value: '"
                  <> name
                  <> "' isn't supported for "
                  <> type_name
                  <> ".",
                "INVALID_OPTION",
              ),
            ]
            True -> validate_validation_value(index, record, type_name, name)
          }
      }
    }
  }
}

fn allowed_validation_option_names(type_name: String) -> List(String) {
  let base = scalar_definition_type_name(type_name)
  case base {
    "single_line_text_field" | "multi_line_text_field" -> [
      "min", "max", "regex", "choices",
    ]
    "number_integer" -> ["min", "max"]
    "number_decimal" -> ["min", "max"]
    "rating" -> ["scale_min", "scale_max"]
    "metaobject_reference" -> ["metaobject_definition_id"]
    "file_reference" -> ["file_type_options"]
    "json" | "rich_text_field" -> ["schema"]
    "dimension" | "volume" | "weight" | "money" -> ["min", "max"]
    _ -> []
  }
}

fn scalar_definition_type_name(type_name: String) -> String {
  case string.starts_with(type_name, "list.") {
    True -> string.drop_start(type_name, 5)
    False -> type_name
  }
}

fn validate_validation_value(
  _index: Int,
  record: Dict(String, root_field.ResolvedValue),
  type_name: String,
  name: String,
) -> List(definition_types.UserError) {
  case definition_types.read_optional_string(record, "value") {
    None -> [
      validations_user_error(
        "Validations value for option " <> name <> " is required.",
        "INVALID_OPTION",
      ),
    ]
    Some(value) -> validate_validation_string_value(type_name, name, value)
  }
}

fn validate_validation_string_value(
  type_name: String,
  name: String,
  value: String,
) -> List(definition_types.UserError) {
  let base = scalar_definition_type_name(type_name)
  case name {
    "min" | "max" -> validate_min_max_value_for_type(base, name, value)
    "scale_min" | "scale_max" -> validate_decimal_validation_option(name, value)
    "regex" -> validate_regex_validation_option(value)
    "choices" -> validate_choices_validation_option(value)
    "file_type_options" -> validate_file_type_options_validation_option(value)
    "schema" -> validate_json_validation_option(name, value)
    "metaobject_definition_id" ->
      validate_metaobject_definition_id_option(value)
    _ -> []
  }
}

fn validate_min_max_value_for_type(
  type_name: String,
  name: String,
  value: String,
) -> List(definition_types.UserError) {
  case type_name {
    "number_integer" ->
      case int.parse(value) {
        Ok(_) -> []
        Error(_) -> [
          validations_user_error(
            "Validations value for option " <> name <> " must be an integer.",
            "INVALID_OPTION",
          ),
        ]
      }
    "number_decimal" -> validate_decimal_validation_option(name, value)
    "single_line_text_field" | "multi_line_text_field" ->
      validate_positive_int_validation_option(name, value)
    "dimension" | "volume" | "weight" ->
      case definition_types.valid_value_unit_json_object(value) {
        True -> []
        False -> [
          validations_user_error(
            "Validations value for option "
              <> name
              <> " must be a stringified JSON object with a value (numeric) and unit (string from one the supported measurement units) fields.",
            "INVALID_OPTION",
          ),
        ]
      }
    "money" ->
      case definition_types.valid_money_json_object(value) {
        True -> []
        False -> [
          validations_user_error(
            "Validations value for option "
              <> name
              <> " must be a stringified JSON object with a value (numeric) and unit (string from one the supported measurement units) fields.",
            "INVALID_OPTION",
          ),
        ]
      }
    _ -> []
  }
}

fn validate_positive_int_validation_option(
  name: String,
  value: String,
) -> List(definition_types.UserError) {
  case int.parse(value) {
    Ok(parsed) ->
      case parsed < 0 {
        True -> [
          validations_user_error(
            "Validations contains an invalid value: '"
              <> name
              <> "' must be positive.",
            "INVALID_OPTION",
          ),
        ]
        False -> []
      }
    Error(_) -> [
      validations_user_error(
        "Validations value for option " <> name <> " must be an integer.",
        "INVALID_OPTION",
      ),
    ]
  }
}

fn validate_decimal_validation_option(
  name: String,
  value: String,
) -> List(definition_types.UserError) {
  case parse_validation_float(value) {
    Some(_) -> []
    None -> [
      validations_user_error(
        "Validations value for option " <> name <> " must be a number.",
        "INVALID_OPTION",
      ),
    ]
  }
}

fn validate_regex_validation_option(
  value: String,
) -> List(definition_types.UserError) {
  case regexp.from_string(value) {
    Ok(_) -> []
    Error(_) -> [
      validations_user_error(
        "Validations has the following regex error: invalid regular expression.",
        "INVALID_OPTION",
      ),
    ]
  }
}

fn validate_choices_validation_option(
  value: String,
) -> List(definition_types.UserError) {
  case json.parse(value, commit.json_value_decoder()) {
    Ok(commit.JsonArray(items)) ->
      case list.length(items) > 128 {
        True -> [
          validations_user_error(
            "Validations choices cannot contain more than 128 values.",
            "INVALID_OPTION",
          ),
        ]
        False -> {
          case
            list.all(items, fn(item) {
              case item {
                commit.JsonString(_) -> True
                _ -> False
              }
            })
          {
            True -> []
            False -> [
              validations_user_error(
                "Validations value for option choices must be an array of strings.",
                "INVALID_OPTION",
              ),
            ]
          }
        }
      }
    Ok(_) -> [
      validations_user_error(
        "Validations value for option choices must be an array.",
        "INVALID_OPTION",
      ),
    ]
    Error(_) -> [
      validations_user_error(
        "Validations value for option choices is invalid JSON.",
        "INVALID_OPTION",
      ),
    ]
  }
}

fn validate_file_type_options_validation_option(
  value: String,
) -> List(definition_types.UserError) {
  case json.parse(value, commit.json_value_decoder()) {
    Ok(commit.JsonArray(items)) -> {
      let allowed = [
        "Image", "GenericFile", "Video", "Model3dEnvironmentImage", "Model3d",
      ]
      case
        list.all(items, fn(item) {
          case item {
            commit.JsonString(file_type) -> list.contains(allowed, file_type)
            _ -> False
          }
        })
      {
        True -> []
        False -> [
          validations_user_error(
            "Validations must be one of the following file types: Image, GenericFile, Video, Model3dEnvironmentImage, Model3d.",
            "INVALID_OPTION",
          ),
        ]
      }
    }
    Ok(_) -> [
      validations_user_error(
        "Validations value for option file_type_options must be an array.",
        "INVALID_OPTION",
      ),
    ]
    Error(_) -> [
      validations_user_error(
        "Validations value for option file_type_options is invalid JSON.",
        "INVALID_OPTION",
      ),
    ]
  }
}

fn validate_json_validation_option(
  name: String,
  value: String,
) -> List(definition_types.UserError) {
  case json.parse(value, commit.json_value_decoder()) {
    Ok(_) -> []
    Error(_) -> [
      validations_user_error(
        "Validations value for option " <> name <> " is invalid JSON.",
        "INVALID_OPTION",
      ),
    ]
  }
}

fn validate_metaobject_definition_id_option(
  value: String,
) -> List(definition_types.UserError) {
  case string.starts_with(value, "gid://shopify/MetaobjectDefinition/") {
    True -> []
    False -> [
      validations_user_error(
        "Validations must be a valid metaobject definition belonging to your shop.",
        "INVALID_OPTION",
      ),
    ]
  }
}

fn validate_required_validation_options(
  records: List(Dict(String, root_field.ResolvedValue)),
  type_name: String,
) -> List(definition_types.UserError) {
  let names = validation_option_names(records)
  let base = scalar_definition_type_name(type_name)
  case base {
    "metaobject_reference" ->
      case list.contains(names, "metaobject_definition_id") {
        True -> []
        False -> [
          validations_user_error(
            "Validations require that you select a metaobject.",
            "INVALID_OPTION",
          ),
        ]
      }
    "rating" -> {
      let max_error = case list.contains(names, "scale_max") {
        True -> []
        False -> [
          validations_user_error(
            "Validations requires 'scale_max' to be provided.",
            "INVALID_OPTION",
          ),
        ]
      }
      let min_error = case list.contains(names, "scale_min") {
        True -> []
        False -> [
          validations_user_error(
            "Validations requires 'scale_min' to be provided.",
            "INVALID_OPTION",
          ),
        ]
      }
      list.append(max_error, min_error)
    }
    _ -> []
  }
}

fn validation_option_names(
  records: List(Dict(String, root_field.ResolvedValue)),
) -> List(String) {
  list.filter_map(records, fn(record) {
    case definition_types.read_optional_string(record, "name") {
      Some(name) -> Ok(name)
      None -> Error(Nil)
    }
  })
}

fn validate_min_max_validation_options(
  records: List(Dict(String, root_field.ResolvedValue)),
  type_name: String,
) -> List(definition_types.UserError) {
  let base = scalar_definition_type_name(type_name)
  case min_max_values(records, base) {
    Some(#(min, max)) ->
      case min >. max {
        True -> [
          validations_user_error(
            "Validations contains an invalid value: 'min' must be less than 'max'.",
            "INVALID_OPTION",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn min_max_values(
  records: List(Dict(String, root_field.ResolvedValue)),
  type_name: String,
) -> Option(#(Float, Float)) {
  case
    validation_record_value(records, "min"),
    validation_record_value(records, "max")
  {
    Some(min_raw), Some(max_raw) -> {
      case
        parse_min_max_float(type_name, min_raw),
        parse_min_max_float(type_name, max_raw)
      {
        Some(min), Some(max) -> Some(#(min, max))
        _, _ -> None
      }
    }
    _, _ -> None
  }
}

fn validation_record_value(
  records: List(Dict(String, root_field.ResolvedValue)),
  name: String,
) -> Option(String) {
  case records {
    [] -> None
    [record, ..rest] -> {
      case definition_types.read_optional_string(record, "name") {
        Some(record_name) if record_name == name ->
          definition_types.read_optional_string(record, "value")
        _ -> validation_record_value(rest, name)
      }
    }
  }
}

fn parse_min_max_float(type_name: String, value: String) -> Option(Float) {
  case type_name {
    "dimension" | "volume" | "weight" | "money" ->
      parse_json_measurement_float(value)
    _ -> parse_validation_float(value)
  }
}

fn parse_json_measurement_float(value: String) -> Option(Float) {
  case json.parse(value, commit.json_value_decoder()) {
    Ok(commit.JsonObject(fields)) ->
      case definition_types.json_number_string_field(fields, "value") {
        Some(number) -> parse_validation_float(number)
        None ->
          case definition_types.json_number_string_field(fields, "amount") {
            Some(number) -> parse_validation_float(number)
            None -> None
          }
      }
    _ -> None
  }
}

fn parse_validation_float(value: String) -> Option(Float) {
  case float.parse(value) {
    Ok(value) -> Some(value)
    Error(_) ->
      case int.parse(value) {
        Ok(value) -> Some(int.to_float(value))
        Error(_) -> None
      }
  }
}

fn validate_metaobject_definition_id_immutability(
  input: Dict(String, root_field.ResolvedValue),
  definition: MetafieldDefinitionRecord,
) -> List(definition_types.UserError) {
  let base = scalar_definition_type_name(definition.type_.name)
  case base {
    "metaobject_reference" -> {
      let current =
        definition.validations
        |> list.find(fn(validation) {
          validation.name == "metaobject_definition_id"
        })
        |> option.from_result
        |> option.map(fn(validation) { validation.value })
        |> option.flatten
      let requested =
        read_validation_records(input)
        |> list.find(fn(validation) {
          validation.name == "metaobject_definition_id"
        })
        |> option.from_result
        |> option.map(fn(validation) { validation.value })
        |> option.flatten
      case current, requested {
        Some(current), Some(requested) if current != requested -> [
          definition_types.UserError(
            field: Some(["definition", "validations"]),
            message: "Validations must not change the existing metaobject definition value",
            code: "METAOBJECT_DEFINITION_CHANGED",
          ),
        ]
        _, _ -> []
      }
    }
    _ -> []
  }
}

@internal
pub fn serialize_definition_delete_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  serialize_definition_delete_root_with_requesting_api_client_id(
    store_in,
    identity,
    field,
    variables,
    None,
  )
}

@internal
pub fn serialize_definition_delete_root_with_requesting_api_client_id(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  let definition =
    definition_types.find_definition_from_args_with_requesting_api_client_id(
      store_in,
      args,
      requesting_api_client_id,
    )
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
          build_definition_capabilities(
            definition_types.read_object(input, "capabilities"),
            definition.owner_type,
            definition.type_.name,
          )
        False -> definition.capabilities
      },
      constraints: update_definition_constraints(input, definition.constraints),
    )
  case definition_types.read_optional_bool(input, "pin") {
    Some(True) -> {
      case definition_types.validate_definition_pin(store_in, updated) {
        [_, ..] as user_errors -> #(
          serializers.serialize_definition_mutation_payload(
            store_in,
            "updatedDefinition",
            None,
            definition_input_user_errors(user_errors),
            field,
            variables,
          ),
          store_in,
          identity,
          [],
        )
        [] -> {
          let pinned = definition_types.pin_definition(store_in, updated)
          let next_store =
            store.upsert_staged_metafield_definitions(store_in, [pinned])
          #(
            serializers.serialize_definition_mutation_payload(
              store_in,
              "updatedDefinition",
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
    Some(False) -> {
      let #(unpinned, compacted) =
        definition_types.unpin_definition(store_in, updated)
      let next_store =
        store.upsert_staged_metafield_definitions(store_in, compacted)
      #(
        serializers.serialize_definition_mutation_payload(
          store_in,
          "updatedDefinition",
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
    _ -> {
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
  }
}

@internal
pub fn constraint_update_input_conflict_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(definition_types.UserError) {
  let has_constraints = definition_types.has_field(input, "constraints")
  let has_constraints_updates =
    definition_types.has_field(input, "constraintsUpdates")
  let has_constraints_set = definition_types.has_field(input, "constraintsSet")

  case has_constraints, has_constraints_updates, has_constraints_set {
    True, True, _ -> [
      invalid_constraint_update_input_error(
        "Cannot use both `constraints` and `constraintsUpdates` in the same request.",
      ),
    ]
    True, _, True -> [
      invalid_constraint_update_input_error(
        "Cannot use both `constraints` and `constraintsSet` in the same request.",
      ),
    ]
    _, True, True -> [
      invalid_constraint_update_input_error(
        "Cannot use both `constraintsUpdates` and `constraintsSet` in the same request.",
      ),
    ]
    _, _, _ -> []
  }
}

fn invalid_constraint_update_input_error(
  message: String,
) -> definition_types.UserError {
  definition_types.UserError(
    field: None,
    message: message,
    code: "INVALID_INPUT",
  )
}

@internal
pub fn update_definition_constraints(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(MetafieldDefinitionConstraintsRecord),
) -> Option(MetafieldDefinitionConstraintsRecord) {
  case definition_types.has_field(input, "constraintsSet") {
    True -> Some(read_constraints_set(input))
    False ->
      case definition_types.has_field(input, "constraintsUpdates") {
        True ->
          Some(apply_constraints_updates(
            normalize_constraints(existing),
            definition_types.read_object(input, "constraintsUpdates"),
          ))
        False ->
          case definition_types.has_field(input, "constraints") {
            True ->
              Some(apply_legacy_constraints(
                normalize_constraints(existing),
                definition_types.read_input_objects(input, "constraints"),
              ))
            False -> existing
          }
      }
  }
}

fn normalize_constraints(
  constraints: Option(MetafieldDefinitionConstraintsRecord),
) -> MetafieldDefinitionConstraintsRecord {
  case constraints {
    Some(record) -> record
    None -> empty_definition_constraints()
  }
}

fn read_constraints_set(
  input: Dict(String, root_field.ResolvedValue),
) -> MetafieldDefinitionConstraintsRecord {
  let constraints_set = definition_types.read_object(input, "constraintsSet")
  MetafieldDefinitionConstraintsRecord(
    key: definition_types.read_optional_string(constraints_set, "key"),
    values: read_string_list(constraints_set, "values")
      |> string_values_to_constraint_values,
  )
}

fn apply_constraints_updates(
  existing: MetafieldDefinitionConstraintsRecord,
  updates: Dict(String, root_field.ResolvedValue),
) -> MetafieldDefinitionConstraintsRecord {
  let values = definition_types.read_input_objects(updates, "values")
  case definition_types.has_field(updates, "key"), values {
    True, [] ->
      case definition_types.read_optional_string(updates, "key") {
        None -> empty_definition_constraints()
        Some(key) ->
          MetafieldDefinitionConstraintsRecord(key: Some(key), values: [])
      }
    _, _ -> {
      let starting_key = case
        definition_types.read_optional_string(updates, "key")
      {
        Some(key) -> Some(key)
        None ->
          case definition_types.has_field(updates, "key") {
            True -> None
            False -> existing.key
          }
      }
      let starting =
        MetafieldDefinitionConstraintsRecord(..existing, key: starting_key)
      list.fold(values, starting, apply_constraint_update_value)
    }
  }
}

fn apply_constraint_update_value(
  constraints: MetafieldDefinitionConstraintsRecord,
  update: Dict(String, root_field.ResolvedValue),
) -> MetafieldDefinitionConstraintsRecord {
  case read_constraint_operation(update, "delete", constraints.key) {
    Some(#(key, value)) -> delete_constraint_value(constraints, key, value)
    None ->
      case read_constraint_operation(update, "update", constraints.key) {
        Some(#(key, value)) -> upsert_constraint_value(constraints, key, value)
        None ->
          case read_constraint_operation(update, "create", constraints.key) {
            Some(#(key, value)) ->
              upsert_constraint_value(constraints, key, value)
            None -> constraints
          }
      }
  }
}

fn apply_legacy_constraints(
  existing: MetafieldDefinitionConstraintsRecord,
  operations: List(Dict(String, root_field.ResolvedValue)),
) -> MetafieldDefinitionConstraintsRecord {
  list.fold(operations, existing, apply_legacy_constraint_operation)
}

fn apply_legacy_constraint_operation(
  constraints: MetafieldDefinitionConstraintsRecord,
  operation: Dict(String, root_field.ResolvedValue),
) -> MetafieldDefinitionConstraintsRecord {
  case read_constraint_operation(operation, "delete", constraints.key) {
    Some(#(key, value)) -> delete_constraint_value(constraints, key, value)
    None ->
      case read_constraint_operation(operation, "update", constraints.key) {
        Some(#(key, value)) -> upsert_constraint_value(constraints, key, value)
        None ->
          case read_constraint_operation(operation, "create", constraints.key) {
            Some(#(key, value)) ->
              upsert_constraint_value(constraints, key, value)
            None -> constraints
          }
      }
  }
}

fn read_constraint_operation(
  operation: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback_key: Option(String),
) -> Option(#(Option(String), String)) {
  case dict.get(operation, name) {
    Ok(root_field.StringVal(value)) ->
      Some(#(fallback_key, normalize_constraint_value(value)))
    Ok(root_field.ObjectVal(input)) ->
      case definition_types.read_optional_string(input, "value") {
        Some(value) ->
          Some(#(
            first_some(
              definition_types.read_optional_string(input, "key"),
              fallback_key,
            ),
            normalize_constraint_value(value),
          ))
        None -> None
      }
    _ -> None
  }
}

fn first_some(first: Option(a), second: Option(a)) -> Option(a) {
  case first {
    Some(_) -> first
    None -> second
  }
}

fn upsert_constraint_value(
  constraints: MetafieldDefinitionConstraintsRecord,
  key: Option(String),
  value: String,
) -> MetafieldDefinitionConstraintsRecord {
  let next_key = first_some(key, constraints.key)
  let existing_values = constraint_value_strings(constraints.values)
  case list.contains(existing_values, value) {
    True -> MetafieldDefinitionConstraintsRecord(..constraints, key: next_key)
    False ->
      MetafieldDefinitionConstraintsRecord(
        key: next_key,
        values: list.append(constraints.values, [
          MetafieldDefinitionConstraintValueRecord(value: value),
        ]),
      )
  }
}

fn delete_constraint_value(
  constraints: MetafieldDefinitionConstraintsRecord,
  key: Option(String),
  value: String,
) -> MetafieldDefinitionConstraintsRecord {
  MetafieldDefinitionConstraintsRecord(
    key: first_some(key, constraints.key),
    values: list.filter(constraints.values, fn(record) { record.value != value }),
  )
}

fn string_values_to_constraint_values(
  values: List(String),
) -> List(MetafieldDefinitionConstraintValueRecord) {
  values
  |> list.map(normalize_constraint_value)
  |> dedupe_strings
  |> list.map(fn(value) {
    MetafieldDefinitionConstraintValueRecord(value: value)
  })
}

fn normalize_constraint_value(value: String) -> String {
  let taxonomy_category_prefix = "gid://shopify/TaxonomyCategory/"
  case string.starts_with(value, taxonomy_category_prefix) {
    True ->
      value
      |> string.drop_start(string.length(taxonomy_category_prefix))
    False -> value
  }
}

fn constraint_value_strings(
  values: List(MetafieldDefinitionConstraintValueRecord),
) -> List(String) {
  list.map(values, fn(record) { record.value })
}

fn dedupe_strings(values: List(String)) -> List(String) {
  list.fold(values, [], fn(seen, value) {
    case list.contains(seen, value) {
      True -> seen
      False -> list.append(seen, [value])
    }
  })
}

@internal
pub fn serialize_definition_pin_root(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  serialize_definition_pin_root_with_requesting_api_client_id(
    store_in,
    identity,
    field,
    variables,
    upstream,
    None,
  )
}

@internal
pub fn serialize_definition_pin_root_with_requesting_api_client_id(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  // Pattern 2: pinning is a supported local mutation, but a cold LiveHybrid
  // request may target an upstream definition. Hydrate the definition catalog
  // first, then stage only the pin effect locally. Snapshot/no-transport mode
  // keeps the current local not-found behavior.
  let store_in =
    definition_types.maybe_hydrate_definition_for_args_with_requesting_api_client_id(
      store_in,
      args,
      upstream,
      requesting_api_client_id,
    )
  case
    definition_types.find_definition_from_args_with_requesting_api_client_id(
      store_in,
      args,
      requesting_api_client_id,
    )
  {
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
  serialize_definition_unpin_root_with_requesting_api_client_id(
    store_in,
    identity,
    field,
    variables,
    upstream,
    None,
  )
}

@internal
pub fn serialize_definition_unpin_root_with_requesting_api_client_id(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> #(Json, Store, SyntheticIdentityRegistry, List(String)) {
  let args = definition_types.read_args(field, variables)
  // Pattern 2: unpinning mirrors pinning — hydrate any upstream definition
  // before applying the local stage so downstream reads observe local state.
  let store_in =
    definition_types.maybe_hydrate_definition_for_args_with_requesting_api_client_id(
      store_in,
      args,
      upstream,
      requesting_api_client_id,
    )
  case
    definition_types.find_definition_from_args_with_requesting_api_client_id(
      store_in,
      args,
      requesting_api_client_id,
    )
  {
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
      capabilities: build_definition_capabilities(
        definition_types.read_object(input, "capabilities"),
        owner_type,
        type_name,
      ),
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
    "id" -> Some("ID")
    "json" -> Some("JSON")
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
