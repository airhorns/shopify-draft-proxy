import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/string
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/metafield_definitions
import shopify_draft_proxy/proxy/metafield_definitions/mutations as metafield_definition_mutations
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity

fn graphql(proxy: draft_proxy.DraftProxy, query: String) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":" <> json.to_string(json.string(query)) <> "}",
    )
  draft_proxy.process_request(proxy, request)
}

fn graphql_with_api_client(proxy: draft_proxy.DraftProxy, query: String) {
  graphql_with_api_client_id(proxy, query, "999001")
}

fn graphql_with_api_client_id(
  proxy: draft_proxy.DraftProxy,
  query: String,
  api_client_id: String,
) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.from_list([
        #(app_identity.api_client_id_header, api_client_id),
      ]),
      body: "{\"query\":" <> json.to_string(json.string(query)) <> "}",
    )
  draft_proxy.process_request(proxy, request)
}

const path = "/admin/api/2025-01/graphql.json"

fn run_mutation(
  s: store.Store,
  identity: synthetic_identity.SyntheticIdentityRegistry,
  query: String,
) -> MutationOutcome {
  metafield_definitions.process_mutation(
    s,
    identity,
    path,
    query,
    dict.new(),
    empty_upstream_context(),
  )
}

fn run_mutation_with_api_client(
  s: store.Store,
  identity: synthetic_identity.SyntheticIdentityRegistry,
  query: String,
  api_client_id: String,
) -> MutationOutcome {
  metafield_definition_mutations.process_mutation_with_headers(
    s,
    identity,
    path,
    query,
    dict.new(),
    empty_upstream_context(),
    dict.from_list([#(app_identity.api_client_id_header, api_client_id)]),
  )
}

fn run_query(s: store.Store, query: String) -> String {
  let assert Ok(data) = metafield_definitions.process(s, query, dict.new())
  json.to_string(data)
}

fn create_definition_query(key: String) -> String {
  "mutation {
    metafieldDefinitionCreate(definition: {
      name: \"HAR 699 " <> key <> "\",
      namespace: \"har699\",
      key: \"" <> key <> "\",
      ownerType: PRODUCT,
      type: \"single_line_text_field\"
    }) {
      createdDefinition { id key pinnedPosition }
      userErrors { field message code }
    }
  }"
}

fn create_app_definition_query(key: String) -> String {
  "mutation {
    metafieldDefinitionCreate(definition: {
      name: \"App Limit " <> key <> "\",
      namespace: \"$app:resource_limit\",
      key: \"" <> key <> "\",
      ownerType: PRODUCT,
      type: \"single_line_text_field\"
    }) {
      createdDefinition { id namespace key }
      userErrors { field message code }
    }
  }"
}

fn create_definition_validation_query(
  namespace: String,
  key: String,
  name: String,
  type_name: String,
  extra: String,
) -> String {
  "mutation {
    metafieldDefinitionCreate(definition: {
      name: \"" <> name <> "\",
      namespace: \"" <> namespace <> "\",
      key: \"" <> key <> "\",
      ownerType: PRODUCT,
      type: \"" <> type_name <> "\"" <> extra <> "
    }) {
      createdDefinition { id }
      userErrors { field message code }
    }
  }"
}

fn pin_definition_query(key: String) -> String {
  "mutation {
    metafieldDefinitionPin(identifier: {
      ownerType: PRODUCT,
      namespace: \"har699\",
      key: \"" <> key <> "\"
    }) {
      pinnedDefinition { id key pinnedPosition }
      userErrors { field message code }
    }
  }"
}

fn unpin_definition_query(key: String) -> String {
  "mutation {
    metafieldDefinitionUnpin(identifier: {
      ownerType: PRODUCT,
      namespace: \"har699\",
      key: \"" <> key <> "\"
    }) {
      unpinnedDefinition { id key pinnedPosition }
      userErrors { field message code }
    }
  }"
}

fn standard_enable_query(template_id: String, extra: String) -> String {
  "mutation {
    standardMetafieldDefinitionEnable(
      ownerType: PRODUCT,
      id: \"" <> template_id <> "\"" <> extra <> "
    ) {
      createdDefinition { id namespace key ownerType pinnedPosition }
      userErrors { field message code }
    }
  }"
}

pub fn metafield_definition_create_rejects_namespace_and_key_length_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "ab",
        "x",
        "X",
        "single_line_text_field",
        "",
      ),
    )

  assert result.staged_resource_ids == []
  assert json.to_string(result.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"namespace\"],\"message\":\"Namespace is too short (minimum is 3 characters)\",\"code\":\"TOO_SHORT\"},{\"field\":[\"definition\",\"key\"],\"message\":\"Key is too short (minimum is 2 characters)\",\"code\":\"TOO_SHORT\"}]}}}"
}

pub fn metafield_definition_create_rejects_invalid_characters_test() {
  let namespace_result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "my space",
        "valid_key",
        "X",
        "single_line_text_field",
        "",
      ),
    )
  let key_result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "loyalty",
        "bad.key!",
        "X",
        "single_line_text_field",
        "",
      ),
    )

  assert namespace_result.staged_resource_ids == []
  assert json.to_string(namespace_result.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"namespace\"],\"message\":\"Namespace contains one or more invalid characters.\",\"code\":\"INVALID_CHARACTER\"}]}}}"
  assert key_result.staged_resource_ids == []
  assert json.to_string(key_result.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"key\"],\"message\":\"Key contains one or more invalid characters.\",\"code\":\"INVALID_CHARACTER\"}]}}}"
}

pub fn metafield_definition_create_rejects_unknown_type_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "loyalty",
        "tier",
        "Tier",
        "totally_made_up_type",
        "",
      ),
    )
  let body = json.to_string(result.data)

  assert result.staged_resource_ids == []
  assert string.contains(body, "\"createdDefinition\":null")
  assert string.contains(body, "\"field\":[\"definition\",\"type\"]")
  assert string.contains(body, "\"code\":\"INCLUSION\"")
  assert string.contains(body, "totally_made_up_type is not a valid type")
}

pub fn metafield_definition_create_rejects_invalid_validation_options_test() {
  let invalid_integer =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "bad_integer",
        "Bad integer",
        "number_integer",
        ", validations: [{ name: \"min\", value: \"not-a-number\" }]",
      ),
    )
  let unknown =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "unknown",
        "Unknown",
        "single_line_text_field",
        ", validations: [{ name: \"totally_unknown_option\", value: \"x\" }]",
      ),
    )
  let duplicate =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "duplicate",
        "Duplicate",
        "single_line_text_field",
        ", validations: [{ name: \"min\", value: \"5\" }, { name: \"min\", value: \"10\" }]",
      ),
    )
  let min_max =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "min_max",
        "Min max",
        "single_line_text_field",
        ", validations: [{ name: \"min\", value: \"10\" }, { name: \"max\", value: \"5\" }]",
      ),
    )

  assert invalid_integer.staged_resource_ids == []
  assert string.contains(
    json.to_string(invalid_integer.data),
    "\"message\":\"Validations value for option min must be an integer.\"",
  )
  assert string.contains(
    json.to_string(unknown.data),
    "\"createdDefinition\":null",
  )
  assert string.contains(
    json.to_string(unknown.data),
    "totally_unknown_option' isn't supported for single_line_text_field",
  )
  assert string.contains(
    json.to_string(duplicate.data),
    "\"code\":\"DUPLICATE_OPTION\"",
  )
  assert string.contains(
    json.to_string(min_max.data),
    "\"message\":\"Validations contains an invalid value: 'min' must be less than 'max'.\"",
  )
}

