import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/draft_proxy.{type Request}
import shopify_draft_proxy/proxy/proxy_state.{
  Config, PassthroughUnsupportedMutations, Request, Response, Snapshot,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/types.{
  type CollectionRecord, type CollectionRuleSetRecord, type InventoryLevelRecord,
  type InventoryQuantityRecord, type InventoryTransferRecord,
  type LocationRecord, type MetafieldDefinitionCapabilitiesRecord,
  type MetafieldDefinitionCapabilityRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionValidationRecord, type ProductMediaRecord,
  type ProductRecord, type ProductVariantRecord, type SellingPlanGroupRecord,
  CollectionRecord, CollectionRuleRecord, CollectionRuleSetRecord,
  InventoryItemRecord, InventoryLevelRecord, InventoryLocationRecord,
  InventoryQuantityRecord, InventoryTransferLineItemRecord,
  InventoryTransferRecord, LocationRecord, MetafieldDefinitionCapabilitiesRecord,
  MetafieldDefinitionCapabilityRecord, MetafieldDefinitionRecord,
  MetafieldDefinitionTypeRecord, MetafieldDefinitionValidationRecord,
  ProductCollectionRecord, ProductMediaRecord, ProductMetafieldRecord,
  ProductOptionRecord, ProductOptionValueRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ProductVariantSelectedOptionRecord,
  SellingPlanGroupRecord,
}

fn empty_headers() -> dict.Dict(String, String) {
  dict.new()
}

fn graphql_request(query: String) -> Request {
  graphql_request_for_version(query, "2025-01")
}

fn graphql_request_for_version(query: String, api_version: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/" <> api_version <> "/graphql.json",
    headers: empty_headers(),
    body: "{\"query\":\"" <> query <> "\"}",
  )
}

fn graphql_request_body(body: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: empty_headers(),
    body: body,
  )
}

fn graphql_request_with_variables(
  query: String,
  variables: json.Json,
) -> Request {
  graphql_request_body(
    json.to_string(
      json.object([
        #("query", json.string(query)),
        #("variables", variables),
      ]),
    ),
  )
}

fn run_product_mutation(initial_store: store.Store, query: String) {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: initial_store)
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  #(status, json.to_string(body), next_proxy)
}

fn assert_product_option_user_error(
  initial_store: store.Store,
  query: String,
  code: String,
  field_json: String,
) {
  let #(status, body, next_proxy) = run_product_mutation(initial_store, query)

  assert status == 200
  assert string.contains(body, "\"code\":\"" <> code <> "\"")
  assert string.contains(body, "\"field\":" <> field_json)
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
}

fn assert_combined_listing_user_error(
  initial_store: store.Store,
  query: String,
  code: String,
) {
  let #(status, body, next_proxy) = run_product_mutation(initial_store, query)

  assert status == 200
  assert string.contains(body, "\"code\":\"" <> code <> "\"")
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
}

fn assert_product_variant_media_user_error(
  initial_store: store.Store,
  query: String,
  code: String,
  field_json: String,
  message: String,
) {
  let #(status, body, next_proxy) = run_product_mutation(initial_store, query)

  assert status == 200
  assert string.contains(body, "\"code\":\"" <> code <> "\"")
  assert string.contains(body, "\"field\":" <> field_json)
  assert string.contains(body, "\"message\":\"" <> message <> "\"")
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
}

pub fn product_variant_append_media_rejects_variant_from_other_product_test() {
  assert_product_variant_media_user_error(
    variant_media_validation_store(),
    "mutation { productVariantAppendMedia(productId: \\\"gid://shopify/Product/optioned\\\", variantMedia: [{ variantId: \\\"gid://shopify/ProductVariant/child\\\", mediaIds: [\\\"gid://shopify/MediaImage/ready\\\"] }]) { productVariants { id } userErrors { field message code } } }",
    "PRODUCT_VARIANT_DOES_NOT_EXIST_ON_PRODUCT",
    "[\"variantMedia\",\"0\",\"variantId\"]",
    "Variant does not exist on the specified product.",
  )
}

pub fn product_variant_append_media_rejects_media_from_other_product_test() {
  assert_product_variant_media_user_error(
    variant_media_validation_store(),
    "mutation { productVariantAppendMedia(productId: \\\"gid://shopify/Product/optioned\\\", variantMedia: [{ variantId: \\\"gid://shopify/ProductVariant/default\\\", mediaIds: [\\\"gid://shopify/MediaImage/child\\\"] }]) { productVariants { id } userErrors { field message code } } }",
    "MEDIA_DOES_NOT_EXIST_ON_PRODUCT",
    "[\"variantMedia\",\"0\",\"mediaIds\"]",
    "Media does not exist on the specified product.",
  )
}

pub fn product_variant_append_media_rejects_processing_media_test() {
  assert_product_variant_media_user_error(
    variant_media_validation_store(),
    "mutation { productVariantAppendMedia(productId: \\\"gid://shopify/Product/optioned\\\", variantMedia: [{ variantId: \\\"gid://shopify/ProductVariant/default\\\", mediaIds: [\\\"gid://shopify/MediaImage/processing\\\"] }]) { productVariants { id } userErrors { field message code } } }",
    "NON_READY_MEDIA",
    "[\"variantMedia\",\"0\",\"mediaIds\"]",
    "Non-ready media cannot be attached to variants.",
  )
}

pub fn product_variant_detach_media_rejects_unattached_media_test() {
  assert_product_variant_media_user_error(
    variant_media_validation_store(),
    "mutation { productVariantDetachMedia(productId: \\\"gid://shopify/Product/optioned\\\", variantMedia: [{ variantId: \\\"gid://shopify/ProductVariant/default\\\", mediaIds: [\\\"gid://shopify/MediaImage/ready\\\"] }]) { productVariants { id } userErrors { field message code } } }",
    "MEDIA_IS_NOT_ATTACHED_TO_VARIANT",
    "[\"variantMedia\",\"0\",\"variantId\"]",
    "The specified media is not attached to the specified variant.",
  )
}

fn assert_selling_plan_group_create_user_error(
  query: String,
  code: String,
  field_json: String,
  message: String,
) {
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"sellingPlanGroup\":null")
  assert string.contains(serialized, "\"code\":\"" <> code <> "\"")
  assert string.contains(serialized, "\"field\":" <> field_json)
  assert string.contains(serialized, "\"message\":\"" <> message <> "\"")
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

fn assert_variant_relationship_user_error(
  query: String,
  code: String,
  field_json: String,
  message: String,
) {
  let #(status, body, next_proxy) =
    run_product_mutation(variant_relationship_store(), query)

  assert status == 200
  assert string.contains(body, "\"parentProductVariants\":null")
  assert string.contains(body, "\"code\":\"" <> code <> "\"")
  assert string.contains(body, "\"field\":" <> field_json)
  assert string.contains(body, "\"message\":\"" <> message <> "\"")
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn publication_create_rejects_target_validation_errors_test() {
  assert_publication_user_error(
    "mutation { publicationCreate(input: { catalogId: \\\"gid://shopify/MarketCatalog/999\\\", channelId: \\\"gid://shopify/Channel/999\\\" }) { publication { id } userErrors { field message code } } }",
    "publicationCreate",
    "INVALID",
    "[\"input\"]",
    "Only one of catalog or channel can be provided",
  )
  assert_publication_user_error(
    "mutation { publicationCreate(input: {}) { publication { id } userErrors { field message code } } }",
    "publicationCreate",
    "BLANK",
    "[\"input\",\"catalogId\"]",
    "Catalog can't be blank",
  )
  assert_publication_user_error(
    "mutation { publicationCreate(input: { catalogId: \\\"gid://shopify/MarketCatalog/999\\\" }) { publication { id } userErrors { field message code } } }",
    "publicationCreate",
    "NOT_FOUND",
    "[\"input\",\"catalogId\"]",
    "Catalog not found",
  )
  assert_publication_user_error(
    "mutation { publicationCreate(input: { channelId: \\\"gid://shopify/Channel/999\\\" }) { publication { id } userErrors { field message code } } }",
    "publicationCreate",
    "NOT_FOUND",
    "[\"input\",\"channelId\"]",
    "Channel not found",
  )
}

pub fn publication_update_rejects_target_validation_errors_test() {
  let create_query =
    "mutation { publicationCreate(input: { name: \\\"Seed\\\" }) { publication { id } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(create_query),
    )
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let update_both_query =
    "mutation { publicationUpdate(id: \\\"gid://shopify/Publication/2\\\", input: { catalogId: \\\"gid://shopify/MarketCatalog/999\\\", channelId: \\\"gid://shopify/Channel/999\\\" }) { publication { id } userErrors { field message code } } }"
  let #(Response(status: both_status, body: both_body, ..), both_proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_both_query))
  let both_serialized = json.to_string(both_body)
  assert both_status == 200
  assert string.contains(both_serialized, "\"publication\":null")
  assert string.contains(both_serialized, "\"code\":\"INVALID\"")
  assert string.contains(both_serialized, "\"field\":[\"input\"]")
  assert string.contains(
    both_serialized,
    "\"message\":\"Only one of catalog or channel can be provided\"",
  )

  let update_catalog_query =
    "mutation { publicationUpdate(id: \\\"gid://shopify/Publication/2\\\", input: { catalogId: \\\"gid://shopify/MarketCatalog/999\\\" }) { publication { id } userErrors { field message code } } }"
  let #(Response(status: catalog_status, body: catalog_body, ..), next_proxy) =
    draft_proxy.process_request(
      both_proxy,
      graphql_request(update_catalog_query),
    )
  let catalog_serialized = json.to_string(catalog_body)
  assert catalog_status == 200
  assert string.contains(catalog_serialized, "\"publication\":null")
  assert string.contains(catalog_serialized, "\"code\":\"NOT_FOUND\"")
  assert string.contains(
    catalog_serialized,
    "\"field\":[\"input\",\"catalogId\"]",
  )

  let assert [_, both_entry, catalog_entry] = store.get_log(next_proxy.store)
  assert both_entry.status == store_types.Failed
  assert catalog_entry.status == store_types.Failed
}

pub fn publication_delete_rejects_default_online_store_publication_test() {
  let query =
    "mutation { publicationDelete(id: \\\"gid://shopify/Publication/1\\\") { deletedId userErrors { field message code } } }"
  let #(status, body, next_proxy) = run_product_mutation(store.new(), query)

  assert status == 200
  assert string.contains(body, "\"deletedId\":null")
  assert string.contains(body, "\"code\":\"CANNOT_DELETE_DEFAULT_PUBLICATION\"")
  assert string.contains(body, "\"field\":[\"id\"]")
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn publication_create_rejects_invalid_default_state_enum_test() {
  let body =
    json.to_string(
      json.object([
        #(
          "query",
          json.string(
            "mutation($input: PublicationCreateInput!) { publicationCreate(input: $input) { publication { id } userErrors { field message code } } }",
          ),
        ),
        #(
          "variables",
          json.object([
            #("input", json.object([#("defaultState", json.string("BANANAS"))])),
          ]),
        ),
      ]),
    )
  let #(Response(status: status, body: response_body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request_body(body))
  let serialized = json.to_string(response_body)

  assert status == 200
  assert string.contains(serialized, "\"errors\"")
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    serialized,
    "Expected \\\"BANANAS\\\" to be one of: EMPTY, ALL_PRODUCTS",
  )
  assert store.get_log(next_proxy.store) == []
}

fn assert_publication_user_error(
  query: String,
  root_name: String,
  code: String,
  field_json: String,
  message: String,
) {
  let #(status, body, next_proxy) = run_product_mutation(store.new(), query)

  assert status == 200
  assert string.contains(body, "\"" <> root_name <> "\":{")
  assert string.contains(body, "\"publication\":null")
  assert string.contains(body, "\"code\":\"" <> code <> "\"")
  assert string.contains(body, "\"field\":" <> field_json)
  assert string.contains(body, "\"message\":\"" <> message <> "\"")
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

fn graphql_document_request(query: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: empty_headers(),
    body: json.to_string(json.object([#("query", json.string(query))])),
  )
}

pub fn product_full_sync_unknown_feed_returns_not_found_test() {
  let query =
    "mutation ProductFullSyncUnknown($id: ID!) { productFullSync(id: $id) { id userErrors { field message code } } }"
  let variables =
    json.object([
      #("id", json.string("gid://shopify/ProductFeed/999999999")),
    ])
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request_with_variables(query, variables),
    )

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productFullSync\":{\"id\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"ProductFeed does not exist\",\"code\":\"NOT_FOUND\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_full_sync_returns_pollable_job_test() {
  let create_query =
    "mutation ProductFeedCreateForSync($input: ProductFeedInput) { productFeedCreate(input: $input) { productFeed { id } userErrors { field message code } } }"
  let create_variables =
    json.object([
      #(
        "input",
        json.object([
          #("country", json.string("US")),
          #("language", json.string("EN")),
        ]),
      ),
    ])
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request_with_variables(create_query, create_variables),
    )

  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productFeedCreate\":{\"productFeed\":{\"id\":\"gid://shopify/ProductFeed/US-EN\"},\"userErrors\":[]}}}"

  let sync_query =
    "mutation ProductFullSyncJob($id: ID!) { productFullSync(id: $id) { __typename id job { __typename id done query { __typename } } userErrors { field message code } } }"
  let sync_variables =
    json.object([#("id", json.string("gid://shopify/ProductFeed/US-EN"))])
  let #(Response(status: sync_status, body: sync_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request_with_variables(sync_query, sync_variables),
    )

  assert sync_status == 200
  assert json.to_string(sync_body)
    == "{\"data\":{\"productFullSync\":{\"__typename\":\"ProductFullSyncPayload\",\"id\":\"gid://shopify/ProductFeed/US-EN\",\"job\":{\"__typename\":\"Job\",\"id\":\"gid://shopify/Job/2\",\"done\":false,\"query\":{\"__typename\":\"QueryRoot\"}},\"userErrors\":[]}}}"

  let job_query =
    "query ProductFullSyncJobPoll($id: ID!) { job(id: $id) { __typename id done query { __typename } } }"
  let job_variables = json.object([#("id", json.string("gid://shopify/Job/2"))])
  let #(Response(status: job_status, body: job_body, ..), next_proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request_with_variables(job_query, job_variables),
    )

  assert job_status == 200
  assert json.to_string(job_body)
    == "{\"data\":{\"job\":{\"__typename\":\"Job\",\"id\":\"gid://shopify/Job/2\",\"done\":false,\"query\":{\"__typename\":\"QueryRoot\"}}}}"
  let assert [_, sync_entry] = store.get_log(next_proxy.store)
  assert sync_entry.staged_resource_ids
    == ["gid://shopify/ProductFeed/US-EN", "gid://shopify/Job/2"]
}

fn valid_selling_plan_input() -> String {
  "name: \\\"Monthly delivery\\\", options: [\\\"Monthly\\\"], position: 1, category: SUBSCRIPTION, billingPolicy: { recurring: { interval: MONTH, intervalCount: 1, minCycles: 1, maxCycles: 12 } }, deliveryPolicy: { recurring: { interval: MONTH, intervalCount: 1, cutoff: 0 } }, inventoryPolicy: { reserve: ON_FULFILLMENT }, pricingPolicies: [{ fixed: { adjustmentType: PERCENTAGE, adjustmentValue: { percentage: 10 } } }]"
}

pub fn product_feedback_invalid_state_uses_resource_feedback_enum_coercion_test() {
  let query =
    "mutation { bulkProductResourceFeedbackCreate(feedbackInput: [{ productId: \\\"gid://shopify/Product/optioned\\\", state: BANANAS, feedbackGeneratedAt: \\\"2024-01-01T00:00:00Z\\\", productUpdatedAt: \\\"2024-01-01T00:00:00Z\\\", messages: [] }]) { feedback { productId } userErrors { field message code } } }"
  let #(status, body, next_proxy) =
    run_product_mutation(default_option_store(), query)

  assert status == 200
  assert string.contains(
    body,
    "Expected \\\"BANANAS\\\" to be one of: ACCEPTED, REQUIRES_ACTION",
  )
  assert string.contains(body, "\"code\":\"argumentLiteralsIncompatible\"")
  assert store.get_log(next_proxy.store) == []
}

pub fn shop_feedback_invalid_state_uses_resource_feedback_enum_coercion_test() {
  let query =
    "mutation { shopResourceFeedbackCreate(input: { state: BANANAS, feedbackGeneratedAt: \\\"2024-01-01T00:00:00Z\\\", messages: [] }) { feedback { state } userErrors { field message code } } }"
  let #(status, body, next_proxy) = run_product_mutation(store.new(), query)

  assert status == 200
  assert string.contains(
    body,
    "Expected \\\"BANANAS\\\" to be one of: ACCEPTED, REQUIRES_ACTION",
  )
  assert string.contains(body, "\"code\":\"argumentLiteralsIncompatible\"")
  assert store.get_log(next_proxy.store) == []
}

pub fn product_feedback_create_rejects_validation_errors_without_staging_test() {
  let too_long_message = string.repeat("x", times: 101)
  let batch_input =
    string.repeat(
      "{ productId: \\\"gid://shopify/Product/optioned\\\", state: ACCEPTED, feedbackGeneratedAt: \\\"2024-01-01T00:00:00Z\\\", productUpdatedAt: \\\"2024-01-01T00:00:00Z\\\", messages: [] },",
      times: 51,
    )
  let query =
    "mutation { blankMessages: bulkProductResourceFeedbackCreate(feedbackInput: [{ productId: \\\"gid://shopify/Product/optioned\\\", state: REQUIRES_ACTION, feedbackGeneratedAt: \\\"2024-01-01T00:00:00Z\\\", productUpdatedAt: \\\"2024-01-01T00:00:00Z\\\", messages: [] }]) { feedback { productId } userErrors { field message code } } futureGeneratedAt: bulkProductResourceFeedbackCreate(feedbackInput: [{ productId: \\\"gid://shopify/Product/optioned\\\", state: ACCEPTED, feedbackGeneratedAt: \\\"2099-01-01T00:00:00Z\\\", productUpdatedAt: \\\"2024-01-01T00:00:00Z\\\", messages: [] }]) { feedback { productId } userErrors { field message code } } tooLongMessage: bulkProductResourceFeedbackCreate(feedbackInput: [{ productId: \\\"gid://shopify/Product/optioned\\\", state: REQUIRES_ACTION, feedbackGeneratedAt: \\\"2024-01-01T00:00:00Z\\\", productUpdatedAt: \\\"2024-01-01T00:00:00Z\\\", messages: [\\\""
    <> too_long_message
    <> "\\\"] }]) { feedback { productId } userErrors { field message code } } batchTooLong: bulkProductResourceFeedbackCreate(feedbackInput: ["
    <> batch_input
    <> "]) { feedback { productId } userErrors { field message code } } }"
  let #(status, body, next_proxy) =
    run_product_mutation(default_option_store(), query)

  assert status == 200
  assert string.contains(
    body,
    "\"blankMessages\":{\"feedback\":[],\"userErrors\":[{\"field\":[\"feedback\",\"0\",\"messages\"],\"message\":\"Messages can't be blank\",\"code\":\"BLANK\"}]}",
  )
  assert string.contains(
    body,
    "\"futureGeneratedAt\":{\"feedback\":[],\"userErrors\":[{\"field\":[\"feedback\",\"0\",\"feedbackGeneratedAt\"],\"message\":\"Feedback generated at must not be in the future\",\"code\":\"INVALID\"}]}",
  )
  assert string.contains(
    body,
    "\"tooLongMessage\":{\"feedback\":[],\"userErrors\":[{\"field\":[\"feedback\",\"0\",\"messages\",\"0\"],\"message\":\"Message is too long (maximum is 100 characters)\",\"code\":\"TOO_LONG\"}]}",
  )
  assert string.contains(
    body,
    "\"batchTooLong\":{\"feedback\":[],\"userErrors\":[{\"field\":[\"feedback\"],\"message\":\"Feedback cannot contain more than 50 entries\",\"code\":\"TOO_LONG\"}]}",
  )
  assert store.get_effective_product_resource_feedback(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
    == None
}

pub fn shop_feedback_create_rejects_validation_errors_without_staging_test() {
  let too_long_message = string.repeat("x", times: 101)
  let query =
    "mutation { blankMessages: shopResourceFeedbackCreate(input: { state: REQUIRES_ACTION, feedbackGeneratedAt: \\\"2024-01-01T00:00:00Z\\\", messages: [] }) { feedback { state } userErrors { field message code } } futureGeneratedAt: shopResourceFeedbackCreate(input: { state: ACCEPTED, feedbackGeneratedAt: \\\"2099-01-01T00:00:00Z\\\", messages: [] }) { feedback { state } userErrors { field message code } } tooLongMessage: shopResourceFeedbackCreate(input: { state: REQUIRES_ACTION, feedbackGeneratedAt: \\\"2024-01-01T00:00:00Z\\\", messages: [\\\""
    <> too_long_message
    <> "\\\"] }) { feedback { state } userErrors { field message code } } }"
  let #(status, body, next_proxy) = run_product_mutation(store.new(), query)

  assert status == 200
  assert string.contains(
    body,
    "\"blankMessages\":{\"feedback\":null,\"userErrors\":[{\"field\":[\"feedback\",\"messages\"],\"message\":\"Messages can't be blank\",\"code\":\"BLANK\"}]}",
  )
  assert string.contains(
    body,
    "\"futureGeneratedAt\":{\"feedback\":null,\"userErrors\":[{\"field\":[\"feedback\",\"feedbackGeneratedAt\"],\"message\":\"Feedback generated at must not be in the future\",\"code\":\"INVALID\"}]}",
  )
  assert string.contains(
    body,
    "\"tooLongMessage\":{\"feedback\":null,\"userErrors\":[{\"field\":[\"feedback\",\"messages\",\"0\"],\"message\":\"Message is too long (maximum is 100 characters)\",\"code\":\"TOO_LONG\"}]}",
  )
  assert store.get_log(next_proxy.store) != []
}

pub fn selling_plan_group_create_rejects_group_input_validation_errors_test() {
  assert_selling_plan_group_create_user_error(
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Too many\\\", options: [\\\"a\\\", \\\"b\\\", \\\"c\\\", \\\"d\\\"] }, resources: {}) { sellingPlanGroup { id options } userErrors { field message code } } }",
    "TOO_LONG",
    "[\"input\",\"options\"]",
    "Too many selling plan group options (maximum 3 options)",
  )
  assert_selling_plan_group_create_user_error(
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Bad position\\\", position: 9999999999 }, resources: {}) { sellingPlanGroup { id position } userErrors { field message code } } }",
    "INVALID",
    "[\"input\",\"position\"]",
    "Position must be within the range of -2,147,483,648 to 2,147,483,647",
  )
}

pub fn selling_plan_group_create_rejects_nested_plan_validation_errors_test() {
  let base_plan = valid_selling_plan_input()
  let recurring_policy =
    "{ recurring: { adjustmentType: PERCENTAGE, adjustmentValue: { percentage: 5 }, afterCycle: 2 } }"
  assert_selling_plan_group_create_user_error(
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Too many plan options\\\", options: [\\\"Delivery\\\"], sellingPlansToCreate: [{ "
      <> base_plan
      <> ", options: [\\\"a\\\", \\\"b\\\", \\\"c\\\", \\\"d\\\"] }] }, resources: {}) { sellingPlanGroup { id } userErrors { field message code } } }",
    "TOO_LONG",
    "[\"input\",\"sellingPlansToCreate\",\"0\",\"options\"]",
    "Too many selling plan options (maximum 3 options)",
  )
  assert_selling_plan_group_create_user_error(
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Too many pricing policies\\\", options: [\\\"Delivery\\\"], sellingPlansToCreate: [{ "
      <> base_plan
      <> ", pricingPolicies: [{ fixed: { adjustmentType: PERCENTAGE, adjustmentValue: { percentage: 10 } } }, "
      <> recurring_policy
      <> ", "
      <> recurring_policy
      <> "] }] }, resources: {}) { sellingPlanGroup { id } userErrors { field message code } } }",
    "SELLING_PLAN_PRICING_POLICIES_LIMIT",
    "[\"input\",\"sellingPlansToCreate\",\"0\",\"pricingPolicies\"]",
    "Selling plans to create pricing policies can't have more than 2 pricing policies",
  )
  assert_selling_plan_group_create_user_error(
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Bad plan position\\\", options: [\\\"Delivery\\\"], sellingPlansToCreate: [{ "
      <> base_plan
      <> ", position: 9999999999 }] }, resources: {}) { sellingPlanGroup { id } userErrors { field message code } } }",
    "INVALID",
    "[\"input\",\"sellingPlansToCreate\",\"0\",\"position\"]",
    "Position must be within the range of -2,147,483,648 to 2,147,483,647",
  )
  assert_selling_plan_group_create_user_error(
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Policy mismatch\\\", options: [\\\"Delivery\\\"], sellingPlansToCreate: [{ "
      <> base_plan
      <> ", deliveryPolicy: { fixed: { fulfillmentTrigger: ASAP } } }] }, resources: {}) { sellingPlanGroup { id } userErrors { field message code } } }",
    "BILLING_AND_DELIVERY_POLICY_TYPES_MUST_BE_THE_SAME",
    "[\"input\",\"sellingPlansToCreate\",\"0\"]",
    "billing and delivery policy types must be the same.",
  )
  assert_selling_plan_group_create_user_error(
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Zero interval\\\", options: [\\\"Delivery\\\"], sellingPlansToCreate: [{ name: \\\"Monthly delivery\\\", options: [\\\"Monthly\\\"], position: 1, category: SUBSCRIPTION, billingPolicy: { recurring: { interval: MONTH, intervalCount: 0, minCycles: 1, maxCycles: 12 } }, deliveryPolicy: { recurring: { interval: MONTH, intervalCount: 0, cutoff: 0 } }, inventoryPolicy: { reserve: ON_FULFILLMENT }, pricingPolicies: [{ fixed: { adjustmentType: PERCENTAGE, adjustmentValue: { percentage: 10 } } }] }] }, resources: {}) { sellingPlanGroup { id } userErrors { field message code } } }",
    "GREATER_THAN",
    "[\"input\",\"sellingPlansToCreate\",\"0\",\"billingPolicy\",\"recurring\",\"intervalCount\"]",
    "Interval count must be greater than 0",
  )
  assert_selling_plan_group_create_user_error(
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Anchor mismatch\\\", options: [\\\"Delivery\\\"], sellingPlansToCreate: [{ name: \\\"Monthly delivery\\\", options: [\\\"Monthly\\\"], position: 1, category: SUBSCRIPTION, billingPolicy: { recurring: { interval: MONTH, intervalCount: 1, anchors: [{ type: MONTHDAY, day: 1 }], minCycles: 1, maxCycles: 12 } }, deliveryPolicy: { recurring: { interval: MONTH, intervalCount: 1, anchors: [{ type: MONTHDAY, day: 2 }], cutoff: 0 } }, inventoryPolicy: { reserve: ON_FULFILLMENT }, pricingPolicies: [{ fixed: { adjustmentType: PERCENTAGE, adjustmentValue: { percentage: 10 } } }] }] }, resources: {}) { sellingPlanGroup { id } userErrors { field message code } } }",
    "SELLING_PLAN_BILLING_AND_DELIVERY_POLICY_ANCHORS_MUST_BE_EQUAL",
    "[\"input\",\"sellingPlansToCreate\",\"0\"]",
    "Billing and delivery policy anchors must be the same",
  )
}

pub fn selling_plan_group_update_rejects_input_validation_errors_test() {
  let create_query =
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Seed\\\", options: [\\\"Delivery\\\"], sellingPlansToCreate: [{ "
    <> valid_selling_plan_input()
    <> " }] }) { sellingPlanGroup { id } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(create_query),
    )
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")
  let assert [group] = store.list_effective_selling_plan_groups(proxy.store)

  let update_query =
    "mutation { sellingPlanGroupUpdate(id: \\\""
    <> group.id
    <> "\\\", input: { options: [\\\"a\\\", \\\"b\\\", \\\"c\\\", \\\"d\\\"], sellingPlansToUpdate: [{ name: \\\"Missing id\\\" }] }) { deletedSellingPlanIds sellingPlanGroup { id } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  let serialized = json.to_string(update_body)

  assert update_status == 200
  assert string.contains(serialized, "\"deletedSellingPlanIds\":null")
  assert string.contains(serialized, "\"sellingPlanGroup\":null")
  assert string.contains(serialized, "\"code\":\"TOO_LONG\"")
  assert string.contains(serialized, "\"field\":[\"input\",\"options\"]")
  assert string.contains(
    serialized,
    "\"code\":\"PLAN_ID_MUST_BE_SPECIFIED_TO_UPDATE\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"sellingPlansToUpdate\",\"0\",\"id\"]",
  )
  assert string.contains(
    serialized,
    "\"message\":\"Id must be specificed to update a Selling Plan.\"",
  )
  let assert [create_entry, update_entry] = store.get_log(next_proxy.store)
  assert create_entry.status == store_types.Staged
  assert update_entry.status == store_types.Failed
  assert update_entry.staged_resource_ids == []
}

pub fn product_variant_relationship_bulk_update_rejects_parent_child_validation_errors_test() {
  assert_variant_relationship_user_error(
    "mutation { productVariantRelationshipBulkUpdate(input: [{ parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToCreate: [{ id: \\\"gid://shopify/ProductVariant/default\\\", quantity: 1 }] }]) { parentProductVariants { id } userErrors { field message code } } }",
    "CIRCULAR_REFERENCE",
    "[\"input\"]",
    "A parent product variant cannot contain itself as a component.",
  )
  assert_variant_relationship_user_error(
    "mutation { productVariantRelationshipBulkUpdate(input: [{ parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToCreate: [{ id: \\\"gid://shopify/ProductVariant/child\\\", quantity: 0 }] }]) { parentProductVariants { id } userErrors { field message code } } }",
    "INVALID",
    "[\"input\",\"0\",\"productVariantRelationshipsToCreate\",\"0\",\"quantity\"]",
    "Quantity must be greater than or equal to 1",
  )
  assert_variant_relationship_user_error(
    "mutation { productVariantRelationshipBulkUpdate(input: [{ parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToCreate: [{ id: \\\"gid://shopify/ProductVariant/child\\\", quantity: 10000 }] }]) { parentProductVariants { id } userErrors { field message code } } }",
    "INVALID",
    "[\"input\",\"0\",\"productVariantRelationshipsToCreate\",\"0\",\"quantity\"]",
    "Quantity must be less than or equal to 9999",
  )
  assert_variant_relationship_user_error(
    "mutation { productVariantRelationshipBulkUpdate(input: [{ parentProductId: \\\"gid://shopify/Product/optioned\\\", parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToCreate: [{ id: \\\"gid://shopify/ProductVariant/child\\\", quantity: 1 }] }]) { parentProductVariants { id } userErrors { field message code } } }",
    "INVALID_INPUT",
    "[\"input\",\"0\"]",
    "Only one of parentProductId or parentProductVariantId can be specified.",
  )
  assert_variant_relationship_user_error(
    "mutation { productVariantRelationshipBulkUpdate(input: [{ parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToCreate: [{ id: \\\"gid://shopify/ProductVariant/child\\\", quantity: 1 }, { id: \\\"gid://shopify/ProductVariant/child\\\", quantity: 2 }] }]) { parentProductVariants { id } userErrors { field message code } } }",
    "CANNOT_HAVE_DUPLICATED_PRODUCTS",
    "[\"input\",\"0\",\"productVariantRelationshipsToCreate\",\"1\",\"id\"]",
    "cannot_have_duplicated_products",
  )
  assert_variant_relationship_user_error(
    "mutation { productVariantRelationshipBulkUpdate(input: [{ parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToCreate: [{ id: \\\"gid://shopify/ProductVariant/child\\\", quantity: 1 }] }, { parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToCreate: [{ id: \\\"gid://shopify/ProductVariant/child\\\", quantity: 1 }] }]) { parentProductVariants { id } userErrors { field message code } } }",
    "CANNOT_HAVE_DUPLICATED_PRODUCTS",
    "[\"input\",\"1\"]",
    "cannot_have_duplicated_products",
  )
  assert_variant_relationship_user_error(
    "mutation { productVariantRelationshipBulkUpdate(input: [{ parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToUpdate: [{ id: \\\"gid://shopify/ProductVariant/child\\\", quantity: 1 }] }]) { parentProductVariants { id } userErrors { field message code } } }",
    "NOT_A_CHILD",
    "[\"input\",\"0\",\"productVariantRelationshipsToUpdate\",\"0\",\"id\"]",
    "not_a_child",
  )
  assert_variant_relationship_user_error(
    "mutation { productVariantRelationshipBulkUpdate(input: [{ parentProductVariantId: \\\"gid://shopify/ProductVariant/default\\\", productVariantRelationshipsToRemove: [\\\"gid://shopify/ProductVariant/child\\\"] }]) { parentProductVariants { id } userErrors { field message code } } }",
    "NOT_A_CHILD",
    "[\"input\",\"0\",\"productVariantRelationshipsToRemove\",\"0\"]",
    "not_a_child",
  )
}

pub fn product_join_selling_plan_groups_rejects_empty_ids_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productJoinSellingPlanGroups(id: \\\"gid://shopify/Product/optioned\\\", sellingPlanGroupIds: []) { product { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"field\":[\"sellingPlanGroupIds\"]")
  assert string.contains(serialized, "\"code\":\"BLANK\"")
  assert string.contains(
    serialized,
    "\"message\":\"Selling plan group IDs can't be blank\"",
  )
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_join_selling_plan_groups_rejects_duplicate_ids_without_staging_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store([
        selling_plan_group("gid://shopify/SellingPlanGroup/one", [], []),
      ]),
    )
  let query =
    "mutation { productJoinSellingPlanGroups(id: \\\"gid://shopify/Product/optioned\\\", sellingPlanGroupIds: [\\\"gid://shopify/SellingPlanGroup/one\\\", \\\"gid://shopify/SellingPlanGroup/one\\\"]) { product { id sellingPlanGroups(first: 10) { nodes { id } } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"field\":[\"sellingPlanGroupIds\"]")
  assert string.contains(serialized, "\"code\":\"DUPLICATE\"")
  assert string.contains(serialized, "\"nodes\":[]")
  let assert Some(group) =
    store.get_effective_selling_plan_group_by_id(
      next_proxy.store,
      "gid://shopify/SellingPlanGroup/one",
    )
  assert group.product_ids == []
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_variant_join_selling_plan_groups_rejects_duplicate_ids_without_staging_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store([
        selling_plan_group("gid://shopify/SellingPlanGroup/one", [], []),
      ]),
    )
  let query =
    "mutation { productVariantJoinSellingPlanGroups(id: \\\"gid://shopify/ProductVariant/default\\\", sellingPlanGroupIds: [\\\"gid://shopify/SellingPlanGroup/one\\\", \\\"gid://shopify/SellingPlanGroup/one\\\"]) { productVariant { id sellingPlanGroups(first: 10) { nodes { id } } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"field\":[\"sellingPlanGroupIds\"]")
  assert string.contains(serialized, "\"code\":\"DUPLICATE\"")
  assert string.contains(serialized, "\"nodes\":[]")
  let assert Some(group) =
    store.get_effective_selling_plan_group_by_id(
      next_proxy.store,
      "gid://shopify/SellingPlanGroup/one",
    )
  assert group.product_variant_ids == []
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_join_selling_plan_groups_enforces_31_group_cap_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store(numbered_selling_plan_groups(32)),
    )
  let ids =
    numbered_selling_plan_group_ids(32)
    |> list.map(fn(id) { "\\\"" <> id <> "\\\"" })
    |> string.join(", ")
  let query =
    "mutation { productJoinSellingPlanGroups(id: \\\"gid://shopify/Product/optioned\\\", sellingPlanGroupIds: ["
    <> ids
    <> "]) { product { id sellingPlanGroupsCount { count precision } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(
    serialized,
    "\"code\":\"SELLING_PLAN_GROUPS_TOO_MANY\"",
  )
  assert store.list_effective_selling_plan_groups_for_product(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
    == []
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_leave_selling_plan_groups_rejects_non_member_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store([
        selling_plan_group("gid://shopify/SellingPlanGroup/one", [], []),
      ]),
    )
  let query =
    "mutation { productLeaveSellingPlanGroups(id: \\\"gid://shopify/Product/optioned\\\", sellingPlanGroupIds: [\\\"gid://shopify/SellingPlanGroup/one\\\"]) { product { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"field\":[\"sellingPlanGroupIds\"]")
  assert string.contains(serialized, "\"code\":\"NOT_A_MEMBER\"")
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_variant_join_selling_plan_groups_enforces_31_group_cap_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store(numbered_selling_plan_groups(32)),
    )
  let ids =
    numbered_selling_plan_group_ids(32)
    |> list.map(fn(id) { "\\\"" <> id <> "\\\"" })
    |> string.join(", ")
  let query =
    "mutation { productVariantJoinSellingPlanGroups(id: \\\"gid://shopify/ProductVariant/default\\\", sellingPlanGroupIds: ["
    <> ids
    <> "]) { productVariant { id sellingPlanGroupsCount { count precision } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(
    serialized,
    "\"code\":\"SELLING_PLAN_GROUPS_TOO_MANY\"",
  )
  assert store.list_effective_selling_plan_groups_for_product_variant(
      next_proxy.store,
      "gid://shopify/ProductVariant/default",
    )
    == []
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_variant_leave_selling_plan_groups_rejects_non_member_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store([
        selling_plan_group("gid://shopify/SellingPlanGroup/one", [], []),
      ]),
    )
  let query =
    "mutation { productVariantLeaveSellingPlanGroups(id: \\\"gid://shopify/ProductVariant/default\\\", sellingPlanGroupIds: [\\\"gid://shopify/SellingPlanGroup/one\\\"]) { productVariant { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"field\":[\"sellingPlanGroupIds\"]")
  assert string.contains(serialized, "\"code\":\"NOT_A_MEMBER\"")
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn selling_plan_group_add_products_rejects_unknown_and_mixed_duplicate_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store([
          selling_plan_group(
            "gid://shopify/SellingPlanGroup/one",
            [
              "gid://shopify/Product/optioned",
            ],
            [],
          ),
        ])
        |> store.upsert_base_products([
          ProductRecord(
            ..default_product(),
            id: "gid://shopify/Product/second",
            title: "Second Board",
            handle: "second-board",
          ),
        ]),
    )

  let unknown_query =
    "mutation { sellingPlanGroupAddProducts(id: \\\"gid://shopify/SellingPlanGroup/one\\\", productIds: [\\\"gid://shopify/Product/missing\\\"]) { sellingPlanGroup { id } userErrors { field message code } } }"
  let #(Response(status: unknown_status, body: unknown_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(unknown_query))
  let unknown_serialized = json.to_string(unknown_body)

  assert unknown_status == 200
  assert string.contains(unknown_serialized, "\"sellingPlanGroup\":null")
  assert string.contains(unknown_serialized, "\"field\":[\"productIds\"]")
  assert string.contains(unknown_serialized, "\"code\":\"NOT_FOUND\"")

  let duplicate_query =
    "mutation { sellingPlanGroupAddProducts(id: \\\"gid://shopify/SellingPlanGroup/one\\\", productIds: [\\\"gid://shopify/Product/optioned\\\", \\\"gid://shopify/Product/second\\\"]) { sellingPlanGroup { id productsCount { count precision } } userErrors { field message code } } }"
  let #(Response(status: duplicate_status, body: duplicate_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(duplicate_query))
  let duplicate_serialized = json.to_string(duplicate_body)

  assert duplicate_status == 200
  assert string.contains(duplicate_serialized, "\"sellingPlanGroup\":null")
  assert string.contains(duplicate_serialized, "\"code\":\"TAKEN\"")
  let assert Some(group) =
    store.get_effective_selling_plan_group_by_id(
      proxy.store,
      "gid://shopify/SellingPlanGroup/one",
    )
  assert group.product_ids == ["gid://shopify/Product/optioned"]
}

