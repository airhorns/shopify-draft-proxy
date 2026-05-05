import gleam/dict
import gleam/json
import shopify_draft_proxy/proxy/metaobject_definitions
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity

const path = "/admin/api/2026-04/graphql.json"

fn run_query(s: store.Store, query: String) -> String {
  let assert Ok(data) = metaobject_definitions.process(s, query, dict.new())
  json.to_string(data)
}

fn run_mutation(
  s: store.Store,
  identity: synthetic_identity.SyntheticIdentityRegistry,
  query: String,
) -> mutation_helpers.MutationOutcome {
  let assert Ok(outcome) =
    metaobject_definitions.process_mutation(
      s,
      identity,
      path,
      query,
      dict.new(),
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