pub fn metafield_definition_create_rejects_required_and_typed_validations_test() {
  let metaobject_required =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "metaobject_required",
        "Metaobject required",
        "metaobject_reference",
        ", validations: []",
      ),
    )
  let rating_required =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "rating_required",
        "Rating required",
        "rating",
        "",
      ),
    )
  let choices_shape =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "choices_shape",
        "Choices shape",
        "single_line_text_field",
        ", validations: [{ name: \"choices\", value: \"{\\\"x\\\":1}\" }]",
      ),
    )
  let file_type_options =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "file_type_options",
        "File type options",
        "file_reference",
        ", validations: [{ name: \"file_type_options\", value: \"[\\\"bad\\\"]\" }]",
      ),
    )
  let dimension_shape =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "dimension_shape",
        "Dimension shape",
        "dimension",
        ", validations: [{ name: \"min\", value: \"not-json\" }]",
      ),
    )
  let regex_shape =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "regex_shape",
        "Regex shape",
        "single_line_text_field",
        ", validations: [{ name: \"regex\", value: \"[\" }]",
      ),
    )

  assert string.contains(
    json.to_string(metaobject_required.data),
    "Validations require that you select a metaobject.",
  )
  assert string.contains(
    json.to_string(rating_required.data),
    "Validations requires 'scale_max' to be provided.",
  )
  assert string.contains(
    json.to_string(rating_required.data),
    "Validations requires 'scale_min' to be provided.",
  )
  assert string.contains(
    json.to_string(choices_shape.data),
    "Validations value for option choices must be an array.",
  )
  assert string.contains(
    json.to_string(file_type_options.data),
    "Validations must be one of the following file types",
  )
  assert string.contains(
    json.to_string(dimension_shape.data),
    "must be a stringified JSON object",
  )
  assert string.contains(
    json.to_string(regex_shape.data),
    "Validations has the following regex error",
  )
}

pub fn metafield_definition_validation_failure_does_not_allocate_identity_test() {
  let rejected =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "validation_rules",
        "bad_integer",
        "Bad integer",
        "number_integer",
        ", validations: [{ name: \"min\", value: \"not-a-number\" }], pin: true",
      ),
    )
  let accepted =
    run_mutation(
      rejected.store,
      rejected.identity,
      create_definition_validation_query(
        "validation_rules",
        "accepted",
        "Accepted",
        "single_line_text_field",
        "",
      ),
    )

  assert rejected.staged_resource_ids == []
  assert string.contains(
    json.to_string(rejected.data),
    "\"createdDefinition\":null",
  )
  assert string.contains(
    json.to_string(accepted.data),
    "\"createdDefinition\":{\"id\":\"gid://shopify/MetafieldDefinition/1\"}",
  )
}

pub fn metafield_definition_update_validates_validation_options_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation {
      metafieldDefinitionCreate(definition: {
        name: \"Validation update\",
        namespace: \"validation_update\",
        key: \"guard\",
        ownerType: PRODUCT,
        type: \"single_line_text_field\"
      }) {
        createdDefinition { id validations { name value } }
        userErrors { field message code }
      }
    }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let update =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"validation_update\",
        key: \"guard\",
        ownerType: PRODUCT,
        validations: [{ name: \"min\", value: \"10\" }, { name: \"max\", value: \"5\" }]
      }) {
        updatedDefinition { id validations { name value } }
        userErrors { field message code }
        validationJob { id }
      }
    }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"updatedDefinition\":null")
  assert string.contains(
    update_json,
    "\"message\":\"Validations contains an invalid value: 'min' must be less than 'max'.\"",
  )

  let read =
    "{ metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: \"validation_update\", key: \"guard\" }) { id validations { name value } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read)
  assert read_status == 200
  assert string.contains(json.to_string(read_body), "\"validations\":[]")
}

pub fn metafield_definition_update_rejects_metaobject_definition_id_change_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation {
      metafieldDefinitionCreate(definition: {
        name: \"Metaobject link\",
        namespace: \"metaobject_link\",
        key: \"target\",
        ownerType: PRODUCT,
        type: \"metaobject_reference\",
        validations: [{ name: \"metaobject_definition_id\", value: \"gid://shopify/MetaobjectDefinition/1\" }]
      }) {
        createdDefinition { id validations { name value } }
        userErrors { field message code }
      }
    }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let update =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"metaobject_link\",
        key: \"target\",
        ownerType: PRODUCT,
        validations: [{ name: \"metaobject_definition_id\", value: \"gid://shopify/MetaobjectDefinition/2\" }]
      }) {
        updatedDefinition { id validations { name value } }
        userErrors { field message code }
        validationJob { id }
      }
    }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(proxy, update)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"updatedDefinition\":null")
  assert string.contains(
    update_json,
    "\"code\":\"METAOBJECT_DEFINITION_CHANGED\"",
  )
  assert string.contains(
    update_json,
    "Validations must not change the existing metaobject definition value",
  )
}

pub fn metafield_definition_create_rejects_ineligible_unique_values_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metafieldDefinitionCreate(definition: {
          name: \"Tier\",
          namespace: \"loyalty\",
          key: \"tier\",
          ownerType: PRODUCT,
          type: \"json\",
          capabilities: { uniqueValues: { enabled: true } }
        }) {
          createdDefinition { id }
          userErrors { field message code }
        }
      }",
    )
  let body = json.to_string(result.data)

  assert result.staged_resource_ids == []
  assert string.contains(body, "\"createdDefinition\":null")
  assert string.contains(body, "\"field\":[\"definition\"]")
  assert string.contains(body, "\"code\":\"INVALID_CAPABILITY\"")
}

pub fn metafield_definition_create_auto_enables_id_unique_values_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metafieldDefinitionCreate(definition: {
          name: \"External ID\",
          namespace: \"loyalty\",
          key: \"external_id\",
          ownerType: PRODUCT,
          type: \"id\"
        }) {
          createdDefinition {
            id
            capabilities { uniqueValues { enabled eligible } }
          }
          userErrors { field message code }
        }
      }",
    )
  let body = json.to_string(result.data)

  assert result.staged_resource_ids != []
  assert string.contains(body, "\"userErrors\":[]")
  assert string.contains(
    body,
    "\"uniqueValues\":{\"enabled\":true,\"eligible\":true}",
  )
}

pub fn metafield_definition_create_rejects_smart_collection_for_customer_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metafieldDefinitionCreate(definition: {
          name: \"Tier\",
          namespace: \"loyalty\",
          key: \"tier\",
          ownerType: CUSTOMER,
          type: \"single_line_text_field\",
          capabilities: { smartCollectionCondition: { enabled: true } }
        }) {
          createdDefinition { id }
          userErrors { field message code }
        }
      }",
    )
  let body = json.to_string(result.data)

  assert result.staged_resource_ids == []
  assert string.contains(body, "\"createdDefinition\":null")
  assert string.contains(body, "\"field\":[\"definition\"]")
  assert string.contains(body, "\"code\":\"INVALID_CAPABILITY\"")
}