pub fn selling_plan_group_add_product_variants_rejects_mixed_duplicate_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store([
          selling_plan_group("gid://shopify/SellingPlanGroup/one", [], [
            "gid://shopify/ProductVariant/default",
          ]),
        ])
        |> store.upsert_base_product_variants([
          ProductVariantRecord(
            ..default_variant(),
            id: "gid://shopify/ProductVariant/second",
          ),
        ]),
    )
  let query =
    "mutation { sellingPlanGroupAddProductVariants(id: \\\"gid://shopify/SellingPlanGroup/one\\\", productVariantIds: [\\\"gid://shopify/ProductVariant/default\\\", \\\"gid://shopify/ProductVariant/second\\\"]) { sellingPlanGroup { id productVariantsCount { count precision } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"sellingPlanGroup\":null")
  assert string.contains(serialized, "\"code\":\"TAKEN\"")
  let assert Some(group) =
    store.get_effective_selling_plan_group_by_id(
      next_proxy.store,
      "gid://shopify/SellingPlanGroup/one",
    )
  assert group.product_variant_ids == ["gid://shopify/ProductVariant/default"]
}

pub fn selling_plan_group_add_products_enforces_31_group_cap_test() {
  let existing_groups =
    numbered_selling_plan_group_ids(31)
    |> list.map(fn(id) {
      selling_plan_group(id, ["gid://shopify/Product/optioned"], [])
    })
  let target_group =
    selling_plan_group("gid://shopify/SellingPlanGroup/32", [], [])
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store([target_group, ..existing_groups]),
    )
  let query =
    "mutation { sellingPlanGroupAddProducts(id: \\\"gid://shopify/SellingPlanGroup/32\\\", productIds: [\\\"gid://shopify/Product/optioned\\\"]) { sellingPlanGroup { id productsCount { count precision } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"sellingPlanGroup\":null")
  assert string.contains(
    serialized,
    "\"code\":\"SELLING_PLAN_GROUPS_TOO_MANY\"",
  )
  let assert Some(group) =
    store.get_effective_selling_plan_group_by_id(
      next_proxy.store,
      "gid://shopify/SellingPlanGroup/32",
    )
  assert group.product_ids == []
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn selling_plan_group_add_product_variants_enforces_31_group_cap_test() {
  let existing_groups =
    numbered_selling_plan_group_ids(31)
    |> list.map(fn(id) {
      selling_plan_group(id, [], ["gid://shopify/ProductVariant/default"])
    })
  let target_group =
    selling_plan_group("gid://shopify/SellingPlanGroup/32", [], [])
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: selling_plan_membership_store([target_group, ..existing_groups]),
    )
  let query =
    "mutation { sellingPlanGroupAddProductVariants(id: \\\"gid://shopify/SellingPlanGroup/32\\\", productVariantIds: [\\\"gid://shopify/ProductVariant/default\\\"]) { sellingPlanGroup { id productVariantsCount { count precision } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"sellingPlanGroup\":null")
  assert string.contains(
    serialized,
    "\"code\":\"SELLING_PLAN_GROUPS_TOO_MANY\"",
  )
  let assert Some(group) =
    store.get_effective_selling_plan_group_by_id(
      next_proxy.store,
      "gid://shopify/SellingPlanGroup/32",
    )
  assert group.product_variant_ids == []
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn selling_plan_group_add_products_rejects_unknown_and_duplicate_ids_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: default_option_store()
        |> store.upsert_base_products([
          ProductRecord(
            ..default_product(),
            id: "gid://shopify/Product/non-member",
            title: "Non-member Board",
            handle: "non-member-board",
          ),
        ])
        |> store.upsert_base_product_variants([
          ProductVariantRecord(
            ..default_variant(),
            id: "gid://shopify/ProductVariant/non-member",
            product_id: "gid://shopify/Product/non-member",
          ),
        ]),
    )
  let create_query =
    "mutation { sellingPlanGroupCreate(input: { name: \\\"Seed\\\", options: [\\\"Delivery\\\"] }) { sellingPlanGroup { id productsCount { count precision } } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")
  let assert [group] = store.list_effective_selling_plan_groups(proxy.store)

  let unknown_query =
    "mutation { sellingPlanGroupAddProducts(id: \\\""
    <> group.id
    <> "\\\", productIds: [\\\"gid://shopify/Product/missing\\\"]) { sellingPlanGroup { id productsCount { count precision } } userErrors { field message code } } }"
  let #(Response(status: unknown_status, body: unknown_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(unknown_query))
  let unknown_serialized = json.to_string(unknown_body)

  assert unknown_status == 200
  assert string.contains(unknown_serialized, "\"sellingPlanGroup\":null")
  assert string.contains(unknown_serialized, "\"field\":[\"productIds\"]")
  assert string.contains(
    unknown_serialized,
    "\"message\":\"Product gid://shopify/Product/missing does not exist.\"",
  )
  assert string.contains(unknown_serialized, "\"code\":\"NOT_FOUND\"")
  let assert [unchanged_group] =
    store.list_effective_selling_plan_groups(proxy.store)
  assert unchanged_group.product_ids == []

  let add_query =
    "mutation { sellingPlanGroupAddProducts(id: \\\""
    <> group.id
    <> "\\\", productIds: [\\\"gid://shopify/Product/optioned\\\"]) { sellingPlanGroup { id productsCount { count precision } } userErrors { field message code } } }"
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))
  assert add_status == 200
  assert string.contains(json.to_string(add_body), "\"userErrors\":[]")

  let duplicate_query =
    "mutation { sellingPlanGroupAddProducts(id: \\\""
    <> group.id
    <> "\\\", productIds: [\\\"gid://shopify/Product/optioned\\\"]) { sellingPlanGroup { id productsCount { count precision } } userErrors { field message code } } }"
  let #(
    Response(status: duplicate_status, body: duplicate_body, ..),
    duplicate_proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(duplicate_query))
  let duplicate_serialized = json.to_string(duplicate_body)

  assert duplicate_status == 200
  assert string.contains(duplicate_serialized, "\"sellingPlanGroup\":null")
  assert string.contains(duplicate_serialized, "\"field\":[\"productIds\"]")
  assert string.contains(
    duplicate_serialized,
    "\"message\":\"Resource has already been taken\"",
  )
  let assert [duplicate_group] =
    store.list_effective_selling_plan_groups(duplicate_proxy.store)
  assert duplicate_group.product_ids == ["gid://shopify/Product/optioned"]

  let unknown_variant_query =
    "mutation { sellingPlanGroupAddProductVariants(id: \\\""
    <> group.id
    <> "\\\", productVariantIds: [\\\"gid://shopify/ProductVariant/missing\\\"]) { sellingPlanGroup { id productVariantsCount { count precision } } userErrors { field message code } } }"
  let #(
    Response(status: unknown_variant_status, body: unknown_variant_body, ..),
    proxy,
  ) =
    draft_proxy.process_request(
      duplicate_proxy,
      graphql_request(unknown_variant_query),
    )
  let unknown_variant_serialized = json.to_string(unknown_variant_body)

  assert unknown_variant_status == 200
  assert string.contains(
    unknown_variant_serialized,
    "\"sellingPlanGroup\":null",
  )
  assert string.contains(
    unknown_variant_serialized,
    "\"field\":[\"productVariantIds\"]",
  )
  assert string.contains(
    unknown_variant_serialized,
    "\"message\":\"Product variant gid://shopify/ProductVariant/missing does not exist.\"",
  )
  assert string.contains(unknown_variant_serialized, "\"code\":\"NOT_FOUND\"")

  let add_variant_query =
    "mutation { sellingPlanGroupAddProductVariants(id: \\\""
    <> group.id
    <> "\\\", productVariantIds: [\\\"gid://shopify/ProductVariant/default\\\"]) { sellingPlanGroup { id productVariantsCount { count precision } } userErrors { field message code } } }"
  let #(Response(status: add_variant_status, body: add_variant_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_variant_query))
  assert add_variant_status == 200
  assert string.contains(json.to_string(add_variant_body), "\"userErrors\":[]")

  let duplicate_variant_query =
    "mutation { sellingPlanGroupAddProductVariants(id: \\\""
    <> group.id
    <> "\\\", productVariantIds: [\\\"gid://shopify/ProductVariant/default\\\"]) { sellingPlanGroup { id productVariantsCount { count precision } } userErrors { field message code } } }"
  let #(
    Response(status: duplicate_variant_status, body: duplicate_variant_body, ..),
    proxy,
  ) =
    draft_proxy.process_request(proxy, graphql_request(duplicate_variant_query))
  let duplicate_variant_serialized = json.to_string(duplicate_variant_body)

  assert duplicate_variant_status == 200
  assert string.contains(
    duplicate_variant_serialized,
    "\"sellingPlanGroup\":null",
  )
  assert string.contains(
    duplicate_variant_serialized,
    "\"field\":[\"productVariantIds\"]",
  )
  assert string.contains(
    duplicate_variant_serialized,
    "\"message\":\"Resource has already been taken\"",
  )

  let remove_non_member_query =
    "mutation { sellingPlanGroupRemoveProducts(id: \\\""
    <> group.id
    <> "\\\", productIds: [\\\"gid://shopify/Product/non-member\\\", \\\"gid://shopify/Product/unknown\\\"]) { removedProductIds userErrors { field message code } } }"
  let #(
    Response(status: remove_non_member_status, body: remove_non_member_body, ..),
    proxy,
  ) =
    draft_proxy.process_request(proxy, graphql_request(remove_non_member_query))
  assert remove_non_member_status == 200
  assert json.to_string(remove_non_member_body)
    == "{\"data\":{\"sellingPlanGroupRemoveProducts\":{\"removedProductIds\":[],\"userErrors\":[]}}}"

  let remove_non_member_variant_query =
    "mutation { sellingPlanGroupRemoveProductVariants(id: \\\""
    <> group.id
    <> "\\\", productVariantIds: [\\\"gid://shopify/ProductVariant/non-member\\\", \\\"gid://shopify/ProductVariant/unknown\\\"]) { removedProductVariantIds userErrors { field message code } } }"
  let #(
    Response(
      status: remove_non_member_variant_status,
      body: remove_non_member_variant_body,
      ..,
    ),
    proxy,
  ) =
    draft_proxy.process_request(
      proxy,
      graphql_request(remove_non_member_variant_query),
    )
  assert remove_non_member_variant_status == 200
  assert json.to_string(remove_non_member_variant_body)
    == "{\"data\":{\"sellingPlanGroupRemoveProductVariants\":{\"removedProductVariantIds\":[],\"userErrors\":[]}}}"

  let malformed_remove_query =
    "mutation RemoveMalformed($id: ID!, $productIds: [ID!]!) { sellingPlanGroupRemoveProducts(id: $id, productIds: $productIds) { removedProductIds userErrors { field message code } } }"
  let malformed_remove_body =
    json.to_string(
      json.object([
        #("query", json.string(malformed_remove_query)),
        #(
          "variables",
          json.object([
            #("id", json.string(group.id)),
            #("productIds", json.array(["not-a-product-gid"], json.string)),
          ]),
        ),
      ]),
    )
  let #(
    Response(status: malformed_remove_status, body: malformed_remove_body, ..),
    proxy,
  ) =
    draft_proxy.process_request(
      proxy,
      graphql_request_body(malformed_remove_body),
    )
  let malformed_remove_serialized = json.to_string(malformed_remove_body)
  assert malformed_remove_status == 200
  assert string.contains(malformed_remove_serialized, "\"errors\":[")
  assert string.contains(
    malformed_remove_serialized,
    "\"code\":\"INVALID_VARIABLE\"",
  )
  assert string.contains(
    malformed_remove_serialized,
    "Invalid global id 'not-a-product-gid'",
  )

  let malformed_remove_variant_query =
    "mutation RemoveMalformedVariant($id: ID!, $productVariantIds: [ID!]!) { sellingPlanGroupRemoveProductVariants(id: $id, productVariantIds: $productVariantIds) { removedProductVariantIds userErrors { field message code } } }"
  let malformed_remove_variant_body =
    json.to_string(
      json.object([
        #("query", json.string(malformed_remove_variant_query)),
        #(
          "variables",
          json.object([
            #("id", json.string(group.id)),
            #(
              "productVariantIds",
              json.array(["not-a-product-variant-gid"], json.string),
            ),
          ]),
        ),
      ]),
    )
  let #(
    Response(
      status: malformed_remove_variant_status,
      body: malformed_remove_variant_body,
      ..,
    ),
    final_proxy,
  ) =
    draft_proxy.process_request(
      proxy,
      graphql_request_body(malformed_remove_variant_body),
    )
  let malformed_remove_variant_serialized =
    json.to_string(malformed_remove_variant_body)
  assert malformed_remove_variant_status == 200
  assert string.contains(
    malformed_remove_variant_serialized,
    "\"code\":\"INVALID_VARIABLE\"",
  )
  assert string.contains(
    malformed_remove_variant_serialized,
    "Invalid global id 'not-a-product-variant-gid'",
  )

  let assert [final_group] =
    store.list_effective_selling_plan_groups(final_proxy.store)
  assert final_group.product_ids == ["gid://shopify/Product/optioned"]
  assert final_group.product_variant_ids
    == [
      "gid://shopify/ProductVariant/default",
    ]
}

