import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/metaobject_definitions
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/proxy_state
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/shopify/upstream_client
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types as state_types

const path = "/admin/api/2026-04/graphql.json"

const test_api_client_id = "999001"

fn run_query(s: store.Store, query: String) -> String {
  let assert Ok(data) = metaobject_definitions.process(s, query, dict.new())
  json.to_string(data)
}

fn run_query_with_api_client_id(s: store.Store, query: String) -> String {
  let assert Ok(data) =
    metaobject_definitions.process_with_requesting_api_client_id(
      s,
      query,
      dict.new(),
      Some(test_api_client_id),
    )
  json.to_string(data)
}

fn run_mutation(
  s: store.Store,
  identity: synthetic_identity.SyntheticIdentityRegistry,
  query: String,
) -> mutation_helpers.MutationOutcome {
  let outcome =
    metaobject_definitions.process_mutation(
      s,
      identity,
      path,
      query,
      dict.new(),
      upstream_query.empty_upstream_context(),
    )
  outcome
}

fn run_mutation_with_upstream(
  s: store.Store,
  identity: synthetic_identity.SyntheticIdentityRegistry,
  query: String,
  upstream: upstream_query.UpstreamContext,
) -> mutation_helpers.MutationOutcome {
  metaobject_definitions.process_mutation(
    s,
    identity,
    path,
    query,
    dict.new(),
    upstream,
  )
}

fn run_mutation_with_api_client_id(
  s: store.Store,
  identity: synthetic_identity.SyntheticIdentityRegistry,
  query: String,
) -> mutation_helpers.MutationOutcome {
  let outcome =
    metaobject_definitions.process_mutation_with_headers(
      s,
      identity,
      path,
      query,
      dict.new(),
      upstream_query.empty_upstream_context(),
      dict.from_list([
        #(app_identity.api_client_id_header, test_api_client_id),
      ]),
    )
  outcome
}

fn metaobject_create_query(type_: String, handle: String) -> String {
  "mutation {
    metaobjectCreate(metaobject: {
      type: \"" <> type_ <> "\",
      handle: \"" <> handle <> "\",
      fields: [{ key: \"title\", value: \"" <> handle <> "\" }]
    }) {
      metaobject { id }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn definition_hydrate_response(type_: String, count: Int) -> String {
  "{\"data\":{\"metaobjectDefinitionByType\":{\"id\":\"gid://shopify/MetaobjectDefinition/remote-cached-count\",\"type\":\""
  <> type_
  <> "\",\"name\":\"Cached Count\",\"description\":null,\"displayNameKey\":\"title\",\"access\":{\"admin\":\"PUBLIC_READ_WRITE\",\"storefront\":\"NONE\"},\"capabilities\":{\"publishable\":{\"enabled\":false},\"translatable\":{\"enabled\":false},\"renderable\":{\"enabled\":false},\"onlineStore\":{\"enabled\":false}},\"fieldDefinitions\":[{\"key\":\"title\",\"name\":\"Title\",\"description\":null,\"required\":true,\"type\":{\"name\":\"single_line_text_field\",\"category\":\"TEXT\"},\"capabilities\":{\"adminFilterable\":{\"enabled\":false}},\"validations\":[]}],\"hasThumbnailField\":false,\"metaobjectsCount\":"
  <> int.to_string(count)
  <> ",\"standardTemplate\":null,\"createdAt\":\"2024-01-01T00:00:00.000Z\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"}}}"
}

fn definition_hydrate_transport(
  type_: String,
  count: Int,
) -> upstream_client.SyncTransport {
  upstream_client.SyncTransport(send: fn(req) {
    assert string.contains(
      req.body,
      "\"operationName\":\"MetaobjectDefinitionHydrateByType\"",
    )
    assert string.contains(req.body, "\"type\":\"" <> type_ <> "\"")
    Ok(
      commit.HttpOutcome(
        status: 200,
        body: definition_hydrate_response(type_, count),
        headers: [],
      ),
    )
  })
}

fn graphql_request(query: String) -> proxy_state.Request {
  proxy_state.Request(
    method: "POST",
    path: path,
    headers: dict.new(),
    body: "{\"query\":" <> json.to_string(json.string(query)) <> "}",
  )
}

fn meta_state_request() -> proxy_state.Request {
  proxy_state.Request(
    method: "GET",
    path: "/__meta/state",
    headers: dict.new(),
    body: "",
  )
}

fn run_graphql_proxy(
  proxy: draft_proxy.DraftProxy,
  query: String,
) -> #(String, draft_proxy.DraftProxy) {
  let #(proxy_state.Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  assert status == 200
  #(json.to_string(body), proxy)
}