pub fn metafield_definition_create_rejects_reserved_namespaces_test() {
  let shopify_standard =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "shopify_standard",
        "xx",
        "X",
        "single_line_text_field",
        "",
      ),
    )
  let protected =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "protected",
        "xx",
        "X",
        "single_line_text_field",
        "",
      ),
    )

  assert shopify_standard.staged_resource_ids == []
  assert json.to_string(shopify_standard.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"namespace\"],\"message\":\"Namespace shopify_standard is reserved.\",\"code\":\"RESERVED\"}]}}}"
  assert protected.staged_resource_ids == []
  assert json.to_string(protected.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"namespace\"],\"message\":\"Namespace protected is reserved.\",\"code\":\"RESERVED\"}]}}}"
}

fn create_admin_filterable(
  acc: #(store.Store, synthetic_identity.SyntheticIdentityRegistry, String),
  i: Int,
) {
  let #(current_store, current_identity, _) = acc
  let key = "filter_" <> int.to_string(i)
  let created = run_mutation(current_store, current_identity, "mutation {
        metafieldDefinitionCreate(definition: {
          name: \"Filter " <> int.to_string(i) <> "\",
          namespace: \"admin_filter_limit\",
          key: \"" <> key <> "\",
          ownerType: PRODUCT,
          type: \"single_line_text_field\",
          capabilities: { adminFilterable: { enabled: true } }
        }) {
          createdDefinition { id key capabilities { adminFilterable { enabled eligible status } } }
          userErrors { field message code }
        }
      }")
  #(created.store, created.identity, json.to_string(created.data))
}

pub fn metafield_definition_create_rejects_fifty_first_admin_filterable_test() {
  let #(_, _, last_json) =
    list.fold(
      int_range(from: 1, to: 51),
      #(store.new(), synthetic_identity.new(), ""),
      create_admin_filterable,
    )

  assert string.contains(last_json, "\"createdDefinition\":null")
  assert string.contains(last_json, "\"field\":[\"definition\"]")
  assert string.contains(
    last_json,
    "\"code\":\"OWNER_TYPE_LIMIT_EXCEEDED_FOR_USE_AS_ADMIN_FILTERS\"",
  )
}

pub fn metafield_definition_create_rejects_long_name_and_description_test() {
  let name =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "loyalty",
        "long_name",
        string.repeat("N", times: 256),
        "single_line_text_field",
        "",
      ),
    )
  let description =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_validation_query(
        "loyalty",
        "long_description",
        "X",
        "single_line_text_field",
        ", description: \"" <> string.repeat("D", times: 256) <> "\"",
      ),
    )

  assert name.staged_resource_ids == []
  assert json.to_string(name.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"name\"],\"message\":\"Name is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\"}]}}}"
  assert description.staged_resource_ids == []
  assert json.to_string(description.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"description\"],\"message\":\"Description is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\"}]}}}"
}

pub fn metafield_definition_update_rejects_ineligible_unique_values_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metafieldDefinitionCreate(definition: {
          name: \"Payload\",
          namespace: \"loyalty\",
          key: \"payload\",
          ownerType: PRODUCT,
          type: \"json\"
        }) {
          createdDefinition { id }
          userErrors { field message code }
        }
      }",
    )
  let updated =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metafieldDefinitionUpdate(definition: {
          namespace: \"loyalty\",
          key: \"payload\",
          ownerType: PRODUCT,
          capabilities: { uniqueValues: { enabled: true } }
        }) {
          updatedDefinition { id }
          userErrors { field message code }
        }
      }",
    )
  let body = json.to_string(updated.data)

  assert updated.staged_resource_ids == []
  assert string.contains(body, "\"updatedDefinition\":null")
  assert string.contains(body, "\"field\":[\"definition\"]")
  assert string.contains(body, "\"code\":\"INVALID_CAPABILITY\"")
}

pub fn metafield_definition_app_namespace_resolution_lifecycle_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation {
      metafieldDefinitionCreate(definition: {
        name: \"App Tier\",
        namespace: \"$app:loyalty\",
        key: \"tier\",
        ownerType: PRODUCT,
        type: \"single_line_text_field\"
      }) {
        createdDefinition { id namespace key ownerType }
        userErrors { field message code }
      }
    }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql_with_api_client(proxy, create)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"namespace\":\"app--999001--loyalty\"")
  assert string.contains(create_json, "\"userErrors\":[]")

  let #(Response(status: log_status, body: log_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: ""),
    )
  assert log_status == 200
  let log_json = json.to_string(log_body)
  assert string.contains(log_json, "$app:loyalty")
  assert !string.contains(log_json, "app--999001--loyalty")

  let read =
    "query {
      canonical: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: \"app--999001--loyalty\", key: \"tier\" }) { namespace key }
      appPrefix: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: \"$app:loyalty\", key: \"tier\" }) { namespace key }
      catalog: metafieldDefinitions(ownerType: PRODUCT, namespace: \"$app:loyalty\", first: 5) { nodes { namespace key } }
    }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql_with_api_client(proxy, read)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"canonical\":{\"namespace\":\"app--999001--loyalty\",\"key\":\"tier\"}",
  )
  assert string.contains(
    read_json,
    "\"appPrefix\":{\"namespace\":\"app--999001--loyalty\",\"key\":\"tier\"}",
  )
  assert string.contains(
    read_json,
    "\"catalog\":{\"nodes\":[{\"namespace\":\"app--999001--loyalty\",\"key\":\"tier\"}]}",
  )

  let update =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        name: \"App Tier Updated\",
        namespace: \"$app:loyalty\",
        key: \"tier\",
        ownerType: PRODUCT,
        description: \"Updated app namespace definition\"
      }) {
        updatedDefinition { namespace key name description }
        userErrors { field message code }
      }
    }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql_with_api_client(proxy, update)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"namespace\":\"app--999001--loyalty\"")
  assert string.contains(update_json, "\"name\":\"App Tier Updated\"")
  assert string.contains(update_json, "\"userErrors\":[]")

  let pin =
    "mutation {
      metafieldDefinitionPin(identifier: { ownerType: PRODUCT, namespace: \"$app:loyalty\", key: \"tier\" }) {
        pinnedDefinition { namespace key pinnedPosition }
        userErrors { field message code }
      }
    }"
  let #(Response(status: pin_status, body: pin_body, ..), proxy) =
    graphql_with_api_client(proxy, pin)
  assert pin_status == 200
  let pin_json = json.to_string(pin_body)
  assert string.contains(pin_json, "\"pinnedPosition\":1")
  assert string.contains(pin_json, "\"userErrors\":[]")

  let unpin =
    "mutation {
      metafieldDefinitionUnpin(identifier: { ownerType: PRODUCT, namespace: \"app--999001--loyalty\", key: \"tier\" }) {
        unpinnedDefinition { namespace key pinnedPosition }
        userErrors { field message code }
      }
    }"
  let #(Response(status: unpin_status, body: unpin_body, ..), proxy) =
    graphql_with_api_client(proxy, unpin)
  assert unpin_status == 200
  let unpin_json = json.to_string(unpin_body)
  assert string.contains(unpin_json, "\"pinnedPosition\":null")
  assert string.contains(unpin_json, "\"userErrors\":[]")

  let delete =
    "mutation {
      metafieldDefinitionDelete(identifier: { ownerType: PRODUCT, namespace: \"app--999001--loyalty\", key: \"tier\" }, deleteAllAssociatedMetafields: true) {
        deletedDefinition { namespace key }
        userErrors { field message code }
      }
    }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql_with_api_client(proxy, delete)
  assert delete_status == 200
  let delete_json = json.to_string(delete_body)
  assert string.contains(
    delete_json,
    "\"deletedDefinition\":{\"namespace\":\"app--999001--loyalty\",\"key\":\"tier\"}",
  )
  assert string.contains(delete_json, "\"userErrors\":[]")

  let #(Response(status: after_status, body: after_body, ..), _) =
    graphql_with_api_client(proxy, read)
  assert after_status == 200
  let after_json = json.to_string(after_body)
  assert string.contains(after_json, "\"canonical\":null")
  assert string.contains(after_json, "\"appPrefix\":null")
}

