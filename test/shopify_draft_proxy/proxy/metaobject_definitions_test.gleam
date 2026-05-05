import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{Some}
import gleam/string
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/metaobject_definitions
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity

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
      empty_upstream_context(),
    )
  outcome
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
      empty_upstream_context(),
      dict.from_list([
        #(app_identity.api_client_id_header, test_api_client_id),
      ]),
    )
  outcome
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

fn int_range(from from: Int, to to: Int) -> List(Int) {
  case from > to {
    True -> []
    False -> [from, ..int_range(from + 1, to)]
  }
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

pub fn definition_create_rejects_invalid_field_key_test() {
  let outcome =
    run_mutation(
      store.new(),
      synthetic_identity.new(),
      create_definition_with_field_key_query("codex_rows_invalid_key", "Title"),
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"metaobjectDefinitionCreate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"key\"],\"message\":\"is invalid\",\"code\":\"INVALID\",\"elementKey\":\"Title\",\"elementIndex\":0}]}}}"
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

  let invalid_key_update =
    run_mutation(
      created.store,
      created.identity,
      "mutation {
        metaobjectDefinitionUpdate(
          id: \"gid://shopify/MetaobjectDefinition/1?shopify-draft-proxy=synthetic\",
          definition: { fieldDefinitions: [{ update: { key: \"BadKey\", name: \"Bad\" } }] }
        ) {
          metaobjectDefinition { id type fieldDefinitions { key name } }
          userErrors { field message code elementKey elementIndex }
        }
      }",
    )
  assert json.to_string(invalid_key_update.data)
    == "{\"data\":{\"metaobjectDefinitionUpdate\":{\"metaobjectDefinition\":null,\"userErrors\":[{\"field\":[\"definition\",\"fieldDefinitions\",\"0\",\"key\"],\"message\":\"is invalid\",\"code\":\"INVALID\",\"elementKey\":\"BadKey\",\"elementIndex\":0}]}}}"
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