pub fn product_options_create_stages_default_product_options_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", position: 1, values: [{ name: \\\"Red\\\" }, { name: \\\"Green\\\" }] }]) { product { id options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productOptionsCreate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"name\":\"Color\",\"position\":1,\"values\":[\"Red\"],\"optionValues\":[{\"name\":\"Red\",\"hasVariants\":true},{\"name\":\"Green\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Red\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"}]}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { options { name values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"options\":[{\"name\":\"Color\",\"values\":[\"Red\"],\"optionValues\":[{\"name\":\"Red\",\"hasVariants\":true},{\"name\":\"Green\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Red\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"}]}]}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_options_create_rejects_invalid_option_inputs_test() {
  let long_name = repeated_text("N", 256)
  let long_value = repeated_text("V", 256)

  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [{ name: \\\"Red\\\" }] }, { name: \\\"Color\\\", values: [{ name: \\\"Blue\\\" }] }]) { product { id } userErrors { field message code } } }",
    "DUPLICATED_OPTION_NAME",
    "[\"options\",\"1\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [{ name: \\\"Red\\\" }, { name: \\\"Red\\\" }] }]) { product { id } userErrors { field message code } } }",
    "DUPLICATED_OPTION_VALUE",
    "[\"options\",\"0\",\"values\",\"1\",\"name\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [] }]) { product { id } userErrors { field message code } } }",
    "OPTION_VALUES_MISSING",
    "[\"options\",\"0\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"\\\", values: [{ name: \\\"Red\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTION_NAME_MISSING",
    "[\"options\",\"0\",\"name\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\""
      <> long_name
      <> "\\\", values: [{ name: \\\"Red\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTION_NAME_TOO_LONG",
    "[\"options\",\"0\"]",
  )
  assert_product_option_user_error(
    default_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [{ name: \\\""
      <> long_value
      <> "\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTION_VALUE_NAME_TOO_LONG",
    "[\"options\",\"0\",\"values\",\"0\",\"name\"]",
  )
}

pub fn product_options_create_rejects_product_level_constraints_test() {
  assert_product_option_user_error(
    three_option_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Finish\\\", values: [{ name: \\\"Matte\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTIONS_OVER_LIMIT",
    "[\"options\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Color\\\", values: [{ name: \\\"Blue\\\" }] }]) { product { id } userErrors { field message code } } }",
    "OPTION_ALREADY_EXISTS",
    "[\"options\",\"0\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ name: \\\"Material\\\" }]) { product { id } userErrors { field message code } } }",
    "NEW_OPTION_WITHOUT_VALUE_FOR_EXISTING_VARIANTS",
    "[\"options\",\"0\",\"values\"]",
  )
  assert_product_option_user_error(
    variant_cap_store(),
    "mutation { productOptionsCreate(productId: \\\"gid://shopify/Product/optioned\\\", variantStrategy: CREATE, options: [{ name: \\\"Color\\\", values: [{ name: \\\"Red\\\" }, { name: \\\"Blue\\\" }] }]) { product { id } userErrors { field message code } } }",
    "TOO_MANY_VARIANTS_CREATED",
    "[\"options\"]",
  )
}

pub fn product_option_update_rejects_invalid_option_inputs_test() {
  let long_value = repeated_text("V", 256)

  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\", name: \\\"Size\\\" }) { product { id } userErrors { field message code } } }",
    "OPTION_ALREADY_EXISTS",
    "[\"option\",\"name\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\" }, optionValuesToAdd: [{ name: \\\"Blue\\\" }, { name: \\\"Blue\\\" }]) { product { id } userErrors { field message code } } }",
    "DUPLICATED_OPTION_VALUE",
    "[\"optionValuesToAdd\",\"1\",\"name\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\" }, optionValuesToAdd: [{ name: \\\"red\\\" }]) { product { id } userErrors { field message code } } }",
    "OPTION_VALUE_ALREADY_EXISTS",
    "[\"optionValuesToAdd\",\"0\",\"name\"]",
  )
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\" }, optionValuesToUpdate: [{ id: \\\"gid://shopify/ProductOptionValue/red\\\", name: \\\""
      <> long_value
      <> "\\\" }]) { product { id } userErrors { field message code } } }",
    "OPTION_VALUE_NAME_TOO_LONG",
    "[\"optionValuesToUpdate\",\"0\",\"name\"]",
  )
}

pub fn product_options_delete_reports_option_codes_for_unknown_ids_test() {
  assert_product_option_user_error(
    option_update_store(),
    "mutation { productOptionsDelete(productId: \\\"gid://shopify/Product/optioned\\\", options: [\\\"gid://shopify/ProductOption/missing\\\"]) { deletedOptionsIds product { id } userErrors { field message code } } }",
    "OPTION_DOES_NOT_EXIST",
    "[\"options\",\"0\"]",
  )
}

pub fn product_option_update_repositions_values_and_variants_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { productOptionUpdate(productId: \\\"gid://shopify/Product/optioned\\\", option: { id: \\\"gid://shopify/ProductOption/color\\\", name: \\\"Shade\\\", position: 2 }, optionValuesToAdd: [{ name: \\\"Blue\\\" }], optionValuesToUpdate: [{ id: \\\"gid://shopify/ProductOptionValue/red\\\", name: \\\"Crimson\\\" }], optionValuesToDelete: [\\\"gid://shopify/ProductOptionValue/green\\\"]) { product { id options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productOptionUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"name\":\"Size\",\"position\":1,\"values\":[\"Small\"],\"optionValues\":[{\"name\":\"Small\",\"hasVariants\":true}]},{\"name\":\"Shade\",\"position\":2,\"values\":[\"Crimson\"],\"optionValues\":[{\"name\":\"Crimson\",\"hasVariants\":true},{\"name\":\"Blue\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Small / Crimson\",\"selectedOptions\":[{\"name\":\"Size\",\"value\":\"Small\"},{\"name\":\"Shade\",\"value\":\"Crimson\"}]}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"options\":[{\"name\":\"Size\",\"position\":1,\"values\":[\"Small\"],\"optionValues\":[{\"name\":\"Small\",\"hasVariants\":true}]},{\"name\":\"Shade\",\"position\":2,\"values\":[\"Crimson\"],\"optionValues\":[{\"name\":\"Crimson\",\"hasVariants\":true},{\"name\":\"Blue\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Small / Crimson\",\"selectedOptions\":[{\"name\":\"Size\",\"value\":\"Small\"},{\"name\":\"Shade\",\"value\":\"Crimson\"}]}]}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_options_delete_restores_default_option_state_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { productOptionsDelete(productId: \\\"gid://shopify/Product/optioned\\\", options: [\\\"gid://shopify/ProductOption/color\\\", \\\"gid://shopify/ProductOption/size\\\"]) { deletedOptionsIds product { id options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productOptionsDelete\":{\"deletedOptionsIds\":[\"gid://shopify/ProductOption/color\",\"gid://shopify/ProductOption/size\"],\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"name\":\"Title\",\"position\":1,\"values\":[\"Default Title\"],\"optionValues\":[{\"name\":\"Default Title\",\"hasVariants\":true}]}],\"variants\":{\"nodes\":[{\"title\":\"Default Title\",\"selectedOptions\":[{\"name\":\"Title\",\"value\":\"Default Title\"}]}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"options\":[{\"name\":\"Title\",\"position\":1,\"values\":[\"Default Title\"],\"optionValues\":[{\"name\":\"Default Title\",\"hasVariants\":true}]}],\"variants\":{\"nodes\":[{\"title\":\"Default Title\",\"selectedOptions\":[{\"name\":\"Title\",\"value\":\"Default Title\"}]}]}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_user_error_shape_validation_branches_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "mutation { productCreate(product: { title: \\\"\\\" }) { product { id } userErrors { field message code } } }",
      ),
    )
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"

  let #(Response(status: options_status, body: options_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "mutation { productOptionsDelete(productId: \\\"gid://shopify/Product/missing\\\", options: [\\\"gid://shopify/ProductOption/missing\\\"]) { deletedOptionsIds product { id } userErrors { field message code } } }",
      ),
    )
  assert options_status == 200
  assert json.to_string(options_body)
    == "{\"data\":{\"productOptionsDelete\":{\"deletedOptionsIds\":[],\"product\":null,\"userErrors\":[{\"field\":[\"productId\"],\"message\":\"Product does not exist\",\"code\":\"PRODUCT_DOES_NOT_EXIST\"}]}}}"

  let #(Response(status: collection_status, body: collection_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "mutation { collectionCreate(input: { title: \\\"\\\" }) { collection { id } userErrors { field message code } } }",
      ),
    )
  assert collection_status == 200
  assert json.to_string(collection_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"

  let #(Response(status: activate_status, body: activate_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/missing\\\", locationId: \\\"gid://shopify/Location/missing\\\") { inventoryLevel { id } userErrors { field message code } } }",
      ),
    )
  assert activate_status == 200
  assert json.to_string(activate_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":null,\"userErrors\":[{\"field\":[\"inventoryItemId\"],\"message\":\"Inventory item does not exist\",\"code\":\"NOT_FOUND\"}]}}}"
}

pub fn combined_listing_update_rejects_non_parent_product_test() {
  assert_combined_listing_user_error(
    default_option_store(),
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/optioned\\\") { product { id } userErrors { field message code } } }",
    "PARENT_PRODUCT_MUST_BE_A_COMBINED_LISTING",
  )
}

pub fn combined_listing_update_rejects_parent_as_child_test() {
  assert_combined_listing_user_error(
    combined_listing_parent_store(),
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/parent\\\", productsAdded: [{ childProductId: \\\"gid://shopify/Product/parent\\\", selectedParentOptionValues: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }], optionsAndValues: [{ name: \\\"Title\\\", values: [\\\"Default Title\\\"] }]) { product { id } userErrors { field message code } } }",
    "CANNOT_HAVE_PARENT_AS_CHILD",
  )
}

pub fn combined_listing_update_rejects_duplicate_child_inputs_test() {
  assert_combined_listing_user_error(
    combined_listing_parent_store(),
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/parent\\\", productsAdded: [{ childProductId: \\\"gid://shopify/Product/child\\\", selectedParentOptionValues: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }, { childProductId: \\\"gid://shopify/Product/child\\\", selectedParentOptionValues: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }], optionsAndValues: [{ name: \\\"Title\\\", values: [\\\"Default Title\\\"] }]) { product { id } userErrors { field message code } } }",
    "CANNOT_HAVE_DUPLICATED_PRODUCTS",
  )
}

pub fn combined_listing_update_rejects_missing_child_product_test() {
  assert_combined_listing_user_error(
    combined_listing_parent_store(),
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/parent\\\", productsAdded: [{ childProductId: \\\"gid://shopify/Product/missing\\\", selectedParentOptionValues: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }], optionsAndValues: [{ name: \\\"Title\\\", values: [\\\"Default Title\\\"] }]) { product { id } userErrors { field message code } } }",
    "PRODUCT_NOT_FOUND",
  )
}

pub fn combined_listing_update_rejects_empty_selected_parent_option_values_test() {
  assert_combined_listing_user_error(
    combined_listing_parent_store(),
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/parent\\\", productsAdded: [{ childProductId: \\\"gid://shopify/Product/child\\\", selectedParentOptionValues: [] }], optionsAndValues: [{ name: \\\"Title\\\", values: [\\\"Default Title\\\"] }]) { product { id } userErrors { field message code } } }",
    "MUST_HAVE_SELECTED_OPTION_VALUES",
  )
}

pub fn combined_listing_update_rejects_overlong_title_test() {
  assert_combined_listing_user_error(
    combined_listing_parent_store(),
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/parent\\\", title: \\\""
      <> repeated_text("T", 256)
      <> "\\\") { product { id } userErrors { field message code } } }",
    "TITLE_TOO_LONG",
  )
}

pub fn combined_listing_update_rejects_missing_options_and_values_test() {
  assert_combined_listing_user_error(
    combined_listing_parent_store(),
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/parent\\\", productsAdded: [{ childProductId: \\\"gid://shopify/Product/child\\\", selectedParentOptionValues: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }]) { product { id } userErrors { field message code } } }",
    "MISSING_OPTION_VALUES",
  )
}

pub fn combined_listing_update_rejects_edit_remove_overlap_test() {
  assert_combined_listing_user_error(
    combined_listing_parent_store(),
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/parent\\\", productsEdited: [{ childProductId: \\\"gid://shopify/Product/child\\\", selectedParentOptionValues: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }], productsRemovedIds: [\\\"gid://shopify/Product/child\\\"], optionsAndValues: [{ name: \\\"Title\\\", values: [\\\"Default Title\\\"] }]) { product { id } userErrors { field message code } } }",
    "EDIT_AND_REMOVE_ON_SAME_PRODUCTS",
  )
}

pub fn combined_listing_update_stages_child_membership_locally_test() {
  let query =
    "mutation { combinedListingUpdate(parentProductId: \"gid://shopify/Product/parent\", productsAdded: [{ childProductId: \"gid://shopify/Product/child\", selectedParentOptionValues: [{ name: \"Title\", value: \"Default Title\" }] }], optionsAndValues: [{ name: \"Title\", values: [\"Default Title\"] }]) { product { id combinedListingRole } userErrors { field message code } } }"
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(..proxy, store: combined_listing_parent_store())
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_document_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"combinedListingUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/parent\",\"combinedListingRole\":\"PARENT\"},\"userErrors\":[]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Staged
  assert entry.query == query

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { parent: product(id: \\\"gid://shopify/Product/parent\\\") { id combinedListingRole } child: product(id: \\\"gid://shopify/Product/child\\\") { id combinedListingRole } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"parent\":{\"id\":\"gid://shopify/Product/parent\",\"combinedListingRole\":\"PARENT\"},\"child\":{\"id\":\"gid://shopify/Product/child\",\"combinedListingRole\":\"CHILD\"}}}"
}

pub fn combined_listing_update_rejects_staged_child_readdition_test() {
  let add_query =
    "mutation { combinedListingUpdate(parentProductId: \\\"gid://shopify/Product/parent\\\", productsAdded: [{ childProductId: \\\"gid://shopify/Product/child\\\", selectedParentOptionValues: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }], optionsAndValues: [{ name: \\\"Title\\\", values: [\\\"Default Title\\\"] }]) { product { id } userErrors { field message code } } }"
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(..proxy, store: combined_listing_parent_store())
  let #(Response(status: first_status, body: first_body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))
  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"combinedListingUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/parent\"},\"userErrors\":[]}}}"

  let #(Response(status: second_status, body: second_body, ..), next_proxy) =
    draft_proxy.process_request(next_proxy, graphql_request(add_query))
  assert second_status == 200
  assert string.contains(
    json.to_string(second_body),
    "\"field\":[\"productsAdded\"]",
  )
  assert string.contains(
    json.to_string(second_body),
    "\"code\":\"PRODUCT_IS_ALREADY_A_CHILD\"",
  )
  let assert [first_entry, second_entry] = store.get_log(next_proxy.store)
  assert first_entry.status == store_types.Staged
  assert second_entry.status == store_types.Failed
}

pub fn product_options_reorder_reorders_variants_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { productOptionsReorder(productId: \\\"gid://shopify/Product/optioned\\\", options: [{ id: \\\"gid://shopify/ProductOption/size\\\", values: [{ id: \\\"gid://shopify/ProductOptionValue/small\\\" }] }, { id: \\\"gid://shopify/ProductOption/color\\\", values: [{ id: \\\"gid://shopify/ProductOptionValue/green\\\" }, { id: \\\"gid://shopify/ProductOptionValue/red\\\" }] }]) { product { id options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productOptionsReorder\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"options\":[{\"name\":\"Size\",\"position\":1,\"values\":[\"Small\"],\"optionValues\":[{\"name\":\"Small\",\"hasVariants\":true}]},{\"name\":\"Color\",\"position\":2,\"values\":[\"Red\"],\"optionValues\":[{\"name\":\"Red\",\"hasVariants\":true},{\"name\":\"Green\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Small / Red\",\"selectedOptions\":[{\"name\":\"Size\",\"value\":\"Small\"},{\"name\":\"Color\",\"value\":\"Red\"}]}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { options { name position values optionValues { name hasVariants } } variants(first: 10) { nodes { title selectedOptions { name value } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"options\":[{\"name\":\"Size\",\"position\":1,\"values\":[\"Small\"],\"optionValues\":[{\"name\":\"Small\",\"hasVariants\":true}]},{\"name\":\"Color\",\"position\":2,\"values\":[\"Red\"],\"optionValues\":[{\"name\":\"Red\",\"hasVariants\":true},{\"name\":\"Green\",\"hasVariants\":false}]}],\"variants\":{\"nodes\":[{\"title\":\"Small / Red\",\"selectedOptions\":[{\"name\":\"Size\",\"value\":\"Small\"},{\"name\":\"Color\",\"value\":\"Red\"}]}]}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_change_status_stages_search_lagged_status_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productChangeStatus(productId: \\\"gid://shopify/Product/optioned\\\", status: ARCHIVED) { product { id status updatedAt } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productChangeStatus\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"status\":\"ARCHIVED\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id status updatedAt } products(first: 10, query: \\\"status:archived tag:existing\\\") { nodes { id status } } productsCount(query: \\\"status:archived tag:existing\\\") { count precision } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"status\":\"ARCHIVED\",\"updatedAt\":\"2024-01-01T00:00:00.000Z\"},\"products\":{\"nodes\":[]},\"productsCount\":{\"count\":0,\"precision\":\"EXACT\"}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_delete_stages_downstream_no_data_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productDelete(input: { id: \\\"gid://shopify/Product/optioned\\\" }) { deletedProductId userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productDelete\":{\"deletedProductId\":\"gid://shopify/Product/optioned\",\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id title } products(first: 10, query: \\\"tag:existing\\\") { nodes { id title } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":null,\"products\":{\"nodes\":[]}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn metafield_delete_stages_product_owned_deletion_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: metafield_store())
  let query =
    "mutation { metafieldDelete(input: { id: \\\"gid://shopify/Metafield/material\\\" }) { deletedId userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldDelete\":{\"deletedId\":\"gid://shopify/Metafield/material\",\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { material: metafield(namespace: \\\"custom\\\", key: \\\"material\\\") { id } origin: metafield(namespace: \\\"details\\\", key: \\\"origin\\\") { id namespace key value } metafields(first: 10) { nodes { id namespace key value } pageInfo { hasNextPage hasPreviousPage } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"material\":null,\"origin\":{\"id\":\"gid://shopify/Metafield/origin\",\"namespace\":\"details\",\"key\":\"origin\",\"value\":\"VN\"},\"metafields\":{\"nodes\":[{\"id\":\"gid://shopify/Metafield/origin\",\"namespace\":\"details\",\"key\":\"origin\",\"value\":\"VN\"}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false}}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn metafield_delete_unknown_id_keeps_compatibility_payload_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: metafield_store())
  let query =
    "mutation { metafieldDelete(input: { id: \\\"gid://shopify/Metafield/missing\\\" }) { deletedId userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldDelete\":{\"deletedId\":\"gid://shopify/Metafield/missing\",\"userErrors\":[]}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn metafields_set_rejects_invalid_input_shape_and_values_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let owner_id = "gid://shopify/Product/optioned"
  let long_namespace = string.repeat("n", times: 256)
  let long_key = string.repeat("k", times: 65)
  let query =
    "mutation { metafieldsSet(metafields: ["
    <> metafields_set_input(owner_id, "ab", "x", "single_line_text_field", "v")
    <> ","
    <> metafields_set_input(
      owner_id,
      long_namespace,
      "long_namespace",
      "single_line_text_field",
      "v",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      long_key,
      "single_line_text_field",
      "v",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "bad namespace",
      "good_key",
      "single_line_text_field",
      "v",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "bad.key",
      "single_line_text_field",
      "v",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "shopify_standard",
      "title",
      "single_line_text_field",
      "x",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "protected",
      "title",
      "single_line_text_field",
      "x",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "shopify-l10n-fields",
      "title",
      "single_line_text_field",
      "x",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "tier",
      "number_integer",
      "not a number",
    )
    <> ","
    <> metafields_set_input(owner_id, "loyalty", "flag", "boolean", "yes")
    <> ","
    <> metafields_set_input(owner_id, "loyalty", "color", "color", "blue")
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "published",
      "date_time",
      "tomorrow",
    )
    <> ","
    <> metafields_set_input(owner_id, "loyalty", "data", "json", "{nope")
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "related",
      "product_reference",
      "gid://shopify/Product/missing",
    )
    <> "]) { metafields { id } userErrors { field code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_document_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldsSet\":{\"metafields\":[],\"userErrors\":[{\"field\":[\"metafields\",\"0\",\"namespace\"],\"code\":\"TOO_SHORT\"},{\"field\":[\"metafields\",\"0\",\"key\"],\"code\":\"TOO_SHORT\"},{\"field\":[\"metafields\",\"1\",\"namespace\"],\"code\":\"TOO_LONG\"},{\"field\":[\"metafields\",\"2\",\"key\"],\"code\":\"TOO_LONG\"},{\"field\":[\"metafields\",\"3\",\"namespace\"],\"code\":\"INVALID\"},{\"field\":[\"metafields\",\"4\",\"key\"],\"code\":\"INVALID\"},{\"field\":[\"metafields\",\"5\",\"namespace\"],\"code\":null},{\"field\":[\"metafields\",\"6\",\"namespace\"],\"code\":null},{\"field\":[\"metafields\",\"7\",\"namespace\"],\"code\":null},{\"field\":[\"metafields\",\"8\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"9\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"10\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"11\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"12\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"13\",\"value\"],\"code\":\"INVALID_VALUE\"}]}}}"
  assert store.get_effective_metafields_by_owner_id(next_proxy.store, owner_id)
    |> list.length
    == 0
}

pub fn metafields_set_rejects_invalid_list_structured_and_definition_values_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(..proxy, store: definition_validation_store())
  let owner_id = "gid://shopify/Product/optioned"
  let query =
    "mutation { metafieldsSet(metafields: ["
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "scores",
      "list.number_integer",
      "not-json",
    )
    <> ","
    <> metafields_set_input(owner_id, "loyalty", "weight", "weight", "heavy")
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "min_tier",
      "number_integer",
      "1",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "max_tier",
      "number_integer",
      "10",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "sku_code",
      "single_line_text_field",
      "abc123",
    )
    <> ","
    <> metafields_set_input(
      owner_id,
      "loyalty",
      "plan",
      "single_line_text_field",
      "bronze",
    )
    <> "]) { metafields { id } userErrors { field code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_document_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldsSet\":{\"metafields\":null,\"userErrors\":[{\"field\":[\"metafields\",\"0\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"1\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"2\",\"value\"],\"code\":\"GREATER_THAN_OR_EQUAL_TO\"},{\"field\":[\"metafields\",\"3\",\"value\"],\"code\":\"LESS_THAN_OR_EQUAL_TO\"},{\"field\":[\"metafields\",\"4\",\"value\"],\"code\":\"INVALID_VALUE\"},{\"field\":[\"metafields\",\"5\",\"value\"],\"code\":\"INCLUSION\"}]}}}"
  assert store.get_effective_metafields_by_owner_id(next_proxy.store, owner_id)
    |> list.length
    == 0
}