pub fn metafield_definition_cross_app_namespace_rejected_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation {
      metafieldDefinitionCreate(definition: {
        name: \"Other App\",
        namespace: \"app--999002--loyalty\",
        key: \"tier\",
        ownerType: PRODUCT,
        type: \"single_line_text_field\"
      }) {
        createdDefinition { namespace key }
        userErrors { field message code }
      }
    }"
  let #(Response(status: create_status, body: create_body, ..), _) =
    graphql_with_api_client(proxy, create)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(
    create_json,
    "\"message\":\"Access denied for metafieldDefinitionCreate field. Required access: API client to have access to the namespace and the resource type associated with the metafield definition.\\n\"",
  )
  assert string.contains(create_json, "\"code\":\"ACCESS_DENIED\"")
  assert string.contains(create_json, "\"metafieldDefinitionCreate\":null")

  let update =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        name: \"Other App\",
        namespace: \"app--999002--loyalty\",
        key: \"tier\",
        ownerType: PRODUCT
      }) {
        updatedDefinition { namespace key }
        userErrors { field message code }
      }
    }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql_with_api_client(proxy, update)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(
    update_json,
    "\"message\":\"Access denied for metafieldDefinitionUpdate field. Required access: API client to have access to the namespace and the resource type associated with the metafield definition.\\n\"",
  )
  assert string.contains(update_json, "\"code\":\"ACCESS_DENIED\"")
  assert string.contains(update_json, "\"metafieldDefinitionUpdate\":null")
}

fn create_and_pin(
  acc: #(store.Store, synthetic_identity.SyntheticIdentityRegistry, String),
  i: Int,
) {
  let #(current_store, current_identity, _) = acc
  let key = "pin_" <> int.to_string(i)
  let created =
    run_mutation(current_store, current_identity, create_definition_query(key))
  let pinned =
    run_mutation(created.store, created.identity, pin_definition_query(key))
  #(pinned.store, pinned.identity, json.to_string(pinned.data))
}

fn create_definition_only(
  acc: #(store.Store, synthetic_identity.SyntheticIdentityRegistry, String),
  i: Int,
) {
  let #(current_store, current_identity, _) = acc
  let key = "limit_" <> int.to_string(i)
  let created =
    run_mutation(current_store, current_identity, create_definition_query(key))
  #(created.store, created.identity, json.to_string(created.data))
}

fn create_app_definition_only(
  acc: #(store.Store, synthetic_identity.SyntheticIdentityRegistry, String),
  i: Int,
) {
  let #(current_store, current_identity, _) = acc
  let key = "limit_" <> int.to_string(i)
  let created =
    run_mutation_with_api_client(
      current_store,
      current_identity,
      create_app_definition_query(key),
      "999001",
    )
  #(created.store, created.identity, json.to_string(created.data))
}

fn create_proxy_definition(
  proxy: draft_proxy.DraftProxy,
  index: Int,
) -> draft_proxy.DraftProxy {
  let #(Response(status: status, body: body, ..), next_proxy) =
    graphql(proxy, create_definition_query("limit_" <> int.to_string(index)))
  assert status == 200
  assert string.contains(json.to_string(body), "\"userErrors\":[]")
  next_proxy
}

fn create_proxy_app_definition(
  proxy: draft_proxy.DraftProxy,
  index: Int,
) -> draft_proxy.DraftProxy {
  let #(Response(status: status, body: body, ..), next_proxy) =
    graphql_with_api_client_id(
      proxy,
      create_app_definition_query("limit_" <> int.to_string(index)),
      "999001",
    )
  assert status == 200
  assert string.contains(json.to_string(body), "\"userErrors\":[]")
  next_proxy
}

fn int_range(from start: Int, to stop: Int) -> List(Int) {
  case start > stop {
    True -> []
    False -> [start, ..int_range(from: start + 1, to: stop)]
  }
}

pub fn metafield_definition_pin_rejects_twenty_first_product_pin_test() {
  let #(final_store, _, last_pin_json) =
    list.fold(
      int_range(from: 1, to: 21),
      #(store.new(), synthetic_identity.new(), ""),
      create_and_pin,
    )

  assert last_pin_json
    == "{\"data\":{\"metafieldDefinitionPin\":{\"pinnedDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"Limit of 20 pinned definitions.\",\"code\":\"PINNED_LIMIT_REACHED\"}]}}}"

  let listing =
    run_query(
      final_store,
      "{ metafieldDefinitions(ownerType: PRODUCT, first: 25, namespace: \"har699\", pinnedStatus: PINNED, sortKey: PINNED_POSITION) { nodes { key pinnedPosition } } }",
    )
  assert string.contains(listing, "\"key\":\"pin_20\",\"pinnedPosition\":20")
  assert string.contains(listing, "\"key\":\"pin_1\",\"pinnedPosition\":1")
  assert !string.contains(listing, "\"key\":\"pin_21\"")
}

pub fn metafield_definition_pin_rejects_constrained_definition_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metafieldDefinitionCreate(definition: {
          name: \"HAR 699 constrained\",
          namespace: \"har699\",
          key: \"constrained\",
          ownerType: PRODUCT,
          type: \"single_line_text_field\",
          constraints: { key: \"category\", values: [\"gid://shopify/TaxonomyCategory/ap-2\"] }
        }) {
          createdDefinition { id key constraints { key } }
          userErrors { field message code }
        }
      }",
    )
  let pinned =
    run_mutation(
      created.store,
      created.identity,
      pin_definition_query("constrained"),
    )

  assert json.to_string(pinned.data)
    == "{\"data\":{\"metafieldDefinitionPin\":{\"pinnedDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"Constrained metafield definitions do not support pinning.\",\"code\":\"UNSUPPORTED_PINNING\"}]}}}"
}