fn create_definition_query(type_: String) -> String {
  "mutation {
    metaobjectDefinitionCreate(definition: {
      type: \"" <> type_ <> "\",
      name: \"Codex Rows\",
      displayNameKey: \"title\",
      capabilities: { publishable: { enabled: true } },
      fieldDefinitions: [
        { key: \"title\", name: \"Title\", type: \"single_line_text_field\", required: true },
        { key: \"body\", name: \"Body\", type: \"multi_line_text_field\", required: false }
      ]
    }) {
      metaobjectDefinition {
        id
        type
        displayNameKey
        capabilities { publishable { enabled } translatable { enabled } }
        fieldDefinitions { key name required type { name category } }
        metaobjectsCount
      }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn create_definition_with_capabilities_query(type_: String) -> String {
  "mutation {
    metaobjectDefinitionCreate(definition: {
      type: \"" <> type_ <> "\",
      name: \"Codex Capability Rows\",
      displayNameKey: \"title\",
      capabilities: {
        publishable: { enabled: true },
        translatable: { enabled: true },
        renderable: { enabled: true },
        onlineStore: { enabled: true }
      },
      fieldDefinitions: [
        { key: \"title\", name: \"Title\", type: \"single_line_text_field\", required: true },
        { key: \"count\", name: \"Count\", type: \"number_integer\", required: false }
      ]
    }) {
      metaobjectDefinition {
        id
        type
        name
        capabilities {
          publishable { enabled }
          translatable { enabled }
          renderable { enabled }
          onlineStore { enabled }
        }
        fieldDefinitions { key type { name category } }
      }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn create_renderable_definition_query(type_: String) -> String {
  "mutation {
    metaobjectDefinitionCreate(definition: {
      type: \"" <> type_ <> "\",
      name: \"Codex Redirect Rows\",
      displayNameKey: \"title\",
      capabilities: {
        publishable: { enabled: true },
        renderable: { enabled: true, data: { metaTitleKey: \"title\" } },
        onlineStore: { enabled: true, data: { urlHandle: \"title\" } }
      },
      fieldDefinitions: [
        { key: \"title\", name: \"Title\", type: \"single_line_text_field\", required: true }
      ]
    }) {
      metaobjectDefinition { id type capabilities { onlineStore { enabled } renderable { enabled } } }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn standard_enable_query(type_: String) -> String {
  "mutation {
    standardMetaobjectDefinitionEnable(type: \"" <> type_ <> "\") {
      metaobjectDefinition {
        id
        type
        name
        description
        displayNameKey
        access { admin storefront }
        capabilities {
          publishable { enabled }
          translatable { enabled }
          renderable { enabled }
          onlineStore { enabled }
        }
        fieldDefinitions {
          key
          name
          description
          required
          type { name category }
          validations { name value }
        }
        hasThumbnailField
        metaobjectsCount
        standardTemplate { type name }
      }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn create_definition_with_field_key_query(
  type_: String,
  key: String,
) -> String {
  "mutation {
    metaobjectDefinitionCreate(definition: {
      type: \"" <> type_ <> "\",
      name: \"Codex Rows\",
      displayNameKey: \"" <> key <> "\",
      fieldDefinitions: [
        { key: \"" <> key <> "\", name: \"Title\", type: \"single_line_text_field\", required: true }
      ]
    }) {
      metaobjectDefinition { id type displayNameKey fieldDefinitions { key } }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn field_definition_input(key: String) -> String {
  "{ key: \""
  <> key
  <> "\", name: \""
  <> key
  <> "\", type: \"single_line_text_field\", required: false }"
}

fn admin_filterable_field_definition_input(key: String) -> String {
  "{ key: \""
  <> key
  <> "\", name: \""
  <> key
  <> "\", type: \"single_line_text_field\", required: false, capabilities: { adminFilterable: { enabled: true } } }"
}

fn create_definition_with_field_keys_query(
  type_: String,
  display_name_key: String,
  keys: List(String),
) -> String {
  "mutation {
    metaobjectDefinitionCreate(definition: {
      type: \"" <> type_ <> "\",
      name: \"Codex Rows\",
      displayNameKey: \"" <> display_name_key <> "\",
      fieldDefinitions: [
        " <> string.join(list.map(keys, field_definition_input), ",") <> "
      ]
    }) {
      metaobjectDefinition { id type displayNameKey fieldDefinitions { key } }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn create_definition_with_admin_filterable_fields_query(
  type_: String,
  field_count: Int,
) -> String {
  "mutation {
    metaobjectDefinitionCreate(definition: {
      type: \"" <> type_ <> "\",
      name: \"Admin Filterable Limit\",
      displayNameKey: \"field_1\",
      fieldDefinitions: [
        " <> string.join(
    int_range(from: 1, to: field_count)
      |> list.map(fn(index) {
        admin_filterable_field_definition_input(
          "field_" <> int.to_string(index),
        )
      }),
    ",",
  ) <> "
      ]
    }) {
      metaobjectDefinition {
        id
        fieldDefinitions { key capabilities { adminFilterable { enabled } } }
      }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn create_definition_with_scalars_query(
  type_: String,
  name: String,
  description: String,
) -> String {
  "mutation {
    metaobjectDefinitionCreate(definition: {
      type: \"" <> type_ <> "\",
      name: \"" <> name <> "\",
      description: \"" <> description <> "\",
      fieldDefinitions: [
        { key: \"title\", name: \"Title\", type: \"single_line_text_field\", required: true }
      ]
    }) {
      metaobjectDefinition { id name description }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn create_definition_with_access_query(type_: String) -> String {
  "mutation {
    metaobjectDefinitionCreate(definition: {
      type: \"" <> type_ <> "\",
      name: \"App Rows\",
      access: { admin: MERCHANT_READ_WRITE },
      fieldDefinitions: [
        { key: \"title\", name: \"Title\", type: \"single_line_text_field\", required: true }
      ]
    }) {
      metaobjectDefinition { id type access { admin storefront } fieldDefinitions { key } }
      userErrors { field message code elementKey elementIndex }
    }
  }"
}

fn assert_metaobject_handle_user_error(
  serialized: String,
  root: String,
  field: String,
  code: String,
) {
  assert string.contains(
    serialized,
    "\"" <> root <> "\":{\"metaobject\":null,\"userErrors\":[",
  )
  assert string.contains(serialized, "\"field\":" <> field)
  assert string.contains(serialized, "\"code\":\"" <> code <> "\"")
}

fn int_range(from from: Int, to to: Int) -> List(Int) {
  case from > to {
    True -> []
    False -> [from, ..int_range(from + 1, to)]
  }
}

fn text_field_definition(
  key: String,
) -> state_types.MetaobjectFieldDefinitionRecord {
  state_types.MetaobjectFieldDefinitionRecord(
    key: key,
    name: Some(key),
    description: None,
    required: Some(False),
    type_: state_types.MetaobjectDefinitionTypeRecord(
      "single_line_text_field",
      Some("TEXT"),
    ),
    capabilities: state_types.MetaobjectFieldDefinitionCapabilitiesRecord(
      admin_filterable: Some(state_types.MetaobjectDefinitionCapabilityRecord(
        False,
      )),
    ),
    validations: [],
  )
}

fn definition_record(
  id_suffix: String,
  type_: String,
  standard_template: Bool,
  metaobjects_count: Int,
) -> state_types.MetaobjectDefinitionRecord {
  state_types.MetaobjectDefinitionRecord(
    id: "gid://shopify/MetaobjectDefinition/" <> id_suffix,
    type_: type_,
    name: Some("Seed " <> id_suffix),
    description: None,
    display_name_key: Some("title"),
    online_store_url_handle: None,
    access: dict.new(),
    capabilities: state_types.MetaobjectDefinitionCapabilitiesRecord(
      publishable: Some(state_types.MetaobjectDefinitionCapabilityRecord(False)),
      translatable: Some(state_types.MetaobjectDefinitionCapabilityRecord(False)),
      renderable: Some(state_types.MetaobjectDefinitionCapabilityRecord(False)),
      online_store: Some(state_types.MetaobjectDefinitionCapabilityRecord(False)),
    ),
    field_definitions: [text_field_definition("title")],
    has_thumbnail_field: Some(False),
    metaobjects_count: Some(metaobjects_count),
    standard_template: case standard_template {
      True ->
        Some(state_types.MetaobjectStandardTemplateRecord(
          type_: Some(type_),
          name: Some("Seed standard"),
        ))
      False -> None
    },
    linked_metafields: [],
    created_at: Some("2024-01-01T00:00:00.000Z"),
    updated_at: Some("2024-01-01T00:00:00.000Z"),
  )
}

fn seed_definitions(
  store_in: store.Store,
  prefix: String,
  count: Int,
  standard_template: Bool,
) -> store.Store {
  let records =
    int_range(from: 1, to: count)
    |> list.map(fn(index) {
      definition_record(
        prefix <> int.to_string(index),
        prefix <> int.to_string(index),
        standard_template,
        0,
      )
    })
  store.upsert_base_metaobject_definitions(store_in, records)
}

pub fn is_metaobject_root_predicates_test() {
  assert metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobject",
  )
  assert metaobject_definitions.is_metaobject_definitions_query_root(
    "metaobjectDefinitionByType",
  )
  assert metaobject_definitions.is_metaobject_definitions_mutation_root(
    "metaobjectCreate",
  )
  assert metaobject_definitions.is_metaobject_definitions_mutation_root(
    "metaobjectBulkDelete",
  )
  assert !metaobject_definitions.is_metaobject_definitions_mutation_root("shop")
}

pub fn empty_reads_match_shopify_like_no_data_test() {
  let result =
    run_query(
      store.new(),
      "{ metaobject(id: \"gid://shopify/Metaobject/1\") { id } metaobjectDefinitions(first: 2) { nodes { id } pageInfo { hasNextPage startCursor endCursor } } }",
    )
  assert result
    == "{\"data\":{\"metaobject\":null,\"metaobjectDefinitions\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"startCursor\":null,\"endCursor\":null}}}}"
}

pub fn definition_create_rejects_invalid_type_values_test() {
  let too_short =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("AB"),
    )
  assert json.to_string(too_short.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"type\"],\"message\":\"Type is too short (minimum is 3 characters)\",\"code\":\"TOO_SHORT\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let invalid =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("Has Spaces!"),
    )
  assert json.to_string(invalid.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"type\"],\"message\":\"Type contains one or more invalid characters. Only alphanumeric characters, underscores, and dashes are allowed.\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let too_long_type = string.repeat("x", times: 256)
  let too_long =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query(too_long_type),
    )
  assert json.to_string(too_long.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"type\"],\"message\":\"Type is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn definition_create_rejects_invalid_name_and_description_test() {
  let blank_name =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_scalars_query("codex_blank_name", "", ""),
    )
  assert json.to_string(blank_name.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"name\"],\"message\":\"Name can't be blank\",\"code\":\"BLANK\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let too_long_name =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_scalars_query(
        "codex_long_name",
        string.repeat("n", times: 256),
        "",
      ),
    )
  assert json.to_string(too_long_name.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"name\"],\"message\":\"Name is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let too_long_description =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_scalars_query(
        "codex_long_description",
        "Codex Rows",
        string.repeat("d", times: 256),
      ),
    )
  assert json.to_string(too_long_description.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"description\"],\"message\":\"Description is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn definition_create_resolves_app_type_before_storage_test() {
  let outcome =
    run_mutation_with_api_client_id(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_access_query("$app:My_Thing"),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":{\"id\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"type\":\"app--999001--my_thing\",\"access\":{\"admin\":\"MERCHANT_READ_WRITE\",\"storefront\":\"NONE\"},\"fieldDefinitions\":[{\"key\":\"title\"}]},\"userErrors\":[]}}}"

  let read_back =
    run_query_with_api_client_id(
      outcome.store,
      "{ metaobjectDefinitionByType(type: \"$app:My_Thing\") { id type } }",
    )
  assert read_back
    == "{\"data\":{\"metaobjectDefinitionByType\":{\"id\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"type\":\"app--999001--my_thing\"}}}"
}

pub fn definition_create_rejects_admin_access_for_non_app_type_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_access_query("app--999001--manual"),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"access\",\"admin\"],\"message\":\"Admin access can only be specified on metaobject definitions that have an app-reserved type.\",\"code\":\"ADMIN_ACCESS_INPUT_NOT_ALLOWED\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn definition_create_downcases_type_before_uniqueness_test() {
  let first =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("MyType"),
    )
  assert json.to_string(first.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":{\"id\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"type\":\"mytype\",\"displayNameKey\":\"title\",\"capabilities\":{\"publishable\":{\"enabled\":true},\"translatable\":{\"enabled\":false}},\"fieldDefinitions\":[{\"key\":\"title\",\"name\":\"Title\",\"required\":true,\"type\":{\"name\":\"single_line_text_field\",\"category\":\"TEXT\"}},{\"key\":\"body\",\"name\":\"Body\",\"required\":false,\"type\":{\"name\":\"multi_line_text_field\",\"category\":\"TEXT\"}}],\"metaobjectsCount\":0},\"userErrors\":[]}}}"

  let duplicate =
    run_mutation(first.store, first.identity, create_definition_query("mytype"))
  assert json.to_string(duplicate.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"type\"],\"message\":\"Type has already been taken\",\"code\":\"TAKEN\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn standard_definition_enable_uses_captured_catalog_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query("shopify--color-pattern"),
    )
  let serialized = json.to_string(outcome.data)

  assert string.contains(serialized, "\"type\":\"shopify--color-pattern\"")
  assert string.contains(serialized, "\"name\":\"Color\"")
  assert string.contains(
    serialized,
    "\"standardTemplate\":{\"type\":\"shopify--color-pattern\",\"name\":\"Color\"}",
  )
  assert string.contains(serialized, "\"metaobjectsCount\":0")
  assert string.contains(serialized, "\"key\":\"color_taxonomy_reference\"")
  assert string.contains(
    serialized,
    "\"name\":\"product_taxonomy_attribute_handle\",\"value\":\"color\"",
  )
  assert string.contains(serialized, "\"hasThumbnailField\":true")
  assert string.contains(serialized, "\"userErrors\":[]")
}

pub fn standard_definition_enable_unknown_returns_record_not_found_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query("made-up-template"),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"standardMetaobjectDefinitionEnable\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"type\"],\"message\":\"Record not found\",\"code\":\"RECORD_NOT_FOUND\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn standard_definition_enable_existing_type_is_idempotent_test() {
  let first =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query("shopify--material"),
    )
  let second =
    run_mutation(
      first.store,
      first.identity,
      standard_enable_query("shopify--material"),
    )
  let serialized = json.to_string(second.data)

  assert string.contains(serialized, "\"type\":\"shopify--material\"")
  assert string.contains(serialized, "\"name\":\"Material\"")
  assert string.contains(serialized, "\"userErrors\":[]")
  assert first.identity == second.identity
}

pub fn definition_update_rejects_standard_template_definition_test() {
  let enabled =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      standard_enable_query("shopify--qa-pair"),
    )

  let update =
    run_mutation(
      enabled.store,
      enabled.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            name: \"Renamed\",
            fieldDefinitions: [{ delete: { key: \"answer\" } }]
          }
        ) {
          metaobjectDefinition { id name standardTemplate { type name } fieldDefinitions { key } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Standard metaobject definitions can't be updated\",\"code\":\"IMMUTABLE\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let read_after =
    run_query(
      update.store,
      "{ metaobjectDefinition(id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\") { name standardTemplate { type name } fieldDefinitions { key } } }",
    )
  assert read_after
    == "{\"data\":{\"metaobjectDefinition\":{\"name\":\"Question and Answer Pairs\",\"standardTemplate\":{\"type\":\"shopify--qa-pair\",\"name\":\"Question and Answer Pairs\"},\"fieldDefinitions\":[{\"key\":\"question\"},{\"key\":\"answer\"},{\"key\":\"sources\"}]}}}"
}

pub fn definition_update_rejects_standard_type_even_without_template_metadata_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("shopify--qa-pair"),
    )

  let update =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            name: \"Renamed\",
            description: \"Changed\",
            fieldDefinitions: [{ delete: { key: \"body\" } }]
          }
        ) {
          metaobjectDefinition { id name description fieldDefinitions { key } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Standard metaobject definitions can't be updated\",\"code\":\"IMMUTABLE\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let read_after =
    run_query(
      update.store,
      "{ metaobjectDefinition(id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\") { name description fieldDefinitions { key } } }",
    )
  assert read_after
    == "{\"data\":{\"metaobjectDefinition\":{\"name\":\"Codex Rows\",\"description\":null,\"fieldDefinitions\":[{\"key\":\"title\"},{\"key\":\"body\"}]}}}"
}

pub fn definition_update_rejects_shopify_reserved_namespace_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("shopify--not-a-standard-template"),
    )

  let update =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            name: \"Renamed\",
            fieldDefinitions: [{ delete: { key: \"body\" } }]
          }
        ) {
          metaobjectDefinition { id name fieldDefinitions { key } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\"],\"message\":\"Standard metaobject definitions can't be updated\",\"code\":\"IMMUTABLE\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let read_after =
    run_query(
      update.store,
      "{ metaobjectDefinitionByType(type: \"shopify--not-a-standard-template\") { name fieldDefinitions { key } } }",
    )
  assert read_after
    == "{\"data\":{\"metaobjectDefinitionByType\":{\"name\":\"Codex Rows\",\"fieldDefinitions\":[{\"key\":\"title\"},{\"key\":\"body\"}]}}}"
}

pub fn definition_create_rejects_invalid_field_key_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query(
        "codex_rows_invalid_key",
        "bad key",
      ),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\"],\"message\":\"Key contains one or more invalid characters.\",\"code\":\"INVALID\",\"elementKey\":\"bad key\",\"elementIndex\":null}]}}}"
}

pub fn definition_create_rejects_short_field_keys_test() {
  let single =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query("codex_rows_short_key", "a"),
    )
  assert json.to_string(single.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\"],\"message\":\"Key is too short (minimum is 2 characters)\",\"code\":\"TOO_SHORT\",\"elementKey\":\"a\",\"elementIndex\":null}]}}}"

  let empty =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query("codex_rows_empty_key", ""),
    )
  assert json.to_string(empty.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\"],\"message\":\"Key can't be blank\",\"code\":\"BLANK\",\"elementKey\":\"\",\"elementIndex\":null},{\"field\":[\"definition\",\"fieldDefinitions\",\"0\"],\"message\":\"Key is too short (minimum is 2 characters)\",\"code\":\"TOO_SHORT\",\"elementKey\":\"\",\"elementIndex\":null},{\"field\":[\"definition\",\"fieldDefinitions\",\"0\"],\"message\":\"Key contains one or more invalid characters.\",\"code\":\"INVALID\",\"elementKey\":\"\",\"elementIndex\":null}]}}}"

  let boundary =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query("codex_rows_min_key", "ab"),
    )
  assert json.to_string(boundary.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":{\"id\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"type\":\"codex_rows_min_key\",\"displayNameKey\":\"ab\",\"fieldDefinitions\":[{\"key\":\"ab\"}]},\"userErrors\":[]}}}"
}

pub fn definition_create_validates_field_definition_input_test() {
  let reserved =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_keys_query(
        "codex_rows_reserved_key",
        "handle",
        ["handle"],
      ),
    )
  assert json.to_string(reserved.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\"],\"message\":\"The name \\\"handle\\\" is reserved for system use\",\"code\":\"RESERVED_NAME\",\"elementKey\":\"handle\",\"elementIndex\":null}]}}}"

  let duplicate =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_keys_query(
        "codex_rows_duplicate_key",
        "title",
        ["title", "title"],
      ),
    )
  assert json.to_string(duplicate.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"1\"],\"message\":\"Field \\\"title\\\" duplicates other inputs\",\"code\":\"DUPLICATE_FIELD_INPUT\",\"elementKey\":\"title\",\"elementIndex\":null}]}}}"

  let missing_display_name_key =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_keys_query(
        "codex_rows_missing_display",
        "missing",
        ["title"],
      ),
    )
  assert json.to_string(missing_display_name_key.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"displayNameKey\"],\"message\":\"Field definition \\\"missing\\\" does not exist\",\"code\":\"UNDEFINED_OBJECT_FIELD\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let hyphen_key =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_keys_query(
        "codex_rows_hyphen_key",
        "field-key",
        ["field-key"],
      ),
    )
  assert json.to_string(hyphen_key.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":{\"id\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"type\":\"codex_rows_hyphen_key\",\"displayNameKey\":\"field-key\",\"fieldDefinitions\":[{\"key\":\"field-key\"}]},\"userErrors\":[]}}}"

  let too_many =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_keys_query(
        "codex_rows_too_many",
        "field_1",
        int_range(from: 1, to: 41)
          |> list.map(fn(index) { "field_" <> int.to_string(index) }),
      ),
    )
  assert json.to_string(too_many.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\"],\"message\":\"Maximum 40 fields per metaobject definition\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn definition_create_rejects_more_than_forty_admin_filterable_fields_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_admin_filterable_fields_query(
        "codex_admin_filterable_limit",
        41,
      ),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\"],\"message\":\"Maximum 40 admin-filterable fields per metaobject definition\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []

  let read_after =
    run_query(
      outcome.store,
      "{ metaobjectDefinitionByType(type: \"codex_admin_filterable_limit\") { id } }",
    )
  assert read_after == "{\"data\":{\"metaobjectDefinitionByType\":null}}"
}

pub fn definition_create_rejects_default_merchant_definition_cap_test() {
  let seeded = seed_definitions(store.new(), "merchant_seed_", 128, False)
  let outcome =
    run_mutation(
      seeded,
      synthetic_identity.new(),
      create_definition_query("merchant_seed_next"),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"Maximum metaobject definitions has been exceeded\",\"code\":\"MAX_DEFINITIONS_EXCEEDED\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []

  let read_after =
    run_query(
      outcome.store,
      "{ metaobjectDefinitionByType(type: \"merchant_seed_next\") { id } }",
    )
  assert read_after == "{\"data\":{\"metaobjectDefinitionByType\":null}}"
}

pub fn definition_create_rejects_app_owned_definition_cap_per_api_client_test() {
  let seeded = seed_definitions(store.new(), "app--999001--seed_", 128, False)
  let outcome =
    run_mutation_with_api_client_id(
      seeded,
      synthetic_identity.new(),
      create_definition_with_access_query("$app:LimitNext"),
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":null,\"message\":\"Maximum metaobject definitions has been exceeded\",\"code\":\"MAX_DEFINITIONS_EXCEEDED\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []

  let read_after =
    run_query_with_api_client_id(
      outcome.store,
      "{ metaobjectDefinitionByType(type: \"$app:LimitNext\") { id } }",
    )
  assert read_after == "{\"data\":{\"metaobjectDefinitionByType\":null}}"
}

pub fn definition_create_excludes_standard_templates_from_definition_cap_test() {
  let standard_seeded =
    seed_definitions(store.new(), "shopify--standard-seed-", 128, True)
  let seeded =
    seed_definitions(standard_seeded, "merchant_cap_seed_", 127, False)
  let outcome =
    run_mutation(
      seeded,
      synthetic_identity.new(),
      create_definition_query("merchant_cap_allowed"),
    )

  assert string.contains(
    json.to_string(outcome.data),
    "\"type\":\"merchant_cap_allowed\"",
  )
  assert string.contains(json.to_string(outcome.data), "\"userErrors\":[]")
  assert outcome.staged_resource_ids
    == ["gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic"]
}

pub fn definition_update_validates_type_and_field_key_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query(
        "codex_rows_update_validation",
        "title",
      ),
    )

  let invalid_type_update =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { type: \"ab\" }
        ) {
          metaobjectDefinition { id type }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(invalid_type_update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"type\"],\"message\":\"Type is too short (minimum is 3 characters)\",\"code\":\"TOO_SHORT\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let too_long_type_update =
    run_mutation(created.store, created.identity, "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { type: \"" <> string.repeat("t", times: 256) <> "\" }
        ) {
          metaobjectDefinition { id type }
          userErrors { field message code elementKey elementIndex }
        }
      }")
  assert json.to_string(too_long_type_update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"type\"],\"message\":\"Type is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let invalid_format_type_update =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { type: \"Has Spaces!\" }
        ) {
          metaobjectDefinition { id type }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(invalid_format_type_update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"type\"],\"message\":\"Type contains one or more invalid characters. Only alphanumeric characters, underscores, and dashes are allowed.\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let blank_name_update =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { name: \"\" }
        ) {
          metaobjectDefinition { id name }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(blank_name_update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"name\"],\"message\":\"Name can't be blank\",\"code\":\"BLANK\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let too_long_name_update =
    run_mutation(created.store, created.identity, "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { name: \"" <> string.repeat("n", times: 256) <> "\" }
        ) {
          metaobjectDefinition { id name }
          userErrors { field message code elementKey elementIndex }
        }
      }")
  assert json.to_string(too_long_name_update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"name\"],\"message\":\"Name is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let too_long_description_update =
    run_mutation(created.store, created.identity, "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { description: \"" <> string.repeat("d", times: 256) <> "\" }
        ) {
          metaobjectDefinition { id description }
          userErrors { field message code elementKey elementIndex }
        }
      }")
  assert json.to_string(too_long_description_update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"description\"],\"message\":\"Description is too long (maximum is 255 characters)\",\"code\":\"TOO_LONG\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let invalid_key_update =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { fieldDefinitions: [{ update: { key: \"Bad Key\", name: \"Bad\" } }] }
        ) {
          metaobjectDefinition { id type fieldDefinitions { key name } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(invalid_key_update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"update\"],\"message\":\"Key contains one or more invalid characters.\",\"code\":\"INVALID\",\"elementKey\":\"Bad Key\",\"elementIndex\":null}]}}}"
}

pub fn definition_update_rejects_short_field_keys_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query(
        "codex_rows_update_short_key",
        "title",
      ),
    )

  let single =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { fieldDefinitions: [{ create: { key: \"a\", name: \"A\", type: \"single_line_text_field\" } }] }
        ) {
          metaobjectDefinition { id type fieldDefinitions { key } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(single.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"create\"],\"message\":\"Key is too short (minimum is 2 characters)\",\"code\":\"TOO_SHORT\",\"elementKey\":\"a\",\"elementIndex\":null}]}}}"

  let empty =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { fieldDefinitions: [{ create: { key: \"\", name: \"Empty Key\", type: \"single_line_text_field\" } }] }
        ) {
          metaobjectDefinition { id type fieldDefinitions { key } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(empty.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"create\"],\"message\":\"Key can't be blank\",\"code\":\"BLANK\",\"elementKey\":\"\",\"elementIndex\":null},{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"create\"],\"message\":\"Key is too short (minimum is 2 characters)\",\"code\":\"TOO_SHORT\",\"elementKey\":\"\",\"elementIndex\":null},{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"create\"],\"message\":\"Key contains one or more invalid characters.\",\"code\":\"INVALID\",\"elementKey\":\"\",\"elementIndex\":null}]}}}"

  let boundary =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { fieldDefinitions: [{ create: { key: \"ab\", name: \"AB\", type: \"single_line_text_field\" } }] }
        ) {
          metaobjectDefinition { id type displayNameKey fieldDefinitions { key } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(boundary.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":{\"id\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"type\":\"codex_rows_update_short_key\",\"displayNameKey\":\"title\",\"fieldDefinitions\":[{\"key\":\"title\"},{\"key\":\"ab\"}]},\"userErrors\":[]}}}"
}

pub fn definition_update_validates_field_definition_input_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query(
        "codex_rows_update_field_validation",
        "title",
      ),
    )

  let reserved =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { fieldDefinitions: [{ create: { key: \"handle\", name: \"Handle\", type: \"single_line_text_field\" } }] }
        ) {
          metaobjectDefinition { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(reserved.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"create\"],\"message\":\"The name \\\"handle\\\" is reserved for system use\",\"code\":\"RESERVED_NAME\",\"elementKey\":\"handle\",\"elementIndex\":null}]}}}"

  let duplicate =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            fieldDefinitions: [
              { create: { key: \"subtitle\", name: \"Subtitle\", type: \"single_line_text_field\" } },
              { create: { key: \"subtitle\", name: \"Subtitle\", type: \"single_line_text_field\" } }
            ]
          }
        ) {
          metaobjectDefinition { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(duplicate.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"1\",\"create\"],\"message\":\"Field \\\"subtitle\\\" duplicates other inputs\",\"code\":\"DUPLICATE_FIELD_INPUT\",\"elementKey\":\"subtitle\",\"elementIndex\":null}]}}}"

  let delete_empty =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { fieldDefinitions: [{ delete: { key: \"\" } }] }
        ) {
          metaobjectDefinition { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(delete_empty.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"delete\",\"key\"],\"message\":\"Field definition \\\"\\\" does not exist\",\"code\":\"UNDEFINED_OBJECT_FIELD\",\"elementKey\":\"\",\"elementIndex\":null}]}}}"

  let missing_display_name_key =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { displayNameKey: \"missing\" }
        ) {
          metaobjectDefinition { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(missing_display_name_key.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"displayNameKey\"],\"message\":\"Field definition \\\"missing\\\" does not exist\",\"code\":\"UNDEFINED_OBJECT_FIELD\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let too_many = run_mutation(created.store, created.identity, "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            fieldDefinitions: [" <> string.join(
      int_range(from: 2, to: 41)
        |> list.map(fn(index) {
          "{ create: { key: \"field_"
          <> int.to_string(index)
          <> "\", name: \"Field "
          <> int.to_string(index)
          <> "\", type: \"single_line_text_field\" } }"
        }),
      ",",
    ) <> "]
          }
        ) {
          metaobjectDefinition { id }
          userErrors { field message code elementKey elementIndex }
        }
      }")
  assert json.to_string(too_many.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\"],\"message\":\"Maximum 40 fields per metaobject definition\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn definition_update_field_operation_conflicts_match_shopify_user_errors_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query("codex_field_conflicts", "title"),
    )

  let conflicts =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            fieldDefinitions: [
              { create: { key: \"title\", name: \"Title again\", type: \"single_line_text_field\" } },
              { update: { key: \"missing_update\", name: \"Missing update\" } },
              { delete: { key: \"missing_delete\" } }
            ]
          }
        ) {
          metaobjectDefinition { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )

  assert json.to_string(conflicts.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"create\",\"key\"],\"message\":\"Field definition \\\"title\\\" is already taken\",\"code\":\"OBJECT_FIELD_TAKEN\",\"elementKey\":\"title\",\"elementIndex\":null},{\"field\":[\"definition\",\"fieldDefinitions\",\"1\",\"update\",\"key\"],\"message\":\"Field definition \\\"missing_update\\\" does not exist\",\"code\":\"UNDEFINED_OBJECT_FIELD\",\"elementKey\":\"missing_update\",\"elementIndex\":null},{\"field\":[\"definition\",\"fieldDefinitions\",\"2\",\"delete\",\"key\"],\"message\":\"Field definition \\\"missing_delete\\\" does not exist\",\"code\":\"UNDEFINED_OBJECT_FIELD\",\"elementKey\":\"missing_delete\",\"elementIndex\":null}]}}}"
}

pub fn definition_update_rejects_capability_disables_atomically_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_capabilities_query("codex_capability_disable"),
    )

  let entry =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_capability_disable\",
          handle: \"draft-row\",
          fields: [
            { key: \"title\", value: \"Draft row\" },
            { key: \"count\", value: \"4\" }
          ]
        }) {
          metaobject { id capabilities { publishable { status } } }
          userErrors { field message code }
        }
      }",
    )
  assert json.to_string(entry.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"capabilities\":{\"publishable\":{\"status\":\"DRAFT\"}}},\"userErrors\":[]}}}"

  let update =
    run_mutation(
      entry.store,
      entry.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            name: \"Should Not Persist\",
            capabilities: {
              publishable: { enabled: false },
              onlineStore: { enabled: false },
              renderable: { enabled: false }
            }
          }
        ) {
          metaobjectDefinition { id name capabilities { publishable { enabled } onlineStore { enabled } renderable { enabled } } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"capabilities\",\"publishable\"],\"message\":\"Cannot disable publishable while draft metaobjects exist.\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null},{\"field\":[\"definition\",\"capabilities\",\"onlineStore\"],\"message\":\"Cannot disable online store while metaobjects exist.\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null},{\"field\":[\"definition\",\"capabilities\",\"renderable\"],\"message\":\"Cannot disable renderable while metaobjects exist.\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let read_after =
    run_query(
      update.store,
      "{ metaobjectDefinitionByType(type: \"codex_capability_disable\") { name capabilities { publishable { enabled } onlineStore { enabled } renderable { enabled } translatable { enabled } } metaobjectsCount } }",
    )
  assert read_after
    == "{\"data\":{\"metaobjectDefinitionByType\":{\"name\":\"Codex Capability Rows\",\"capabilities\":{\"publishable\":{\"enabled\":true},\"onlineStore\":{\"enabled\":true},\"renderable\":{\"enabled\":true},\"translatable\":{\"enabled\":true}},\"metaobjectsCount\":1}}}"
}

pub fn definition_update_rejects_translatable_disable_with_translations_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_capabilities_query("codex_translatable_disable"),
    )
  let entry =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_translatable_disable\",
          handle: \"translated-row\",
          fields: [{ key: \"title\", value: \"Translated row\" }]
        }) {
          metaobject { id }
          userErrors { field message code }
        }
      }",
    )
  let #(_, translated_store) =
    store.stage_translation(
      entry.store,
      state_types.TranslationRecord(
        resource_id: "gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic",
        key: "title",
        locale: "fr",
        value: "Ligne traduite",
        translatable_content_digest: "digest-title",
        market_id: None,
        updated_at: "2024-01-01T00:00:03.000Z",
        outdated: False,
      ),
    )

  let update =
    run_mutation(
      translated_store,
      entry.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { capabilities: { translatable: { enabled: false } } }
        ) {
          metaobjectDefinition { id capabilities { translatable { enabled } } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"capabilities\",\"translatable\"],\"message\":\"Cannot disable translatable while translations exist.\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn definition_update_validates_renderable_enable_data_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_keys_query(
        "codex_renderable_enable",
        "title",
        ["title"],
      ),
    )

  let missing =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            name: \"Should Not Persist\",
            capabilities: { renderable: { enabled: true, data: { metaTitleKey: \"missing\" } } }
          }
        ) {
          metaobjectDefinition { id name capabilities { renderable { enabled } } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(missing.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"capabilities\",\"renderable\"],\"message\":\"Field definition \\\"missing\\\" does not exist\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let with_number_field =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            fieldDefinitions: [{ create: { key: \"count\", name: \"Count\", type: \"number_integer\" } }],
            capabilities: { renderable: { enabled: true, data: { metaDescriptionKey: \"count\" } } }
          }
        ) {
          metaobjectDefinition { id capabilities { renderable { enabled } } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(with_number_field.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"capabilities\",\"renderable\"],\"message\":\"Renderable Capability \\\"meta_description_key\\\" cannot reference the field definition \\\"count\\\" of type \\\"number_integer\\\". Only single_line_text_field, multi_line_text_field, rich_text_field definitions are allowed.\",\"code\":\"FIELD_TYPE_INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let read_after =
    run_query(
      missing.store,
      "{ metaobjectDefinitionByType(type: \"codex_renderable_enable\") { name capabilities { renderable { enabled } } fieldDefinitions { key } } }",
    )
  assert read_after
    == "{\"data\":{\"metaobjectDefinitionByType\":{\"name\":\"Codex Rows\",\"capabilities\":{\"renderable\":{\"enabled\":false}},\"fieldDefinitions\":[{\"key\":\"title\"}]}}}"
}

pub fn definition_update_rejects_more_than_forty_admin_filterable_fields_test() {
  let created =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_admin_filterable_fields_query(
        "codex_update_admin_filterable_limit",
        40,
      ),
    )
  assert string.contains(json.to_string(created.data), "\"userErrors\":[]")

  let rejected = run_mutation(created.store, created.identity, "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: {
            fieldDefinitions: [
              { create: " <> admin_filterable_field_definition_input("field_41") <> " }
            ]
          }
        ) {
          metaobjectDefinition { id fieldDefinitions { key capabilities { adminFilterable { enabled } } } }
          userErrors { field message code elementKey elementIndex }
        }
      }")

  assert json.to_string(rejected.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\"],\"message\":\"Maximum 40 admin-filterable fields per metaobject definition\",\"code\":\"INVALID\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert rejected.staged_resource_ids == []
  assert rejected.log_drafts == []

  let read_after =
    run_query(
      rejected.store,
      "{ metaobjectDefinitionByType(type: \"codex_update_admin_filterable_limit\") { fieldDefinitions { key } } }",
    )
  assert !string.contains(read_after, "\"key\":\"field_41\"")
}

pub fn definition_and_entry_lifecycle_stages_locally_test() {
  let identity = synthetic_identity.new()
  let definition_outcome =
    run_mutation(store.new(), identity, create_definition_query("codex_rows"))
  assert json.to_string(definition_outcome.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":{\"id\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"type\":\"codex_rows\",\"displayNameKey\":\"title\",\"capabilities\":{\"publishable\":{\"enabled\":true},\"translatable\":{\"enabled\":false}},\"fieldDefinitions\":[{\"key\":\"title\",\"name\":\"Title\",\"required\":true,\"type\":{\"name\":\"single_line_text_field\",\"category\":\"TEXT\"}},{\"key\":\"body\",\"name\":\"Body\",\"required\":false,\"type\":{\"name\":\"multi_line_text_field\",\"category\":\"TEXT\"}}],\"metaobjectsCount\":0},\"userErrors\":[]}}}"

  let create_entry =
    "mutation {
      metaobjectCreate(metaobject: {
        type: \"codex_rows\",
        handle: \"created-entry\",
        capabilities: { publishable: { status: \"ACTIVE\" } },
        fields: [
          { key: \"title\", value: \"Created title\" },
          { key: \"body\", value: \"Created body\" }
        ]
      }) {
        metaobject {
          id
          handle
          type
          displayName
          capabilities { publishable { status } onlineStore { templateSuffix } }
          fields { key type value jsonValue definition { key name required type { name category } } }
          titleField: field(key: \"title\") { key value jsonValue }
        }
        userErrors { field message code elementKey elementIndex }
      }
    }"
  let create_outcome =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      create_entry,
    )
  assert json.to_string(create_outcome.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"created-entry\",\"type\":\"codex_rows\",\"displayName\":\"Created title\",\"capabilities\":{\"publishable\":{\"status\":\"ACTIVE\"},\"onlineStore\":null},\"fields\":[{\"key\":\"title\",\"type\":\"single_line_text_field\",\"value\":\"Created title\",\"jsonValue\":\"Created title\",\"definition\":{\"key\":\"title\",\"name\":\"Title\",\"required\":true,\"type\":{\"name\":\"single_line_text_field\",\"category\":\"TEXT\"}}},{\"key\":\"body\",\"type\":\"multi_line_text_field\",\"value\":\"Created body\",\"jsonValue\":\"Created body\",\"definition\":{\"key\":\"body\",\"name\":\"Body\",\"required\":false,\"type\":{\"name\":\"multi_line_text_field\",\"category\":\"TEXT\"}}}],\"titleField\":{\"key\":\"title\",\"value\":\"Created title\",\"jsonValue\":\"Created title\"}},\"userErrors\":[]}}}"

  let read_back =
    run_query(
      create_outcome.store,
      "{ detail: metaobject(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\") { id handle displayName } byHandle: metaobjectByHandle(handle: { type: \"codex_rows\", handle: \"created-entry\" }) { id handle displayName } catalog: metaobjects(type: \"codex_rows\", first: 10) { nodes { id handle displayName } } definition: metaobjectDefinitionByType(type: \"codex_rows\") { metaobjectsCount } }",
    )
  assert read_back
    == "{\"data\":{\"detail\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"created-entry\",\"displayName\":\"Created title\"},\"byHandle\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"created-entry\",\"displayName\":\"Created title\"},\"catalog\":{\"nodes\":[{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"created-entry\",\"displayName\":\"Created title\"}]},\"definition\":{\"metaobjectsCount\":1}}}"
}

pub fn metaobject_update_redirect_new_handle_stages_url_redirect_test() {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry()
  let #(create_definition_body, proxy) =
    run_graphql_proxy(
      proxy,
      create_renderable_definition_query("codex_redirect_rows"),
    )
  assert create_definition_body
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":{\"id\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"type\":\"codex_redirect_rows\",\"capabilities\":{\"onlineStore\":{\"enabled\":true},\"renderable\":{\"enabled\":true}}},\"userErrors\":[]}}}"

  let #(create_entry_body, proxy) =
    run_graphql_proxy(
      proxy,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_redirect_rows\",
          handle: \"old-handle\",
          capabilities: { publishable: { status: \"ACTIVE\" }, onlineStore: { templateSuffix: \"\" } },
          fields: [{ key: \"title\", value: \"Old title\" }]
        }) {
          metaobject { id handle capabilities { onlineStore { templateSuffix } } }
          userErrors { field message code }
        }
      }",
    )
  assert create_entry_body
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic\",\"handle\":\"old-handle\",\"capabilities\":{\"onlineStore\":{\"templateSuffix\":\"\"}}},\"userErrors\":[]}}}"

  let #(update_body, proxy) =
    run_graphql_proxy(
      proxy,
      "mutation {
        metaobjectUpdate(
          id: \"gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic\",
          metaobject: {
            handle: \"new-handle\",
            redirectNewHandle: true,
            fields: [{ key: \"title\", value: \"New title\" }]
          }
        ) {
          metaobject { id handle displayName }
          userErrors { field message code }
        }
      }",
    )
  assert update_body
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic\",\"handle\":\"new-handle\",\"displayName\":\"New title\"},\"userErrors\":[]}}}"

  let #(redirects_body, proxy) =
    run_graphql_proxy(
      proxy,
      "{ urlRedirects(first: 5, query: \"path:/pages/title/old-handle\") { nodes { id path target } } }",
    )
  assert redirects_body
    == "{\"data\":{\"urlRedirects\":{\"nodes\":[{\"id\":\"gid://shopify/UrlRedirect/5?shopify-draft-proxy=synthetic\",\"path\":\"/pages/title/old-handle\",\"target\":\"/pages/title/new-handle\"}]}}}"

  let #(redirect_body, proxy) =
    run_graphql_proxy(
      proxy,
      "{ urlRedirect(id: \"gid://shopify/UrlRedirect/5?shopify-draft-proxy=synthetic\") { id path target } }",
    )
  assert redirect_body
    == "{\"data\":{\"urlRedirect\":{\"id\":\"gid://shopify/UrlRedirect/5?shopify-draft-proxy=synthetic\",\"path\":\"/pages/title/old-handle\",\"target\":\"/pages/title/new-handle\"}}}"

  let #(proxy_state.Response(status: state_status, body: state_body, ..), _) =
    draft_proxy.process_request(proxy, meta_state_request())
  assert state_status == 200
  let serialized_state = json.to_string(state_body)
  assert string.contains(serialized_state, "\"urlRedirects\"")
  assert string.contains(serialized_state, "\"/pages/title/old-handle\"")
}

pub fn metaobject_update_redirect_false_does_not_stage_url_redirect_test() {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry()
  let #(_, proxy) =
    run_graphql_proxy(
      proxy,
      create_renderable_definition_query("codex_redirect_false"),
    )
  let #(_, proxy) =
    run_graphql_proxy(
      proxy,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_redirect_false\",
          handle: \"old-handle\",
          capabilities: { publishable: { status: \"ACTIVE\" }, onlineStore: { templateSuffix: \"\" } },
          fields: [{ key: \"title\", value: \"Old title\" }]
        }) { metaobject { id } userErrors { field message code } }
      }",
    )
  let #(update_body, proxy) =
    run_graphql_proxy(
      proxy,
      "mutation {
        metaobjectUpdate(
          id: \"gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic\",
          metaobject: { handle: \"new-handle\", redirectNewHandle: false }
        ) { metaobject { id handle } userErrors { field message code } }
      }",
    )
  assert update_body
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic\",\"handle\":\"new-handle\"},\"userErrors\":[]}}}"

  let #(redirects_body, _) =
    run_graphql_proxy(
      proxy,
      "{ urlRedirects(first: 5, query: \"path:/pages/title/old-handle\") { nodes { id path target } } }",
    )
  assert redirects_body == "{\"data\":{\"urlRedirects\":{\"nodes\":[]}}}"
}

pub fn metaobject_update_redirect_new_handle_non_renderable_noops_test() {
  let identity = synthetic_identity.new()
  let definition_outcome =
    run_mutation(
      store.new(),
      identity,
      create_definition_with_field_key_query(
        "codex_redirect_nonrender",
        "title",
      ),
    )
  let create_outcome =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_redirect_nonrender\",
          handle: \"old-handle\",
          fields: [{ key: \"title\", value: \"Old title\" }]
        }) { metaobject { id handle } userErrors { field message code } }
      }",
    )
  let update_outcome =
    run_mutation(
      create_outcome.store,
      create_outcome.identity,
      "mutation {
        metaobjectUpdate(
          id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",
          metaobject: { handle: \"new-handle\", redirectNewHandle: true }
        ) { metaobject { id handle } userErrors { field message code } }
      }",
    )
  assert json.to_string(update_outcome.data)
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"new-handle\"},\"userErrors\":[]}}}"
  assert update_outcome.staged_resource_ids
    == [
      "gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic",
    ]
  assert store.list_effective_url_redirects(update_outcome.store) == []
}

pub fn metaobject_capabilities_require_enabled_definition_capability_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query("codex_capability_guards", "title"),
    )

  let rejected_create =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_capability_guards\",
          handle: \"rejected-publishable\",
          capabilities: { publishable: { status: \"ACTIVE\" } },
          fields: [{ key: \"title\", value: \"Rejected\" }]
        }) {
          metaobject { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(rejected_create.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"capabilities\",\"publishable\"],\"message\":\"Capability is not enabled on this definition\",\"code\":\"CAPABILITY_NOT_ENABLED\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert rejected_create.log_drafts == []
  assert rejected_create.staged_resource_ids == []

  let after_rejected_create =
    run_query(
      rejected_create.store,
      "{ metaobjects(type: \"codex_capability_guards\", first: 5) { nodes { handle } } definition: metaobjectDefinitionByType(type: \"codex_capability_guards\") { metaobjectsCount } }",
    )
  assert after_rejected_create
    == "{\"data\":{\"metaobjects\":{\"nodes\":[]},\"definition\":{\"metaobjectsCount\":0}}}"

  let create_existing =
    run_mutation(
      rejected_create.store,
      rejected_create.identity,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_capability_guards\",
          handle: \"existing\",
          fields: [{ key: \"title\", value: \"Original\" }]
        }) {
          metaobject { id handle displayName }
          userErrors { field message code }
        }
      }",
    )
  assert json.to_string(create_existing.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"existing\",\"displayName\":\"Original\"},\"userErrors\":[]}}}"

  let rejected_update =
    run_mutation(
      create_existing.store,
      create_existing.identity,
      "mutation {
        metaobjectUpdate(
          id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",
          metaobject: {
            handle: \"changed\",
            capabilities: { onlineStore: { templateSuffix: \"landing\" } },
            fields: [{ key: \"title\", value: \"Changed\" }]
          }
        ) {
          metaobject { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(rejected_update.data)
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"capabilities\",\"onlineStore\"],\"message\":\"Capability is not enabled on this definition\",\"code\":\"CAPABILITY_NOT_ENABLED\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert rejected_update.log_drafts == []
  assert rejected_update.staged_resource_ids == []

  let after_rejected_update =
    run_query(
      rejected_update.store,
      "{ detail: metaobject(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\") { id handle displayName fields { key value } } byChanged: metaobjectByHandle(handle: { type: \"codex_capability_guards\", handle: \"changed\" }) { id } }",
    )
  assert after_rejected_update
    == "{\"data\":{\"detail\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"existing\",\"displayName\":\"Original\",\"fields\":[{\"key\":\"title\",\"value\":\"Original\"}]},\"byChanged\":null}}"

  let rejected_upsert =
    run_mutation(
      rejected_update.store,
      rejected_update.identity,
      "mutation {
        metaobjectUpsert(
          handle: { type: \"codex_capability_guards\", handle: \"upserted\" },
          metaobject: {
            capabilities: { publishable: { status: \"ACTIVE\" }, onlineStore: { templateSuffix: \"landing\" } },
            fields: [{ key: \"title\", value: \"Upserted\" }]
          }
        ) {
          metaobject { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(rejected_upsert.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"capabilities\",\"publishable\"],\"message\":\"Capability is not enabled on this definition\",\"code\":\"CAPABILITY_NOT_ENABLED\",\"elementKey\":null,\"elementIndex\":null},{\"field\":[\"capabilities\",\"onlineStore\"],\"message\":\"Capability is not enabled on this definition\",\"code\":\"CAPABILITY_NOT_ENABLED\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert rejected_upsert.log_drafts == []
  assert rejected_upsert.staged_resource_ids == []

  let after_rejected_upsert =
    run_query(
      rejected_upsert.store,
      "{ existing: metaobject(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\") { handle } upserted: metaobjectByHandle(handle: { type: \"codex_capability_guards\", handle: \"upserted\" }) { id } definition: metaobjectDefinitionByType(type: \"codex_capability_guards\") { metaobjectsCount } }",
    )
  assert after_rejected_upsert
    == "{\"data\":{\"existing\":{\"handle\":\"existing\"},\"upserted\":null,\"definition\":{\"metaobjectsCount\":1}}}"
}

pub fn metaobject_create_rejects_non_standard_type_at_default_cap_test() {
  let seeded =
    store.upsert_base_metaobject_definitions(store.new(), [
      definition_record("9001", "codex_metaobject_cap", False, 1_000_000),
    ])
  let outcome =
    run_mutation(
      seeded,
      synthetic_identity.new(),
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_metaobject_cap\",
          handle: \"over-limit\",
          fields: [{ key: \"title\", value: \"Over limit\" }]
        }) {
          metaobject { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":null,\"userErrors\":[{\"field\":null,\"message\":\"Maximum metaobjects of type 'codex_metaobject_cap' has been exceeded\",\"code\":\"MAX_OBJECTS_EXCEEDED\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []

  let read_after =
    run_query(
      outcome.store,
      "{ metaobjects(type: \"codex_metaobject_cap\", first: 5) { nodes { id } } definition: metaobjectDefinitionByType(type: \"codex_metaobject_cap\") { metaobjectsCount } }",
    )
  assert read_after
    == "{\"data\":{\"metaobjects\":{\"nodes\":[]},\"definition\":{\"metaobjectsCount\":1000000}}}"
}

pub fn metaobject_upsert_rejects_non_standard_create_at_default_cap_test() {
  let seeded =
    store.upsert_base_metaobject_definitions(store.new(), [
      definition_record("9002", "codex_metaobject_upsert_cap", False, 1_000_000),
    ])
  let outcome =
    run_mutation(
      seeded,
      synthetic_identity.new(),
      "mutation {
        metaobjectUpsert(
          handle: { type: \"codex_metaobject_upsert_cap\", handle: \"over-limit\" },
          metaobject: { fields: [{ key: \"title\", value: \"Over limit\" }] }
        ) {
          metaobject { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )

  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":null,\"userErrors\":[{\"field\":null,\"message\":\"Maximum metaobjects of type 'codex_metaobject_upsert_cap' has been exceeded\",\"code\":\"MAX_OBJECTS_EXCEEDED\",\"elementKey\":null,\"elementIndex\":null}]}}}"
  assert outcome.staged_resource_ids == []
  assert outcome.log_drafts == []

  let read_after =
    run_query(
      outcome.store,
      "{ metaobjectByHandle(handle: { type: \"codex_metaobject_upsert_cap\", handle: \"over-limit\" }) { id } definition: metaobjectDefinitionByType(type: \"codex_metaobject_upsert_cap\") { metaobjectsCount } }",
    )
  assert read_after
    == "{\"data\":{\"metaobjectByHandle\":null,\"definition\":{\"metaobjectsCount\":1000000}}}"
}

pub fn metaobject_create_uses_cached_hydrated_definition_count_test() {
  let type_ = "codex_cached_count"
  let first =
    run_mutation_with_upstream(
      store.new(),
      synthetic_identity.new(),
      metaobject_create_query(type_, "fills-cap"),
      upstream_query.UpstreamContext(
        transport: Some(definition_hydrate_transport(type_, 999_999)),
        origin: "https://example.myshopify.com",
        headers: dict.new(),
        allow_upstream_reads: True,
      ),
    )
  assert json.to_string(first.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"

  let second =
    run_mutation(
      first.store,
      first.identity,
      metaobject_create_query(type_, "over-cap"),
    )
  assert json.to_string(second.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":null,\"userErrors\":[{\"field\":null,\"message\":\"Maximum metaobjects of type 'codex_cached_count' has been exceeded\",\"code\":\"MAX_OBJECTS_EXCEEDED\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let read_after =
    run_query(
      second.store,
      "{ definition: metaobjectDefinitionByType(type: \"codex_cached_count\") { metaobjectsCount } }",
    )
  assert read_after
    == "{\"data\":{\"definition\":{\"metaobjectsCount\":1000000}}}"
}

pub fn metaobject_create_and_upsert_skip_cap_for_standard_templates_test() {
  let seeded =
    store.upsert_base_metaobject_definitions(store.new(), [
      definition_record("9003", "shopify--standard-cap", True, 1_000_000),
    ])
  let created =
    run_mutation(
      seeded,
      synthetic_identity.new(),
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"shopify--standard-cap\",
          handle: \"created\",
          fields: [{ key: \"title\", value: \"Created\" }]
        }) {
          metaobject { id handle displayName }
          userErrors { field message code }
        }
      }",
    )

  assert json.to_string(created.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/1?shopify-draft-proxy=synthetic\",\"handle\":\"created\",\"displayName\":\"Created\"},\"userErrors\":[]}}}"

  let upserted =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectUpsert(
          handle: { type: \"shopify--standard-cap\", handle: \"upserted\" },
          metaobject: { fields: [{ key: \"title\", value: \"Upserted\" }] }
        ) {
          metaobject { id handle displayName }
          userErrors { field message code }
        }
      }",
    )

  assert json.to_string(upserted.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"upserted\",\"displayName\":\"Upserted\"},\"userErrors\":[]}}}"
}

pub fn metaobject_update_missing_id_returns_record_not_found_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation { metaobjectUpdate(id: \"gid://shopify/Metaobject/999999999\", metaobject: { fields: [{ key: \"title\", value: \"Nope\" }] }) { metaobject { id } userErrors { field message code elementKey elementIndex } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Record not found\",\"code\":\"RECORD_NOT_FOUND\",\"elementKey\":null,\"elementIndex\":null}]}}}"
}

pub fn metaobject_entry_mutations_reject_duplicate_field_input_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_duplicate_fields"),
    )

  let duplicate_create =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_duplicate_fields\", fields: [{ key: \"title\", value: \"One\" }, { key: \"title\", value: \"Two\" }] }) { metaobject { id } userErrors { field message code elementKey elementIndex } } }",
    )
  assert json.to_string(duplicate_create.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"metaobject\",\"fields\",\"1\"],\"message\":\"Field \\\"title\\\" duplicates other inputs\",\"code\":\"DUPLICATE_FIELD_INPUT\",\"elementKey\":\"title\",\"elementIndex\":null},{\"field\":[\"metaobject\",\"fields\",\"1\"],\"message\":\"Title can't be blank\",\"code\":\"OBJECT_FIELD_REQUIRED\",\"elementKey\":\"title\",\"elementIndex\":null}]}}}"

  let create_one =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_duplicate_fields\", handle: \"one\", fields: [{ key: \"title\", value: \"One\" }] }) { metaobject { id } userErrors { field message code } } }",
    )
  let duplicate_update =
    run_mutation(
      create_one.store,
      create_one.identity,
      "mutation { metaobjectUpdate(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\", metaobject: { fields: [{ key: \"title\", value: \"One\" }, { key: \"title\", value: \"Two\" }] }) { metaobject { id } userErrors { field message code elementKey elementIndex } } }",
    )
  assert json.to_string(duplicate_update.data)
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"metaobject\",\"fields\",\"1\"],\"message\":\"Field \\\"title\\\" duplicates other inputs\",\"code\":\"DUPLICATE_FIELD_INPUT\",\"elementKey\":\"title\",\"elementIndex\":null},{\"field\":[\"metaobject\",\"fields\",\"1\"],\"message\":\"Title can't be blank\",\"code\":\"OBJECT_FIELD_REQUIRED\",\"elementKey\":\"title\",\"elementIndex\":null}]}}}"

  let duplicate_upsert =
    run_mutation(
      create_one.store,
      create_one.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_duplicate_fields\", handle: \"two\" }, metaobject: { fields: [{ key: \"title\", value: \"One\" }, { key: \"title\", value: \"Two\" }] }) { metaobject { id } userErrors { field message code elementKey elementIndex } } }",
    )
  assert json.to_string(duplicate_upsert.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"fields\",\"1\"],\"message\":\"Field \\\"title\\\" duplicates other inputs\",\"code\":\"DUPLICATE_FIELD_INPUT\",\"elementKey\":\"title\",\"elementIndex\":null},{\"field\":[\"fields\",\"1\"],\"message\":\"Title can't be blank\",\"code\":\"OBJECT_FIELD_REQUIRED\",\"elementKey\":\"title\",\"elementIndex\":null}]}}}"
}

pub fn metaobject_update_preserves_display_name_until_display_field_changes_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_display_name_update"),
    )
  let create =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_display_name_update\", handle: \"one\", fields: [{ key: \"title\", value: \"Original title\" }, { key: \"body\", value: \"Original body\" }] }) { metaobject { id } userErrors { field message code } } }",
    )
  let assert Some(existing) =
    store.get_effective_metaobject_by_id(
      create.store,
      "gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic",
    )
  let #(_, captured_display_store) =
    store.upsert_staged_metaobject(
      create.store,
      state_types.MetaobjectRecord(
        ..existing,
        display_name: Some("Captured display name"),
      ),
    )

  let body_update =
    run_mutation(
      captured_display_store,
      create.identity,
      "mutation { metaobjectUpdate(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\", metaobject: { fields: [{ key: \"body\", value: \"Changed body\" }] }) { metaobject { id displayName updatedAt fields { key value } } userErrors { field message code } } }",
    )
  assert json.to_string(body_update.data)
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"displayName\":\"Captured display name\",\"updatedAt\":\"2024-01-01T00:00:03.000Z\",\"fields\":[{\"key\":\"title\",\"value\":\"Original title\"},{\"key\":\"body\",\"value\":\"Changed body\"}]},\"userErrors\":[]}}}"

  let title_update =
    run_mutation(
      body_update.store,
      body_update.identity,
      "mutation { metaobjectUpdate(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\", metaobject: { fields: [{ key: \"title\", value: \"Changed title\" }] }) { metaobject { id displayName updatedAt fields { key value } } userErrors { field message code } } }",
    )
  assert json.to_string(title_update.data)
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"displayName\":\"Changed title\",\"updatedAt\":\"2024-01-01T00:00:04.000Z\",\"fields\":[{\"key\":\"title\",\"value\":\"Changed title\"},{\"key\":\"body\",\"value\":\"Changed body\"}]},\"userErrors\":[]}}}"
}

pub fn definition_delete_cascades_associated_entries_locally_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_delete_cascade"),
    )
  let create_one =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_delete_cascade\", handle: \"one\", fields: [{ key: \"title\", value: \"One\" }] }) { metaobject { id } userErrors { message code } } }",
    )
  let create_two =
    run_mutation(
      create_one.store,
      create_one.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_delete_cascade\", handle: \"two\", fields: [{ key: \"title\", value: \"Two\" }] }) { metaobject { id } userErrors { message code } } }",
    )
  let delete_query =
    "mutation { metaobjectDefinitionDelete(id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\") { deletedId userErrors { field message code } } }"
  let delete_definition =
    run_mutation(create_two.store, create_two.identity, delete_query)

  assert json.to_string(delete_definition.data)
    == "{\"data\":{\"metaobjectDefinitionDelete\":{\"deletedId\":\"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",\"userErrors\":[]}}}"
  assert delete_definition.staged_resource_ids
    == [
      "gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic",
      "gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic",
      "gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic",
    ]
  let assert [delete_draft] = delete_definition.log_drafts
  assert delete_draft.staged_resource_ids
    == delete_definition.staged_resource_ids

  let #(logged_store, _) =
    mutation_helpers.record_log_drafts(
      delete_definition.store,
      delete_definition.identity,
      path,
      delete_query,
      dict.new(),
      delete_definition.log_drafts,
    )
  let assert [log_entry] = store.get_log(logged_store)
  assert log_entry.operation_name == Some("metaobjectDefinitionDelete")
  assert log_entry.staged_resource_ids == delete_definition.staged_resource_ids

  let read_after =
    run_query(
      delete_definition.store,
      "{ definition: metaobjectDefinition(id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\") { id } byType: metaobjectDefinitionByType(type: \"codex_delete_cascade\") { id } one: metaobject(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\") { id } two: metaobject(id: \"gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic\") { id } byHandle: metaobjectByHandle(handle: { type: \"codex_delete_cascade\", handle: \"one\" }) { id } catalog: metaobjects(type: \"codex_delete_cascade\", first: 10) { nodes { id } } }",
    )
  assert read_after
    == "{\"data\":{\"definition\":null,\"byType\":null,\"one\":null,\"two\":null,\"byHandle\":null,\"catalog\":{\"nodes\":[]}}}"
}

pub fn update_upsert_delete_and_bulk_delete_stage_locally_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_rows_two"),
    )
  let create_one =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_rows_two\", handle: \"one\", fields: [{ key: \"title\", value: \"One\" }] }) { metaobject { id } userErrors { message } } }",
    )
  let update_one =
    run_mutation(
      create_one.store,
      create_one.identity,
      "mutation { metaobjectUpdate(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\", metaobject: { handle: \"renamed\", fields: [{ key: \"title\", value: \"Renamed\" }] }) { metaobject { id handle displayName } userErrors { message code } } }",
    )
  assert json.to_string(update_one.data)
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"renamed\",\"displayName\":\"Renamed\"},\"userErrors\":[]}}}"

  let upsert_two =
    run_mutation(
      update_one.store,
      update_one.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_rows_two\", handle: \"two\" }, metaobject: { fields: [{ key: \"title\", value: \"Two\" }] }) { metaobject { id handle displayName } userErrors { message code } } }",
    )
  assert json.to_string(upsert_two.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic\",\"handle\":\"two\",\"displayName\":\"Two\"},\"userErrors\":[]}}}"

  let delete_one =
    run_mutation(
      upsert_two.store,
      upsert_two.identity,
      "mutation { metaobjectDelete(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\") { deletedId userErrors { message code } } }",
    )
  assert json.to_string(delete_one.data)
    == "{\"data\":{\"metaobjectDelete\":{\"deletedId\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"userErrors\":[]}}}"

  let bulk_delete =
    run_mutation(
      delete_one.store,
      delete_one.identity,
      "mutation { metaobjectBulkDelete(where: { type: \"codex_rows_two\" }) { job { id done } userErrors { message code } } }",
    )
  assert json.to_string(bulk_delete.data)
    == "{\"data\":{\"metaobjectBulkDelete\":{\"job\":{\"id\":\"gid://shopify/Job/4\",\"done\":true},\"userErrors\":[]}}}"

  let read_back =
    run_query(
      bulk_delete.store,
      "{ metaobjects(type: \"codex_rows_two\", first: 10) { nodes { id } } definition: metaobjectDefinitionByType(type: \"codex_rows_two\") { metaobjectsCount } }",
    )
  assert read_back
    == "{\"data\":{\"metaobjects\":{\"nodes\":[]},\"definition\":{\"metaobjectsCount\":0}}}"
}

pub fn metaobject_mutations_validate_explicit_handle_format_length_and_blank_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_handle_validation"),
    )
  let valid_entry =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_handle_validation\", handle: \"valid\", fields: [{ key: \"title\", value: \"Valid\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  let too_long_handle = string.repeat("x", times: 256)

  let create_invalid =
    run_mutation(
      valid_entry.store,
      valid_entry.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_handle_validation\", handle: \"hello world!\", fields: [{ key: \"title\", value: \"Invalid\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert_metaobject_handle_user_error(
    json.to_string(create_invalid.data),
    "metaobjectCreate",
    "[\"metaobject\",\"handle\"]",
    "INVALID",
  )
  assert create_invalid.staged_resource_ids == []
  assert create_invalid.log_drafts == []

  let create_too_long =
    run_mutation(
      valid_entry.store,
      valid_entry.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_handle_validation\", handle: \""
        <> too_long_handle
        <> "\", fields: [{ key: \"title\", value: \"Too long\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert_metaobject_handle_user_error(
    json.to_string(create_too_long.data),
    "metaobjectCreate",
    "[\"metaobject\",\"handle\"]",
    "TOO_LONG",
  )
  assert create_too_long.staged_resource_ids == []
  assert create_too_long.log_drafts == []

  let create_blank =
    run_mutation(
      valid_entry.store,
      valid_entry.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_handle_validation\", handle: \"\", fields: [{ key: \"title\", value: \"Blank\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert json.to_string(create_blank.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/3?shopify-draft-proxy=synthetic\",\"handle\":\"blank\"},\"userErrors\":[]}}}"

  let update_invalid =
    run_mutation(
      valid_entry.store,
      valid_entry.identity,
      "mutation { metaobjectUpdate(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\", metaobject: { handle: \"hello world!\", fields: [{ key: \"title\", value: \"Invalid\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert_metaobject_handle_user_error(
    json.to_string(update_invalid.data),
    "metaobjectUpdate",
    "[\"metaobject\",\"handle\"]",
    "INVALID",
  )
  assert update_invalid.staged_resource_ids == []
  assert update_invalid.log_drafts == []

  let update_too_long =
    run_mutation(
      valid_entry.store,
      valid_entry.identity,
      "mutation { metaobjectUpdate(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\", metaobject: { handle: \""
        <> too_long_handle
        <> "\", fields: [{ key: \"title\", value: \"Too long\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert_metaobject_handle_user_error(
    json.to_string(update_too_long.data),
    "metaobjectUpdate",
    "[\"metaobject\",\"handle\"]",
    "TOO_LONG",
  )
  assert update_too_long.staged_resource_ids == []
  assert update_too_long.log_drafts == []

  let update_blank =
    run_mutation(
      valid_entry.store,
      valid_entry.identity,
      "mutation { metaobjectUpdate(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\", metaobject: { handle: \"\", fields: [{ key: \"title\", value: \"Blank\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert_metaobject_handle_user_error(
    json.to_string(update_blank.data),
    "metaobjectUpdate",
    "[\"metaobject\",\"handle\"]",
    "BLANK",
  )
  assert string.contains(
    json.to_string(update_blank.data),
    "\"code\":\"INVALID\"",
  )
  assert update_blank.staged_resource_ids == []
  assert update_blank.log_drafts == []

  let upsert_invalid =
    run_mutation(
      valid_entry.store,
      valid_entry.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_handle_validation\", handle: \"hello world!\" }, metaobject: { fields: [{ key: \"title\", value: \"Invalid\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert_metaobject_handle_user_error(
    json.to_string(upsert_invalid.data),
    "metaobjectUpsert",
    "[\"handle\",\"handle\"]",
    "INVALID",
  )
  assert upsert_invalid.staged_resource_ids == []
  assert upsert_invalid.log_drafts == []

  let upsert_too_long =
    run_mutation(
      valid_entry.store,
      valid_entry.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_handle_validation\", handle: \""
        <> too_long_handle
        <> "\" }, metaobject: { fields: [{ key: \"title\", value: \"Too long\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert_metaobject_handle_user_error(
    json.to_string(upsert_too_long.data),
    "metaobjectUpsert",
    "[\"handle\",\"handle\"]",
    "TOO_LONG",
  )
  assert upsert_too_long.staged_resource_ids == []
  assert upsert_too_long.log_drafts == []

  let upsert_blank =
    run_mutation(
      create_blank.store,
      create_blank.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_handle_validation\", handle: \"\" }, metaobject: { fields: [{ key: \"title\", value: \"Blank\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  assert string.contains(
    json.to_string(upsert_blank.data),
    "\"handle\":\"blank-1\"",
  )
  assert string.contains(json.to_string(upsert_blank.data), "\"userErrors\":[]")

  let read_back =
    run_query(
      valid_entry.store,
      "{ valid: metaobject(id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\") { id handle displayName } byHandle: metaobjectByHandle(handle: { type: \"codex_handle_validation\", handle: \"valid\" }) { id handle displayName } }",
    )
  assert read_back
    == "{\"data\":{\"valid\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"valid\",\"displayName\":\"Valid\"},\"byHandle\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"valid\",\"displayName\":\"Valid\"}}}"
}

pub fn metaobject_create_omitted_handle_generates_valid_capped_handle_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_auto_handle"),
    )
  let capped = string.repeat("a", times: 255)
  let create =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_auto_handle\", fields: [{ key: \"title\", value: \""
        <> string.repeat("A", times: 260)
        <> " ! héllo/world\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  let serialized = json.to_string(create.data)
  assert string.contains(serialized, "\"userErrors\":[]")
  assert string.contains(serialized, "\"handle\":\"" <> capped <> "\"")

  let duplicate =
    run_mutation(
      create.store,
      create.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_auto_handle\", fields: [{ key: \"title\", value: \""
        <> string.repeat("A", times: 260)
        <> " ! héllo/world\" }] }) { metaobject { id handle } userErrors { field message code } } }",
    )
  let duplicate_handle = string.repeat("a", times: 253) <> "-1"
  assert string.contains(json.to_string(duplicate.data), "\"userErrors\":[]")
  assert string.contains(
    json.to_string(duplicate.data),
    "\"handle\":\"" <> duplicate_handle <> "\"",
  )
}

pub fn metaobject_upsert_exact_match_preserves_updated_at_and_skips_log_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_upsert_exact"),
    )
  let create =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_upsert_exact\", handle: \"same\" }, metaobject: { fields: [{ key: \"title\", value: \"Same\" }, { key: \"body\", value: \"Body\" }] }) { metaobject { id handle displayName updatedAt fields { key value } } userErrors { field message code } } }",
    )
  assert json.to_string(create.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"same\",\"displayName\":\"Same\",\"updatedAt\":\"2024-01-01T00:00:01.000Z\",\"fields\":[{\"key\":\"title\",\"value\":\"Same\"},{\"key\":\"body\",\"value\":\"Body\"}]},\"userErrors\":[]}}}"
  assert list.length(create.log_drafts) == 1

  let exact =
    run_mutation(
      create.store,
      create.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_upsert_exact\", handle: \"same\" }, metaobject: { handle: \"same\", fields: [{ key: \"title\", value: \"Same\" }, { key: \"body\", value: \"Body\" }] }) { metaobject { id handle displayName updatedAt fields { key value } } userErrors { field message code } } }",
    )
  assert json.to_string(exact.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":{\"id\":\"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",\"handle\":\"same\",\"displayName\":\"Same\",\"updatedAt\":\"2024-01-01T00:00:01.000Z\",\"fields\":[{\"key\":\"title\",\"value\":\"Same\"},{\"key\":\"body\",\"value\":\"Body\"}]},\"userErrors\":[]}}}"
  assert exact.log_drafts == []
}

pub fn metaobject_upsert_partitions_handle_and_value_error_prefixes_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_upsert_prefix"),
    )
  let one =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_upsert_prefix\", handle: \"one\", fields: [{ key: \"title\", value: \"One\" }] }) { metaobject { id } userErrors { field message code } } }",
    )
  let two =
    run_mutation(
      one.store,
      one.identity,
      "mutation { metaobjectCreate(metaobject: { type: \"codex_upsert_prefix\", handle: \"two\", fields: [{ key: \"title\", value: \"Two\" }] }) { metaobject { id } userErrors { field message code } } }",
    )

  let taken =
    run_mutation(
      two.store,
      two.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_upsert_prefix\", handle: \"one\" }, metaobject: { handle: \"two\", fields: [{ key: \"title\", value: \"One\" }] }) { metaobject { id } userErrors { field message code elementKey elementIndex } } }",
    )
  assert json.to_string(taken.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"handle\",\"handle\"],\"message\":\"Handle has already been taken\",\"code\":\"TAKEN\",\"elementKey\":null,\"elementIndex\":null}]}}}"

  let missing_required =
    run_mutation(
      two.store,
      two.identity,
      "mutation { metaobjectUpsert(handle: { type: \"codex_upsert_prefix\", handle: \"missing-title\" }, metaobject: { fields: [{ key: \"body\", value: \"Only body\" }] }) { metaobject { id } userErrors { field message code elementKey elementIndex } } }",
    )
  assert json.to_string(missing_required.data)
    == "{\"data\":{\"metaobjectUpsert\":{\"metaobject\":null,\"userErrors\":[{\"field\":[],\"message\":\"Title can't be blank\",\"code\":\"OBJECT_FIELD_REQUIRED\",\"elementKey\":\"title\",\"elementIndex\":null}]}}}"
}

pub fn metaobject_upsert_key_value_shape_requires_metaobject_or_values_test() {
  let #(proxy_state.Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(
        "mutation { metaobjectUpsert(handle: { type: \"codex_rows\", handle: \"blank\" }) { metaobject { id } userErrors { field message code } } }",
      ),
    )
  assert status == 200
  let serialized = json.to_string(body)
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(serialized, "metaobject")
  assert string.contains(serialized, "values")
  assert !string.contains(serialized, "\"userErrors\"")
}

pub fn metaobject_field_values_validate_and_coerce_by_type_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation {
        metaobjectDefinitionCreate(definition: {
          type: \"codex_field_validation\",
          name: \"Codex Field Validation\",
          fieldDefinitions: [
            { key: \"dec\", name: \"Decimal\", type: \"number_decimal\", required: false },
            { key: \"date\", name: \"Date\", type: \"date\", required: false },
            { key: \"title\", name: \"Title\", type: \"single_line_text_field\", required: false, validations: [{ name: \"max\", value: \"3\" }] },
            { key: \"list_int\", name: \"List Integer\", type: \"list.number_integer\", required: false },
            { key: \"rating\", name: \"Rating\", type: \"rating\", required: false, validations: [{ name: \"scale_min\", value: \"1.0\" }, { name: \"scale_max\", value: \"5.0\" }] },
            { key: \"num\", name: \"Number\", type: \"number_integer\", required: false },
            { key: \"flag\", name: \"Flag\", type: \"boolean\", required: false }
          ]
        }) {
          metaobjectDefinition { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )

  let invalid_create =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_field_validation\",
          fields: [
            { key: \"dec\", value: \"hello\" },
            { key: \"date\", value: \"2024-99-01\" },
            { key: \"title\", value: \"abcd\" },
            { key: \"list_int\", value: \"[\\\"hello\\\"]\" },
            { key: \"rating\", value: \"{\\\"value\\\":\\\"10\\\",\\\"scale_min\\\":\\\"1.0\\\",\\\"scale_max\\\":\\\"5.0\\\"}\" }
          ]
        }) {
          metaobject { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(invalid_create.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"metaobject\",\"fields\",\"0\"],\"message\":\"Value must be a decimal.\",\"code\":\"INVALID_VALUE\",\"elementKey\":\"dec\",\"elementIndex\":null},{\"field\":[\"metaobject\",\"fields\",\"1\"],\"message\":\"Value must be in YYYY-MM-DD format.\",\"code\":\"INVALID_VALUE\",\"elementKey\":\"date\",\"elementIndex\":null},{\"field\":[\"metaobject\",\"fields\",\"2\"],\"message\":\"Value has a maximum length of 3.\",\"code\":\"INVALID_VALUE\",\"elementKey\":\"title\",\"elementIndex\":null},{\"field\":[\"metaobject\",\"fields\",\"3\"],\"message\":\"Value must be an integer.\",\"code\":\"INVALID_VALUE\",\"elementKey\":\"list_int\",\"elementIndex\":0},{\"field\":[\"metaobject\",\"fields\",\"4\"],\"message\":\"Value has a maximum of 5.0.\",\"code\":\"INVALID_VALUE\",\"elementKey\":\"rating\",\"elementIndex\":null}]}}}"

  let coerced_create =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation {
        metaobjectCreate(metaobject: {
          type: \"codex_field_validation\",
          fields: [
            { key: \"num\", value: \"hello\" },
            { key: \"flag\", value: \"hello\" }
          ]
        }) {
          metaobject { fields { key value jsonValue } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(coerced_create.data)
    == "{\"data\":{\"metaobjectCreate\":{\"metaobject\":{\"fields\":[{\"key\":\"dec\",\"value\":null,\"jsonValue\":null},{\"key\":\"date\",\"value\":null,\"jsonValue\":null},{\"key\":\"title\",\"value\":null,\"jsonValue\":null},{\"key\":\"list_int\",\"value\":null,\"jsonValue\":null},{\"key\":\"rating\",\"value\":null,\"jsonValue\":null},{\"key\":\"num\",\"value\":\"0\",\"jsonValue\":0},{\"key\":\"flag\",\"value\":\"true\",\"jsonValue\":true}]},\"userErrors\":[]}}}"

  let invalid_update =
    run_mutation(
      coerced_create.store,
      coerced_create.identity,
      "mutation {
        metaobjectUpdate(
          id: \"gid://shopify/Metaobject/2?shopify-draft-proxy=synthetic\",
          metaobject: { fields: [{ key: \"flag\", value: \"hello\" }] }
        ) {
          metaobject { id }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(invalid_update.data)
    == "{\"data\":{\"metaobjectUpdate\":{\"metaobject\":null,\"userErrors\":[{\"field\":[\"metaobject\",\"fields\",\"0\"],\"message\":\"Value must be true or false.\",\"code\":\"INVALID_VALUE\",\"elementKey\":\"flag\",\"elementIndex\":null}]}}}"
}

pub fn bulk_delete_empty_ids_returns_empty_job_success_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation { metaobjectBulkDelete(where: { ids: [] }) { job { id done } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectBulkDelete\":{\"job\":{\"id\":\"gid://shopify/Job/1\",\"done\":false},\"userErrors\":[]}}}"
}

pub fn bulk_delete_unknown_type_returns_type_not_found_user_error_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation { metaobjectBulkDelete(where: { type: \"does_not_exist\" }) { job { id done } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectBulkDelete\":{\"job\":null,\"userErrors\":[{\"field\":[\"where\",\"type\"],\"message\":\"No metaobject definition exists for type \\\"does_not_exist\\\"\",\"code\":\"RECORD_NOT_FOUND\"}]}}}"
}

pub fn bulk_delete_known_empty_type_returns_empty_job_success_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_empty_type"),
    )
  let outcome =
    run_mutation(
      definition_outcome.store,
      definition_outcome.identity,
      "mutation { metaobjectBulkDelete(where: { type: \"codex_empty_type\" }) { job { id done } userErrors { field message code } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectBulkDelete\":{\"job\":{\"id\":\"gid://shopify/Job/2\",\"done\":false},\"userErrors\":[]}}}"
}

pub fn bulk_delete_with_type_and_ids_returns_top_level_error_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      "mutation { metaobjectBulkDelete(where: { type: \"codex_rows\", ids: [] }) { job { id done } userErrors { field message code } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"errors\":[{\"message\":\"MetaobjectBulkDeleteWhereCondition requires exactly one of type, ids\",\"locations\":[{\"line\":1,\"column\":12},{\"line\":1,\"column\":62}],\"path\":[\"metaobjectBulkDelete\"],\"extensions\":{\"code\":\"INVALID_FIELD_ARGUMENTS\"}}],\"data\":{\"metaobjectBulkDelete\":null}}"
}

pub fn bulk_delete_ids_caps_to_first_250_test() {
  let definition_outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_query("codex_many_rows"),
    )
  let seeded =
    list.fold(int_range(1, 251), definition_outcome, fn(acc, index) {
      run_mutation(
        acc.store,
        acc.identity,
        "mutation { metaobjectCreate(metaobject: { type: \"codex_many_rows\", handle: \"row-"
          <> int.to_string(index)
          <> "\", fields: [{ key: \"title\", value: \"Row "
          <> int.to_string(index)
          <> "\" }] }) { metaobject { id } userErrors { message } } }",
      )
    })
  let id_literals =
    int_range(2, 252)
    |> list.map(fn(index) {
      "\"gid://shopify/Metaobject/"
      <> int.to_string(index)
      <> "?shopify-draft-proxy=synthetic\""
    })
    |> string.join(", ")
  let bulk_delete =
    run_mutation(
      seeded.store,
      seeded.identity,
      "mutation { metaobjectBulkDelete(where: { ids: ["
        <> id_literals
        <> "] }) { job { id done } userErrors { field message code elementIndex } } }",
    )
  assert json.to_string(bulk_delete.data)
    == "{\"data\":{\"metaobjectBulkDelete\":{\"job\":{\"id\":\"gid://shopify/Job/253\",\"done\":true},\"userErrors\":[]}}}"

  let read_back =
    run_query(
      bulk_delete.store,
      "{ deleted: metaobject(id: \"gid://shopify/Metaobject/251?shopify-draft-proxy=synthetic\") { id } retained: metaobject(id: \"gid://shopify/Metaobject/252?shopify-draft-proxy=synthetic\") { id handle } definition: metaobjectDefinitionByType(type: \"codex_many_rows\") { metaobjectsCount } }",
    )
  assert read_back
    == "{\"data\":{\"deleted\":null,\"retained\":{\"id\":\"gid://shopify/Metaobject/252?shopify-draft-proxy=synthetic\",\"handle\":\"row-251\"},\"definition\":{\"metaobjectsCount\":1}}}"
}