pub fn metafields_delete_stages_product_owned_deletions_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: metafield_store())
  let query =
    "mutation { metafieldsDelete(metafields: [{ ownerId: \\\"gid://shopify/Product/optioned\\\", namespace: \\\"custom\\\", key: \\\"material\\\" }, { ownerId: \\\"gid://shopify/Product/optioned\\\", namespace: \\\"custom\\\", key: \\\"missing\\\" }]) { deletedMetafields { ownerId namespace key } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"metafieldsDelete\":{\"deletedMetafields\":[{\"ownerId\":\"gid://shopify/Product/optioned\",\"namespace\":\"custom\",\"key\":\"material\"},null],\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { material: metafield(namespace: \\\"custom\\\", key: \\\"material\\\") { id } origin: metafield(namespace: \\\"details\\\", key: \\\"origin\\\") { id namespace key value } metafields(first: 10) { nodes { id namespace key value } pageInfo { hasNextPage hasPreviousPage } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"material\":null,\"origin\":{\"id\":\"gid://shopify/Metafield/origin\",\"namespace\":\"details\",\"key\":\"origin\",\"value\":\"VN\"},\"metafields\":{\"nodes\":[{\"id\":\"gid://shopify/Metafield/origin\",\"namespace\":\"details\",\"key\":\"origin\",\"value\":\"VN\"}],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false}}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn tags_add_stages_tags_and_preserves_base_tag_search_lag_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { tagsAdd(id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\"winter\\\", \\\"existing\\\", \\\"fall\\\"]) { node { ... on Product { id tags } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"tagsAdd\":{\"node\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"fall\",\"winter\"]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id tags } products(first: 10, query: \\\"tag:fall\\\") { nodes { id tags } } productsCount(query: \\\"tag:fall\\\") { count precision } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"fall\",\"winter\"]},\"products\":{\"nodes\":[]},\"productsCount\":{\"count\":0,\"precision\":\"EXACT\"}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn tags_remove_stages_tags_and_keeps_removed_tag_searchable_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tagged_product_store())
  let query =
    "mutation { tagsRemove(id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\"sale\\\", \\\"missing\\\"]) { node { ... on Product { id tags } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"tagsRemove\":{\"node\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id tags } remaining: products(first: 10, query: \\\"tag:summer\\\") { nodes { id tags } } removed: products(first: 10, query: \\\"tag:sale\\\") { nodes { id tags } } remainingCount: productsCount(query: \\\"tag:summer\\\") { count precision } removedCount: productsCount(query: \\\"tag:sale\\\") { count precision } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]},\"remaining\":{\"nodes\":[{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]}]},\"removed\":{\"nodes\":[{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]}]},\"remainingCount\":{\"count\":1,\"precision\":\"EXACT\"},\"removedCount\":{\"count\":1,\"precision\":\"EXACT\"}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn tags_add_and_remove_use_shopify_tag_identity_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tagged_product_store())
  let add_query =
    "mutation { tagsAdd(id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\" Sale \\\", \\\"SALE\\\", \\\"blue\\\", \\\"Blue\\\"]) { node { ... on Product { id tags } } userErrors { field message } } }"

  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))

  assert add_status == 200
  assert json.to_string(add_body)
    == "{\"data\":{\"tagsAdd\":{\"node\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"blue\",\"existing\",\"sale\",\"summer\"]},\"userErrors\":[]}}}"

  let remove_query =
    "mutation { tagsRemove(id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\"SALE\\\", \\\"BLUE\\\"]) { node { ... on Product { id tags } } userErrors { field message } } }"
  let #(Response(status: remove_status, body: remove_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(remove_query))

  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"tagsRemove\":{\"node\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\",\"summer\"]},\"userErrors\":[]}}}"
}

pub fn product_update_stages_fields_and_downstream_reads_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", title: \\\"Updated Board\\\", vendor: \\\"HERMES\\\", productType: \\\"BOARDS\\\", tags: [\\\"beta\\\", \\\"alpha\\\"], descriptionHtml: \\\"<p>Updated</p>\\\", templateSuffix: \\\"custom\\\", seo: { title: \\\"SEO title\\\", description: \\\"SEO description\\\" } }) { product { id title vendor productType tags descriptionHtml templateSuffix seo { title description } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"title\":\"Updated Board\",\"vendor\":\"HERMES\",\"productType\":\"BOARDS\",\"tags\":[\"alpha\",\"beta\"],\"descriptionHtml\":\"<p>Updated</p>\",\"templateSuffix\":\"custom\",\"seo\":{\"title\":\"SEO title\",\"description\":\"SEO description\"}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id title vendor productType tags descriptionHtml templateSuffix seo { title description } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"title\":\"Updated Board\",\"vendor\":\"HERMES\",\"productType\":\"BOARDS\",\"tags\":[\"alpha\",\"beta\"],\"descriptionHtml\":\"<p>Updated</p>\",\"templateSuffix\":\"custom\",\"seo\":{\"title\":\"SEO title\",\"description\":\"SEO description\"}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_update_normalizes_tags_like_shopify_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\" Red \\\", \\\"red\\\", \\\"RED\\\", \\\" big   sale \\\"] }) { product { id tags } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"big   sale\",\"Red\"]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id tags } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"big   sale\",\"Red\"]}}}"
}

pub fn product_update_rejects_invalid_product_tags_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let long_tag = string.repeat("x", times: 256)
  let long_query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", tags: [\\\""
    <> long_tag
    <> "\\\"] }) { product { id tags } userErrors { field message } } }"

  let #(Response(status: long_status, body: long_body, ..), long_proxy) =
    draft_proxy.process_request(proxy, graphql_request(long_query))

  assert long_status == 200
  assert json.to_string(long_body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"tags\":[\"existing\"]},\"userErrors\":[{\"field\":[\"tags\"],\"message\":\"Product tags is invalid\"}]}}}"
  assert store.get_log(long_proxy.store)
    |> list.length
    == 1

  let too_many_tags = string.repeat("\\\"tag\\\",", times: 251)
  let too_many_query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", tags: ["
    <> too_many_tags
    <> "] }) { product { id tags } userErrors { field message } } }"
  let #(
    Response(status: too_many_status, body: too_many_body, ..),
    too_many_proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(too_many_query))
  let too_many_json = json.to_string(too_many_body)
  assert too_many_status == 200
  assert !string.contains(too_many_json, "\"data\"")
  assert string.contains(
    too_many_json,
    "\"message\":\"The input array size of 251 is greater than the maximum allowed of 250.\"",
  )
  assert string.contains(
    too_many_json,
    "\"path\":[\"productUpdate\",\"product\",\"tags\"]",
  )
  assert string.contains(too_many_json, "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"")
  assert store.get_log(too_many_proxy.store) == []
}

pub fn product_update_blank_title_returns_existing_product_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", title: \\\"\\\" }) { product { id title handle } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"title\":\"Optioned Board\",\"handle\":\"optioned-board\"},\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productUpdate")
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_update_rejects_product_scalar_validation_errors_test() {
  let long_vendor = string.repeat("v", times: 256)
  let vendor_query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", vendor: \\\""
    <> long_vendor
    <> "\\\" }) { product { id title vendor } userErrors { field message code } } }"
  let proxy =
    proxy_state.DraftProxy(..draft_proxy.new(), store: default_option_store())
  let #(Response(status: vendor_status, body: vendor_body, ..), vendor_proxy) =
    draft_proxy.process_request(proxy, graphql_request(vendor_query))
  assert vendor_status == 200
  assert json.to_string(vendor_body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"title\":\"Optioned Board\",\"vendor\":null},\"userErrors\":[{\"field\":[\"vendor\"],\"message\":\"Vendor is too long (maximum is 255 characters)\",\"code\":null}]}}}"
  let assert [vendor_entry] = store.get_log(vendor_proxy.store)
  assert vendor_entry.operation_name == Some("productUpdate")
  assert vendor_entry.status == store_types.Failed
  assert vendor_entry.staged_resource_ids == []

  let long_description = string.repeat("x", times: 524_288)
  let description_query =
    "mutation { productUpdate(product: { id: \\\"gid://shopify/Product/optioned\\\", descriptionHtml: \\\""
    <> long_description
    <> "\\\" }) { product { id title descriptionHtml } userErrors { field message code } } }"
  let description_proxy =
    proxy_state.DraftProxy(..draft_proxy.new(), store: default_option_store())
  let #(
    Response(status: description_status, body: description_body, ..),
    description_next_proxy,
  ) =
    draft_proxy.process_request(
      description_proxy,
      graphql_request(description_query),
    )
  assert description_status == 200
  assert json.to_string(description_body)
    == "{\"data\":{\"productUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"title\":\"Optioned Board\",\"descriptionHtml\":\"\"},\"userErrors\":[{\"field\":[\"bodyHtml\"],\"message\":\"Body (HTML) is too big (maximum is 512 KB)\",\"code\":null}]}}}"
  let assert [description_entry] = store.get_log(description_next_proxy.store)
  assert description_entry.operation_name == Some("productUpdate")
  assert description_entry.status == store_types.Failed
  assert description_entry.staged_resource_ids == []
}

pub fn product_create_and_set_normalize_tags_like_shopify_test() {
  let create_query =
    "mutation { productCreate(product: { title: \\\"Created Board\\\", vendor: \\\"Hermes\\\", status: DRAFT, tags: [\\\"Red\\\", \\\"blue\\\", \\\"red\\\"] }) { product { id tags } userErrors { field message } } }"
  let #(Response(status: create_status, body: create_body, ..), _) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(create_query),
    )
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"tags\":[\"blue\",\"Red\"]},\"userErrors\":[]}}}"

  let set_query =
    "mutation { productSet(input: { title: \\\"Set Board\\\", vendor: \\\"Hermes\\\", status: DRAFT, tags: [\\\"Red\\\", \\\"blue\\\", \\\"red\\\"] }, synchronous: true) { product { id tags } userErrors { field message } } }"
  let #(Response(status: set_status, body: set_body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(set_query))
  assert set_status == 200
  assert json.to_string(set_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"tags\":[\"blue\",\"Red\"]},\"userErrors\":[]}}}"
}

pub fn product_set_variable_log_replays_product_set_input_type_test() {
  let query =
    "
mutation HAR548ProductSetCommitReplay($input: ProductSetInput!, $synchronous: Boolean!) {
  productSet(input: $input, synchronous: $synchronous) {
    product {
      id
      title
      handle
      status
    }
    userErrors {
      field
      message
      code
    }
  }
}
"
  let body =
    json.to_string(
      json.object([
        #("query", json.string(query)),
        #(
          "variables",
          json.object([
            #(
              "input",
              json.object([
                #("title", json.string("Variable ProductSet")),
                #("vendor", json.string("Hermes")),
                #("status", json.string("DRAFT")),
                #("tags", json.array(["har-548", "commit-replay"], json.string)),
              ]),
            ),
            #("synchronous", json.bool(True)),
          ]),
        ),
      ]),
    )

  let #(Response(status: status, body: response_body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request_body(body))

  assert status == 200
  assert json.to_string(response_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Variable ProductSet\",\"handle\":\"variable-productset\",\"status\":\"DRAFT\"},\"userErrors\":[]}}}"

  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productSet")
  assert entry.path == "/admin/api/2025-01/graphql.json"
  assert string.contains(entry.query, "$input: ProductSetInput!")

  let replay_body = commit.build_replay_body(entry)
  assert string.contains(replay_body, "$input: ProductSetInput!")
  assert string.contains(replay_body, "\"title\":\"Variable ProductSet\"")
  assert string.contains(replay_body, "\"vendor\":\"Hermes\"")
  assert string.contains(replay_body, "\"status\":\"DRAFT\"")
  assert string.contains(replay_body, "\"synchronous\":true")
}

pub fn product_create_stages_product_default_variant_and_inventory_test() {
  let proxy = draft_proxy.new()
  let query =
    "mutation { productCreate(product: { title: \\\"Created Board\\\", status: DRAFT, vendor: \\\"HERMES\\\", productType: \\\"BOARDS\\\", tags: [\\\"beta\\\", \\\"alpha\\\"], descriptionHtml: \\\"<p>Created</p>\\\", templateSuffix: \\\"custom\\\", seo: { title: \\\"SEO title\\\", description: \\\"SEO description\\\" } }) { product { id title handle status vendor productType tags descriptionHtml templateSuffix seo { title description } totalInventory tracksInventory variants(first: 10) { nodes { id title inventoryQuantity inventoryItem { id tracked requiresShipping } } } } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Created Board\",\"handle\":\"created-board\",\"status\":\"DRAFT\",\"vendor\":\"HERMES\",\"productType\":\"BOARDS\",\"tags\":[\"alpha\",\"beta\"],\"descriptionHtml\":\"<p>Created</p>\",\"templateSuffix\":\"custom\",\"seo\":{\"title\":\"SEO title\",\"description\":\"SEO description\"},\"totalInventory\":0,\"tracksInventory\":false,\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Default Title\",\"inventoryQuantity\":0,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/5\",\"tracked\":false,\"requiresShipping\":true}}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\\\") { id title handle status totalInventory tracksInventory variants(first: 10) { nodes { id title inventoryQuantity inventoryItem { id tracked requiresShipping } } } } variant: productVariant(id: \\\"gid://shopify/ProductVariant/4\\\") { id title inventoryQuantity inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory } } stock: inventoryItem(id: \\\"gid://shopify/InventoryItem/5\\\") { id tracked requiresShipping variant { id title sku inventoryQuantity product { id title handle status totalInventory tracksInventory } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Created Board\",\"handle\":\"created-board\",\"status\":\"DRAFT\",\"totalInventory\":0,\"tracksInventory\":false,\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Default Title\",\"inventoryQuantity\":0,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/5\",\"tracked\":false,\"requiresShipping\":true}}]}},\"variant\":{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Default Title\",\"inventoryQuantity\":0,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/5\",\"tracked\":false,\"requiresShipping\":true},\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Created Board\",\"handle\":\"created-board\",\"status\":\"DRAFT\",\"totalInventory\":0,\"tracksInventory\":false}},\"stock\":{\"id\":\"gid://shopify/InventoryItem/5\",\"tracked\":false,\"requiresShipping\":true,\"variant\":{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Default Title\",\"sku\":null,\"inventoryQuantity\":0,\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Created Board\",\"handle\":\"created-board\",\"status\":\"DRAFT\",\"totalInventory\":0,\"tracksInventory\":false}}}}}"
  assert store.get_log(next_proxy.store)
    |> list.length
    == 1
}

pub fn product_create_defaults_missing_vendor_from_shop_origin_test() {
  let proxy =
    draft_proxy.with_config(Config(
      read_mode: Snapshot,
      unsupported_mutation_mode: PassthroughUnsupportedMutations,
      port: 4000,
      shopify_admin_origin: "https://acme.myshopify.com",
      snapshot_path: None,
    ))
  let query =
    "mutation { productCreate(product: { title: \\\"Origin Vendor\\\" }) { product { id title vendor } userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Origin Vendor\",\"vendor\":\"acme\"},\"userErrors\":[]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Staged
}

pub fn product_variant_mutations_recompute_product_derived_fields_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { productCreate(product: { title: \\\"Hat\\\", productOptions: [{ name: \\\"Color\\\", values: [{ name: \\\"Red\\\" }] }] }) { product { id priceRangeV2 { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory variants(first: 10) { nodes { id title price selectedOptions { name value } } } } userErrors { field message } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"priceRangeV2\":{\"minVariantPrice\":{\"amount\":\"0.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"0.00\",\"currencyCode\":\"USD\"}},\"totalVariants\":1,\"hasOnlyDefaultVariant\":false,\"hasOutOfStockVariants\":false,\"tracksInventory\":false,\"totalInventory\":0,\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/4\",\"title\":\"Red\",\"price\":\"0.00\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"}]}]}},\"userErrors\":[]}}}"

  let product_id = "gid://shopify/Product/1?shopify-draft-proxy=synthetic"
  let price_update_query =
    "mutation { productVariantsBulkUpdate(productId: \\\""
    <> product_id
    <> "\\\", variants: [{ id: \\\"gid://shopify/ProductVariant/4\\\", price: \\\"10.00\\\" }]) { product { priceRangeV2 { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory } productVariants { id price } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(price_update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"productVariantsBulkUpdate\":{\"product\":{\"priceRangeV2\":{\"minVariantPrice\":{\"amount\":\"10.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"10.00\",\"currencyCode\":\"USD\"}},\"totalVariants\":1,\"hasOnlyDefaultVariant\":false,\"hasOutOfStockVariants\":false,\"tracksInventory\":false,\"totalInventory\":0},\"productVariants\":[{\"id\":\"gid://shopify/ProductVariant/4\",\"price\":\"10.00\"}],\"userErrors\":[]}}}"

  let bulk_query =
    "mutation { productVariantsBulkCreate(productId: \\\""
    <> product_id
    <> "\\\", variants: [{ optionValues: [{ optionName: \\\"Color\\\", name: \\\"Blue\\\" }], price: \\\"5.00\\\" }, { optionValues: [{ optionName: \\\"Color\\\", name: \\\"Green\\\" }], price: \\\"20.00\\\" }]) { product { id priceRangeV2 { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } priceRange { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory variants(first: 10) { nodes { title price selectedOptions { name value } } } } productVariants { title price } userErrors { field message code } } }"
  let #(Response(status: bulk_status, body: bulk_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(bulk_query))
  assert bulk_status == 200
  assert json.to_string(bulk_body)
    == "{\"data\":{\"productVariantsBulkCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"priceRangeV2\":{\"minVariantPrice\":{\"amount\":\"5.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"20.00\",\"currencyCode\":\"USD\"}},\"priceRange\":{\"minVariantPrice\":{\"amount\":\"5.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"20.00\",\"currencyCode\":\"USD\"}},\"totalVariants\":3,\"hasOnlyDefaultVariant\":false,\"hasOutOfStockVariants\":true,\"tracksInventory\":true,\"totalInventory\":0,\"variants\":{\"nodes\":[{\"title\":\"Red\",\"price\":\"10.00\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Red\"}]},{\"title\":\"Blue\",\"price\":\"5.00\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Blue\"}]},{\"title\":\"Green\",\"price\":\"20.00\",\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Green\"}]}]}},\"productVariants\":[{\"title\":\"Blue\",\"price\":\"5.00\"},{\"title\":\"Green\",\"price\":\"20.00\"}],\"userErrors\":[]}}}"

  let read_query =
    "query { product(id: \\\""
    <> product_id
    <> "\\\") { priceRangeV2 { minVariantPrice { amount currencyCode } maxVariantPrice { amount currencyCode } } totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_query))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"priceRangeV2\":{\"minVariantPrice\":{\"amount\":\"5.00\",\"currencyCode\":\"USD\"},\"maxVariantPrice\":{\"amount\":\"20.00\",\"currencyCode\":\"USD\"}},\"totalVariants\":3,\"hasOnlyDefaultVariant\":false,\"hasOutOfStockVariants\":true,\"tracksInventory\":true,\"totalInventory\":0}}}"
}