pub fn metafield_definition_update_rejects_conflicting_constraint_inputs_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation {
      metafieldDefinitionCreate(definition: {
        name: \"Constraint update guard\",
        namespace: \"constraint_update\",
        key: \"guard\",
        ownerType: PRODUCT,
        type: \"single_line_text_field\"
      }) {
        createdDefinition { id }
        userErrors { field message code }
      }
    }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let constraints_and_updates =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"constraint_update\",
        key: \"guard\",
        ownerType: PRODUCT,
        constraints: [{ create: { key: \"shopify--tag\", value: \"fashion\" } }],
        constraintsUpdates: { key: \"shopify--tag\", values: [{ create: \"fashion\" }] }
      }) {
        updatedDefinition { id }
        userErrors { field message code }
      }
    }"
  let #(Response(status: cu_status, body: cu_body, ..), proxy) =
    graphql(proxy, constraints_and_updates)
  assert cu_status == 200
  let cu_json = json.to_string(cu_body)
  assert string.contains(cu_json, "\"updatedDefinition\":null")
  assert string.contains(cu_json, "\"field\":null")
  assert string.contains(cu_json, "\"code\":\"INVALID_INPUT\"")
  assert string.contains(
    cu_json,
    "Cannot use both `constraints` and `constraintsUpdates` in the same request.",
  )

  let constraints_and_set =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"constraint_update\",
        key: \"guard\",
        ownerType: PRODUCT,
        constraints: [{ create: { key: \"shopify--tag\", value: \"fashion\" } }],
        constraintsSet: { key: \"shopify--tag\", values: [\"fashion\"] }
      }) {
        updatedDefinition { id }
        userErrors { field message code }
      }
    }"
  let #(Response(status: cs_status, body: cs_body, ..), proxy) =
    graphql(proxy, constraints_and_set)
  assert cs_status == 200
  let cs_json = json.to_string(cs_body)
  assert string.contains(cs_json, "\"updatedDefinition\":null")
  assert string.contains(cs_json, "\"field\":null")
  assert string.contains(cs_json, "\"code\":\"INVALID_INPUT\"")
  assert string.contains(
    cs_json,
    "Cannot use both `constraints` and `constraintsSet` in the same request.",
  )

  let updates_and_set =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"constraint_update\",
        key: \"guard\",
        ownerType: PRODUCT,
        constraintsUpdates: { key: \"shopify--tag\", values: [{ create: \"fashion\" }] },
        constraintsSet: { key: \"shopify--tag\", values: [\"fashion\"] }
      }) {
        updatedDefinition { id }
        userErrors { field message code }
      }
    }"
  let #(Response(status: us_status, body: us_body, ..), _) =
    graphql(proxy, updates_and_set)
  assert us_status == 200
  let us_json = json.to_string(us_body)
  assert string.contains(us_json, "\"updatedDefinition\":null")
  assert string.contains(us_json, "\"field\":null")
  assert string.contains(us_json, "\"code\":\"INVALID_INPUT\"")
  assert string.contains(
    us_json,
    "Cannot use both `constraintsUpdates` and `constraintsSet` in the same request.",
  )
}

pub fn metafield_definition_update_applies_constraint_inputs_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation {
      metafieldDefinitionCreate(definition: {
        name: \"Constraint update tier\",
        namespace: \"constraint_update\",
        key: \"tier\",
        ownerType: PRODUCT,
        type: \"single_line_text_field\"
      }) {
        createdDefinition { id constraints { key values(first: 10) { nodes { value } } } }
        userErrors { field message code }
      }
    }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let replace_all =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"constraint_update\",
        key: \"tier\",
        ownerType: PRODUCT,
        constraintsSet: {
          key: \"category\",
          values: [
            \"gid://shopify/TaxonomyCategory/ap-2\",
            \"gid://shopify/TaxonomyCategory/ap-2-1\"
          ]
        }
      }) {
        updatedDefinition { constraints { key values(first: 10) { nodes { value } } } }
        userErrors { field message code }
      }
    }"
  let #(Response(status: set_status, body: set_body, ..), proxy) =
    graphql(proxy, replace_all)
  assert set_status == 200
  let set_json = json.to_string(set_body)
  assert string.contains(set_json, "\"userErrors\":[]")
  assert string.contains(set_json, "\"key\":\"category\"")
  assert string.contains(set_json, "\"value\":\"ap-2\"")
  assert string.contains(set_json, "\"value\":\"ap-2-1\"")

  let pin_constrained =
    "mutation {
      metafieldDefinitionPin(identifier: {
        ownerType: PRODUCT,
        namespace: \"constraint_update\",
        key: \"tier\"
      }) {
        pinnedDefinition { id }
        userErrors { field message code }
      }
    }"
  let #(Response(status: pin_status, body: pin_body, ..), proxy) =
    graphql(proxy, pin_constrained)
  assert pin_status == 200
  let pin_json = json.to_string(pin_body)
  assert string.contains(pin_json, "\"pinnedDefinition\":null")
  assert string.contains(pin_json, "\"code\":\"UNSUPPORTED_PINNING\"")

  let remove_and_add =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"constraint_update\",
        key: \"tier\",
        ownerType: PRODUCT,
        constraintsUpdates: {
          key: \"category\",
          values: [
            { delete: \"gid://shopify/TaxonomyCategory/ap-2\" },
            { create: \"gid://shopify/TaxonomyCategory/ap-2-10\" },
            { update: \"gid://shopify/TaxonomyCategory/ap-2-11\" }
          ]
        }
      }) {
        updatedDefinition { constraints { key values(first: 10) { nodes { value } } } }
        userErrors { field message code }
      }
    }"
  let #(Response(status: updates_status, body: updates_body, ..), proxy) =
    graphql(proxy, remove_and_add)
  assert updates_status == 200
  let updates_json = json.to_string(updates_body)
  assert string.contains(updates_json, "\"userErrors\":[]")
  assert string.contains(updates_json, "\"key\":\"category\"")
  assert !string.contains(updates_json, "\"value\":\"ap-2\"")
  assert string.contains(updates_json, "\"value\":\"ap-2-1\"")
  assert string.contains(updates_json, "\"value\":\"ap-2-10\"")
  assert string.contains(updates_json, "\"value\":\"ap-2-11\"")

  let legacy_replace =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"constraint_update\",
        key: \"tier\",
        ownerType: PRODUCT,
        constraints: [
          { delete: { key: \"category\", value: \"gid://shopify/TaxonomyCategory/ap-2-1\" } },
          { update: { key: \"category\", value: \"gid://shopify/TaxonomyCategory/ap-2-12\" } }
        ]
      }) {
        updatedDefinition { constraints { key values(first: 10) { nodes { value } } } }
        userErrors { field message code }
      }
    }"
  let #(Response(status: legacy_status, body: legacy_body, ..), _) =
    graphql(proxy, legacy_replace)
  assert legacy_status == 200
  let legacy_json = json.to_string(legacy_body)
  assert string.contains(legacy_json, "\"userErrors\":[]")
  assert string.contains(legacy_json, "\"key\":\"category\"")
  assert !string.contains(legacy_json, "\"value\":\"ap-2-1\"")
  assert string.contains(legacy_json, "\"value\":\"ap-2-10\"")
  assert string.contains(legacy_json, "\"value\":\"ap-2-11\"")
  assert string.contains(legacy_json, "\"value\":\"ap-2-12\"")
}

pub fn metafield_definition_update_constraints_updates_unconstrains_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation {
      metafieldDefinitionCreate(definition: {
        name: \"Constraint update unconstrain\",
        namespace: \"constraint_update\",
        key: \"unconstrain\",
        ownerType: PRODUCT,
        type: \"single_line_text_field\",
        constraints: { key: \"category\", values: [\"gid://shopify/TaxonomyCategory/ap-2\"] }
      }) {
        createdDefinition { id constraints { key values(first: 10) { nodes { value } } } }
        userErrors { field message code }
      }
    }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let unconstrain =
    "mutation {
      metafieldDefinitionUpdate(definition: {
        namespace: \"constraint_update\",
        key: \"unconstrain\",
        ownerType: PRODUCT,
        constraintsUpdates: { key: null, values: [] }
      }) {
        updatedDefinition { constraints { key values(first: 10) { nodes { value } } } }
        userErrors { field message code }
      }
    }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, unconstrain)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"userErrors\":[]")
  assert string.contains(update_json, "\"key\":null")
  assert string.contains(update_json, "\"nodes\":[]")

  let pin =
    "mutation {
      metafieldDefinitionPin(identifier: {
        ownerType: PRODUCT,
        namespace: \"constraint_update\",
        key: \"unconstrain\"
      }) {
        pinnedDefinition { key pinnedPosition }
        userErrors { field message code }
      }
    }"
  let #(Response(status: pin_status, body: pin_body, ..), _) =
    graphql(proxy, pin)
  assert pin_status == 200
  let pin_json = json.to_string(pin_body)
  assert string.contains(pin_json, "\"pinnedPosition\":1")
  assert string.contains(pin_json, "\"userErrors\":[]")
}

