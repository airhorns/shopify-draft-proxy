import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/string
import shopify_draft_proxy/proxy/metafield_definitions
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity

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