pub fn generated_product_handles_increment_numeric_suffixes_test() {
  let query =
    "mutation { productCreate(product: { title: \\\"Red shirt\\\" }) { product { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: fourth_status, body: fourth_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert third_status == 200
  assert fourth_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-1\"},\"userErrors\":[]}}}"
  assert json.to_string(third_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(fourth_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn generated_product_handles_increment_existing_numeric_suffix_test() {
  let query =
    "mutation { productCreate(product: { title: \\\"Red shirt 2\\\" }) { product { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn product_set_generated_handles_increment_numeric_suffixes_test() {
  let query =
    "mutation { productSet(input: { title: \\\"Red shirt\\\" }, synchronous: true) { product { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: fourth_status, body: fourth_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert third_status == 200
  assert fourth_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"handle\":\"red-shirt\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"handle\":\"red-shirt-1\"},\"userErrors\":[]}}}"
  assert json.to_string(third_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(fourth_body)
    == "{\"data\":{\"productSet\":{\"product\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn product_duplicate_generated_handles_increment_numeric_suffixes_test() {
  let source_query =
    "mutation { productCreate(product: { title: \\\"Red shirt\\\" }) { product { id handle } userErrors { field message } } }"
  let copy_query =
    "mutation { productCreate(product: { title: \\\"Red shirt Copy\\\" }) { product { handle } userErrors { field message } } }"
  let #(Response(status: source_status, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(source_query),
    )
  let #(Response(status: first_copy_status, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(copy_query))
  let #(Response(status: second_copy_status, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(copy_query))
  let duplicate_query =
    "mutation { productDuplicate(productId: \\\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\\\", newTitle: \\\"Red shirt Copy\\\", synchronous: true) { newProduct { title handle } userErrors { field message } } }"
  let #(Response(status: duplicate_status, body: duplicate_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(duplicate_query))

  assert source_status == 200
  assert first_copy_status == 200
  assert second_copy_status == 200
  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"productDuplicate\":{\"newProduct\":{\"title\":\"Red shirt Copy\",\"handle\":\"red-shirt-copy-2\"},\"userErrors\":[]}}}"
}

pub fn collection_create_generated_handles_increment_numeric_suffixes_test() {
  let query =
    "mutation { collectionCreate(input: { title: \\\"Red shirt\\\" }) { collection { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let #(Response(status: fourth_status, body: fourth_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert third_status == 200
  assert fourth_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-1\"},\"userErrors\":[]}}}"
  assert json.to_string(third_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(fourth_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn collection_create_generated_handles_increment_existing_numeric_suffix_test() {
  let query =
    "mutation { collectionCreate(input: { title: \\\"Red shirt 2\\\" }) { collection { handle } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  let #(Response(status: second_status, body: second_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert second_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-2\"},\"userErrors\":[]}}}"
  assert json.to_string(second_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"handle\":\"red-shirt-3\"},\"userErrors\":[]}}}"
}

pub fn explicit_product_handle_collisions_return_user_errors_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: default_option_store())
  let create_query =
    "mutation { productCreate(product: { title: \\\"Explicit Collision\\\", handle: \\\"optioned-board\\\" }) { product { handle } userErrors { field message } } }"
  let #(Response(status: create_status, body: create_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(create_query))

  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"handle\"],\"message\":\"Handle 'optioned-board' already in use. Please provide a new handle.\"}]}}}"

  let set_query =
    "mutation { productSet(input: { title: \\\"Explicit Collision\\\", handle: \\\"optioned-board\\\" }, synchronous: true) { product { handle } userErrors { field message } } }"
  let #(Response(status: set_status, body: set_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(set_query))

  assert set_status == 200
  assert json.to_string(set_body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"handle\"],\"message\":\"Handle 'optioned-board' already in use. Please provide a new handle.\"}]}}}"
}

pub fn product_create_validation_branches_return_user_errors_test() {
  let blank_query =
    "mutation { productCreate(product: { title: \\\"\\\", vendor: \\\"Hermes\\\" }) { product { id title handle } userErrors { field message code } } }"
  let #(Response(status: blank_status, body: blank_body, ..), blank_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(blank_query))
  assert blank_status == 200
  assert json.to_string(blank_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"
  let assert [blank_entry] = store.get_log(blank_proxy.store)
  assert blank_entry.operation_name == Some("productCreate")
  assert blank_entry.status == store_types.Failed
  assert blank_entry.staged_resource_ids == []

  let missing_title_query =
    "mutation { productCreate(product: { vendor: \\\"Hermes\\\" }) { product { id title handle } userErrors { field message code } } }"
  let #(
    Response(status: missing_title_status, body: missing_title_body, ..),
    missing_title_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(missing_title_query),
    )
  assert missing_title_status == 200
  assert json.to_string(missing_title_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"
  let assert [missing_title_entry] = store.get_log(missing_title_proxy.store)
  assert missing_title_entry.operation_name == Some("productCreate")
  assert missing_title_entry.status == store_types.Failed
  assert missing_title_entry.staged_resource_ids == []

  let long_handle = string.repeat("a", times: 260)
  let handle_query =
    "mutation { productCreate(product: { title: \\\"Too Long\\\", vendor: \\\"Hermes\\\", handle: \\\""
    <> long_handle
    <> "\\\" }) { product { id title handle } userErrors { field message code } } }"
  let #(Response(status: handle_status, body: handle_body, ..), handle_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(handle_query),
    )
  assert handle_status == 200
  assert json.to_string(handle_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"handle\"],\"message\":\"Handle is too long (maximum is 255 characters)\",\"code\":null}]}}}"
  let assert [handle_entry] = store.get_log(handle_proxy.store)
  assert handle_entry.operation_name == Some("productCreate")
  assert handle_entry.status == store_types.Failed
  assert handle_entry.staged_resource_ids == []

  let long_vendor = string.repeat("v", times: 256)
  let vendor_query =
    "mutation { productCreate(product: { title: \\\"Too Long Vendor\\\", vendor: \\\""
    <> long_vendor
    <> "\\\" }) { product { id title vendor } userErrors { field message code } } }"
  let #(Response(status: vendor_status, body: vendor_body, ..), vendor_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(vendor_query),
    )
  assert vendor_status == 200
  assert json.to_string(vendor_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"vendor\"],\"message\":\"Vendor is too long (maximum is 255 characters)\",\"code\":null}]}}}"
  let assert [vendor_entry] = store.get_log(vendor_proxy.store)
  assert vendor_entry.operation_name == Some("productCreate")
  assert vendor_entry.status == store_types.Failed
  assert vendor_entry.staged_resource_ids == []

  let long_product_type = string.repeat("p", times: 256)
  let product_type_query =
    "mutation { productCreate(product: { title: \\\"Too Long Type\\\", vendor: \\\"Hermes\\\", productType: \\\""
    <> long_product_type
    <> "\\\" }) { product { id title productType } userErrors { field message code } } }"
  let #(
    Response(status: product_type_status, body: product_type_body, ..),
    product_type_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(product_type_query),
    )
  assert product_type_status == 200
  assert json.to_string(product_type_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"productType\"],\"message\":\"Product type is too long (maximum is 255 characters)\",\"code\":null},{\"field\":[\"customProductType\"],\"message\":\"Custom product type is too long (maximum is 255 characters)\",\"code\":null}]}}}"
  let assert [product_type_entry] = store.get_log(product_type_proxy.store)
  assert product_type_entry.operation_name == Some("productCreate")
  assert product_type_entry.status == store_types.Failed
  assert product_type_entry.staged_resource_ids == []

  let long_description = string.repeat("x", times: 524_288)
  let description_query =
    "mutation { productCreate(product: { title: \\\"Too Big Body\\\", vendor: \\\"Hermes\\\", descriptionHtml: \\\""
    <> long_description
    <> "\\\" }) { product { id title descriptionHtml } userErrors { field message code } } }"
  let #(
    Response(status: description_status, body: description_body, ..),
    description_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(description_query),
    )
  assert description_status == 200
  assert json.to_string(description_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"bodyHtml\"],\"message\":\"Body (HTML) is too big (maximum is 512 KB)\",\"code\":null}]}}}"
  let assert [description_entry] = store.get_log(description_proxy.store)
  assert description_entry.operation_name == Some("productCreate")
  assert description_entry.status == store_types.Failed
  assert description_entry.staged_resource_ids == []

  let variant_query =
    "mutation { productCreate(product: { title: \\\"Invalid Variant Slice\\\", vendor: \\\"Hermes\\\", variants: [{ price: \\\"-5\\\" }] }) { product { id } userErrors { field message code } } }"
  let #(Response(status: variant_status, body: variant_body, ..), variant_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(variant_query),
    )
  assert variant_status == 200
  assert json.to_string(variant_body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"variants\",\"0\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"}]}}}"
  assert store.list_effective_products(variant_proxy.store) == []
}

pub fn collection_create_rejects_long_title_and_handle_test() {
  let long_title = string.repeat("T", times: 256)
  let title_query =
    "mutation { collectionCreate(input: { title: \\\""
    <> long_title
    <> "\\\" }) { collection { id } userErrors { field message code } } }"
  let #(Response(status: title_status, body: title_body, ..), title_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(title_query))

  assert title_status == 200
  assert json.to_string(title_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title is too long (maximum is 255 characters)\",\"code\":\"INVALID\"}]}}}"
  assert store.get_log(title_proxy.store)
    |> list.length
    == 1

  let long_handle = string.repeat("h", times: 256)
  let handle_query =
    "mutation { collectionCreate(input: { title: \\\"Handle Probe\\\", handle: \\\""
    <> long_handle
    <> "\\\" }) { collection { id } userErrors { field message code } } }"
  let #(Response(status: handle_status, body: handle_body, ..), handle_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(handle_query),
    )

  assert handle_status == 200
  assert json.to_string(handle_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"handle\"],\"message\":\"Handle is too long (maximum is 255 characters)\",\"code\":\"INVALID\"}]}}}"
  assert store.get_log(handle_proxy.store)
    |> list.length
    == 1
}

pub fn collection_create_allows_reserved_like_titles_from_live_probe_test() {
  let create_query =
    "mutation { collectionCreate(input: { title: \\\"Frontpage\\\" }) { collection { id title handle } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(create_query),
    )

  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/1\",\"title\":\"Frontpage\",\"handle\":\"frontpage\"},\"userErrors\":[]}}}"
  assert store.get_log(proxy.store)
    |> list.length
    == 1

  let proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: collection_membership_store(),
    )
  let update_query =
    "mutation { collectionUpdate(input: { id: \\\"gid://shopify/Collection/custom\\\", title: \\\"Vendors\\\" }) { collection { id title } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(update_query))

  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"collectionUpdate\":{\"collection\":{\"id\":\"gid://shopify/Collection/custom\",\"title\":\"Vendors\"},\"userErrors\":[]}}}"
}

pub fn collection_create_rejects_invalid_sort_order_as_graphql_error_test() {
  let body =
    "{\"query\":\"mutation($input: CollectionInput!) { collectionCreate(input: $input) { collection { id } userErrors { field message } } }\",\"variables\":{\"input\":{\"title\":\"Sort Probe\",\"sortOrder\":\"INVALID_VALUE\"}}}"

  let #(Response(status: status, body: response_body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request_body(body))

  assert status == 200
  let serialized = json.to_string(response_body)
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(serialized, "\"path\":[\"sortOrder\"]")
  assert string.contains(serialized, "\"title\":\"Sort Probe\"")
  assert string.contains(serialized, "\"sortOrder\":\"INVALID_VALUE\"")
  assert string.contains(
    serialized,
    "Variable $input of type CollectionInput! was provided invalid value for sortOrder",
  )
  assert string.contains(
    serialized,
    "Expected \\\"INVALID_VALUE\\\" to be one of: ALPHA_ASC, ALPHA_DESC, BEST_SELLING, CREATED, CREATED_DESC, MANUAL, PRICE_ASC, PRICE_DESC",
  )
  assert store.get_log(next_proxy.store) == []
}

pub fn collection_create_stages_rule_set_and_unique_handles_test() {
  let first =
    "mutation { collectionCreate(input: { title: \\\"Dedup Probe\\\" }) { collection { id handle sortOrder } userErrors { field message } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(first))
  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/1\",\"handle\":\"dedup-probe\",\"sortOrder\":\"BEST_SELLING\"},\"userErrors\":[]}}}"

  let second =
    "mutation { collectionCreate(input: { title: \\\"Dedup Probe\\\" }) { collection { id handle } userErrors { field message } } }"
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(second))
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/3\",\"handle\":\"dedup-probe-1\"},\"userErrors\":[]}}}"

  let third =
    "mutation { collectionCreate(input: { title: \\\"Dedup Probe\\\" }) { collection { id handle } userErrors { field message } } }"
  let #(Response(status: third_status, body: third_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(third))
  assert third_status == 200
  assert json.to_string(third_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/5\",\"handle\":\"dedup-probe-2\"},\"userErrors\":[]}}}"

  let smart =
    "mutation { collectionCreate(input: { title: \\\"Smart Probe\\\", ruleSet: { appliedDisjunctively: false, rules: [{ column: TITLE, relation: CONTAINS, condition: \\\"Probe\\\" }] } }) { collection { id ruleSet { appliedDisjunctively rules { column relation condition } } } userErrors { field message } } }"
  let #(Response(status: smart_status, body: smart_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(smart))
  assert smart_status == 200
  assert json.to_string(smart_body)
    == "{\"data\":{\"collectionCreate\":{\"collection\":{\"id\":\"gid://shopify/Collection/7\",\"ruleSet\":{\"appliedDisjunctively\":false,\"rules\":[{\"column\":\"TITLE\",\"relation\":\"CONTAINS\",\"condition\":\"Probe\"}]}},\"userErrors\":[]}}}"
}

pub fn collection_add_remove_products_updates_count_and_rejects_smart_collections_test() {
  let proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: collection_membership_store(),
    )
  let add_query =
    "mutation { collectionAddProducts(id: \\\"gid://shopify/Collection/custom\\\", productIds: [\\\"gid://shopify/Product/second\\\"]) { collection { id products(first: 10) { nodes { id } } } userErrors { field message } } }"
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))

  assert add_status == 200
  assert json.to_string(add_body)
    == "{\"data\":{\"collectionAddProducts\":{\"collection\":{\"id\":\"gid://shopify/Collection/custom\",\"products\":{\"nodes\":[{\"id\":\"gid://shopify/Product/optioned\"},{\"id\":\"gid://shopify/Product/second\"}]}},\"userErrors\":[]}}}"

  let remove_query =
    "mutation { collectionRemoveProducts(id: \\\"gid://shopify/Collection/custom\\\", productIds: [\\\"gid://shopify/Product/optioned\\\"]) { job { id done } userErrors { field message } } }"
  let #(Response(status: remove_status, body: remove_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(remove_query))
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"collectionRemoveProducts\":{\"job\":{\"id\":\"gid://shopify/Job/2\",\"done\":false},\"userErrors\":[]}}}"

  let read_query =
    "query { collection(id: \\\"gid://shopify/Collection/custom\\\") { id productsCount { count precision } products(first: 10) { nodes { id } } } }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(read_query))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"collection\":{\"id\":\"gid://shopify/Collection/custom\",\"productsCount\":{\"count\":1,\"precision\":\"EXACT\"},\"products\":{\"nodes\":[{\"id\":\"gid://shopify/Product/second\"}]}}}}"

  let smart_add =
    "mutation { collectionAddProducts(id: \\\"gid://shopify/Collection/smart\\\", productIds: [\\\"gid://shopify/Product/second\\\"]) { collection { id } userErrors { field message } } }"
  let #(Response(status: smart_add_status, body: smart_add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(smart_add))
  assert smart_add_status == 200
  assert json.to_string(smart_add_body)
    == "{\"data\":{\"collectionAddProducts\":{\"collection\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Can't manually add products to a smart collection\"}]}}}"

  let smart_add_v2 =
    "mutation { collectionAddProductsV2(id: \\\"gid://shopify/Collection/smart\\\", productIds: [\\\"gid://shopify/Product/second\\\"]) { job { id done } userErrors { field message } } }"
  let #(
    Response(status: smart_add_v2_status, body: smart_add_v2_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(smart_add_v2))
  assert smart_add_v2_status == 200
  assert json.to_string(smart_add_v2_body)
    == "{\"data\":{\"collectionAddProductsV2\":{\"job\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Can't manually add products to a smart collection\"}]}}}"

  let smart_remove =
    "mutation { collectionRemoveProducts(id: \\\"gid://shopify/Collection/smart\\\", productIds: [\\\"gid://shopify/Product/optioned\\\"]) { job { id done } userErrors { field message } } }"
  let #(Response(status: smart_remove_status, body: smart_remove_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(smart_remove))
  assert smart_remove_status == 200
  assert json.to_string(smart_remove_body)
    == "{\"data\":{\"collectionRemoveProducts\":{\"job\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Can't manually remove products from a smart collection\"}]}}}"
}

pub fn collection_async_product_membership_jobs_and_caps_test() {
  let proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: collection_membership_store(),
    )
  let add_success =
    "mutation { collectionAddProductsV2(id: \\\"gid://shopify/Collection/custom\\\", productIds: [\\\"gid://shopify/Product/second\\\"]) { job { id done query { __typename } } userErrors { field message } } }"
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_success))
  assert add_status == 200
  assert json.to_string(add_body)
    == "{\"data\":{\"collectionAddProductsV2\":{\"job\":{\"id\":\"gid://shopify/Job/1\",\"done\":false,\"query\":null},\"userErrors\":[]}}}"

  let add_job_read =
    "query { job(id: \\\"gid://shopify/Job/1\\\") { __typename id done query { __typename } } }"
  let #(Response(status: add_job_status, body: add_job_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_job_read))
  assert add_job_status == 200
  assert json.to_string(add_job_body)
    == "{\"data\":{\"job\":{\"__typename\":\"Job\",\"id\":\"gid://shopify/Job/1\",\"done\":true,\"query\":{\"__typename\":\"QueryRoot\"}}}}"

  let unknown_add =
    "mutation { collectionAddProductsV2(id: \\\"gid://shopify/Collection/custom\\\", productIds: [\\\"gid://shopify/Product/missing\\\"]) { job { id done query { __typename } } userErrors { field message } } }"
  let #(Response(status: unknown_add_status, body: unknown_add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(unknown_add))
  assert unknown_add_status == 200
  let unknown_add_json = json.to_string(unknown_add_body)
  assert string.contains(
    unknown_add_json,
    "\"job\":{\"id\":\"gid://shopify/Job/",
  )
  assert string.contains(unknown_add_json, "\"done\":false,\"query\":null")
  assert string.contains(unknown_add_json, "\"userErrors\":[]")

  let too_many_ids =
    repeated_product_ids_csv("gid://shopify/Product/second", 251)
  let too_many_add =
    "mutation { collectionAddProductsV2(id: \\\"gid://shopify/Collection/custom\\\", productIds: ["
    <> too_many_ids
    <> "]) { job { id done } userErrors { field message } } }"
  let #(
    Response(status: too_many_add_status, body: too_many_add_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(too_many_add))
  let too_many_add_json = json.to_string(too_many_add_body)
  assert too_many_add_status == 200
  assert !string.contains(too_many_add_json, "\"data\"")
  assert string.contains(
    too_many_add_json,
    "\"message\":\"The input array size of 251 is greater than the maximum allowed of 250.\"",
  )
  assert string.contains(
    too_many_add_json,
    "\"path\":[\"collectionAddProductsV2\",\"productIds\"]",
  )
  assert string.contains(
    too_many_add_json,
    "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"",
  )

  let remove_success =
    "mutation { collectionRemoveProducts(id: \\\"gid://shopify/Collection/custom\\\", productIds: [\\\"gid://shopify/Product/second\\\"]) { job { id done query { __typename } } userErrors { field message } } }"
  let #(Response(status: remove_status, body: remove_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(remove_success))
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"collectionRemoveProducts\":{\"job\":{\"id\":\"gid://shopify/Job/5\",\"done\":false,\"query\":null},\"userErrors\":[]}}}"

  let remove_job_read =
    "query { job(id: \\\"gid://shopify/Job/5\\\") { __typename id done query { __typename } } }"
  let #(Response(status: remove_job_status, body: remove_job_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(remove_job_read))
  assert remove_job_status == 200
  assert json.to_string(remove_job_body)
    == "{\"data\":{\"job\":{\"__typename\":\"Job\",\"id\":\"gid://shopify/Job/5\",\"done\":true,\"query\":{\"__typename\":\"QueryRoot\"}}}}"

  let unknown_remove =
    "mutation { collectionRemoveProducts(id: \\\"gid://shopify/Collection/custom\\\", productIds: [\\\"gid://shopify/Product/missing\\\"]) { job { id done query { __typename } } userErrors { field message } } }"
  let #(
    Response(status: unknown_remove_status, body: unknown_remove_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(unknown_remove))
  assert unknown_remove_status == 200
  let unknown_remove_json = json.to_string(unknown_remove_body)
  assert string.contains(
    unknown_remove_json,
    "\"job\":{\"id\":\"gid://shopify/Job/",
  )
  assert string.contains(unknown_remove_json, "\"done\":false,\"query\":null")
  assert string.contains(unknown_remove_json, "\"userErrors\":[]")

  let too_many_remove =
    "mutation { collectionRemoveProducts(id: \\\"gid://shopify/Collection/custom\\\", productIds: ["
    <> too_many_ids
    <> "]) { job { id done } userErrors { field message } } }"
  let #(
    Response(status: too_many_remove_status, body: too_many_remove_body, ..),
    _,
  ) = draft_proxy.process_request(proxy, graphql_request(too_many_remove))
  let too_many_remove_json = json.to_string(too_many_remove_body)
  assert too_many_remove_status == 200
  assert !string.contains(too_many_remove_json, "\"data\"")
  assert string.contains(
    too_many_remove_json,
    "\"message\":\"The input array size of 251 is greater than the maximum allowed of 250.\"",
  )
  assert string.contains(
    too_many_remove_json,
    "\"path\":[\"collectionRemoveProducts\",\"productIds\"]",
  )
  assert string.contains(
    too_many_remove_json,
    "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"",
  )
}

pub fn collection_update_rejects_invalid_fields_and_custom_to_smart_switch_test() {
  let proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: collection_membership_store(),
    )
  let long_title = string.repeat("T", times: 256)
  let title_query =
    "mutation { collectionUpdate(input: { id: \\\"gid://shopify/Collection/custom\\\", title: \\\""
    <> long_title
    <> "\\\" }) { collection { id } userErrors { field message } } }"
  let #(Response(status: title_status, body: title_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(title_query))
  assert title_status == 200
  assert json.to_string(title_body)
    == "{\"data\":{\"collectionUpdate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"title\"],\"message\":\"Title is too long (maximum is 255 characters)\"}]}}}"

  let long_handle = string.repeat("h", times: 256)
  let handle_query =
    "mutation { collectionUpdate(input: { id: \\\"gid://shopify/Collection/custom\\\", handle: \\\""
    <> long_handle
    <> "\\\" }) { collection { id } userErrors { field message } } }"
  let #(Response(status: handle_status, body: handle_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(handle_query))
  assert handle_status == 200
  assert json.to_string(handle_body)
    == "{\"data\":{\"collectionUpdate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"handle\"],\"message\":\"Handle is too long (maximum is 255 characters)\"}]}}}"

  let switch_query =
    "mutation { collectionUpdate(input: { id: \\\"gid://shopify/Collection/custom\\\", ruleSet: { appliedDisjunctively: false, rules: [{ column: TITLE, relation: CONTAINS, condition: \\\"Probe\\\" }] } }) { collection { id ruleSet { appliedDisjunctively } } userErrors { field message } } }"
  let #(Response(status: switch_status, body: switch_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(switch_query))
  assert switch_status == 200
  assert json.to_string(switch_body)
    == "{\"data\":{\"collectionUpdate\":{\"collection\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Cannot update rule set of a custom collection\"}]}}}"
}

pub fn product_set_rejects_duplicate_variant_option_tuples_test() {
  // Reproduces the QA-witnessed productSet failure mode where the
  // proxy used to accept input with duplicate variant option-value
  // tuples, only discovering Shopify's userErrors at __meta/commit
  // replay. The local proxy must reject the duplicates immediately
  // and record the entry as Failed so commit replay does not re-send
  // a payload Shopify will also reject. See
  // config/parity-specs/products/productSet-duplicate-variants.json.
  let query =
    "mutation { productSet(input: { title: \\\"Duplicate Variant Probe\\\", vendor: \\\"Hermes\\\", status: DRAFT, productOptions: [{ name: \\\"Size\\\", position: 1, values: [{ name: \\\"S\\\" }, { name: \\\"M\\\" }] }, { name: \\\"Color\\\", position: 2, values: [{ name: \\\"Red\\\" }, { name: \\\"Blue\\\" }] }], variants: [{ optionValues: [{ optionName: \\\"Size\\\", name: \\\"S\\\" }, { optionName: \\\"Color\\\", name: \\\"Red\\\" }] }, { optionValues: [{ optionName: \\\"Size\\\", name: \\\"M\\\" }, { optionName: \\\"Color\\\", name: \\\"Blue\\\" }] }, { optionValues: [{ optionName: \\\"Size\\\", name: \\\"S\\\" }, { optionName: \\\"Color\\\", name: \\\"Red\\\" }] }] }, synchronous: true) { product { id } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"variants\",\"2\"],\"message\":\"The variant 'S / Red' already exists. Please change at least one option value.\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productSet")
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_set_requires_variants_when_updating_options_test() {
  let query =
    "mutation { productSet(input: { title: \\\"Options Only\\\", vendor: \\\"Hermes\\\", status: DRAFT, productOptions: [{ name: \\\"Color\\\", position: 1, values: [{ name: \\\"Red\\\" }, { name: \\\"Blue\\\" }] }, { name: \\\"Size\\\", position: 2, values: [{ name: \\\"Small\\\" }, { name: \\\"Large\\\" }] }] }, synchronous: true) { product { id } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"variants\"],\"message\":\"Variants input is required when updating product options\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productSet")
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_set_rejects_product_scalar_validation_errors_test() {
  let long_custom_product_type = string.repeat("c", times: 256)
  let custom_product_type_query =
    "mutation { productSet(input: { title: \\\"Too Long Custom Type\\\", vendor: \\\"Hermes\\\", customProductType: \\\""
    <> long_custom_product_type
    <> "\\\" }, synchronous: true) { product { id title vendor } userErrors { field message code } } }"
  let #(
    Response(
      status: custom_product_type_status,
      body: custom_product_type_body,
      ..,
    ),
    custom_product_type_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(custom_product_type_query),
    )
  assert custom_product_type_status == 200
  assert json.to_string(custom_product_type_body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"customProductType\"],\"message\":\"Custom product type is too long (maximum is 255 characters)\",\"code\":null}]}}}"
  let assert [custom_product_type_entry] =
    store.get_log(custom_product_type_proxy.store)
  assert custom_product_type_entry.operation_name == Some("productSet")
  assert custom_product_type_entry.status == store_types.Failed
  assert custom_product_type_entry.staged_resource_ids == []
}

pub fn product_set_shape_validator_rejects_collection_limits_test() {
  let too_many_variants =
    string.repeat("{ title: \\\"Overflow\\\" },", times: 2049)
  let variant_query =
    "mutation { productSet(input: { title: \\\"Too Many Variants\\\", vendor: \\\"Hermes\\\", variants: ["
    <> too_many_variants
    <> "] }, synchronous: true) { product { id } userErrors { field message code } } }"
  let #(Response(status: variant_status, body: variant_body, ..), variant_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(variant_query),
    )
  let variant_json = json.to_string(variant_body)
  assert variant_status == 200
  assert !string.contains(variant_json, "\"data\"")
  assert string.contains(
    variant_json,
    "\"message\":\"The input array size of 2049 is greater than the maximum allowed of 2048.\"",
  )
  assert string.contains(
    variant_json,
    "\"path\":[\"productSet\",\"input\",\"variants\"]",
  )
  assert string.contains(variant_json, "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"")
  assert store.get_log(variant_proxy.store) == []

  let too_many_option_values =
    string.repeat("{ name: \\\"Overflow\\\" },", times: 101)
  let too_many_files =
    string.repeat(
      "{ originalSource: \\\"https://example.com/file.jpg\\\" },",
      times: 251,
    )
  let shape_query =
    "mutation { productSet(input: { title: \\\"Shape Limits\\\", vendor: \\\"Hermes\\\", productOptions: [{ name: \\\"Color\\\", position: 1, values: ["
    <> too_many_option_values
    <> "] }, { name: \\\"Size\\\", position: 2, values: [{ name: \\\"Small\\\" }] }, { name: \\\"Material\\\", position: 3, values: [{ name: \\\"Cotton\\\" }] }, { name: \\\"Fit\\\", position: 4, values: [{ name: \\\"Regular\\\" }] }], files: ["
    <> too_many_files
    <> "] }, synchronous: true) { product { id } userErrors { field message code } } }"
  let #(Response(status: shape_status, body: shape_body, ..), shape_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(shape_query))
  let shape_json = json.to_string(shape_body)
  assert shape_status == 200
  assert string.contains(shape_json, "\"product\":null")
  assert string.contains(shape_json, "\"field\":[\"input\",\"productOptions\"]")
  assert string.contains(
    shape_json,
    "\"field\":[\"input\",\"productOptions\",\"0\",\"values\"]",
  )
  assert string.contains(shape_json, "\"field\":[\"input\",\"files\"]")
  assert string.contains(shape_json, "\"code\":\"INVALID_INPUT\"")
  let assert [shape_entry] = store.get_log(shape_proxy.store)
  assert shape_entry.operation_name == Some("productSet")
  assert shape_entry.status == store_types.Failed
  assert shape_entry.staged_resource_ids == []

  let too_many_quantities =
    string.repeat(
      "{ locationId: \\\"gid://shopify/Location/1\\\", name: \\\"available\\\", quantity: 1 },",
      times: 251,
    )
  let inventory_query =
    "mutation { productSet(input: { title: \\\"Too Many Quantities\\\", vendor: \\\"Hermes\\\", variants: [{ inventoryQuantities: ["
    <> too_many_quantities
    <> "] }] }, synchronous: true) { product { id } userErrors { field message code } } }"
  let #(
    Response(status: inventory_status, body: inventory_body, ..),
    inventory_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(inventory_query),
    )
  let inventory_json = json.to_string(inventory_body)
  assert inventory_status == 200
  assert !string.contains(inventory_json, "\"data\"")
  assert string.contains(
    inventory_json,
    "\"message\":\"The input array size of 251 is greater than the maximum allowed of 250.\"",
  )
  assert string.contains(
    inventory_json,
    "\"path\":[\"productSet\",\"input\",\"variants\",\"inventoryQuantities\"]",
  )
  assert string.contains(inventory_json, "\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"")
  assert store.get_log(inventory_proxy.store) == []
}

pub fn product_set_rejects_missing_and_suspended_product_references_test() {
  let missing_query =
    "mutation { productSet(input: { id: \\\"gid://shopify/Product/999999999999\\\", title: \\\"Missing\\\", vendor: \\\"Hermes\\\" }, synchronous: true) { product { id } userErrors { field message code } } }"
  let #(Response(status: missing_status, body: missing_body, ..), missing_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(missing_query),
    )
  let missing_json = json.to_string(missing_body)
  assert missing_status == 200
  assert string.contains(missing_json, "\"product\":null")
  assert string.contains(missing_json, "\"field\":[\"input\",\"id\"]")
  assert string.contains(missing_json, "\"code\":\"PRODUCT_DOES_NOT_EXIST\"")
  let assert [missing_entry] = store.get_log(missing_proxy.store)
  assert missing_entry.operation_name == Some("productSet")
  assert missing_entry.status == store_types.Failed
  assert missing_entry.staged_resource_ids == []

  let suspended_query =
    "mutation { productSet(input: { id: \\\"gid://shopify/Product/suspended\\\", title: \\\"Suspended\\\", vendor: \\\"Hermes\\\" }, synchronous: true) { product { id } userErrors { field message code } } }"
  let suspended_proxy =
    proxy_state.DraftProxy(
      ..draft_proxy.new(),
      store: suspended_product_store(),
    )
  let #(
    Response(status: suspended_status, body: suspended_body, ..),
    next_proxy,
  ) =
    draft_proxy.process_request(
      suspended_proxy,
      graphql_request(suspended_query),
    )
  let suspended_json = json.to_string(suspended_body)
  assert suspended_status == 200
  assert string.contains(suspended_json, "\"product\":null")
  assert string.contains(suspended_json, "\"field\":[\"input\"]")
  assert string.contains(suspended_json, "\"code\":\"INVALID_PRODUCT\"")
  let assert [suspended_entry] = store.get_log(next_proxy.store)
  assert suspended_entry.operation_name == Some("productSet")
  assert suspended_entry.status == store_types.Failed
  assert suspended_entry.staged_resource_ids == []
}