pub fn metafield_definition_unpin_compacts_pinned_positions_test() {
  let #(pinned_store, pinned_identity, _) =
    list.fold(
      int_range(from: 1, to: 3),
      #(store.new(), synthetic_identity.new(), ""),
      create_and_pin,
    )
  let unpinned =
    run_mutation(pinned_store, pinned_identity, unpin_definition_query("pin_2"))

  assert json.to_string(unpinned.data)
    == "{\"data\":{\"metafieldDefinitionUnpin\":{\"unpinnedDefinition\":{\"id\":\"gid://shopify/MetafieldDefinition/2\",\"key\":\"pin_2\",\"pinnedPosition\":null},\"userErrors\":[]}}}"

  let listing =
    run_query(
      unpinned.store,
      "{ metafieldDefinitions(ownerType: PRODUCT, first: 10, namespace: \"har699\", pinnedStatus: PINNED, sortKey: PINNED_POSITION) { nodes { key pinnedPosition } } }",
    )
  assert listing
    == "{\"data\":{\"metafieldDefinitions\":{\"nodes\":[{\"key\":\"pin_3\",\"pinnedPosition\":2},{\"key\":\"pin_1\",\"pinnedPosition\":1}]}}}"
}

pub fn metafield_definition_create_with_pin_rejects_owner_type_cap_test() {
  let #(pinned_store, pinned_identity, _) =
    list.fold(
      int_range(from: 1, to: 20),
      #(store.new(), synthetic_identity.new(), ""),
      create_and_pin,
    )

  let limit_result =
    run_mutation(
      pinned_store,
      pinned_identity,
      "mutation {
        metafieldDefinitionCreate(definition: {
          name: \"Pinned over cap\",
          namespace: \"pin_guard\",
          key: \"over_cap\",
          ownerType: PRODUCT,
          type: \"single_line_text_field\",
          pin: true
        }) {
          createdDefinition { id key pinnedPosition }
          userErrors { field message code }
        }
      }",
    )

  assert limit_result.staged_resource_ids == []
  assert json.to_string(limit_result.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Limit of 20 pinned definitions.\",\"code\":\"PINNED_LIMIT_REACHED\"}]}}}"
}

pub fn metafield_definition_create_with_pin_rejects_constraints_test() {
  let constrained_result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metafieldDefinitionCreate(definition: {
          name: \"Pinned constrained\",
          namespace: \"pin_guard\",
          key: \"constrained\",
          ownerType: PRODUCT,
          type: \"single_line_text_field\",
          constraints: { key: \"category\", values: [\"gid://shopify/TaxonomyCategory/ap-2\"] },
          pin: true
        }) {
          createdDefinition { id key pinnedPosition }
          userErrors { field message code }
        }
      }",
    )

  assert constrained_result.staged_resource_ids == []
  assert json.to_string(constrained_result.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Constrained metafield definitions do not support pinning.\",\"code\":\"UNSUPPORTED_PINNING\"}]}}}"
}

pub fn standard_metafield_definition_enable_with_pin_rejects_owner_type_cap_test() {
  let #(pinned_store, pinned_identity, _) =
    list.fold(
      int_range(from: 1, to: 20),
      #(store.new(), synthetic_identity.new(), ""),
      create_and_pin,
    )
  let result =
    run_mutation(
      pinned_store,
      pinned_identity,
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/1",
        ", pin: true",
      ),
    )

  assert result.staged_resource_ids == []
  assert json.to_string(result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"Limit of 20 pinned definitions.\",\"code\":\"PINNED_LIMIT_REACHED\"}]}}}"
}

pub fn standard_metafield_definition_enable_with_pin_rejects_constrained_template_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/10004",
        ", pin: true",
      ),
    )

  assert result.staged_resource_ids == []
  assert json.to_string(result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"Constrained metafield definitions do not support pinning.\",\"code\":\"UNSUPPORTED_PINNING\"}]}}}"
}

pub fn standard_metafield_definition_enable_rejects_ineligible_capability_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/10004",
        ", capabilities: { uniqueValues: { enabled: true } }",
      ),
    )
  let body = json.to_string(result.data)

  assert result.staged_resource_ids == []
  assert string.contains(body, "\"createdDefinition\":null")
  assert string.contains(body, "\"field\":null")
  assert string.contains(body, "\"code\":\"INVALID_CAPABILITY\"")
}

pub fn standard_metafield_definition_enable_resolves_captured_namespace_key_template_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        standardMetafieldDefinitionEnable(
          ownerType: PRODUCT,
          namespace: \"shopify\",
          key: \"color-pattern\"
        ) {
          createdDefinition { namespace key type { name } }
          userErrors { field message code }
        }
      }",
    )

  assert result.staged_resource_ids == ["gid://shopify/MetafieldDefinition/1"]
  assert json.to_string(result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":{\"namespace\":\"shopify\",\"key\":\"color-pattern\",\"type\":{\"name\":\"list.metaobject_reference\"}},\"userErrors\":[]}}}"
}

pub fn standard_metafield_definition_enable_rejects_unstructured_without_force_test() {
  let set_result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metafieldsSet(metafields: [{
          ownerId: \"gid://shopify/Product/1\",
          namespace: \"descriptors\",
          key: \"subtitle\",
          type: \"single_line_text_field\",
          value: \"Existing subtitle\"
        }]) {
          metafields { id namespace key }
          userErrors { field message code }
        }
      }",
    )
  let enable_result =
    run_mutation(
      set_result.store,
      set_result.identity,
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/1",
        ", forceEnable: false",
      ),
    )

  assert enable_result.staged_resource_ids == []
  assert json.to_string(enable_result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"Unstructured metafields already exist for this owner type, namespace, and key.\",\"code\":\"UNSTRUCTURED_ALREADY_EXISTS\"}]}}}"
}

pub fn standard_metafield_definition_enable_force_true_allows_unstructured_test() {
  let set_result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metafieldsSet(metafields: [{
          ownerId: \"gid://shopify/Product/1\",
          namespace: \"descriptors\",
          key: \"subtitle\",
          type: \"single_line_text_field\",
          value: \"Existing subtitle\"
        }]) {
          metafields { id namespace key }
          userErrors { field message code }
        }
      }",
    )
  let enable_result =
    run_mutation(
      set_result.store,
      set_result.identity,
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/1",
        ", forceEnable: true",
      ),
    )

  assert enable_result.staged_resource_ids
    == [
      "gid://shopify/MetafieldDefinition/2",
    ]
  assert string.contains(
    json.to_string(enable_result.data),
    "\"userErrors\":[]",
  )
}

pub fn standard_metafield_definition_enable_rejects_ineligible_deprecated_condition_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/2",
        ", useAsCollectionCondition: true",
      ),
    )

  assert result.staged_resource_ids == []
  assert json.to_string(result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"Definition type is not allowed for smart collection conditions.\",\"code\":\"TYPE_NOT_ALLOWED_FOR_CONDITIONS\"}]}}}"
}

