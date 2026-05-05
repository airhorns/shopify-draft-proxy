import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/metafield_definitions
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
      body: "{\"query\":\"" <> escape(query) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
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