pub fn product_bundle_create_rejects_component_validation_branches_test() {
  let missing_query =
    "mutation { productBundleCreate(input: { title: \\\"Bundle\\\", components: [{ productId: \\\"gid://shopify/Product/0\\\", quantity: 1, optionSelections: [] }] }) { productBundleOperation { id status product { id } } userErrors { field message } } }"
  let #(Response(status: missing_status, body: missing_body, ..), missing_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(missing_query),
    )
  assert missing_status == 200
  assert json.to_string(missing_body)
    == "{\"data\":{\"productBundleCreate\":{\"productBundleOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"Failed to locate the following products: [0]\"}]}}}"
  let assert [missing_entry] = store.get_log(missing_proxy.store)
  assert missing_entry.operation_name == Some("productBundleCreate")
  assert missing_entry.status == store_types.Failed
  assert missing_entry.staged_resource_ids == []

  let valid_option_selections =
    "{ componentOptionId: \\\"gid://shopify/ProductOption/color\\\", name: \\\"Color\\\", values: [\\\"Red\\\"] }, { componentOptionId: \\\"gid://shopify/ProductOption/size\\\", name: \\\"Size\\\", values: [\\\"Small\\\"] }"
  let max_quantity_query =
    "mutation { productBundleCreate(input: { title: \\\"Bundle\\\", components: [{ productId: \\\"gid://shopify/Product/optioned\\\", quantity: 2001, optionSelections: ["
    <> valid_option_selections
    <> "] }] }) { productBundleOperation { id status } userErrors { field message } } }"
  let max_quantity_proxy =
    proxy_state.DraftProxy(..draft_proxy.new(), store: option_update_store())
  let #(
    Response(status: max_quantity_status, body: max_quantity_body, ..),
    max_quantity_proxy,
  ) =
    draft_proxy.process_request(
      max_quantity_proxy,
      graphql_request(max_quantity_query),
    )
  assert max_quantity_status == 200
  assert json.to_string(max_quantity_body)
    == "{\"data\":{\"productBundleCreate\":{\"productBundleOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"Quantity cannot be greater than 2000. The following products have a quantity that exceeds the maximum: [optioned]\"}]}}}"
  let assert [max_quantity_entry] = store.get_log(max_quantity_proxy.store)
  assert max_quantity_entry.status == store_types.Failed
  assert max_quantity_entry.staged_resource_ids == []

  let quantity_option_query =
    "mutation { productBundleCreate(input: { title: \\\"Bundle\\\", components: [{ productId: \\\"gid://shopify/Product/optioned\\\", quantityOption: { name: \\\"Pack\\\", values: [{ name: \\\"One\\\", quantity: 1 }] }, optionSelections: ["
    <> valid_option_selections
    <> "] }] }) { productBundleOperation { id status } userErrors { field message } } }"
  let quantity_option_proxy =
    proxy_state.DraftProxy(..draft_proxy.new(), store: option_update_store())
  let #(
    Response(status: quantity_option_status, body: quantity_option_body, ..),
    quantity_option_proxy,
  ) =
    draft_proxy.process_request(
      quantity_option_proxy,
      graphql_request(quantity_option_query),
    )
  assert quantity_option_status == 200
  assert json.to_string(quantity_option_body)
    == "{\"data\":{\"productBundleCreate\":{\"productBundleOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"Quantity options must have at least two values. Invalid quantity options found for components targeting product_ids [optioned].\"}]}}}"
  let assert [quantity_option_entry] =
    store.get_log(quantity_option_proxy.store)
  assert quantity_option_entry.status == store_types.Failed
  assert quantity_option_entry.staged_resource_ids == []

  let invalid_mapping_query =
    "mutation { productBundleCreate(input: { title: \\\"Bundle\\\", components: [{ productId: \\\"gid://shopify/Product/optioned\\\", quantity: 1, optionSelections: ["
    <> valid_option_selections
    <> ", { componentOptionId: \\\"gid://shopify/ProductOption/color\\\", name: \\\"Color\\\", values: [\\\"Red\\\"] }] }] }) { productBundleOperation { id status } userErrors { field message } } }"
  let invalid_mapping_proxy =
    proxy_state.DraftProxy(..draft_proxy.new(), store: option_update_store())
  let #(
    Response(status: invalid_mapping_status, body: invalid_mapping_body, ..),
    invalid_mapping_proxy,
  ) =
    draft_proxy.process_request(
      invalid_mapping_proxy,
      graphql_request(invalid_mapping_query),
    )
  assert invalid_mapping_status == 200
  assert json.to_string(invalid_mapping_body)
    == "{\"data\":{\"productBundleCreate\":{\"productBundleOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"Mapping of components targeting products need to map all of the options of the product. Missing or invalid options found for components targeting product_ids [optioned].\"}]}}}"
  let assert [invalid_mapping_entry] =
    store.get_log(invalid_mapping_proxy.store)
  assert invalid_mapping_entry.status == store_types.Failed
  assert invalid_mapping_entry.staged_resource_ids == []

  let consolidated_option_selections =
    "{ componentOptionId: \\\"gid://shopify/ProductOption/color\\\", name: \\\"Color\\\", values: [\\\"Red\\\"] }, { componentOptionId: \\\"gid://shopify/ProductOption/size\\\", name: \\\"Size\\\", values: [\\\"Small\\\"] }, { componentOptionId: \\\"gid://shopify/ProductOption/material\\\", name: \\\"Material\\\", values: [\\\"Cotton\\\"] }"
  let invalid_consolidated_query =
    "mutation { productBundleCreate(input: { title: \\\"Bundle\\\", components: [{ productId: \\\"gid://shopify/Product/optioned\\\", quantity: 1, optionSelections: ["
    <> consolidated_option_selections
    <> "] }], consolidatedOptions: [{ optionName: \\\"Bundle Color\\\", optionSelections: [{ optionValue: \\\"Red Display\\\", components: [{ componentOptionId: \\\"gid://shopify/ProductOption/color\\\", componentOptionValue: \\\"Blue\\\" }] }] }] }) { productBundleOperation { id status } userErrors { field message } } }"
  let invalid_consolidated_proxy =
    proxy_state.DraftProxy(..draft_proxy.new(), store: three_option_store())
  let #(
    Response(
      status: invalid_consolidated_status,
      body: invalid_consolidated_body,
      ..,
    ),
    invalid_consolidated_proxy,
  ) =
    draft_proxy.process_request(
      invalid_consolidated_proxy,
      graphql_request(invalid_consolidated_query),
    )
  assert invalid_consolidated_status == 200
  assert json.to_string(invalid_consolidated_body)
    == "{\"data\":{\"productBundleCreate\":{\"productBundleOperation\":null,\"userErrors\":[{\"field\":null,\"message\":\"Consolidated option selections are invalid.\"}]}}}"
  let assert [invalid_consolidated_entry] =
    store.get_log(invalid_consolidated_proxy.store)
  assert invalid_consolidated_entry.status == store_types.Failed
  assert invalid_consolidated_entry.staged_resource_ids == []
}

pub fn product_bundle_create_stages_operation_and_operation_read_test() {
  let option_selections =
    "{ componentOptionId: \\\"gid://shopify/ProductOption/color\\\", name: \\\"Color\\\", values: [\\\"Red\\\"] }, { componentOptionId: \\\"gid://shopify/ProductOption/size\\\", name: \\\"Size\\\", values: [\\\"Small\\\"] }, { componentOptionId: \\\"gid://shopify/ProductOption/material\\\", name: \\\"Material\\\", values: [\\\"Cotton\\\"] }"
  let mutation =
    "mutation { productBundleCreate(input: { title: \\\"Bundle\\\", components: [{ productId: \\\"gid://shopify/Product/optioned\\\", quantity: 0, optionSelections: ["
    <> option_selections
    <> "] }] }) { productBundleOperation { id status product { id } } userErrors { field message } } }"
  let proxy =
    proxy_state.DraftProxy(..draft_proxy.new(), store: three_option_store())
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(mutation))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productBundleCreate\":{\"productBundleOperation\":{\"id\":\"gid://shopify/ProductBundleOperation/1\",\"status\":\"CREATED\",\"product\":null},\"userErrors\":[]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productBundleCreate")
  assert entry.status == store_types.Staged
  assert entry.staged_resource_ids == ["gid://shopify/ProductBundleOperation/1"]

  let operation_read =
    "query { productOperation(id: \\\"gid://shopify/ProductBundleOperation/1\\\") { __typename status product { id } ... on ProductBundleOperation { id } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(next_proxy, graphql_request(operation_read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"productOperation\":{\"__typename\":\"ProductBundleOperation\",\"status\":\"ACTIVE\",\"product\":null,\"id\":\"gid://shopify/ProductBundleOperation/1\"}}}"
}

pub fn product_set_async_operation_completes_on_product_operation_read_test() {
  let mutation =
    "mutation { productSet(input: { title: \\\"Async ProductSet\\\", vendor: \\\"Hermes\\\", status: DRAFT }, synchronous: false) { product { id } productSetOperation { id status product { id } userErrors { field message code } } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(mutation))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"productSetOperation\":{\"id\":\"gid://shopify/ProductSetOperation/6\",\"status\":\"CREATED\",\"product\":null,\"userErrors\":[]},\"userErrors\":[]}}}"

  let operation_read =
    "query { productOperation(id: \\\"gid://shopify/ProductSetOperation/6\\\") { __typename status product { id title } ... on ProductSetOperation { id userErrors { field message code } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(next_proxy, graphql_request(operation_read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"productOperation\":{\"__typename\":\"ProductSetOperation\",\"status\":\"COMPLETE\",\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Async ProductSet\"},\"id\":\"gid://shopify/ProductSetOperation/6\",\"userErrors\":[]}}}"
}

pub fn product_create_accepts_legacy_input_argument_shape_test() {
  // Real Shopify accepts both `productCreate(product: ProductCreateInput!)`
  // (current) and `productCreate(input: ProductInput!)` (older API versions).
  // The proxy used to read only `product`, fabricating a misleading
  // `["title"], "Title can't be blank"` userError when callers used the
  // legacy `input:` keyword.
  let query =
    "mutation { productCreate(input: { title: \\\"Legacy Shape\\\", vendor: \\\"Hermes\\\", status: DRAFT }) { product { id title status vendor } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productCreate\":{\"product\":{\"id\":\"gid://shopify/Product/1?shopify-draft-proxy=synthetic\",\"title\":\"Legacy Shape\",\"status\":\"DRAFT\",\"vendor\":\"Hermes\"},\"userErrors\":[]}}}"
}

pub fn product_create_legacy_input_validation_uses_input_field_path_test() {
  let query =
    "mutation { productCreate(input: { title: \\\"\\\", vendor: \\\"Hermes\\\" }) { product { id title } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productCreate\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productCreate")
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_create_missing_input_argument_top_level_error_test() {
  // No `product:` and no `input:` argument → top-level GraphQL error,
  // mirroring real Shopify's `missingRequiredArguments` extension code,
  // rather than fabricating a misleading "Title can't be blank" userError.
  let query =
    "mutation { productCreate { product { id } userErrors { field message } } }"
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"Field 'productCreate' is missing required arguments: product\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"productCreate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"productCreate\",\"arguments\":\"product\"}}]}"
}

pub fn product_variant_create_update_delete_stages_lifecycle_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let create_query =
    "mutation { productVariantCreate(input: { productId: \\\"gid://shopify/Product/optioned\\\", title: \\\"Blue\\\", sku: \\\"BLUE-1\\\", barcode: \\\"2222222222222\\\", price: \\\"12.00\\\", inventoryQuantity: 5, selectedOptions: [{ name: \\\"Color\\\", value: \\\"Blue\\\" }], inventoryItem: { tracked: true, requiresShipping: false } }) { product { id totalInventory tracksInventory } productVariant { id title sku barcode price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } userErrors { field message } } }"

  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productVariantCreate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"totalInventory\":5,\"tracksInventory\":true},\"productVariant\":{\"id\":\"gid://shopify/ProductVariant/1\",\"title\":\"Blue\",\"sku\":\"BLUE-1\",\"barcode\":\"2222222222222\",\"price\":\"12.00\",\"inventoryQuantity\":5,\"selectedOptions\":[{\"name\":\"Color\",\"value\":\"Blue\"}],\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/2\",\"tracked\":true,\"requiresShipping\":false}},\"userErrors\":[]}}}"

  let update_query =
    "mutation { productVariantUpdate(input: { id: \\\"gid://shopify/ProductVariant/1\\\", title: \\\"Blue Deluxe\\\", sku: \\\"BLUE-2\\\", inventoryQuantity: 7, inventoryItem: { tracked: false, requiresShipping: true } }) { product { id totalInventory tracksInventory } productVariant { id title sku inventoryQuantity inventoryItem { id tracked requiresShipping } } userErrors { field message } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"productVariantUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"totalInventory\":0,\"tracksInventory\":false},\"productVariant\":{\"id\":\"gid://shopify/ProductVariant/1\",\"title\":\"Blue Deluxe\",\"sku\":\"BLUE-2\",\"inventoryQuantity\":7,\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/2\",\"tracked\":false,\"requiresShipping\":true}},\"userErrors\":[]}}}"

  let delete_query =
    "mutation { productVariantDelete(id: \\\"gid://shopify/ProductVariant/1\\\") { deletedProductVariantId userErrors { field message } } }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(delete_query))
  assert delete_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"productVariantDelete\":{\"deletedProductVariantId\":\"gid://shopify/ProductVariant/1\",\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/optioned\\\") { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku inventoryQuantity } } } productVariant(id: \\\"gid://shopify/ProductVariant/1\\\") { id } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\",\"totalInventory\":0,\"tracksInventory\":false,\"variants\":{\"nodes\":[{\"id\":\"gid://shopify/ProductVariant/optioned\",\"title\":\"Red / Small\",\"sku\":null,\"inventoryQuantity\":0}]}},\"productVariant\":null}}"
  assert store.get_log(proxy.store)
    |> list.length
    == 3
}

pub fn product_variants_bulk_create_rejects_invalid_scalar_fields_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let long_text = repeated_text("S", 256)
  let valid_options =
    "selectedOptions: [{ name: \\\"Color\\\", value: \\\"Red\\\" }, { name: \\\"Size\\\", value: \\\"Small\\\" }]"
  let query =
    "mutation { productVariantsBulkCreate(productId: \\\"gid://shopify/Product/optioned\\\", variants: [{ price: null, "
    <> valid_options
    <> " }, { price: \\\"-5\\\", "
    <> valid_options
    <> " }, { price: \\\"1000000000000000000\\\", "
    <> valid_options
    <> " }, { price: \\\"10\\\", compareAtPrice: \\\"1000000000000000000\\\", "
    <> valid_options
    <> " }, { price: \\\"10\\\", inventoryItem: { measurement: { weight: { value: -1, unit: KILOGRAMS } } }, "
    <> valid_options
    <> " }, { price: \\\"10\\\", inventoryQuantities: [{ locationId: \\\"gid://shopify/Location/1\\\", availableQuantity: 2000000000 }], "
    <> valid_options
    <> " }, { price: \\\"10\\\", inventoryItem: { sku: \\\""
    <> long_text
    <> "\\\" }, "
    <> valid_options
    <> " }, { price: \\\"10\\\", barcode: \\\""
    <> long_text
    <> "\\\", "
    <> valid_options
    <> " }, { price: \\\"10\\\", selectedOptions: [{ name: \\\"Color\\\", value: \\\""
    <> long_text
    <> "\\\" }, { name: \\\"Size\\\", value: \\\"Small\\\" }] }]) { product { id } productVariants { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productVariantsBulkCreate\":{\"product\":null,\"productVariants\":[],\"userErrors\":[{\"field\":[\"variants\",\"0\",\"price\"],\"message\":\"Price can't be blank\",\"code\":\"INVALID\"},{\"field\":[\"variants\",\"1\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"},{\"field\":[\"variants\",\"2\",\"price\"],\"message\":\"Price must be less than 1000000000000000000\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"3\",\"compareAtPrice\"],\"message\":\"must be less than 1000000000000000000\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"4\"],\"message\":\"Weight must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"},{\"field\":[\"variants\",\"5\",\"inventoryQuantities\"],\"message\":\"Inventory quantity must be less than or equal to 1000000000\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"6\"],\"message\":\"SKU is too long (maximum is 255 characters)\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"6\"],\"message\":\"is too long (maximum is 255 characters)\",\"code\":null},{\"field\":[\"variants\",\"7\",\"barcode\"],\"message\":\"Barcode is too long (maximum is 255 characters)\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"8\",\"selectedOptions\",\"0\",\"value\"],\"message\":\"Option value name is too long\",\"code\":\"INVALID_INPUT\"}]}}}"
  assert store.get_effective_variants_by_product_id(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
    |> list.length
    == 1
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productVariantsBulkCreate")
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn product_variants_bulk_create_rejects_oversized_input_test() {
  let proxy = draft_proxy.new()
  let variants = repeat_csv("{ price: \\\"1\\\" }", 2049)
  let query =
    "mutation { productVariantsBulkCreate(productId: \\\"gid://shopify/Product/optioned\\\", variants: ["
    <> variants
    <> "]) { productVariants { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"The input array size of 2049 is greater than the maximum allowed of 2048.\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"productVariantsBulkCreate\",\"variants\"],\"extensions\":{\"code\":\"MAX_INPUT_SIZE_EXCEEDED\"}}]}"
  assert store.get_log(next_proxy.store) == []
}

pub fn product_variants_bulk_create_rejects_cumulative_variant_cap_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: variant_cap_store())
  let query =
    "mutation { productVariantsBulkCreate(productId: \\\"gid://shopify/Product/optioned\\\", variants: [{ price: \\\"1\\\", selectedOptions: [{ name: \\\"Title\\\", value: \\\"Default Title\\\" }] }]) { product { id } productVariants { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productVariantsBulkCreate\":{\"product\":null,\"productVariants\":[],\"userErrors\":[{\"field\":null,\"message\":\"You can only have a maximum of 2048 variants per product\",\"code\":\"LIMIT_EXCEEDED\"}]}}}"
  assert store.get_effective_variants_by_product_id(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
    |> list.length
    == 2048
}

pub fn product_variants_bulk_update_rejects_invalid_scalar_fields_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { productVariantsBulkUpdate(productId: \\\"gid://shopify/Product/optioned\\\", variants: [{ id: \\\"gid://shopify/ProductVariant/optioned\\\", price: \\\"-5\\\", inventoryQuantity: 1000000001, inventoryItem: { measurement: { weight: { value: 2000000000, unit: KILOGRAMS } } } }]) { product { id } productVariants { id price inventoryQuantity } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productVariantsBulkUpdate\":{\"product\":{\"id\":\"gid://shopify/Product/optioned\"},\"productVariants\":null,\"userErrors\":[{\"field\":[\"variants\",\"0\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"},{\"field\":[\"variants\",\"0\"],\"message\":\"Weight must be less than 2000000000\",\"code\":\"INVALID_INPUT\"},{\"field\":[\"variants\",\"0\",\"inventoryQuantity\"],\"message\":\"Inventory quantity must be less than or equal to 1000000000\",\"code\":\"INVALID_INPUT\"}]}}}"
  let assert [variant] =
    store.get_effective_variants_by_product_id(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
  assert variant.price == Some("0.00")
  assert variant.inventory_quantity == Some(0)
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
}

pub fn inventory_item_update_rejects_invalid_scalar_user_errors_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { inventoryItemUpdate(id: \\\"gid://shopify/InventoryItem/optioned\\\", input: { cost: \\\"-5.00\\\", measurement: { weight: { value: -1, unit: KILOGRAMS } }, countryCodeOfOrigin: US, provinceCodeOfOrigin: \\\"ON\\\", harmonizedSystemCode: \\\"12\\\", countryHarmonizedSystemCodes: [{ countryCode: US, harmonizedSystemCode: \\\"123456\\\" }, { countryCode: US, harmonizedSystemCode: \\\"123456\\\" }] }) { inventoryItem { id countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode measurement { weight { unit value } } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"inventoryItem\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"cost\"],\"message\":\"Cost must be greater than or equal to 0\",\"code\":\"INVALID\"",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"measurement\",\"weight\"]",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"provinceCodeOfOrigin\"]",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"harmonizedSystemCode\"]",
  )
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"countryHarmonizedSystemCodes\",\"1\",\"countryCode\"]",
  )
  assert string.contains(serialized, "\"code\":\"TAKEN\"")
  let assert [variant] =
    store.get_effective_variants_by_product_id(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
  let assert Some(item) = variant.inventory_item
  assert item.country_code_of_origin == None
  assert item.province_code_of_origin == None
  assert item.harmonized_system_code == None
  assert item.measurement == None
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
}

pub fn inventory_item_update_rejects_unknown_country_user_error_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let query =
    "mutation { inventoryItemUpdate(id: \\\"gid://shopify/InventoryItem/optioned\\\", input: { countryCodeOfOrigin: ZZ }) { inventoryItem { id countryCodeOfOrigin } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"inventoryItem\":null")
  assert string.contains(
    serialized,
    "\"field\":[\"input\",\"countryCodeOfOrigin\"]",
  )
  assert string.contains(
    serialized,
    "\"message\":\"Country code of origin is invalid\"",
  )
  assert string.contains(serialized, "\"code\":\"INVALID\"")
  let assert [variant] =
    store.get_effective_variants_by_product_id(
      next_proxy.store,
      "gid://shopify/Product/optioned",
    )
  let assert Some(item) = variant.inventory_item
  assert item.country_code_of_origin == None
}

pub fn inventory_item_update_rejects_weight_unit_variable_coercion_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let body =
    "{\"query\":\"mutation($id: ID!, $input: InventoryItemInput!) { inventoryItemUpdate(id: $id, input: $input) { inventoryItem { id } userErrors { field message code } } }\",\"variables\":{\"id\":\"gid://shopify/InventoryItem/optioned\",\"input\":{\"measurement\":{\"weight\":{\"value\":1,\"unit\":\"STONES\"}}}}}"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request_body(body))
  let serialized = json.to_string(body)

  assert status == 200
  assert string.contains(serialized, "\"errors\":[")
  assert string.contains(serialized, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(
    serialized,
    "\"path\":[\"measurement\",\"weight\",\"unit\"]",
  )
  assert string.contains(serialized, "Expected \\\"STONES\\\" to be one of:")
  assert store.get_log(next_proxy.store) == []
}

pub fn product_variant_create_and_update_reject_invalid_scalars_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: option_update_store())
  let create_query =
    "mutation { productVariantCreate(input: { productId: \\\"gid://shopify/Product/optioned\\\", price: \\\"-5\\\", selectedOptions: [{ name: \\\"Color\\\", value: \\\"Blue\\\" }] }) { product { id } productVariant { id } userErrors { field message code } } }"

  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"productVariantCreate\":{\"product\":null,\"productVariant\":null,\"userErrors\":[{\"field\":[\"input\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"GREATER_THAN_OR_EQUAL_TO\"}]}}}"

  let update_query =
    "mutation { productVariantUpdate(input: { id: \\\"gid://shopify/ProductVariant/optioned\\\", compareAtPrice: \\\"1000000000000000000\\\" }) { product { id } productVariant { id compareAtPrice } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"productVariantUpdate\":{\"product\":null,\"productVariant\":null,\"userErrors\":[{\"field\":[\"input\",\"compareAtPrice\"],\"message\":\"must be less than 1000000000000000000\",\"code\":\"INVALID_INPUT\"}]}}}"

  let assert [variant] =
    store.get_effective_variants_by_product_id(
      proxy.store,
      "gid://shopify/Product/optioned",
    )
  assert variant.price == Some("0.00")
  assert variant.compare_at_price == None
  let assert [create_entry, update_entry] = store.get_log(proxy.store)
  assert create_entry.status == store_types.Failed
  assert update_entry.status == store_types.Failed
}

pub fn product_set_rejects_invalid_variant_scalars_test() {
  let query =
    "mutation { productSet(input: { title: \\\"Scalar Validation Probe\\\", vendor: \\\"Hermes\\\", productOptions: [{ name: \\\"Title\\\", position: 1, values: [{ name: \\\"Default Title\\\" }] }], variants: [{ price: \\\"-5\\\", optionValues: [{ optionName: \\\"Title\\\", name: \\\"Default Title\\\" }] }] }, synchronous: true) { product { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"productSet\":{\"product\":null,\"userErrors\":[{\"field\":[\"input\",\"variants\",\"0\",\"price\"],\"message\":\"Price must be greater than or equal to 0\",\"code\":\"INVALID_VARIANT\"}]}}}"
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.operation_name == Some("productSet")
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
}