pub fn standard_metafield_definition_enable_rejects_invalid_unique_values_capability_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/2",
        ", capabilities: { uniqueValues: { enabled: true } }",
      ),
    )

  assert result.staged_resource_ids == []
  assert json.to_string(result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"The capability unique_values is not valid for this definition.\",\"code\":\"INVALID_CAPABILITY\"}]}}}"
}

pub fn standard_metafield_definition_enable_rejects_admin_access_for_public_template_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/1",
        ", access: { admin: MERCHANT_READ }",
      ),
    )

  assert result.staged_resource_ids == []
  assert json.to_string(result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"access\"],\"message\":\"Admin access input is not allowed for this standard metafield definition.\",\"code\":\"ADMIN_ACCESS_INPUT_NOT_ALLOWED\"}]}}}"
}

pub fn standard_metafield_definition_enable_translates_deprecated_access_and_filter_args_test() {
  let result =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        standardMetafieldDefinitionEnable(
          ownerType: PRODUCT,
          id: \"gid://shopify/StandardMetafieldDefinitionTemplate/1\",
          useAsAdminFilter: true,
          visibleToStorefrontApi: false
        ) {
          createdDefinition {
            access { storefront }
            capabilities { adminFilterable { enabled eligible status } }
          }
          userErrors { field message code }
        }
      }",
    )

  assert result.staged_resource_ids == ["gid://shopify/MetafieldDefinition/1"]
  assert json.to_string(result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":{\"access\":{\"storefront\":\"NONE\"},\"capabilities\":{\"adminFilterable\":{\"enabled\":true,\"eligible\":true,\"status\":\"FILTERABLE\"}}},\"userErrors\":[]}}}"
}

pub fn metafield_definition_create_rejects_resource_type_limit_test() {
  let #(full_store, full_identity, _) =
    list.fold(
      int_range(from: 1, to: 256),
      #(store.new(), synthetic_identity.new(), ""),
      create_definition_only,
    )
  let result =
    run_mutation(
      full_store,
      full_identity,
      create_definition_query("limit_257"),
    )

  assert result.staged_resource_ids == []
  assert json.to_string(result.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Stores can only have 256 definitions for each store resource.\",\"code\":\"RESOURCE_TYPE_LIMIT_EXCEEDED\"}]}}}"

  let listing =
    run_query(
      full_store,
      "{ metafieldDefinitions(ownerType: PRODUCT, first: 300, namespace: \"har699\") { nodes { key } } }",
    )
  assert string.contains(listing, "\"key\":\"limit_256\"")
  assert !string.contains(listing, "\"key\":\"limit_257\"")
}

pub fn metafield_definition_create_counts_app_namespace_limit_per_api_client_test() {
  let #(full_store, full_identity, _) =
    list.fold(
      int_range(from: 1, to: 256),
      #(store.new(), synthetic_identity.new(), ""),
      create_app_definition_only,
    )
  let over_limit =
    run_mutation_with_api_client(
      full_store,
      full_identity,
      create_app_definition_query("limit_257"),
      "999001",
    )

  assert over_limit.staged_resource_ids == []
  assert json.to_string(over_limit.data)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Stores can only have 256 definitions for each store resource.\",\"code\":\"RESOURCE_TYPE_LIMIT_EXCEEDED\"}]}}}"

  let other_app =
    run_mutation_with_api_client(
      full_store,
      full_identity,
      create_app_definition_query("other_app_1"),
      "999002",
    )
  let other_app_json = json.to_string(other_app.data)
  assert string.contains(
    other_app_json,
    "\"namespace\":\"app--999002--resource_limit\"",
  )
  assert string.contains(other_app_json, "\"userErrors\":[]")
}

pub fn standard_metafield_definition_enable_ignores_custom_definition_resource_type_limit_test() {
  let #(full_store, full_identity, _) =
    list.fold(
      int_range(from: 1, to: 256),
      #(store.new(), synthetic_identity.new(), ""),
      create_definition_only,
    )
  let result =
    run_mutation(
      full_store,
      full_identity,
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/1",
        "",
      ),
    )

  assert result.staged_resource_ids == ["gid://shopify/MetafieldDefinition/257"]
  assert json.to_string(result.data)
    == "{\"data\":{\"standardMetafieldDefinitionEnable\":{\"createdDefinition\":{\"id\":\"gid://shopify/MetafieldDefinition/257\",\"namespace\":\"descriptors\",\"key\":\"subtitle\",\"ownerType\":\"PRODUCT\",\"pinnedPosition\":null},\"userErrors\":[]}}}"
}

pub fn metafield_definition_resource_type_limit_integration_test() {
  let full_proxy =
    list.fold(
      int_range(from: 1, to: 256),
      draft_proxy.new(),
      create_proxy_definition,
    )
  let #(Response(status: create_status, body: create_body, ..), full_proxy) =
    graphql(full_proxy, create_definition_query("limit_257"))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Stores can only have 256 definitions for each store resource.\",\"code\":\"RESOURCE_TYPE_LIMIT_EXCEEDED\"}]}}}"

  let #(Response(status: list_status, body: list_body, ..), full_proxy) =
    graphql(
      full_proxy,
      "{ metafieldDefinitions(ownerType: PRODUCT, first: 300, namespace: \"har699\") { nodes { key } } }",
    )
  assert list_status == 200
  let list_json = json.to_string(list_body)
  assert string.contains(list_json, "\"key\":\"limit_256\"")
  assert !string.contains(list_json, "\"key\":\"limit_257\"")

  let #(Response(status: standard_status, body: standard_body, ..), _) =
    graphql(
      full_proxy,
      standard_enable_query(
        "gid://shopify/StandardMetafieldDefinitionTemplate/1",
        "",
      ),
    )
  assert standard_status == 200
  assert string.contains(json.to_string(standard_body), "\"userErrors\":[]")

  let app_full_proxy =
    list.fold(
      int_range(from: 1, to: 256),
      draft_proxy.new(),
      create_proxy_app_definition,
    )
  let #(Response(status: app_status, body: app_body, ..), app_full_proxy) =
    graphql_with_api_client_id(
      app_full_proxy,
      create_app_definition_query("limit_257"),
      "999001",
    )
  assert app_status == 200
  assert json.to_string(app_body)
    == "{\"data\":{\"metafieldDefinitionCreate\":{\"createdDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Stores can only have 256 definitions for each store resource.\",\"code\":\"RESOURCE_TYPE_LIMIT_EXCEEDED\"}]}}}"

  let #(Response(status: other_status, body: other_body, ..), _) =
    graphql_with_api_client_id(
      app_full_proxy,
      create_app_definition_query("other_app_1"),
      "999002",
    )
  assert other_status == 200
  let other_json = json.to_string(other_body)
  assert string.contains(
    other_json,
    "\"namespace\":\"app--999002--resource_limit\"",
  )
  assert string.contains(other_json, "\"userErrors\":[]")
}

pub fn non_product_owner_definition_lifecycle_test() {
  let proxy = draft_proxy.new()
  let create_customer =
    "mutation { metafieldDefinitionCreate(definition: { name: \"Loyalty Tier\", namespace: \"loyalty\", key: \"tier\", type: \"single_line_text_field\", ownerType: CUSTOMER }) { createdDefinition { id ownerType namespace key name pinnedPosition } userErrors { field code message } } }"
  let #(Response(status: customer_status, body: customer_body, ..), proxy) =
    graphql(proxy, create_customer)
  assert customer_status == 200
  let customer_json = json.to_string(customer_body)
  assert string.contains(
    customer_json,
    "\"id\":\"gid://shopify/MetafieldDefinition/1\"",
  )
  assert string.contains(customer_json, "\"ownerType\":\"CUSTOMER\"")
  assert string.contains(customer_json, "\"userErrors\":[]")
  assert !string.contains(customer_json, "UNSUPPORTED_OWNER_TYPE")

  let create_order =
    "mutation { metafieldDefinitionCreate(definition: { name: \"Order Channel\", namespace: \"fulfillment\", key: \"channel\", type: \"single_line_text_field\", ownerType: ORDER }) { createdDefinition { id ownerType namespace key name } userErrors { field code message } } }"
  let #(Response(status: order_status, body: order_body, ..), proxy) =
    graphql(proxy, create_order)
  assert order_status == 200
  let order_json = json.to_string(order_body)
  assert string.contains(order_json, "\"ownerType\":\"ORDER\"")
  assert string.contains(order_json, "\"userErrors\":[]")
  assert !string.contains(order_json, "UNSUPPORTED_OWNER_TYPE")

  let create_company =
    "mutation { metafieldDefinitionCreate(definition: { name: \"Company Segment\", namespace: \"b2b\", key: \"segment\", type: \"single_line_text_field\", ownerType: COMPANY }) { createdDefinition { id ownerType namespace key name } userErrors { field code message } } }"
  let #(Response(status: company_status, body: company_body, ..), proxy) =
    graphql(proxy, create_company)
  assert company_status == 200
  let company_json = json.to_string(company_body)
  assert string.contains(company_json, "\"ownerType\":\"COMPANY\"")
  assert string.contains(company_json, "\"userErrors\":[]")
  assert !string.contains(company_json, "UNSUPPORTED_OWNER_TYPE")

  let update_customer =
    "mutation { metafieldDefinitionUpdate(definition: { name: \"Loyalty Tier Updated\", namespace: \"loyalty\", key: \"tier\", ownerType: CUSTOMER, description: \"Customer loyalty tier\" }) { updatedDefinition { id ownerType namespace key name description } userErrors { field code message } validationJob { id } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    graphql(proxy, update_customer)
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"ownerType\":\"CUSTOMER\"")
  assert string.contains(update_json, "\"name\":\"Loyalty Tier Updated\"")
  assert string.contains(
    update_json,
    "\"description\":\"Customer loyalty tier\"",
  )
  assert string.contains(update_json, "\"userErrors\":[]")
  assert !string.contains(update_json, "UNSUPPORTED_OWNER_TYPE")

  let set_metafields =
    "mutation { metafieldsSet(metafields: [{ ownerId: \"gid://shopify/Customer/1\", namespace: \"loyalty\", key: \"tier\", type: \"single_line_text_field\", value: \"gold\" }, { ownerId: \"gid://shopify/Order/1\", namespace: \"fulfillment\", key: \"channel\", type: \"single_line_text_field\", value: \"web\" }, { ownerId: \"gid://shopify/Company/1\", namespace: \"b2b\", key: \"segment\", type: \"single_line_text_field\", value: \"enterprise\" }]) { metafields { id ownerType namespace key type value } userErrors { field code message } } }"
  let #(Response(status: set_status, body: set_body, ..), proxy) =
    graphql(proxy, set_metafields)
  assert set_status == 200
  let set_json = json.to_string(set_body)
  assert string.contains(set_json, "\"ownerType\":\"CUSTOMER\"")
  assert string.contains(set_json, "\"ownerType\":\"ORDER\"")
  assert string.contains(set_json, "\"ownerType\":\"COMPANY\"")
  assert string.contains(set_json, "\"value\":\"gold\"")
  assert string.contains(set_json, "\"value\":\"web\"")
  assert string.contains(set_json, "\"value\":\"enterprise\"")
  assert string.contains(set_json, "\"userErrors\":[]")

  let read_customer =
    "query { byId: metafieldDefinition(id: \"gid://shopify/MetafieldDefinition/1\") { id ownerType namespace key name description metafields(first: 5) { nodes { ownerType value } } } byIdentifier: metafieldDefinition(identifier: { ownerType: CUSTOMER, namespace: \"loyalty\", key: \"tier\" }) { id ownerType namespace key name } customer(id: \"gid://shopify/Customer/1\") { id metafieldDefinitions(first: 5, namespace: \"loyalty\") { nodes { id ownerType namespace key name } } metafield(namespace: \"loyalty\", key: \"tier\") { ownerType value } } metafieldDefinitions(ownerType: ORDER, namespace: \"fulfillment\", first: 5) { nodes { id ownerType namespace key name metafields(first: 5) { nodes { ownerType value } } } } }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(proxy, read_customer)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"byId\":{\"id\":\"gid://shopify/MetafieldDefinition/1\"",
  )
  assert string.contains(
    read_json,
    "\"byIdentifier\":{\"id\":\"gid://shopify/MetafieldDefinition/1\"",
  )
  assert string.contains(
    read_json,
    "\"customer\":{\"id\":\"gid://shopify/Customer/1\"",
  )
  assert string.contains(read_json, "\"name\":\"Loyalty Tier Updated\"")
  assert string.contains(read_json, "\"ownerType\":\"ORDER\"")
  assert string.contains(read_json, "\"value\":\"gold\"")
  assert string.contains(read_json, "\"value\":\"web\"")

  let delete_customer =
    "mutation { metafieldDefinitionDelete(id: \"gid://shopify/MetafieldDefinition/1\", deleteAllAssociatedMetafields: true) { deletedDefinitionId deletedDefinition { ownerType namespace key } userErrors { field code message } } }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(proxy, delete_customer)
  assert delete_status == 200
  let delete_json = json.to_string(delete_body)
  assert string.contains(
    delete_json,
    "\"deletedDefinitionId\":\"gid://shopify/MetafieldDefinition/1\"",
  )
  assert string.contains(delete_json, "\"ownerType\":\"CUSTOMER\"")
  assert string.contains(delete_json, "\"userErrors\":[]")

  let read_after_delete =
    "query { deleted: metafieldDefinition(id: \"gid://shopify/MetafieldDefinition/1\") active: metafieldDefinitions(ownerType: CUSTOMER, namespace: \"loyalty\", first: 5) { nodes { id } } order: metafieldDefinitions(ownerType: ORDER, namespace: \"fulfillment\", first: 5) { nodes { ownerType key } } company: metafieldDefinitions(ownerType: COMPANY, namespace: \"b2b\", first: 5) { nodes { ownerType key } } }"
  let #(Response(status: after_status, body: after_body, ..), _) =
    graphql(proxy, read_after_delete)
  assert after_status == 200
  let after_json = json.to_string(after_body)
  assert string.contains(after_json, "\"deleted\":null")
  assert string.contains(after_json, "\"active\":{\"nodes\":[]}")
  assert string.contains(
    after_json,
    "\"order\":{\"nodes\":[{\"ownerType\":\"ORDER\"",
  )
  assert string.contains(
    after_json,
    "\"company\":{\"nodes\":[{\"ownerType\":\"COMPANY\"",
  )
}