pub fn inventory_shipment_extended_roots_stage_locally_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_transfer("READY_TO_SHIP", 5),
    )
  let create_query =
    "mutation { inventoryShipmentCreate(input: { movementId: \\\"gid://shopify/InventoryTransfer/7001\\\", trackingInput: { trackingNumber: \\\"1Z999\\\", company: \\\"UPS\\\" }, lineItems: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 2 }] }) { inventoryShipment { id status lineItemTotalQuantity tracking { trackingNumber company } lineItems(first: 10) { nodes { id quantity unreceivedQuantity } } } userErrors { field message code } } }"

  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"inventoryShipmentCreate\":{\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"status\":\"DRAFT\",\"lineItemTotalQuantity\":2,\"tracking\":{\"trackingNumber\":\"1Z999\",\"company\":\"UPS\"},\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryShipmentLineItem/2\",\"quantity\":2,\"unreceivedQuantity\":2}]}},\"userErrors\":[]}}}"

  let transit_query =
    "mutation { inventoryShipmentMarkInTransit(id: \\\"gid://shopify/InventoryShipment/1\\\") { inventoryShipment { id status } userErrors { field message code } } }"
  let #(Response(status: transit_status, body: transit_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(transit_query))
  assert transit_status == 200
  assert json.to_string(transit_body)
    == "{\"data\":{\"inventoryShipmentMarkInTransit\":{\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"status\":\"IN_TRANSIT\"},\"userErrors\":[]}}}"

  let add_query =
    "mutation { inventoryShipmentAddItems(id: \\\"gid://shopify/InventoryShipment/1\\\", lineItems: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 3 }]) { addedItems { id quantity unreceivedQuantity } inventoryShipment { id lineItemTotalQuantity } userErrors { field message code } } }"
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))
  assert add_status == 200
  assert json.to_string(add_body)
    == "{\"data\":{\"inventoryShipmentAddItems\":{\"addedItems\":[{\"id\":\"gid://shopify/InventoryShipmentLineItem/5\",\"quantity\":3,\"unreceivedQuantity\":3}],\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"lineItemTotalQuantity\":5},\"userErrors\":[]}}}"

  let tracking_query =
    "mutation { inventoryShipmentSetTracking(id: \\\"gid://shopify/InventoryShipment/1\\\", tracking: { trackingNumber: \\\"TRACK-2\\\", company: \\\"USPS\\\" }) { inventoryShipment { id tracking { trackingNumber company } } userErrors { field message code } } }"
  let #(Response(status: tracking_status, body: tracking_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(tracking_query))
  assert tracking_status == 200
  assert json.to_string(tracking_body)
    == "{\"data\":{\"inventoryShipmentSetTracking\":{\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"tracking\":{\"trackingNumber\":\"TRACK-2\",\"company\":\"USPS\"}},\"userErrors\":[]}}}"

  let remove_query =
    "mutation { inventoryShipmentRemoveItems(id: \\\"gid://shopify/InventoryShipment/1\\\", lineItems: [\\\"gid://shopify/InventoryShipmentLineItem/2\\\"]) { inventoryShipment { id lineItemTotalQuantity lineItems(first: 10) { nodes { id quantity } } } userErrors { field message code } } }"
  let #(Response(status: remove_status, body: remove_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(remove_query))
  assert remove_status == 200
  assert json.to_string(remove_body)
    == "{\"data\":{\"inventoryShipmentRemoveItems\":{\"inventoryShipment\":{\"id\":\"gid://shopify/InventoryShipment/1\",\"lineItemTotalQuantity\":3,\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryShipmentLineItem/5\",\"quantity\":3}]}},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "query { inventoryItem(id: \\\"gid://shopify/InventoryItem/tracked\\\") { variant { inventoryQuantity } inventoryLevels(first: 1) { nodes { quantities(names: [\\\"available\\\", \\\"on_hand\\\", \\\"incoming\\\"]) { name quantity } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"inventoryItem\":{\"variant\":{\"inventoryQuantity\":1},\"inventoryLevels\":{\"nodes\":[{\"quantities\":[{\"name\":\"available\",\"quantity\":1},{\"name\":\"on_hand\",\"quantity\":1},{\"name\":\"incoming\",\"quantity\":3},{\"name\":\"reserved\",\"quantity\":0}]}]}}}}"
  let assert [
    create_entry,
    transit_entry,
    add_entry,
    tracking_entry,
    remove_entry,
  ] = store.get_log(proxy.store)
  assert create_entry.operation_name == Some("inventoryShipmentCreate")
  assert transit_entry.operation_name == Some("inventoryShipmentMarkInTransit")
  assert add_entry.operation_name == Some("inventoryShipmentAddItems")
  assert tracking_entry.operation_name == Some("inventoryShipmentSetTracking")
  assert remove_entry.operation_name == Some("inventoryShipmentRemoveItems")
  assert string.contains(create_entry.query, "inventoryShipmentCreate")
  assert string.contains(remove_entry.query, "inventoryShipmentRemoveItems")
}

pub fn inventory_shipment_create_validates_transfer_membership_and_quantity_test() {
  let unknown_query =
    "mutation { inventoryShipmentCreate(input: { transferId: \\\"gid://shopify/InventoryTransfer/missing\\\", lineItems: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 1 }] }) { inventoryShipment { id } userErrors { field message code } } }"
  let #(unknown_status, unknown_body, unknown_proxy) =
    run_product_mutation(tracked_inventory_store(), unknown_query)
  assert unknown_status == 200
  assert unknown_body
    == "{\"data\":{\"inventoryShipmentCreate\":{\"inventoryShipment\":null,\"userErrors\":[{\"field\":[\"transferId\"],\"message\":\"The specified inventory transfer could not be found.\",\"code\":\"NOT_FOUND\"}]}}}"
  let assert [unknown_entry] = store.get_log(unknown_proxy.store)
  assert unknown_entry.status == store_types.Failed

  let received_query =
    "mutation { inventoryShipmentCreate(input: { transferId: \\\"gid://shopify/InventoryTransfer/7001\\\", lineItems: [{ inventoryTransferLineItemId: \\\"gid://shopify/InventoryTransferLineItem/7001\\\", inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 1 }] }) { inventoryShipment { id } userErrors { field message code } } }"
  let #(received_status, received_body, received_proxy) =
    run_product_mutation(
      tracked_inventory_store_with_transfer("RECEIVED", 2),
      received_query,
    )
  assert received_status == 200
  assert string.contains(received_body, "\"code\":\"INVALID_STATE\"")
  let assert [received_entry] = store.get_log(received_proxy.store)
  assert received_entry.status == store_types.Failed

  let wrong_line_query =
    "mutation { inventoryShipmentCreate(input: { transferId: \\\"gid://shopify/InventoryTransfer/7001\\\", lineItems: [{ inventoryTransferLineItemId: \\\"gid://shopify/InventoryTransferLineItem/not-member\\\", inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 1 }] }) { inventoryShipment { id } userErrors { field message code } } }"
  let #(wrong_line_status, wrong_line_body, wrong_line_proxy) =
    run_product_mutation(
      tracked_inventory_store_with_transfer("READY_TO_SHIP", 2),
      wrong_line_query,
    )
  assert wrong_line_status == 200
  assert wrong_line_body
    == "{\"data\":{\"inventoryShipmentCreate\":{\"inventoryShipment\":null,\"userErrors\":[{\"field\":[\"lineItems\",\"0\",\"inventoryTransferLineItemId\"],\"message\":\"The specified inventory transfer line item could not be found.\",\"code\":\"NOT_FOUND\"}]}}}"
  let assert [wrong_line_entry] = store.get_log(wrong_line_proxy.store)
  assert wrong_line_entry.status == store_types.Failed

  let quantity_query =
    "mutation { inventoryShipmentCreate(input: { transferId: \\\"gid://shopify/InventoryTransfer/7001\\\", lineItems: [{ inventoryTransferLineItemId: \\\"gid://shopify/InventoryTransferLineItem/7001\\\", inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 3 }] }) { inventoryShipment { id } userErrors { field message code } } }"
  let #(quantity_status, quantity_body, quantity_proxy) =
    run_product_mutation(
      tracked_inventory_store_with_transfer("READY_TO_SHIP", 2),
      quantity_query,
    )
  assert quantity_status == 200
  assert quantity_body
    == "{\"data\":{\"inventoryShipmentCreate\":{\"inventoryShipment\":null,\"userErrors\":[{\"field\":[\"lineItems\",\"0\",\"quantity\"],\"message\":\"Quantity exceeds the remaining quantity for the inventory transfer line item.\",\"code\":\"QUANTITY_EXCEEDS_REMAINING\"}]}}}"
  let assert [quantity_entry] = store.get_log(quantity_proxy.store)
  assert quantity_entry.status == store_types.Failed
}

pub fn inventory_shipment_mutators_validate_state_and_remaining_quantity_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_transfer("READY_TO_SHIP", 2),
    )
  let create_query =
    "mutation { inventoryShipmentCreate(input: { transferId: \\\"gid://shopify/InventoryTransfer/7001\\\", lineItems: [{ inventoryTransferLineItemId: \\\"gid://shopify/InventoryTransferLineItem/7001\\\", inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 1 }] }) { inventoryShipment { id lineItems(first: 5) { nodes { id quantity } } } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let add_query =
    "mutation { inventoryShipmentAddItems(id: \\\"gid://shopify/InventoryShipment/1\\\", lineItems: [{ inventoryTransferLineItemId: \\\"gid://shopify/InventoryTransferLineItem/7001\\\", inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 2 }]) { inventoryShipment { id } addedItems { id } userErrors { field message code } } }"
  let #(Response(status: add_status, body: add_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(add_query))
  assert add_status == 200
  assert string.contains(
    json.to_string(add_body),
    "\"code\":\"QUANTITY_EXCEEDS_REMAINING\"",
  )

  let update_query =
    "mutation { inventoryShipmentUpdateItemQuantities(id: \\\"gid://shopify/InventoryShipment/1\\\", items: [{ shipmentLineItemId: \\\"gid://shopify/InventoryShipmentLineItem/2\\\", quantity: 3 }]) { shipment { id } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert string.contains(
    json.to_string(update_body),
    "\"code\":\"QUANTITY_EXCEEDS_REMAINING\"",
  )

  let receive_query =
    "mutation { inventoryShipmentReceive(id: \\\"gid://shopify/InventoryShipment/1\\\", lineItems: [{ shipmentLineItemId: \\\"gid://shopify/InventoryShipmentLineItem/2\\\", quantity: 1, reason: ACCEPTED }]) { inventoryShipment { id status } userErrors { field message code } } }"
  let #(Response(status: receive_status, body: receive_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(receive_query))
  assert receive_status == 200
  assert string.contains(
    json.to_string(receive_body),
    "\"code\":\"INVALID_STATE\"",
  )

  let transit_query =
    "mutation { inventoryShipmentMarkInTransit(id: \\\"gid://shopify/InventoryShipment/1\\\") { inventoryShipment { id status } userErrors { field message code } } }"
  let #(Response(status: transit_status, body: transit_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(transit_query))
  assert transit_status == 200
  assert string.contains(json.to_string(transit_body), "\"userErrors\":[]")

  let repeated_transit_query =
    "mutation { inventoryShipmentMarkInTransit(id: \\\"gid://shopify/InventoryShipment/1\\\") { inventoryShipment { id status } userErrors { field message code } } }"
  let #(
    Response(status: repeated_transit_status, body: repeated_transit_body, ..),
    proxy,
  ) =
    draft_proxy.process_request(proxy, graphql_request(repeated_transit_query))
  assert repeated_transit_status == 200
  assert string.contains(
    json.to_string(repeated_transit_body),
    "\"code\":\"INVALID_STATE\"",
  )

  let receive_after_transit_query =
    "mutation { inventoryShipmentReceive(id: \\\"gid://shopify/InventoryShipment/1\\\", lineItems: [{ shipmentLineItemId: \\\"gid://shopify/InventoryShipmentLineItem/2\\\", quantity: 1, reason: ACCEPTED }]) { inventoryShipment { id status } userErrors { field message code } } }"
  let #(
    Response(
      status: receive_after_transit_status,
      body: receive_after_transit_body,
      ..,
    ),
    proxy,
  ) =
    draft_proxy.process_request(
      proxy,
      graphql_request(receive_after_transit_query),
    )
  assert receive_after_transit_status == 200
  assert string.contains(
    json.to_string(receive_after_transit_body),
    "\"status\":\"RECEIVED\"",
  )
  assert string.contains(
    json.to_string(receive_after_transit_body),
    "\"userErrors\":[]",
  )

  let delete_received_query =
    "mutation { inventoryShipmentDelete(id: \\\"gid://shopify/InventoryShipment/1\\\") { deletedInventoryShipmentId userErrors { field message code } } }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(delete_received_query))
  assert delete_status == 200
  assert string.contains(
    json.to_string(delete_body),
    "\"code\":\"INVALID_STATE\"",
  )

  let assert [
    create_entry,
    add_entry,
    update_entry,
    receive_entry,
    transit_entry,
    repeated_transit_entry,
    receive_after_transit_entry,
    delete_received_entry,
  ] = store.get_log(proxy.store)
  assert create_entry.status == store_types.Staged
  assert add_entry.status == store_types.Failed
  assert update_entry.status == store_types.Failed
  assert receive_entry.status == store_types.Failed
  assert transit_entry.status == store_types.Staged
  assert repeated_transit_entry.status == store_types.Failed
  assert receive_after_transit_entry.status == store_types.Staged
  assert delete_received_entry.status == store_types.Failed
}

pub fn inventory_shipment_tracking_validates_carrier_and_url_test() {
  let query =
    "mutation { inventoryShipmentCreate(input: { transferId: \\\"gid://shopify/InventoryTransfer/7001\\\", trackingInput: { carrier: BAD_CARRIER, url: \\\"not-a-url\\\" }, lineItems: [{ inventoryTransferLineItemId: \\\"gid://shopify/InventoryTransferLineItem/7001\\\", inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 1 }] }) { inventoryShipment { id } userErrors { field message code } } }"
  let #(status, body, next_proxy) =
    run_product_mutation(
      tracked_inventory_store_with_transfer("READY_TO_SHIP", 2),
      query,
    )

  assert status == 200
  assert string.contains(
    body,
    "\"field\":[\"input\",\"trackingInput\",\"carrier\"]",
  )
  assert string.contains(
    body,
    "\"field\":[\"input\",\"trackingInput\",\"url\"]",
  )
  let assert [entry] = store.get_log(next_proxy.store)
  assert entry.status == store_types.Failed
}

pub fn inventory_set_quantities_validates_name_quantity_and_duplicates_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())

  let invalid_name_query =
    "mutation { inventorySetQuantities(input: { name: \\\"damaged\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 5 }] }) { inventoryAdjustmentGroup { id } userErrors { field message code } } }"
  let #(Response(status: invalid_name_status, body: invalid_name_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(invalid_name_query))
  assert invalid_name_status == 200
  assert json.to_string(invalid_name_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"The quantity name must be either 'available' or 'on_hand'.\",\"code\":\"INVALID_NAME\"}]}}}"

  let too_high_query =
    "mutation { inventorySetQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 1000000001 }] }) { inventoryAdjustmentGroup { id } userErrors { field message code } } }"
  let #(Response(status: too_high_status, body: too_high_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(too_high_query))
  assert too_high_status == 200
  assert json.to_string(too_high_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":null,\"userErrors\":[{\"field\":[\"input\",\"quantities\",\"0\",\"quantity\"],\"message\":\"The quantity can't be higher than 1,000,000,000.\",\"code\":\"INVALID_QUANTITY_TOO_HIGH\"}]}}}"

  let negative_query =
    "mutation { inventorySetQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: -1 }] }) { inventoryAdjustmentGroup { id } userErrors { field message code } } }"
  let #(Response(status: negative_status, body: negative_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(negative_query))
  assert negative_status == 200
  assert json.to_string(negative_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":null,\"userErrors\":[{\"field\":[\"input\",\"quantities\",\"0\",\"quantity\"],\"message\":\"The quantity can't be negative.\",\"code\":\"INVALID_QUANTITY_NEGATIVE\"}]}}}"

  let duplicate_query =
    "mutation { inventorySetQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 2 }, { inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 3 }] }) { inventoryAdjustmentGroup { id } userErrors { field message code } } }"
  let #(Response(status: duplicate_status, body: duplicate_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(duplicate_query))
  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":null,\"userErrors\":[{\"field\":[\"input\",\"quantities\",\"0\",\"locationId\"],\"message\":\"The combination of inventoryItemId and locationId must be unique.\",\"code\":\"NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR\"},{\"field\":[\"input\",\"quantities\",\"1\",\"locationId\"],\"message\":\"The combination of inventoryItemId and locationId must be unique.\",\"code\":\"NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR\"}]}}}"
}

pub fn inventory_set_and_adjust_quantities_accept_on_hand_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())

  let set_query =
    "mutation { inventorySetQuantities(input: { name: \\\"on_hand\\\", reason: \\\"correction\\\", ignoreCompareQuantity: true, quantities: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", quantity: 4 }] }) { inventoryAdjustmentGroup { changes { name delta item { id } location { id name } } } userErrors { field message code } } }"
  let #(Response(status: set_status, body: set_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(set_query))
  assert set_status == 200
  assert json.to_string(set_body)
    == "{\"data\":{\"inventorySetQuantities\":{\"inventoryAdjustmentGroup\":{\"changes\":[{\"name\":\"available\",\"delta\":3,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\",\"name\":\"Shop location\"}},{\"name\":\"on_hand\",\"delta\":3,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\",\"name\":\"Shop location\"}}]},\"userErrors\":[]}}}"

  let adjust_query =
    "mutation { inventoryAdjustQuantities(input: { name: \\\"on_hand\\\", reason: \\\"correction\\\", changes: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", delta: 2, ledgerDocumentUri: \\\"ledger://har-568/on-hand\\\" }] }) { inventoryAdjustmentGroup { changes { name delta ledgerDocumentUri item { id } location { id name } } } userErrors { field message code } } }"
  let #(Response(status: adjust_status, body: adjust_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(adjust_query))
  assert adjust_status == 200
  assert json.to_string(adjust_body)
    == "{\"data\":{\"inventoryAdjustQuantities\":{\"inventoryAdjustmentGroup\":{\"changes\":[{\"name\":\"on_hand\",\"delta\":2,\"ledgerDocumentUri\":\"ledger://har-568/on-hand\",\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\",\"name\":\"Shop location\"}}]},\"userErrors\":[]}}}"
}

pub fn inventory_adjust_quantities_preserves_product_total_inventory_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let adjust_query =
    "mutation { inventoryAdjustQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", changes: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", delta: -1 }] }) { inventoryAdjustmentGroup { changes { name delta item { id } location { id } } } userErrors { field message code } } }"
  let #(Response(status: adjust_status, body: adjust_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(adjust_query))
  assert adjust_status == 200
  assert json.to_string(adjust_body)
    == "{\"data\":{\"inventoryAdjustQuantities\":{\"inventoryAdjustmentGroup\":{\"changes\":[{\"name\":\"available\",\"delta\":-1,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\"}},{\"name\":\"on_hand\",\"delta\":-1,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\"}}]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/tracked\\\") { totalVariants hasOnlyDefaultVariant hasOutOfStockVariants tracksInventory totalInventory variants(first: 5) { nodes { inventoryQuantity inventoryItem { tracked } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"totalVariants\":1,\"hasOnlyDefaultVariant\":true,\"hasOutOfStockVariants\":true,\"tracksInventory\":true,\"totalInventory\":1,\"variants\":{\"nodes\":[{\"inventoryQuantity\":0,\"inventoryItem\":{\"tracked\":true}}]}}}}"
}

pub fn inventory_adjust_quantities_recomputes_product_total_inventory_for_202604_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let adjust_query =
    "mutation { inventoryAdjustQuantities(input: { name: \\\"available\\\", reason: \\\"correction\\\", changes: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", delta: -1, changeFromQuantity: 1 }] }) @idempotent(key: \\\"har-742-adjust\\\") { inventoryAdjustmentGroup { changes { name delta item { id } location { id } } } userErrors { field message code } } }"
  let #(Response(status: adjust_status, body: adjust_body, ..), proxy) =
    draft_proxy.process_request(
      proxy,
      graphql_request_for_version(adjust_query, "2026-04"),
    )
  assert adjust_status == 200
  assert json.to_string(adjust_body)
    == "{\"data\":{\"inventoryAdjustQuantities\":{\"inventoryAdjustmentGroup\":{\"changes\":[{\"name\":\"available\",\"delta\":-1,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\"}},{\"name\":\"on_hand\",\"delta\":-1,\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\"}}]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request_for_version(
        "query { product(id: \\\"gid://shopify/Product/tracked\\\") { totalInventory variants(first: 5) { nodes { inventoryQuantity } } } }",
        "2026-04",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"totalInventory\":0,\"variants\":{\"nodes\":[{\"inventoryQuantity\":0}]}}}}"
}

pub fn non_available_inventory_adjust_preserves_product_total_inventory_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_total_inventory(0),
    )
  let adjust_query =
    "mutation { inventoryAdjustQuantities(input: { name: \\\"incoming\\\", reason: \\\"correction\\\", changes: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", ledgerDocumentUri: \\\"ledger://incoming/test\\\", delta: 2 }] }) { inventoryAdjustmentGroup { changes { name delta ledgerDocumentUri item { id } location { id } } } userErrors { field message code } } }"
  let #(Response(status: adjust_status, body: adjust_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(adjust_query))
  assert adjust_status == 200
  assert json.to_string(adjust_body)
    == "{\"data\":{\"inventoryAdjustQuantities\":{\"inventoryAdjustmentGroup\":{\"changes\":[{\"name\":\"incoming\",\"delta\":2,\"ledgerDocumentUri\":\"ledger://incoming/test\",\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"location\":{\"id\":\"gid://shopify/Location/1\"}}]},\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      proxy,
      graphql_request(
        "query { product(id: \\\"gid://shopify/Product/tracked\\\") { totalInventory variants(first: 5) { nodes { inventoryQuantity inventoryItem { inventoryLevels(first: 5) { nodes { quantities(names: [\\\"available\\\", \\\"incoming\\\", \\\"on_hand\\\"]) { name quantity } } } } } } } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"product\":{\"totalInventory\":0,\"variants\":{\"nodes\":[{\"inventoryQuantity\":1,\"inventoryItem\":{\"inventoryLevels\":{\"nodes\":[{\"quantities\":[{\"name\":\"available\",\"quantity\":1},{\"name\":\"on_hand\",\"quantity\":1},{\"name\":\"incoming\",\"quantity\":2},{\"name\":\"reserved\",\"quantity\":0}]}]}}}]}}}}"
}

pub fn inventory_deactivate_unknown_level_returns_item_error_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let query =
    "mutation { inventoryDeactivate(inventoryLevelId: \\\"gid://shopify/InventoryLevel/999999999999?inventory_item_id=999999999998\\\") { userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryDeactivate\":{\"userErrors\":[{\"field\":null,\"message\":\"The product couldn't be unstocked because the product was deleted.\"}]}}}"
}

pub fn inventory_deactivate_known_item_missing_level_returns_location_deleted_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let query =
    "mutation { inventoryDeactivate(inventoryLevelId: \\\"gid://shopify/InventoryLevel/deleted-location?inventory_item_id=tracked\\\") { userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryDeactivate\":{\"userErrors\":[{\"field\":null,\"message\":\"The product couldn't be unstocked because the location was deleted.\"}]}}}"
}

pub fn inventory_deactivate_allows_non_zero_quantities_test() {
  let target_level =
    inventory_level(
      "gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked",
      "gid://shopify/Location/1",
      "Shop location",
      True,
      [
        inventory_quantity("available", 1),
        inventory_quantity("on_hand", 10),
        inventory_quantity("committed", 2),
        inventory_quantity("incoming", 3),
        inventory_quantity("reserved", 4),
      ],
    )
  let alternate_level =
    inventory_level(
      "gid://shopify/InventoryLevel/alternate?inventory_item_id=tracked",
      "gid://shopify/Location/2",
      "Second location",
      True,
      [
        inventory_quantity("available", 1),
        inventory_quantity("on_hand", 1),
      ],
    )
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_levels([
        target_level,
        alternate_level,
      ]),
    )
  let query =
    "mutation { inventoryDeactivate(inventoryLevelId: \\\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\\\") { userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryDeactivate\":{\"userErrors\":[]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { inventoryLevel(id: \\\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\\\") { id isActive } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\",\"isActive\":false}}}"
}

pub fn inventory_deactivate_only_location_stays_active_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let query =
    "mutation { inventoryDeactivate(inventoryLevelId: \\\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\\\") { userErrors { field message } } }"

  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryDeactivate\":{\"userErrors\":[{\"field\":null,\"message\":\"The product couldn't be unstocked from Shop location because products need to be stocked at a minimum of 1 location.\"}]}}}"

  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(
      next_proxy,
      graphql_request(
        "query { inventoryLevel(id: \\\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\\\") { id isActive } }",
      ),
    )
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\",\"isActive\":true}}}"
}

pub fn inventory_activate_unknown_location_returns_not_found_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let query =
    "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/missing\\\") { inventoryLevel { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":null,\"userErrors\":[{\"field\":[\"locationId\"],\"message\":\"The product couldn't be stocked because the location wasn't found.\",\"code\":\"NOT_FOUND\"}]}}}"
}

pub fn inventory_activate_rejects_negative_quantities_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_locations([
        LocationRecord(
          id: "gid://shopify/Location/1",
          name: "Shop location",
          cursor: None,
        ),
        LocationRecord(
          id: "gid://shopify/Location/2",
          name: "Second location",
          cursor: None,
        ),
      ]),
    )
  let available_query =
    "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/2\\\", available: -1) { inventoryLevel { id } userErrors { field message code } } }"

  let #(Response(status: available_status, body: available_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(available_query))

  assert available_status == 200
  assert json.to_string(available_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":null,\"userErrors\":[{\"field\":[\"available\"],\"message\":\"Available must be greater than or equal to 0\",\"code\":\"NEGATIVE\"}]}}}"

  let on_hand_query =
    "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/2\\\", onHand: -1) { inventoryLevel { id } userErrors { field message code } } }"

  let #(Response(status: on_hand_status, body: on_hand_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(on_hand_query))

  assert on_hand_status == 200
  assert json.to_string(on_hand_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":null,\"userErrors\":[{\"field\":[\"onHand\"],\"message\":\"On hand must be greater than or equal to 0\",\"code\":\"NEGATIVE\"}]}}}"
}

pub fn inventory_activate_duplicate_staged_activation_returns_taken_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_locations([
        LocationRecord(
          id: "gid://shopify/Location/1",
          name: "Shop location",
          cursor: None,
        ),
        LocationRecord(
          id: "gid://shopify/Location/2",
          name: "Second location",
          cursor: None,
        ),
      ]),
    )
  let query =
    "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/2\\\") { inventoryLevel { id isActive location { id name } } userErrors { field message code } } }"

  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/tracked-2?inventory_item_id=gid://shopify/InventoryItem/tracked\",\"isActive\":true,\"location\":{\"id\":\"gid://shopify/Location/2\",\"name\":\"Second location\"}},\"userErrors\":[]}}}"

  let #(Response(status: duplicate_status, body: duplicate_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/tracked-2?inventory_item_id=gid://shopify/InventoryItem/tracked\",\"isActive\":true,\"location\":{\"id\":\"gid://shopify/Location/2\",\"name\":\"Second location\"}},\"userErrors\":[{\"field\":[\"locationId\"],\"message\":\"Inventory level has already been taken\",\"code\":\"TAKEN\"}]}}}"
}

pub fn inventory_activate_available_conflict_requires_active_level_test() {
  let active_proxy = draft_proxy.new()
  let active_proxy =
    proxy_state.DraftProxy(..active_proxy, store: tracked_inventory_store())
  let active_query =
    "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/1\\\", available: 7) { inventoryLevel { id isActive } userErrors { field message } } }"

  let #(Response(status: active_status, body: active_body, ..), _) =
    draft_proxy.process_request(active_proxy, graphql_request(active_query))

  assert active_status == 200
  assert json.to_string(active_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked\",\"isActive\":true},\"userErrors\":[{\"field\":[\"available\"],\"message\":\"Not allowed to set available quantity when the item is already active at the location.\"}]}}}"

  let inactive_level =
    inventory_level(
      "gid://shopify/InventoryLevel/inactive?inventory_item_id=tracked",
      "gid://shopify/Location/2",
      "Second location",
      False,
      [
        inventory_quantity("available", 0),
        inventory_quantity("on_hand", 0),
      ],
    )
  let inactive_proxy = draft_proxy.new()
  let inactive_proxy =
    proxy_state.DraftProxy(
      ..inactive_proxy,
      store: tracked_inventory_store_with_levels([
        tracked_inventory_level(),
        inactive_level,
      ]),
    )
  let inactive_query =
    "mutation { inventoryActivate(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", locationId: \\\"gid://shopify/Location/2\\\", available: 7) { inventoryLevel { id isActive } userErrors { field message } } }"

  let #(Response(status: inactive_status, body: inactive_body, ..), _) =
    draft_proxy.process_request(inactive_proxy, graphql_request(inactive_query))

  assert inactive_status == 200
  assert json.to_string(inactive_body)
    == "{\"data\":{\"inventoryActivate\":{\"inventoryLevel\":{\"id\":\"gid://shopify/InventoryLevel/inactive?inventory_item_id=tracked\",\"isActive\":true},\"userErrors\":[]}}}"
}

pub fn inventory_bulk_toggle_unknown_location_returns_not_found_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let query =
    "mutation { inventoryBulkToggleActivation(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", inventoryItemUpdates: [{ locationId: \\\"gid://shopify/Location/missing\\\", activate: true }]) { inventoryItem { id } inventoryLevels { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryBulkToggleActivation\":{\"inventoryItem\":null,\"inventoryLevels\":null,\"userErrors\":[{\"field\":[\"inventoryItemUpdates\",\"0\",\"locationId\"],\"message\":\"The quantity couldn't be updated because the location was not found.\",\"code\":\"LOCATION_NOT_FOUND\"}]}}}"
}

pub fn inventory_bulk_toggle_activate_known_location_stages_level_test() {
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_locations([
        LocationRecord(
          id: "gid://shopify/Location/1",
          name: "Shop location",
          cursor: None,
        ),
        LocationRecord(
          id: "gid://shopify/Location/2",
          name: "Second location",
          cursor: None,
        ),
      ]),
    )
  let query =
    "mutation { inventoryBulkToggleActivation(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", inventoryItemUpdates: [{ locationId: \\\"gid://shopify/Location/2\\\", activate: true }]) { inventoryItem { id } inventoryLevels { id location { id name } item { id } } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryBulkToggleActivation\":{\"inventoryItem\":{\"id\":\"gid://shopify/InventoryItem/tracked\"},\"inventoryLevels\":[{\"id\":\"gid://shopify/InventoryLevel/tracked-2?inventory_item_id=gid://shopify/InventoryItem/tracked\",\"location\":{\"id\":\"gid://shopify/Location/2\",\"name\":\"Second location\"},\"item\":{\"id\":\"gid://shopify/InventoryItem/tracked\"}}],\"userErrors\":[]}}}"
}

pub fn inventory_bulk_toggle_deactivate_last_active_location_returns_error_test() {
  let inactive_level =
    inventory_level(
      "gid://shopify/InventoryLevel/inactive?inventory_item_id=tracked",
      "gid://shopify/Location/2",
      "Second location",
      False,
      [
        inventory_quantity("available", 0),
        inventory_quantity("on_hand", 0),
      ],
    )
  let proxy = draft_proxy.new()
  let proxy =
    proxy_state.DraftProxy(
      ..proxy,
      store: tracked_inventory_store_with_levels([
        tracked_inventory_level(),
        inactive_level,
      ]),
    )
  let query =
    "mutation { inventoryBulkToggleActivation(inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", inventoryItemUpdates: [{ locationId: \\\"gid://shopify/Location/1\\\", activate: false }]) { inventoryItem { id } inventoryLevels { id } userErrors { field message code } } }"

  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(query))

  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"inventoryBulkToggleActivation\":{\"inventoryItem\":null,\"inventoryLevels\":null,\"userErrors\":[{\"field\":[\"inventoryItemUpdates\",\"0\",\"locationId\"],\"message\":\"The variant couldn't be unstocked from Shop location because products need to be stocked at a minimum of 1 location.\",\"code\":\"CANNOT_DEACTIVATE_FROM_ONLY_LOCATION\"}]}}}"
}

pub fn inventory_transfer_edit_and_duplicate_stage_locally_test() {
  let proxy = draft_proxy.new()
  let proxy = proxy_state.DraftProxy(..proxy, store: tracked_inventory_store())
  let create_query =
    "mutation { inventoryTransferCreate(input: { originLocationId: \\\"gid://shopify/Location/1\\\", destinationLocationId: \\\"gid://shopify/Location/2\\\", referenceName: \\\"HAR-515\\\", note: \\\"local transfer\\\", tags: [\\\"har-515\\\"], lineItems: [{ inventoryItemId: \\\"gid://shopify/InventoryItem/tracked\\\", quantity: 2 }] }) { inventoryTransfer { id name referenceName note tags status lineItems(first: 5) { nodes { id totalQuantity } } } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"inventoryTransferCreate\":{\"inventoryTransfer\":{\"id\":\"gid://shopify/InventoryTransfer/1\",\"name\":\"#T0001\",\"referenceName\":\"HAR-515\",\"note\":\"local transfer\",\"tags\":[\"har-515\"],\"status\":\"DRAFT\",\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryTransferLineItem/2?shopify-draft-proxy=synthetic\",\"totalQuantity\":2}]}},\"userErrors\":[]}}}"

  let edit_query =
    "mutation { inventoryTransferEdit(id: \\\"gid://shopify/InventoryTransfer/1\\\", input: { note: \\\"edited transfer\\\", tags: [\\\"har-515\\\", \\\"edited\\\"] }) { inventoryTransfer { id referenceName note tags } userErrors { field message code } } }"
  let #(Response(status: edit_status, body: edit_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(edit_query))
  assert edit_status == 200
  assert json.to_string(edit_body)
    == "{\"data\":{\"inventoryTransferEdit\":{\"inventoryTransfer\":{\"id\":\"gid://shopify/InventoryTransfer/1\",\"referenceName\":\"HAR-515\",\"note\":\"edited transfer\",\"tags\":[\"har-515\",\"edited\"]},\"userErrors\":[]}}}"

  let duplicate_query =
    "mutation { inventoryTransferDuplicate(id: \\\"gid://shopify/InventoryTransfer/1\\\") { inventoryTransfer { id name status note tags lineItems(first: 5) { nodes { id totalQuantity } } } userErrors { field message code } } }"
  let #(Response(status: duplicate_status, body: duplicate_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(duplicate_query))
  assert duplicate_status == 200
  assert json.to_string(duplicate_body)
    == "{\"data\":{\"inventoryTransferDuplicate\":{\"inventoryTransfer\":{\"id\":\"gid://shopify/InventoryTransfer/5\",\"name\":\"#T0002\",\"status\":\"DRAFT\",\"note\":\"edited transfer\",\"tags\":[\"har-515\",\"edited\"],\"lineItems\":{\"nodes\":[{\"id\":\"gid://shopify/InventoryTransferLineItem/6?shopify-draft-proxy=synthetic\",\"totalQuantity\":2}]}},\"userErrors\":[]}}}"
  let assert [create_entry, edit_entry, duplicate_entry] =
    store.get_log(proxy.store)
  assert create_entry.operation_name == Some("inventoryTransferCreate")
  assert edit_entry.operation_name == Some("inventoryTransferEdit")
  assert duplicate_entry.operation_name == Some("inventoryTransferDuplicate")
  assert string.contains(create_entry.query, "inventoryTransferCreate")
  assert string.contains(edit_entry.query, "inventoryTransferEdit")
  assert string.contains(duplicate_entry.query, "inventoryTransferDuplicate")
}

fn default_option_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([default_product()])
  |> store.upsert_base_product_variants([default_variant()])
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/default",
      product_id: "gid://shopify/Product/optioned",
      name: "Title",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/default-title",
          name: "Default Title",
          has_variants: True,
        ),
      ],
    ),
  ])
}

fn variant_relationship_store() -> store.Store {
  default_option_store()
  |> store.upsert_base_products([child_product()])
  |> store.upsert_base_product_variants([child_variant()])
}

fn selling_plan_membership_store(
  groups: List(SellingPlanGroupRecord),
) -> store.Store {
  default_option_store()
  |> store.upsert_base_selling_plan_groups(groups)
}

fn selling_plan_group(
  id: String,
  product_ids: List(String),
  product_variant_ids: List(String),
) -> SellingPlanGroupRecord {
  SellingPlanGroupRecord(
    id: id,
    app_id: None,
    name: "Membership group " <> id,
    merchant_code: "membership-group",
    description: None,
    options: ["Delivery"],
    position: None,
    summary: None,
    created_at: None,
    product_ids: product_ids,
    product_variant_ids: product_variant_ids,
    selling_plans: [],
    cursor: None,
  )
}

fn numbered_selling_plan_group_ids(count: Int) -> List(String) {
  do_numbered_selling_plan_group_ids(1, count, [])
}

fn do_numbered_selling_plan_group_ids(
  current: Int,
  max: Int,
  acc: List(String),
) -> List(String) {
  case current > max {
    True -> list.reverse(acc)
    False ->
      do_numbered_selling_plan_group_ids(current + 1, max, [
        "gid://shopify/SellingPlanGroup/" <> int.to_string(current),
        ..acc
      ])
  }
}

fn numbered_selling_plan_groups(count: Int) -> List(SellingPlanGroupRecord) {
  numbered_selling_plan_group_ids(count)
  |> list.map(fn(id) { selling_plan_group(id, [], []) })
}

fn tagged_product_store() -> store.Store {
  default_option_store()
  |> store.upsert_base_products([
    ProductRecord(..default_product(), tags: ["existing", "sale", "summer"]),
  ])
}

fn metafield_store() -> store.Store {
  default_option_store()
  |> store.replace_base_metafields_for_owner("gid://shopify/Product/optioned", [
    ProductMetafieldRecord(
      id: "gid://shopify/Metafield/material",
      owner_id: "gid://shopify/Product/optioned",
      namespace: "custom",
      key: "material",
      type_: Some("single_line_text_field"),
      value: Some("Canvas"),
      compare_digest: Some("digest-material"),
      json_value: None,
      created_at: None,
      updated_at: None,
      owner_type: Some("PRODUCT"),
      market_localizable_content: [],
    ),
    ProductMetafieldRecord(
      id: "gid://shopify/Metafield/origin",
      owner_id: "gid://shopify/Product/optioned",
      namespace: "details",
      key: "origin",
      type_: Some("single_line_text_field"),
      value: Some("VN"),
      compare_digest: Some("digest-origin"),
      json_value: None,
      created_at: None,
      updated_at: None,
      owner_type: Some("PRODUCT"),
      market_localizable_content: [],
    ),
  ])
}

fn definition_validation_store() -> store.Store {
  default_option_store()
  |> store.upsert_base_metafield_definitions([
    metafield_definition("loyalty", "min_tier", "number_integer", [
      MetafieldDefinitionValidationRecord(name: "min", value: Some("2")),
    ]),
    metafield_definition("loyalty", "max_tier", "number_integer", [
      MetafieldDefinitionValidationRecord(name: "max", value: Some("5")),
    ]),
    metafield_definition("loyalty", "sku_code", "single_line_text_field", [
      MetafieldDefinitionValidationRecord(
        name: "regex",
        value: Some("^[A-Z]+$"),
      ),
    ]),
    metafield_definition("loyalty", "plan", "single_line_text_field", [
      MetafieldDefinitionValidationRecord(
        name: "allowed_list",
        value: Some("[\"gold\",\"silver\"]"),
      ),
    ]),
  ])
}

fn metafield_definition(
  namespace: String,
  key: String,
  type_name: String,
  validations: List(MetafieldDefinitionValidationRecord),
) -> MetafieldDefinitionRecord {
  MetafieldDefinitionRecord(
    id: "gid://shopify/MetafieldDefinition/" <> namespace <> "-" <> key,
    name: key,
    namespace: namespace,
    key: key,
    owner_type: "PRODUCT",
    type_: MetafieldDefinitionTypeRecord(name: type_name, category: None),
    description: None,
    validations: validations,
    access: dict.new(),
    capabilities: default_metafield_definition_capabilities(),
    constraints: None,
    pinned_position: None,
    validation_status: "ALL_VALID",
  )
}

fn default_metafield_definition_capabilities() -> MetafieldDefinitionCapabilitiesRecord {
  MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: default_metafield_definition_capability(),
    smart_collection_condition: default_metafield_definition_capability(),
    unique_values: default_metafield_definition_capability(),
  )
}

fn default_metafield_definition_capability() -> MetafieldDefinitionCapabilityRecord {
  MetafieldDefinitionCapabilityRecord(
    enabled: False,
    eligible: True,
    status: None,
  )
}

fn metafields_set_input(
  owner_id: String,
  namespace: String,
  key: String,
  type_name: String,
  value: String,
) -> String {
  "{ ownerId: \""
  <> owner_id
  <> "\", namespace: \""
  <> namespace
  <> "\", key: \""
  <> key
  <> "\", type: \""
  <> type_name
  <> "\", value: \""
  <> value
  <> "\" }"
}

fn variant_cap_store() -> store.Store {
  default_option_store()
  |> store.upsert_base_product_variants(variant_cap_records(2047))
}

fn variant_cap_records(count: Int) -> List(ProductVariantRecord) {
  variant_cap_records_loop(count, [])
}

fn variant_cap_records_loop(
  remaining: Int,
  acc: List(ProductVariantRecord),
) -> List(ProductVariantRecord) {
  case remaining <= 0 {
    True -> acc
    False -> {
      let id = int.to_string(remaining)
      variant_cap_records_loop(remaining - 1, [
        ProductVariantRecord(
          ..default_variant(),
          id: "gid://shopify/ProductVariant/cap-" <> id,
          title: "Variant " <> id,
        ),
        ..acc
      ])
    }
  }
}

fn repeated_text(item: String, count: Int) -> String {
  repeated_text_loop(item, count, "")
}

fn repeated_text_loop(item: String, remaining: Int, acc: String) -> String {
  case remaining <= 0 {
    True -> acc
    False -> repeated_text_loop(item, remaining - 1, acc <> item)
  }
}

fn repeat_csv(item: String, count: Int) -> String {
  repeat_csv_loop(item, count, "")
}

fn repeat_csv_loop(item: String, remaining: Int, acc: String) -> String {
  case remaining <= 0 {
    True -> acc
    False -> {
      let next = case acc == "" {
        True -> item
        False -> acc <> ", " <> item
      }
      repeat_csv_loop(item, remaining - 1, next)
    }
  }
}

fn suspended_product_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/suspended",
      handle: "suspended-product",
      status: "SUSPENDED",
    ),
  ])
}

fn collection_membership_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([
    default_product(),
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/second",
      title: "Second Product",
      handle: "second-product",
    ),
  ])
  |> store.upsert_base_collections([
    collection_record(
      "gid://shopify/Collection/custom",
      "Custom",
      "custom",
      None,
    ),
    collection_record(
      "gid://shopify/Collection/smart",
      "Smart",
      "smart",
      Some(
        CollectionRuleSetRecord(applied_disjunctively: False, rules: [
          CollectionRuleRecord(
            column: "TITLE",
            relation: "CONTAINS",
            condition: "Product",
          ),
        ]),
      ),
    ),
  ])
  |> store.upsert_base_product_collections([
    ProductCollectionRecord(
      collection_id: "gid://shopify/Collection/custom",
      product_id: "gid://shopify/Product/optioned",
      position: 0,
      cursor: None,
    ),
    ProductCollectionRecord(
      collection_id: "gid://shopify/Collection/smart",
      product_id: "gid://shopify/Product/optioned",
      position: 0,
      cursor: None,
    ),
  ])
}

fn repeated_product_ids_csv(product_id: String, count: Int) -> String {
  repeat_csv("\\\"" <> product_id <> "\\\"", count)
}

fn collection_record(
  id: String,
  title: String,
  handle: String,
  rule_set: Option(CollectionRuleSetRecord),
) -> CollectionRecord {
  CollectionRecord(
    id: id,
    legacy_resource_id: None,
    title: title,
    handle: handle,
    publication_ids: [],
    updated_at: None,
    description: None,
    description_html: Some(""),
    image: None,
    sort_order: Some("BEST_SELLING"),
    template_suffix: None,
    seo: ProductSeoRecord(title: None, description: None),
    rule_set: rule_set,
    products_count: Some(1),
    is_smart: option.is_some(rule_set),
    cursor: None,
    title_cursor: None,
    updated_at_cursor: None,
  )
}

fn inventory_quantity(name: String, quantity: Int) -> InventoryQuantityRecord {
  InventoryQuantityRecord(name: name, quantity: quantity, updated_at: None)
}

fn inventory_level(
  id: String,
  location_id: String,
  location_name: String,
  is_active: Bool,
  quantities: List(InventoryQuantityRecord),
) -> InventoryLevelRecord {
  InventoryLevelRecord(
    id: id,
    cursor: None,
    is_active: Some(is_active),
    location: InventoryLocationRecord(id: location_id, name: location_name),
    quantities: quantities,
  )
}

fn tracked_inventory_level() -> InventoryLevelRecord {
  inventory_level(
    "gid://shopify/InventoryLevel/tracked?inventory_item_id=tracked",
    "gid://shopify/Location/1",
    "Shop location",
    True,
    [
      inventory_quantity("available", 1),
      inventory_quantity("on_hand", 1),
      inventory_quantity("incoming", 0),
      inventory_quantity("reserved", 0),
    ],
  )
}

fn tracked_inventory_store() -> store.Store {
  tracked_inventory_store_with_levels([tracked_inventory_level()])
}

fn tracked_inventory_store_with_locations(
  locations: List(LocationRecord),
) -> store.Store {
  tracked_inventory_store()
  |> store.upsert_base_locations(locations)
}

fn tracked_inventory_store_with_transfer(
  status: String,
  quantity: Int,
) -> store.Store {
  tracked_inventory_store()
  |> store.upsert_base_inventory_transfers([
    inventory_transfer(status, quantity),
  ])
}

fn inventory_transfer(
  status: String,
  quantity: Int,
) -> InventoryTransferRecord {
  InventoryTransferRecord(
    id: "gid://shopify/InventoryTransfer/7001",
    name: "#T7001",
    reference_name: Some("shipment validation"),
    status: status,
    note: None,
    tags: [],
    date_created: "2024-01-01T00:00:00.000Z",
    origin: None,
    destination: None,
    line_items: [
      InventoryTransferLineItemRecord(
        id: "gid://shopify/InventoryTransferLineItem/7001",
        inventory_item_id: "gid://shopify/InventoryItem/tracked",
        title: Some("Tracked Product"),
        total_quantity: quantity,
        shipped_quantity: 0,
        picked_for_shipment_quantity: 0,
      ),
    ],
  )
}

fn tracked_inventory_store_with_total_inventory(
  total_inventory: Int,
) -> store.Store {
  tracked_inventory_store()
  |> store.upsert_base_products([
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/tracked",
      title: "Tracked Product",
      handle: "tracked-product",
      total_inventory: Some(total_inventory),
      tracks_inventory: Some(True),
    ),
  ])
}

fn tracked_inventory_store_with_levels(
  levels: List(InventoryLevelRecord),
) -> store.Store {
  store.new()
  |> store.upsert_base_products([
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/tracked",
      title: "Tracked Product",
      handle: "tracked-product",
      total_inventory: Some(1),
      tracks_inventory: Some(True),
    ),
  ])
  |> store.upsert_base_product_variants([
    tracked_inventory_variant_with_levels(levels),
  ])
}

fn combined_listing_parent_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/parent",
      title: "Combined Parent",
      handle: "combined-parent",
      combined_listing_role: Some("PARENT"),
    ),
    ProductRecord(
      ..default_product(),
      id: "gid://shopify/Product/child",
      title: "Child Product",
      handle: "child-product",
    ),
  ])
}

fn option_update_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([default_product()])
  |> store.upsert_base_product_variants([option_update_variant()])
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/color",
      product_id: "gid://shopify/Product/optioned",
      name: "Color",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/red",
          name: "Red",
          has_variants: True,
        ),
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/green",
          name: "Green",
          has_variants: False,
        ),
      ],
    ),
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/size",
      product_id: "gid://shopify/Product/optioned",
      name: "Size",
      position: 2,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/small",
          name: "Small",
          has_variants: True,
        ),
      ],
    ),
  ])
}

fn three_option_store() -> store.Store {
  option_update_store()
  |> store.replace_base_options_for_product("gid://shopify/Product/optioned", [
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/color",
      product_id: "gid://shopify/Product/optioned",
      name: "Color",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/red",
          name: "Red",
          has_variants: True,
        ),
      ],
    ),
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/size",
      product_id: "gid://shopify/Product/optioned",
      name: "Size",
      position: 2,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/small",
          name: "Small",
          has_variants: True,
        ),
      ],
    ),
    ProductOptionRecord(
      id: "gid://shopify/ProductOption/material",
      product_id: "gid://shopify/Product/optioned",
      name: "Material",
      position: 3,
      option_values: [
        ProductOptionValueRecord(
          id: "gid://shopify/ProductOptionValue/cotton",
          name: "Cotton",
          has_variants: True,
        ),
      ],
    ),
  ])
}

fn tracked_inventory_variant_with_levels(
  levels: List(InventoryLevelRecord),
) -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/tracked",
    product_id: "gid://shopify/Product/tracked",
    title: "Default Title",
    sku: Some("TRACKED-SKU"),
    barcode: None,
    price: Some("0.00"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(1),
    selected_options: [
      ProductVariantSelectedOptionRecord(name: "Title", value: "Default Title"),
    ],
    media_ids: [],
    inventory_item: Some(InventoryItemRecord(
      id: "gid://shopify/InventoryItem/tracked",
      tracked: Some(True),
      requires_shipping: Some(True),
      measurement: None,
      country_code_of_origin: None,
      province_code_of_origin: None,
      harmonized_system_code: None,
      inventory_levels: levels,
    )),
    contextual_pricing: None,
    cursor: None,
  )
}

fn default_product() -> ProductRecord {
  ProductRecord(
    id: "gid://shopify/Product/optioned",
    legacy_resource_id: None,
    title: "Optioned Board",
    handle: "optioned-board",
    status: "ACTIVE",
    vendor: None,
    product_type: None,
    tags: ["existing"],
    price_range_min: None,
    price_range_max: None,
    total_variants: None,
    has_only_default_variant: None,
    has_out_of_stock_variants: None,
    total_inventory: Some(0),
    tracks_inventory: Some(False),
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
    combined_listing_role: None,
    combined_listing_parent_id: None,
    combined_listing_child_ids: [],
  )
}

fn child_product() -> ProductRecord {
  ProductRecord(
    ..default_product(),
    id: "gid://shopify/Product/child",
    title: "Child Component",
    handle: "child-component",
  )
}

fn option_update_variant() -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/optioned",
    product_id: "gid://shopify/Product/optioned",
    title: "Red / Small",
    sku: None,
    barcode: None,
    price: Some("0.00"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(0),
    selected_options: [
      ProductVariantSelectedOptionRecord(name: "Color", value: "Red"),
      ProductVariantSelectedOptionRecord(name: "Size", value: "Small"),
    ],
    media_ids: [],
    inventory_item: Some(
      InventoryItemRecord(
        id: "gid://shopify/InventoryItem/optioned",
        tracked: Some(False),
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [],
      ),
    ),
    contextual_pricing: None,
    cursor: None,
  )
}

fn child_variant() -> ProductVariantRecord {
  ProductVariantRecord(
    ..default_variant(),
    id: "gid://shopify/ProductVariant/child",
    product_id: "gid://shopify/Product/child",
    title: "Child Variant",
    inventory_item: Some(
      InventoryItemRecord(
        id: "gid://shopify/InventoryItem/child",
        tracked: Some(False),
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [],
      ),
    ),
  )
}

fn product_media_record(
  id: String,
  product_id: String,
  position: Int,
  status: String,
) -> ProductMediaRecord {
  ProductMediaRecord(
    key: product_id <> ":" <> id,
    product_id: product_id,
    position: position,
    id: Some(id),
    media_content_type: Some("IMAGE"),
    alt: None,
    status: Some(status),
    product_image_id: None,
    image_url: None,
    image_width: None,
    image_height: None,
    preview_image_url: None,
    source_url: None,
  )
}

fn variant_media_validation_store() -> store.Store {
  store.new()
  |> store.upsert_base_products([default_product(), child_product()])
  |> store.upsert_base_product_variants([default_variant(), child_variant()])
  |> store.replace_base_media_for_product("gid://shopify/Product/optioned", [
    product_media_record(
      "gid://shopify/MediaImage/ready",
      "gid://shopify/Product/optioned",
      0,
      "READY",
    ),
    product_media_record(
      "gid://shopify/MediaImage/processing",
      "gid://shopify/Product/optioned",
      1,
      "PROCESSING",
    ),
  ])
  |> store.replace_base_media_for_product("gid://shopify/Product/child", [
    product_media_record(
      "gid://shopify/MediaImage/child",
      "gid://shopify/Product/child",
      0,
      "READY",
    ),
  ])
}

fn default_variant() -> ProductVariantRecord {
  ProductVariantRecord(
    id: "gid://shopify/ProductVariant/default",
    product_id: "gid://shopify/Product/optioned",
    title: "Default Title",
    sku: None,
    barcode: None,
    price: Some("0.00"),
    compare_at_price: None,
    taxable: None,
    inventory_policy: None,
    inventory_quantity: Some(0),
    selected_options: [
      ProductVariantSelectedOptionRecord(name: "Title", value: "Default Title"),
    ],
    media_ids: [],
    inventory_item: Some(
      InventoryItemRecord(
        id: "gid://shopify/InventoryItem/default",
        tracked: Some(False),
        requires_shipping: Some(True),
        measurement: None,
        country_code_of_origin: None,
        province_code_of_origin: None,
        harmonized_system_code: None,
        inventory_levels: [],
      ),
    ),
    contextual_pricing: None,
    cursor: None,
  )
}
