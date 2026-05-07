//// Read and mutation tests for the Gleam Marketing domain port.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/graphql_helpers.{SrcList, SrcObject, SrcString}
import shopify_draft_proxy/proxy/marketing
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{
  UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  MarketingBool, MarketingChannelDefinitionRecord, MarketingNull,
  MarketingObject, MarketingRecord, MarketingString,
}

fn empty_vars() {
  dict.new()
}

fn registered_email_store() {
  store.upsert_base_marketing_channel_definitions(store.new(), [
    MarketingChannelDefinitionRecord(handle: "email", api_client_ids: []),
  ])
}

fn registered_email_store_for_app(api_client_id: String) {
  store.upsert_base_marketing_channel_definitions(store.new(), [
    MarketingChannelDefinitionRecord(handle: "email", api_client_ids: [
      api_client_id,
    ]),
  ])
}

fn upstream_context_with_api_client(api_client_id: String) {
  let base = empty_upstream_context()
  UpstreamContext(
    ..base,
    headers: dict.from_list([
      #(app_identity.api_client_id_header, api_client_id),
    ]),
  )
}

fn external_create_doc(
  remote_id: String,
  title: String,
  campaign: String,
  url_parameter_value: String,
) -> String {
  "mutation { marketingActivityCreateExternal(input: { title: \""
  <> title
  <> "\", remoteId: \""
  <> remote_id
  <> "\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \""
  <> url_parameter_value
  <> "\", utm: { campaign: \""
  <> campaign
  <> "\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id title remoteId marketingEvent { id remoteId channelHandle } } userErrors { field message code } } }"
}

fn external_create_with_budget_doc(
  remote_id: String,
  title: String,
  campaign: String,
  url_parameter_value: String,
) -> String {
  "mutation { marketingActivityCreateExternal(input: { title: \""
  <> title
  <> "\", remoteId: \""
  <> remote_id
  <> "\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, budget: { budgetType: DAILY, total: { amount: \"100.00\", currencyCode: USD } }, urlParameterValue: \""
  <> url_parameter_value
  <> "\", utm: { campaign: \""
  <> campaign
  <> "\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id title remoteId } userErrors { field message code } } }"
}

/// Apply the dispatcher-level `record_log_drafts` to the outcome. Tests that
/// exercise `marketing.process_mutation` directly need this so log-buffer
/// assertions still see the drafts the module emitted; centralized recording
/// is the dispatcher's responsibility post-refactor.
fn record_drafts(
  outcome: mutation_helpers.MutationOutcome,
  request_path: String,
  document: String,
) -> mutation_helpers.MutationOutcome {
  let #(logged_store, logged_identity) =
    mutation_helpers.record_log_drafts(
      outcome.store,
      outcome.identity,
      request_path,
      document,
      dict.new(),
      outcome.log_drafts,
    )
  mutation_helpers.MutationOutcome(
    ..outcome,
    store: logged_store,
    identity: logged_identity,
  )
}

fn run(source: store.Store, query: String) -> String {
  let assert Ok(data) =
    marketing.handle_marketing_query(source, query, empty_vars())
  json.to_string(data)
}

fn run_with_api_client(
  source: store.Store,
  query: String,
  api_client_id: String,
) -> String {
  let assert Ok(data) =
    marketing.handle_marketing_query_for_app(
      source,
      query,
      empty_vars(),
      Some(api_client_id),
    )
  json.to_string(data)
}

fn activity(id: String, title: String, remote_id: String, created_at: String) {
  activity_with_utm(
    id,
    title,
    remote_id,
    created_at,
    "spring",
    "email",
    "newsletter",
  )
}

fn activity_with_utm(
  id: String,
  title: String,
  remote_id: String,
  created_at: String,
  campaign: String,
  source: String,
  medium: String,
) {
  let event_id = "gid://shopify/MarketingEvent/" <> string.drop_start(id, 34)
  let event =
    dict.from_list([
      #("__typename", MarketingString("MarketingEvent")),
      #("id", MarketingString(event_id)),
      #("type", MarketingString("NEWSLETTER")),
      #("remoteId", MarketingString(remote_id)),
      #("description", MarketingString(title)),
      #("startedAt", MarketingString(created_at)),
      #("sourceAndMedium", MarketingString("Email newsletter")),
      #("channelHandle", MarketingString("email")),
    ])
  MarketingRecord(
    id: id,
    cursor: None,
    api_client_id: None,
    data: dict.from_list([
      #("__typename", MarketingString("MarketingActivity")),
      #("id", MarketingString(id)),
      #("title", MarketingString(title)),
      #("createdAt", MarketingString(created_at)),
      #("updatedAt", MarketingString(created_at)),
      #("status", MarketingString("ACTIVE")),
      #("statusLabel", MarketingString("Sending")),
      #("tactic", MarketingString("NEWSLETTER")),
      #("marketingChannelType", MarketingString("EMAIL")),
      #("sourceAndMedium", MarketingString("Email newsletter")),
      #("isExternal", MarketingBool(True)),
      #("remoteId", MarketingString(remote_id)),
      #(
        "utmParameters",
        MarketingObject(
          dict.from_list([
            #("campaign", MarketingString(campaign)),
            #("source", MarketingString(source)),
            #("medium", MarketingString(medium)),
          ]),
        ),
      ),
      #("marketingEvent", MarketingObject(event)),
    ]),
  )
}

fn external_activity_with_details(
  id: String,
  remote_id: String,
  title: String,
  channel_handle: String,
  url_parameter_value: String,
  parent_remote_id: String,
  hierarchy_level: String,
) {
  let event_id = "gid://shopify/MarketingEvent/" <> string.drop_start(id, 34)
  let event =
    dict.from_list([
      #("__typename", MarketingString("MarketingEvent")),
      #("id", MarketingString(event_id)),
      #("type", MarketingString("NEWSLETTER")),
      #("remoteId", MarketingString(remote_id)),
      #("description", MarketingString(title)),
      #("startedAt", MarketingString("2026-05-05T00:00:00Z")),
      #("sourceAndMedium", MarketingString("Email newsletter")),
      #("channelHandle", MarketingString(channel_handle)),
    ])
  MarketingRecord(
    id: id,
    cursor: None,
    api_client_id: None,
    data: dict.from_list([
      #("__typename", MarketingString("MarketingActivity")),
      #("id", MarketingString(id)),
      #("title", MarketingString(title)),
      #("createdAt", MarketingString("2026-05-05T00:00:00Z")),
      #("updatedAt", MarketingString("2026-05-05T00:00:00Z")),
      #("status", MarketingString("ACTIVE")),
      #("statusLabel", MarketingString("Sending")),
      #("tactic", MarketingString("NEWSLETTER")),
      #("marketingChannelType", MarketingString("EMAIL")),
      #("sourceAndMedium", MarketingString("Email newsletter")),
      #("isExternal", MarketingBool(True)),
      #("remoteId", MarketingString(remote_id)),
      #("urlParameterValue", MarketingString(url_parameter_value)),
      #("parentRemoteId", MarketingString(parent_remote_id)),
      #("hierarchyLevel", MarketingString(hierarchy_level)),
      #(
        "utmParameters",
        MarketingObject(
          dict.from_list([
            #("campaign", MarketingString("campaign")),
            #("source", MarketingString("email")),
            #("medium", MarketingString("newsletter")),
          ]),
        ),
      ),
      #("marketingEvent", MarketingObject(event)),
    ]),
  )
}

fn marketing_event(id: String, remote_id: String) {
  MarketingRecord(
    id: id,
    cursor: None,
    api_client_id: None,
    data: dict.from_list([
      #("__typename", MarketingString("MarketingEvent")),
      #("id", MarketingString(id)),
      #("type", MarketingString("NEWSLETTER")),
      #("remoteId", MarketingString(remote_id)),
      #("description", MarketingString(remote_id)),
      #("startedAt", MarketingString("2026-05-05T00:00:00Z")),
      #("sourceAndMedium", MarketingString("Email newsletter")),
    ]),
  )
}

fn non_external_activity(id: String, remote_id: String) {
  MarketingRecord(
    id: id,
    cursor: None,
    api_client_id: None,
    data: dict.from_list([
      #("__typename", MarketingString("MarketingActivity")),
      #("id", MarketingString(id)),
      #("title", MarketingString("Native")),
      #("createdAt", MarketingString("2026-05-05T00:00:00Z")),
      #("updatedAt", MarketingString("2026-05-05T00:00:00Z")),
      #("status", MarketingString("ACTIVE")),
      #("statusLabel", MarketingString("Sending")),
      #("tactic", MarketingString("NEWSLETTER")),
      #("marketingChannelType", MarketingString("EMAIL")),
      #("sourceAndMedium", MarketingString("Email newsletter")),
      #("isExternal", MarketingBool(False)),
      #("remoteId", MarketingString(remote_id)),
      #("marketingEvent", MarketingNull),
    ]),
  )
}

fn external_activity_without_event(id: String, remote_id: String) {
  MarketingRecord(
    id: id,
    cursor: None,
    api_client_id: None,
    data: dict.from_list([
      #("__typename", MarketingString("MarketingActivity")),
      #("id", MarketingString(id)),
      #("title", MarketingString("Orphan external")),
      #("createdAt", MarketingString("2026-05-05T00:00:00Z")),
      #("updatedAt", MarketingString("2026-05-05T00:00:00Z")),
      #("status", MarketingString("ACTIVE")),
      #("statusLabel", MarketingString("Sending")),
      #("tactic", MarketingString("NEWSLETTER")),
      #("marketingChannelType", MarketingString("EMAIL")),
      #("sourceAndMedium", MarketingString("Email newsletter")),
      #("isExternal", MarketingBool(True)),
      #("remoteId", MarketingString(remote_id)),
      #("marketingEvent", MarketingNull),
    ]),
  )
}

pub fn root_predicates_test() {
  assert marketing.is_marketing_query_root("marketingActivity")
  assert marketing.is_marketing_query_root("marketingActivities")
  assert marketing.is_marketing_query_root("marketingEvent")
  assert marketing.is_marketing_query_root("marketingEvents")
  assert marketing.is_marketing_mutation_root("marketingActivityCreate")
  assert marketing.is_marketing_mutation_root("marketingActivityUpdate")
  assert marketing.is_marketing_mutation_root("marketingActivityCreateExternal")
  assert marketing.is_marketing_mutation_root("marketingActivityUpdateExternal")
  assert marketing.is_marketing_mutation_root("marketingActivityUpsertExternal")
  assert marketing.is_marketing_mutation_root("marketingActivityDeleteExternal")
  assert marketing.is_marketing_mutation_root(
    "marketingActivitiesDeleteAllExternal",
  )
  assert marketing.is_marketing_mutation_root("marketingEngagementCreate")
  assert marketing.is_marketing_mutation_root("marketingEngagementsDelete")
  assert !marketing.is_marketing_query_root("marketingEngagementCreate")
  assert !marketing.is_marketing_mutation_root("productCreate")
}

pub fn empty_reads_keep_shopify_like_shapes_test() {
  let source = store.new()
  let result =
    run(
      source,
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { id } marketingEvent(id: \"gid://shopify/MarketingEvent/1\") { id } marketingActivities(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } marketingEvents(first: 10) { nodes { id } edges { cursor } } }",
    )
  assert result
    == "{\"marketingActivity\":null,\"marketingEvent\":null,\"marketingActivities\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}},\"marketingEvents\":{\"nodes\":[],\"edges\":[]}}"
}

pub fn reads_stateful_activity_and_event_connections_test() {
  let first =
    activity(
      "gid://shopify/MarketingActivity/101",
      "Spring launch",
      "remote-101",
      "2026-04-27T00:00:01Z",
    )
  let second =
    activity(
      "gid://shopify/MarketingActivity/202",
      "Winter launch",
      "remote-202",
      "2026-04-27T00:00:03Z",
    )
  let source =
    store.upsert_base_marketing_activities(store.new(), [first, second])
  let source =
    store.upsert_base_marketing_events(source, [
      MarketingRecord(
        id: "gid://shopify/MarketingEvent/101",
        cursor: None,
        api_client_id: None,
        data: dict.from_list([
          #("__typename", MarketingString("MarketingEvent")),
          #("id", MarketingString("gid://shopify/MarketingEvent/101")),
          #("type", MarketingString("NEWSLETTER")),
          #("remoteId", MarketingString("remote-101")),
          #("description", MarketingString("Spring launch")),
          #("startedAt", MarketingString("2026-04-27T00:00:01Z")),
        ]),
      ),
    ])

  let result =
    run(
      source,
      "{ byId: marketingActivity(id: \"gid://shopify/MarketingActivity/101\") { id title remoteId } filtered: marketingActivities(first: 5, query: \"title:Spring\", remoteIds: [\"remote-101\"], sortKey: TITLE) { nodes { id title } } paged: marketingActivities(first: 1) { edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } events: marketingEvents(first: 5, query: \"description:Spring\") { nodes { id description } } }",
    )

  assert string.contains(
    result,
    "\"byId\":{\"id\":\"gid://shopify/MarketingActivity/101\",\"title\":\"Spring launch\",\"remoteId\":\"remote-101\"}",
  )
  assert string.contains(
    result,
    "\"filtered\":{\"nodes\":[{\"id\":\"gid://shopify/MarketingActivity/101\",\"title\":\"Spring launch\"}]}",
  )
  assert string.contains(
    result,
    "\"paged\":{\"edges\":[{\"cursor\":\"cursor:gid://shopify/MarketingActivity/101\",\"node\":{\"id\":\"gid://shopify/MarketingActivity/101\"}}],\"pageInfo\":{\"hasNextPage\":true,\"hasPreviousPage\":false,\"startCursor\":\"cursor:gid://shopify/MarketingActivity/101\",\"endCursor\":\"cursor:gid://shopify/MarketingActivity/101\"}}",
  )
  assert string.contains(
    result,
    "\"events\":{\"nodes\":[{\"id\":\"gid://shopify/MarketingEvent/101\",\"description\":\"Spring launch\"}]}",
  )
}

pub fn hydrates_upstream_activity_and_event_payloads_test() {
  let activity_id = "gid://shopify/MarketingActivity/301"
  let event_id = "gid://shopify/MarketingEvent/901"
  let upstream =
    SrcObject(
      dict.from_list([
        #(
          "marketingActivities",
          SrcObject(
            dict.from_list([
              #(
                "edges",
                SrcList([
                  SrcObject(
                    dict.from_list([
                      #("cursor", SrcString("upstream-cursor-301")),
                      #(
                        "node",
                        SrcObject(
                          dict.from_list([
                            #("__typename", SrcString("MarketingActivity")),
                            #("id", SrcString(activity_id)),
                            #("title", SrcString("Hydrated launch")),
                            #(
                              "marketingEvent",
                              SrcObject(
                                dict.from_list([
                                  #("__typename", SrcString("MarketingEvent")),
                                  #("id", SrcString(event_id)),
                                  #("remoteId", SrcString("remote-301")),
                                  #("description", SrcString("Hydrated event")),
                                ]),
                              ),
                            ),
                          ]),
                        ),
                      ),
                    ]),
                  ),
                ]),
              ),
            ]),
          ),
        ),
      ]),
    )

  let hydrated =
    marketing.hydrate_marketing_from_upstream_payload(store.new(), upstream)
  let result =
    run(
      hydrated,
      "{ marketingActivities(first: 1) { edges { cursor node { id title marketingEvent { id remoteId } } } } marketingEvent(id: \""
        <> event_id
        <> "\") { id description } marketingEvents(first: 1) { edges { cursor node { id } } } }",
    )

  assert string.contains(result, "\"cursor\":\"upstream-cursor-301\"")
  assert string.contains(
    result,
    "\"marketingEvents\":{\"edges\":[{\"cursor\":\"cursor:gid://shopify/MarketingEvent/901\"",
  )
  assert string.contains(result, "\"title\":\"Hydrated launch\"")
  assert string.contains(result, "\"remoteId\":\"remote-301\"")
  assert string.contains(
    result,
    "\"marketingEvent\":{\"id\":\"gid://shopify/MarketingEvent/901\",\"description\":\"Hydrated event\"}",
  )
}

pub fn external_activity_create_update_delete_stages_locally_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let create_doc =
    "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id title remoteId marketingEvent { id remoteId channelHandle } } userErrors { field message code } } }"
  let created =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      request_path,
      create_doc,
      empty_vars(),
      empty_upstream_context(),
    )
  let created = record_drafts(created, request_path, create_doc)
  let response = json.to_string(created.data)
  assert string.contains(
    response,
    "\"marketingActivity\":{\"id\":\"gid://shopify/MarketingActivity/1\",\"title\":\"Launch\",\"remoteId\":\"remote-1\",\"marketingEvent\":{\"id\":\"gid://shopify/MarketingEvent/2\",\"remoteId\":\"remote-1\",\"channelHandle\":\"email\"}}",
  )
  assert string.contains(response, "\"userErrors\":[]")
  assert created.staged_resource_ids
    == [
      "gid://shopify/MarketingActivity/1",
      "gid://shopify/MarketingEvent/2",
    ]
  assert list.length(store.get_log(created.store)) == 1

  let read_after_create =
    run(
      created.store,
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { id title remoteId } marketingEvent(id: \"gid://shopify/MarketingEvent/2\") { id remoteId } }",
    )
  assert read_after_create
    == "{\"marketingActivity\":{\"id\":\"gid://shopify/MarketingActivity/1\",\"title\":\"Launch\",\"remoteId\":\"remote-1\"},\"marketingEvent\":{\"id\":\"gid://shopify/MarketingEvent/2\",\"remoteId\":\"remote-1\"}}"

  let updated =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityUpdateExternal(remoteId: \"remote-1\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, input: { title: \"Launch updated\", status: INACTIVE }) { marketingActivity { id title status marketingEvent { endedAt } } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let update_response = json.to_string(updated.data)
  assert string.contains(update_response, "\"title\":\"Launch updated\"")
  assert string.contains(update_response, "\"status\":\"INACTIVE\"")

  let deleted =
    marketing.process_mutation(
      updated.store,
      updated.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityDeleteExternal(remoteId: \"remote-1\") { deletedMarketingActivityId userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert json.to_string(deleted.data)
    == "{\"data\":{\"marketingActivityDeleteExternal\":{\"deletedMarketingActivityId\":\"gid://shopify/MarketingActivity/1\",\"userErrors\":[]}}}"
  let read_after_delete =
    run(
      deleted.store,
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { id } marketingEvent(id: \"gid://shopify/MarketingEvent/2\") { id } }",
    )
  assert read_after_delete
    == "{\"marketingActivity\":null,\"marketingEvent\":null}"
}

pub fn external_activity_url_scheme_validation_rejects_non_http_schemes_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let seed =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-url-scheme-seed\", remoteUrl: \"https://example.com/seed\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, utm: { campaign: \"url-scheme-seed\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id title marketingEvent { remoteId manageUrl previewUrl } } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(json.to_string(seed.data), "\"userErrors\":[]")

  let invalid_create =
    marketing.process_mutation(
      seed.store,
      seed.identity,
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Bad FTP\", remoteId: \"remote-url-scheme-ftp\", remoteUrl: \"ftp://example.com/bad\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, utm: { campaign: \"url-scheme-ftp\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert invalid_create.staged_resource_ids == []
  assert_url_scheme_error(
    json.to_string(invalid_create.data),
    "marketingActivityCreateExternal",
  )
  let read_after_invalid_create =
    run(
      invalid_create.store,
      "{ marketingActivities(first: 5, remoteIds: [\"remote-url-scheme-ftp\"]) { nodes { id } } }",
    )
  assert read_after_invalid_create == "{\"marketingActivities\":{\"nodes\":[]}}"

  let invalid_create_preview =
    marketing.process_mutation(
      seed.store,
      seed.identity,
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Bad preview\", remoteId: \"remote-url-scheme-file\", remoteUrl: \"https://example.com/ok\", remotePreviewImageUrl: \"file://example.com/preview.png\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, utm: { campaign: \"url-scheme-file\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert invalid_create_preview.staged_resource_ids == []
  assert_url_scheme_error(
    json.to_string(invalid_create_preview.data),
    "marketingActivityCreateExternal",
  )

  let invalid_update =
    marketing.process_mutation(
      seed.store,
      seed.identity,
      request_path,
      "mutation { marketingActivityUpdateExternal(remoteId: \"remote-url-scheme-seed\", input: { title: \"Bad update\", remoteUrl: \"mailto:marketing@example.com\" }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert invalid_update.staged_resource_ids == []
  assert_url_scheme_error(
    json.to_string(invalid_update.data),
    "marketingActivityUpdateExternal",
  )
  let read_after_invalid_update =
    run(
      invalid_update.store,
      "{ marketingActivities(first: 5, remoteIds: [\"remote-url-scheme-seed\"]) { nodes { title marketingEvent { manageUrl previewUrl } } } }",
    )
  assert string.contains(
    read_after_invalid_update,
    "\"manageUrl\":\"https://example.com/seed\"",
  )
  assert string.contains(read_after_invalid_update, "\"previewUrl\":null")

  let invalid_upsert =
    marketing.process_mutation(
      seed.store,
      seed.identity,
      request_path,
      "mutation { marketingActivityUpsertExternal(input: { title: \"Bad upsert\", remoteId: \"remote-url-scheme-upsert\", remoteUrl: \"file://example.com/upsert\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, utm: { campaign: \"url-scheme-upsert\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert invalid_upsert.staged_resource_ids == []
  assert_url_scheme_error(
    json.to_string(invalid_upsert.data),
    "marketingActivityUpsertExternal",
  )
}

fn assert_url_scheme_error(response_json: String, root: String) {
  assert string.contains(
    response_json,
    "\"message\":\"The URL scheme must be one of the following: https,http\"",
  )
  assert string.contains(response_json, "\"code\":\"INVALID_FIELD_ARGUMENTS\"")
  assert string.contains(response_json, "\"path\":[\"" <> root <> "\"]")
  assert string.contains(response_json, "\"data\":{\"" <> root <> "\":null}")
}

pub fn external_activity_remote_id_uniqueness_is_app_scoped_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let app_a_create =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      request_path,
      external_create_doc(
        "remote-shared",
        "App A launch",
        "app-a-launch",
        "utm_campaign=app-a",
      ),
      empty_vars(),
      upstream_context_with_api_client("app-a"),
    )

  let app_b_create =
    marketing.process_mutation(
      app_a_create.store,
      app_a_create.identity,
      request_path,
      external_create_doc(
        "remote-shared",
        "App B launch",
        "app-b-launch",
        "utm_campaign=app-b",
      ),
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )

  let response = json.to_string(app_b_create.data)
  assert string.contains(response, "\"userErrors\":[]")
  assert list.length(store.list_effective_marketing_activities(
      app_b_create.store,
    ))
    == 2

  let app_a_duplicate =
    marketing.process_mutation(
      app_b_create.store,
      app_b_create.identity,
      request_path,
      external_create_doc(
        "remote-shared",
        "App A duplicate",
        "app-a-duplicate",
        "utm_campaign=app-a-duplicate",
      ),
      empty_vars(),
      upstream_context_with_api_client("app-a"),
    )
  assert string.contains(
    json.to_string(app_a_duplicate.data),
    "Validation failed: Remote ID has already been taken",
  )
}

pub fn external_activity_selectors_are_app_scoped_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let app_a_create =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      request_path,
      external_create_with_budget_doc(
        "remote-owned",
        "App A scoped",
        "app-a-scoped",
        "utm_campaign=app-a-scoped",
      ),
      empty_vars(),
      upstream_context_with_api_client("app-a"),
    )

  let app_b_update =
    marketing.process_mutation(
      app_a_create.store,
      app_a_create.identity,
      request_path,
      "mutation { marketingActivityUpdateExternal(remoteId: \"remote-owned\", input: { title: \"Foreign update\" }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )
  let update_response = json.to_string(app_b_update.data)
  assert string.contains(
    update_response,
    "\"code\":\"MARKETING_ACTIVITY_DOES_NOT_EXIST\"",
  )

  let app_b_delete =
    marketing.process_mutation(
      app_a_create.store,
      app_a_create.identity,
      request_path,
      "mutation { marketingActivityDeleteExternal(remoteId: \"remote-owned\") { deletedMarketingActivityId userErrors { field message code } } }",
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )
  let delete_response = json.to_string(app_b_delete.data)
  assert string.contains(delete_response, "\"deletedMarketingActivityId\":null")
  assert string.contains(
    delete_response,
    "\"code\":\"MARKETING_ACTIVITY_DOES_NOT_EXIST\"",
  )

  let app_b_engagement =
    marketing.process_mutation(
      app_a_create.store,
      app_a_create.identity,
      request_path,
      "mutation { marketingEngagementCreate(remoteId: \"remote-owned\", marketingEngagement: { occurredOn: \"2026-05-06\", adSpend: { amount: \"10.00\", currencyCode: EUR } }) { marketingEngagement { occurredOn } userErrors { field message code } } }",
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )
  let engagement_response = json.to_string(app_b_engagement.data)
  assert string.contains(
    engagement_response,
    "\"code\":\"MARKETING_ACTIVITY_DOES_NOT_EXIST\"",
  )
  assert !string.contains(
    engagement_response,
    "MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH",
  )

  assert run_with_api_client(
      app_a_create.store,
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { title } }",
      "app-a",
    )
    == "{\"marketingActivity\":{\"title\":\"App A scoped\"}}"
  assert run_with_api_client(
      app_a_create.store,
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { title } }",
      "app-b",
    )
    == "{\"marketingActivity\":null}"
}

pub fn delete_all_external_is_app_scoped_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let app_a_create =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      request_path,
      external_create_doc(
        "remote-delete-a",
        "App A delete all",
        "delete-a",
        "utm_campaign=delete-a",
      ),
      empty_vars(),
      upstream_context_with_api_client("app-a"),
    )
  let app_b_create =
    marketing.process_mutation(
      app_a_create.store,
      app_a_create.identity,
      request_path,
      external_create_doc(
        "remote-delete-b",
        "App B delete all",
        "delete-b",
        "utm_campaign=delete-b",
      ),
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )
  let app_b_delete_all =
    marketing.process_mutation(
      app_b_create.store,
      app_b_create.identity,
      request_path,
      "mutation { marketingActivitiesDeleteAllExternal { job { id done } userErrors { field message code } } }",
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )

  assert store.has_marketing_delete_all_external_in_flight_for_app(
    app_b_delete_all.store,
    Some("app-b"),
  )
  assert !store.has_marketing_delete_all_external_in_flight_for_app(
    app_b_delete_all.store,
    Some("app-a"),
  )
  assert run_with_api_client(
      app_b_delete_all.store,
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { title } }",
      "app-a",
    )
    == "{\"marketingActivity\":{\"title\":\"App A delete all\"}}"
  assert run_with_api_client(
      app_b_delete_all.store,
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/3\") { title } }",
      "app-b",
    )
    == "{\"marketingActivity\":null}"

  let app_a_after_delete_all =
    marketing.process_mutation(
      app_b_delete_all.store,
      app_b_delete_all.identity,
      request_path,
      external_create_doc(
        "remote-delete-a-2",
        "App A after delete all",
        "delete-a-2",
        "utm_campaign=delete-a-2",
      ),
      empty_vars(),
      upstream_context_with_api_client("app-a"),
    )
  assert string.contains(
    json.to_string(app_a_after_delete_all.data),
    "\"userErrors\":[]",
  )

  let app_b_blocked =
    marketing.process_mutation(
      app_b_delete_all.store,
      app_b_delete_all.identity,
      request_path,
      external_create_doc(
        "remote-delete-b-2",
        "App B blocked",
        "delete-b-2",
        "utm_campaign=delete-b-2",
      ),
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )
  assert string.contains(
    json.to_string(app_b_blocked.data),
    "\"code\":\"DELETE_JOB_ENQUEUED\"",
  )
}

pub fn native_activity_update_is_app_scoped_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let create_doc =
    "mutation { marketingActivityCreate(input: { marketingActivityTitle: \"Native scoped\", marketingActivityExtensionId: \"gid://shopify/MarketingActivityExtension/abc\" }) { userErrors { message } } }"
  let created =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      create_doc,
      empty_vars(),
      upstream_context_with_api_client("app-a"),
    )

  let app_b_update =
    marketing.process_mutation(
      created.store,
      created.identity,
      request_path,
      "mutation { marketingActivityUpdate(input: { id: \"gid://shopify/MarketingActivity/1\", marketingActivityTitle: \"Foreign native\" }) { marketingActivity { id title } redirectPath userErrors { field message code } } }",
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )
  let app_b_response = json.to_string(app_b_update.data)
  assert string.contains(app_b_response, "\"marketingActivityUpdate\":null")
  assert string.contains(app_b_response, "\"code\":\"ACCESS_DENIED\"")
  assert run_with_api_client(
      created.store,
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { title } }",
      "app-a",
    )
    == "{\"marketingActivity\":{\"title\":\"Native scoped\"}}"

  let app_a_update =
    marketing.process_mutation(
      created.store,
      created.identity,
      request_path,
      "mutation { marketingActivityUpdate(input: { id: \"gid://shopify/MarketingActivity/1\", marketingActivityTitle: \"Native updated\" }) { marketingActivity { id title } redirectPath userErrors { field message code } } }",
      empty_vars(),
      upstream_context_with_api_client("app-a"),
    )
  assert string.contains(
    json.to_string(app_a_update.data),
    "\"title\":\"Native updated\"",
  )
}

pub fn external_activity_create_rejects_invalid_channel_handle_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let invalid_unknown =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Bad channel\", remoteId: \"remote-invalid-channel\", tactic: NEWSLETTER, marketingChannelType: EMAIL, channelHandle: \"made-up-handle\", utm: { campaign: \"invalid-channel\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let unknown_response = json.to_string(invalid_unknown.data)
  assert string.contains(unknown_response, "\"marketingActivity\":null")
  assert string.contains(unknown_response, "\"field\":[\"input\"]")
  assert string.contains(
    unknown_response,
    "\"code\":\"INVALID_CHANNEL_HANDLE\"",
  )
  assert store.list_effective_marketing_activities(invalid_unknown.store) == []
  assert store.get_log(invalid_unknown.store) == []

  let invalid_app =
    marketing.process_mutation(
      registered_email_store_for_app("app-a"),
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Wrong app\", remoteId: \"remote-wrong-app\", tactic: NEWSLETTER, marketingChannelType: EMAIL, channelHandle: \"email\", utm: { campaign: \"wrong-app\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      upstream_context_with_api_client("app-b"),
    )
  let app_response = json.to_string(invalid_app.data)
  assert string.contains(app_response, "\"marketingActivity\":null")
  assert string.contains(app_response, "\"field\":[\"input\"]")
  assert string.contains(app_response, "\"code\":\"INVALID_CHANNEL_HANDLE\"")
  assert store.list_effective_marketing_activities(invalid_app.store) == []
}

pub fn external_activity_upsert_create_rejects_invalid_channel_handle_test() {
  let result =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityUpsertExternal(input: { title: \"Bad channel\", remoteId: \"remote-upsert-invalid-channel\", tactic: NEWSLETTER, marketingChannelType: EMAIL, channelHandle: \"made-up-handle\", utm: { campaign: \"upsert-invalid-channel\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(result.data)
  assert string.contains(response, "\"marketingActivity\":null")
  assert string.contains(response, "\"field\":[\"input\"]")
  assert string.contains(response, "\"code\":\"INVALID_CHANNEL_HANDLE\"")
  assert store.list_effective_marketing_activities(result.store) == []
}

pub fn external_activity_create_and_upsert_reject_currency_mismatch_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let create =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Bad currency\", remoteId: \"remote-currency-create\", tactic: NEWSLETTER, marketingChannelType: EMAIL, budget: { budgetType: DAILY, total: { amount: \"1.00\", currencyCode: USD } }, adSpend: { amount: \"1.00\", currencyCode: EUR }, utm: { campaign: \"currency-create\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let create_response = json.to_string(create.data)
  assert string.contains(create_response, "\"marketingActivity\":null")
  assert string.contains(create_response, "\"field\":[\"input\"]")
  assert string.contains(
    create_response,
    "\"message\":\"Currency code is not matching between budget and ad spend\"",
  )
  assert string.contains(create_response, "\"code\":null")
  assert store.list_effective_marketing_activities(create.store) == []

  let upsert =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpsertExternal(input: { title: \"Bad currency\", remoteId: \"remote-currency-upsert\", tactic: NEWSLETTER, marketingChannelType: EMAIL, budget: { budgetType: DAILY, total: { amount: \"1.00\", currencyCode: USD } }, adSpend: { amount: \"1.00\", currencyCode: EUR }, utm: { campaign: \"currency-upsert\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let upsert_response = json.to_string(upsert.data)
  assert string.contains(upsert_response, "\"marketingActivity\":null")
  assert string.contains(
    upsert_response,
    "\"message\":\"Currency code is not matching between budget and ad spend\"",
  )
  assert string.contains(upsert_response, "\"code\":null")
  assert store.list_effective_marketing_activities(upsert.store) == []
}

pub fn external_activity_create_rejects_distinct_uniqueness_errors_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let duplicate_remote_doc =
    "mutation { marketingActivityCreateExternal(input: { title: \"Duplicate remote\", remoteId: \"remote-dupe\", tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"url-remote-dupe\", utm: { campaign: \"remote-dupe\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }"
  let seeded_remote =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      duplicate_remote_doc,
      empty_vars(),
      empty_upstream_context(),
    )
  let duplicate_remote =
    marketing.process_mutation(
      seeded_remote.store,
      seeded_remote.identity,
      request_path,
      duplicate_remote_doc,
      empty_vars(),
      empty_upstream_context(),
    )
  let remote_response = json.to_string(duplicate_remote.data)
  assert string.contains(remote_response, "\"marketingActivity\":null")
  assert string.contains(
    remote_response,
    "\"message\":\"Validation failed: Remote ID has already been taken\"",
  )
  assert string.contains(remote_response, "\"code\":null")
  assert list.length(store.list_effective_marketing_activities(
      duplicate_remote.store,
    ))
    == 1
  assert store.get_log(duplicate_remote.store) == []

  let seeded_utm =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Seed UTM\", remoteId: \"remote-utm-1\", tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"url-utm-1\", utm: { campaign: \"same-utm\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let duplicate_utm =
    marketing.process_mutation(
      seeded_utm.store,
      seeded_utm.identity,
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Duplicate UTM\", remoteId: \"remote-utm-2\", tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"url-utm-2\", utm: { campaign: \"same-utm\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let utm_response = json.to_string(duplicate_utm.data)
  assert string.contains(utm_response, "\"marketingActivity\":null")
  assert string.contains(
    utm_response,
    "\"message\":\"Validation failed: Utm campaign has already been taken\"",
  )
  assert string.contains(utm_response, "\"code\":null")
  assert list.length(store.list_effective_marketing_activities(
      duplicate_utm.store,
    ))
    == 1

  let seeded_url =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Seed URL\", remoteId: \"remote-url-1\", tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"same-url\", utm: { campaign: \"url-one\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let duplicate_url =
    marketing.process_mutation(
      seeded_url.store,
      seeded_url.identity,
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Duplicate URL\", remoteId: \"remote-url-2\", tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"same-url\", utm: { campaign: \"url-two\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let url_response = json.to_string(duplicate_url.data)
  assert string.contains(url_response, "\"marketingActivity\":null")
  assert string.contains(
    url_response,
    "\"message\":\"Validation failed: Url parameter value has already been taken\"",
  )
  assert string.contains(url_response, "\"code\":null")
  assert list.length(store.list_effective_marketing_activities(
      duplicate_url.store,
    ))
    == 1
}

pub fn external_activity_immutable_update_and_upsert_fields_reject_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let activity_id = "gid://shopify/MarketingActivity/501"
  let source =
    store.upsert_base_marketing_activities(store.new(), [
      external_activity_with_details(
        activity_id,
        "remote-immutable",
        "Immutable child",
        "channel-a",
        "promo-1",
        "parent-a",
        "CAMPAIGN",
      ),
    ])
  let source =
    store.upsert_base_marketing_events(source, [
      marketing_event("gid://shopify/MarketingEvent/701", "parent-a"),
      marketing_event("gid://shopify/MarketingEvent/702", "parent-b"),
    ])

  let channel_changed =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpsertExternal(input: { remoteId: \"remote-immutable\", title: \"Changed\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, channelHandle: \"channel-b\", utm: { campaign: \"campaign\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let channel_response = json.to_string(channel_changed.data)
  assert string.contains(channel_response, "\"marketingActivity\":null")
  assert string.contains(channel_response, "\"field\":[\"input\"]")
  assert string.contains(
    channel_response,
    "\"code\":\"IMMUTABLE_CHANNEL_HANDLE\"",
  )
  assert run(
      channel_changed.store,
      "{ marketingActivity(id: \"" <> activity_id <> "\") { title } }",
    )
    == "{\"marketingActivity\":{\"title\":\"Immutable child\"}}"

  let url_cleared =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpsertExternal(input: { remoteId: \"remote-immutable\", title: \"Changed\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, channelHandle: \"channel-a\", urlParameterValue: null, utm: { campaign: \"campaign\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(
    json.to_string(url_cleared.data),
    "\"code\":\"IMMUTABLE_URL_PARAMETER\"",
  )

  let utm_changed =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpsertExternal(input: { remoteId: \"remote-immutable\", title: \"Changed\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, channelHandle: \"channel-a\", urlParameterValue: \"promo-1\", parentRemoteId: \"parent-a\", hierarchyLevel: CAMPAIGN, utm: { campaign: \"changed\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(
    json.to_string(utm_changed.data),
    "\"code\":\"IMMUTABLE_UTM_PARAMETERS\"",
  )

  let missing_parent =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpsertExternal(input: { remoteId: \"remote-immutable\", title: \"Changed\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, channelHandle: \"channel-a\", urlParameterValue: \"promo-1\", parentRemoteId: \"missing-parent\", hierarchyLevel: CAMPAIGN, utm: { campaign: \"campaign\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(
    json.to_string(missing_parent.data),
    "\"code\":\"INVALID_REMOTE_ID\"",
  )

  let parent_changed =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpdateExternal(remoteId: \"remote-immutable\", input: { title: \"Changed\", parentRemoteId: \"parent-b\" }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(
    json.to_string(parent_changed.data),
    "\"code\":\"IMMUTABLE_PARENT_ID\"",
  )

  let hierarchy_changed =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpdateExternal(remoteId: \"remote-immutable\", input: { title: \"Changed\", hierarchyLevel: AD_GROUP }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(
    json.to_string(hierarchy_changed.data),
    "\"code\":\"IMMUTABLE_HIERARCHY_LEVEL\"",
  )
}

pub fn external_activity_update_and_upsert_reject_non_external_or_orphan_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let source =
    store.upsert_base_marketing_activities(store.new(), [
      non_external_activity(
        "gid://shopify/MarketingActivity/601",
        "native-remote",
      ),
      external_activity_without_event(
        "gid://shopify/MarketingActivity/602",
        "orphan-remote",
      ),
    ])

  let non_external =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpsertExternal(input: { remoteId: \"native-remote\", title: \"Changed\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, utm: { campaign: \"campaign\", source: \"email\", medium: \"newsletter\" } }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(
    json.to_string(non_external.data),
    "\"code\":\"ACTIVITY_NOT_EXTERNAL\"",
  )

  let orphan =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityUpdateExternal(remoteId: \"orphan-remote\", input: { title: \"Changed\" }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(
    json.to_string(orphan.data),
    "\"code\":\"MARKETING_EVENT_DOES_NOT_EXIST\"",
  )
}

pub fn update_external_requires_a_selector_test() {
  let result =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityUpdateExternal(input: { title: \"Changed\" }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(
    json.to_string(result.data),
    "\"code\":\"INVALID_MARKETING_ACTIVITY_EXTERNAL_ARGUMENTS\"",
  )
}

pub fn update_external_rejects_conflicting_selector_matches_test() {
  let activity_a =
    activity_with_utm(
      "gid://shopify/MarketingActivity/501",
      "Activity A",
      "remote-a",
      "2026-05-05T00:00:00Z",
      "camp-a",
      "src-a",
      "med-a",
    )
  let activity_b =
    activity_with_utm(
      "gid://shopify/MarketingActivity/502",
      "Activity B",
      "remote-b",
      "2026-05-05T00:01:00Z",
      "camp-b",
      "src-b",
      "med-b",
    )
  let source =
    store.upsert_base_marketing_activities(store.new(), [
      activity_a,
      activity_b,
    ])

  let result =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityUpdateExternal(remoteId: \"remote-a\", utm: { campaign: \"camp-b\", source: \"src-b\", medium: \"med-b\" }, input: { title: \"Changed A\" }) { marketingActivity { id title } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(result.data)
  assert string.contains(response, "\"marketingActivity\":null")
  assert string.contains(
    response,
    "\"code\":\"MARKETING_ACTIVITY_DOES_NOT_EXIST\"",
  )

  let read_after =
    run(
      result.store,
      "{ first: marketingActivity(id: \"gid://shopify/MarketingActivity/501\") { id title } second: marketingActivity(id: \"gid://shopify/MarketingActivity/502\") { id title } }",
    )
  assert string.contains(read_after, "\"title\":\"Activity A\"")
  assert string.contains(read_after, "\"title\":\"Activity B\"")
  assert !string.contains(read_after, "Changed A")
}

pub fn delete_external_requires_selector_and_preserves_missing_record_error_test() {
  let no_args =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityDeleteExternal { deletedMarketingActivityId userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let no_args_response = json.to_string(no_args.data)
  assert string.contains(
    no_args_response,
    "\"deletedMarketingActivityId\":null",
  )
  assert string.contains(
    no_args_response,
    "\"code\":\"INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS\"",
  )

  let missing =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityDeleteExternal(remoteId: \"missing\") { deletedMarketingActivityId userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let missing_response = json.to_string(missing.data)
  assert string.contains(
    missing_response,
    "\"deletedMarketingActivityId\":null",
  )
  assert string.contains(
    missing_response,
    "\"code\":\"MARKETING_ACTIVITY_DOES_NOT_EXIST\"",
  )
}

pub fn delete_external_rejects_conflicting_selector_matches_test() {
  let activity_a =
    activity_with_utm(
      "gid://shopify/MarketingActivity/601",
      "Delete Activity A",
      "delete-remote-a",
      "2026-05-05T00:00:00Z",
      "delete-camp-a",
      "delete-src-a",
      "delete-med-a",
    )
  let activity_b =
    activity_with_utm(
      "gid://shopify/MarketingActivity/602",
      "Delete Activity B",
      "delete-remote-b",
      "2026-05-05T00:01:00Z",
      "delete-camp-b",
      "delete-src-b",
      "delete-med-b",
    )
  let source =
    store.upsert_base_marketing_activities(store.new(), [
      activity_a,
      activity_b,
    ])

  let result =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityDeleteExternal(marketingActivityId: \"gid://shopify/MarketingActivity/601\", remoteId: \"delete-remote-b\") { deletedMarketingActivityId userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(result.data)
  assert string.contains(response, "\"deletedMarketingActivityId\":null")
  assert string.contains(
    response,
    "\"code\":\"MARKETING_ACTIVITY_DOES_NOT_EXIST\"",
  )

  let read_after =
    run(
      result.store,
      "{ first: marketingActivity(id: \"gid://shopify/MarketingActivity/601\") { id title } second: marketingActivity(id: \"gid://shopify/MarketingActivity/602\") { id title } }",
    )
  assert string.contains(read_after, "\"title\":\"Delete Activity A\"")
  assert string.contains(read_after, "\"title\":\"Delete Activity B\"")
}

pub fn delete_external_rejects_native_and_parent_activities_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let native_id = "gid://shopify/MarketingActivity/701"
  let parent_id = "gid://shopify/MarketingActivity/702"
  let child_id = "gid://shopify/MarketingActivity/703"
  let source =
    store.upsert_base_marketing_activities(store.new(), [
      non_external_activity(native_id, "native-remote"),
      external_activity_with_details(
        parent_id,
        "parent-remote",
        "Parent",
        "channel-a",
        "promo-parent",
        "",
        "CAMPAIGN",
      ),
      external_activity_with_details(
        child_id,
        "child-remote",
        "Child",
        "channel-a",
        "promo-child",
        "parent-remote",
        "AD_GROUP",
      ),
    ])

  let native_delete =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityDeleteExternal(marketingActivityId: \""
        <> native_id
        <> "\") { deletedMarketingActivityId userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let native_response = json.to_string(native_delete.data)
  assert string.contains(native_response, "\"deletedMarketingActivityId\":null")
  assert string.contains(native_response, "\"code\":\"ACTIVITY_NOT_EXTERNAL\"")
  assert run(
      native_delete.store,
      "{ marketingActivity(id: \"" <> native_id <> "\") { id title } }",
    )
    == "{\"marketingActivity\":{\"id\":\""
    <> native_id
    <> "\",\"title\":\"Native\"}}"

  let parent_delete =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      request_path,
      "mutation { marketingActivityDeleteExternal(remoteId: \"parent-remote\") { deletedMarketingActivityId userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let parent_response = json.to_string(parent_delete.data)
  assert string.contains(parent_response, "\"deletedMarketingActivityId\":null")
  assert string.contains(
    parent_response,
    "\"code\":\"CANNOT_DELETE_ACTIVITY_WITH_CHILD_EVENTS\"",
  )
  assert run(
      parent_delete.store,
      "{ marketingActivity(id: \"" <> parent_id <> "\") { id title } }",
    )
    == "{\"marketingActivity\":{\"id\":\""
    <> parent_id
    <> "\",\"title\":\"Parent\"}}"
}

pub fn delete_all_external_in_flight_blocks_external_writes_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let create_doc =
    "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { field message code } } }"
  let created =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      request_path,
      create_doc,
      empty_vars(),
      empty_upstream_context(),
    )
  let delete_all =
    marketing.process_mutation(
      created.store,
      created.identity,
      request_path,
      "mutation { marketingActivitiesDeleteAllExternal { job { id done } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert store.has_marketing_delete_all_external_in_flight(delete_all.store)

  let blocked_create =
    marketing.process_mutation(
      delete_all.store,
      delete_all.identity,
      request_path,
      "mutation { marketingActivityCreateExternal(input: { title: \"Blocked\", remoteId: \"remote-2\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"utm_campaign=blocked\", utm: { campaign: \"blocked\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let blocked_update =
    marketing.process_mutation(
      delete_all.store,
      delete_all.identity,
      request_path,
      "mutation { marketingActivityUpdateExternal(remoteId: \"remote-1\", input: { title: \"Blocked\" }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let blocked_upsert =
    marketing.process_mutation(
      delete_all.store,
      delete_all.identity,
      request_path,
      "mutation { marketingActivityUpsertExternal(input: { title: \"Blocked\", remoteId: \"remote-3\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"utm_campaign=blocked\", utm: { campaign: \"blocked\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )

  assert string.contains(
    json.to_string(blocked_create.data),
    "\"code\":\"DELETE_JOB_ENQUEUED\"",
  )
  assert string.contains(
    json.to_string(blocked_update.data),
    "\"code\":\"DELETE_JOB_ENQUEUED\"",
  )
  assert string.contains(
    json.to_string(blocked_upsert.data),
    "\"code\":\"DELETE_JOB_ENQUEUED\"",
  )
}

pub fn native_activity_validation_update_and_log_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let missing_doc =
    "mutation { marketingActivityCreate(input: { marketingActivityTitle: \"Native\" }) { userErrors { field message code } } }"
  let missing_extension =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      missing_doc,
      empty_vars(),
      empty_upstream_context(),
    )
  let missing_extension =
    record_drafts(missing_extension, request_path, missing_doc)
  assert string.contains(
    json.to_string(missing_extension.data),
    "Could not find the marketing extension",
  )
  assert store.get_log(missing_extension.store) == []

  let create_doc =
    "mutation { marketingActivityCreate(input: { marketingActivityTitle: \"Native\", marketingActivityExtensionId: \"gid://shopify/MarketingActivityExtension/abc\" }) { userErrors { message } } }"
  let created =
    marketing.process_mutation(
      missing_extension.store,
      missing_extension.identity,
      request_path,
      create_doc,
      empty_vars(),
      empty_upstream_context(),
    )
  let created = record_drafts(created, request_path, create_doc)
  assert created.staged_resource_ids == ["gid://shopify/MarketingActivity/1"]
  let update_doc =
    "mutation { marketingActivityUpdate(input: { id: \"gid://shopify/MarketingActivity/1\", marketingActivityTitle: \"Native updated\", status: PAUSED }) { marketingActivity { id title status statusLabel } redirectPath userErrors { message } } }"
  let updated =
    marketing.process_mutation(
      created.store,
      created.identity,
      request_path,
      update_doc,
      empty_vars(),
      empty_upstream_context(),
    )
  let updated = record_drafts(updated, request_path, update_doc)
  assert string.contains(
    json.to_string(updated.data),
    "\"marketingActivity\":{\"id\":\"gid://shopify/MarketingActivity/1\",\"title\":\"Native updated\",\"status\":\"PAUSED\",\"statusLabel\":\"Paused\"}",
  )
  assert list.length(store.get_log(updated.store)) == 2
}

pub fn engagement_create_and_delete_stages_metric_records_test() {
  let created =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let engagement =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(remoteId: \"remote-1\", marketingEngagement: { occurredOn: \"2026-04-27\", impressionsCount: 10, adSpend: { amount: \"4.50\", currencyCode: USD }, orders: \"2.0\" }) { marketingEngagement { occurredOn impressionsCount adSpend { amount currencyCode } orders marketingActivity { id } } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let engagement_response = json.to_string(engagement.data)
  assert string.contains(engagement_response, "\"impressionsCount\":10")
  assert string.contains(engagement_response, "\"orders\":\"2.0\"")
  assert list.length(store.list_effective_marketing_engagements(
      engagement.store,
    ))
    == 1

  let channel_engagement =
    marketing.process_mutation(
      engagement.store,
      engagement.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(channelHandle: \"email\", marketingEngagement: { occurredOn: \"2026-04-28\", clicksCount: 3 }) { marketingEngagement { occurredOn channelHandle clicksCount } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert list.length(store.list_effective_marketing_engagements(
      channel_engagement.store,
    ))
    == 2

  let deleted =
    marketing.process_mutation(
      channel_engagement.store,
      channel_engagement.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementsDelete(channelHandle: \"email\") { result userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert json.to_string(deleted.data)
    == "{\"data\":{\"marketingEngagementsDelete\":{\"result\":\"Engagement data marked for deletion for 1 channel(s)\",\"userErrors\":[]}}}"
  assert list.length(store.list_effective_marketing_engagements(deleted.store))
    == 1
}

pub fn engagement_create_rejects_mismatched_input_currencies_test() {
  let created =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let engagement =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(remoteId: \"remote-1\", marketingEngagement: { occurredOn: \"2026-04-27\", adSpend: { amount: \"10.00\", currencyCode: USD }, sales: { amount: \"30.00\", currencyCode: EUR } }) { marketingEngagement { occurredOn adSpend { amount currencyCode } sales { amount currencyCode } } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(engagement.data)
  assert string.contains(response, "\"marketingEngagement\":null")
  assert string.contains(response, "\"field\":[\"marketingEngagement\"]")
  assert string.contains(response, "\"code\":\"CURRENCY_CODE_MISMATCH_INPUT\"")
  assert store.list_effective_marketing_engagements(engagement.store) == []
}

pub fn engagement_create_prioritizes_multiple_identifiers_before_input_currency_test() {
  let created =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let engagement =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(marketingActivityId: \"gid://shopify/MarketingActivity/1\", remoteId: \"remote-1\", marketingEngagement: { occurredOn: \"2026-04-27\", adSpend: { amount: \"10.00\", currencyCode: USD }, sales: { amount: \"30.00\", currencyCode: EUR } }) { marketingEngagement { occurredOn } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(engagement.data)
  assert string.contains(response, "\"marketingEngagement\":null")
  assert string.contains(
    response,
    "\"code\":\"INVALID_MARKETING_ENGAGEMENT_ARGUMENTS\"",
  )
  assert !string.contains(response, "\"code\":\"CURRENCY_CODE_MISMATCH_INPUT\"")
  assert store.list_effective_marketing_engagements(engagement.store) == []
}

pub fn engagement_create_prioritizes_channel_multiple_identifiers_before_input_currency_test() {
  let engagement =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(channelHandle: \"email\", remoteId: \"remote-1\", marketingEngagement: { occurredOn: \"2026-04-27\", adSpend: { amount: \"10.00\", currencyCode: USD }, sales: { amount: \"30.00\", currencyCode: EUR } }) { marketingEngagement { occurredOn } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(engagement.data)
  assert string.contains(response, "\"marketingEngagement\":null")
  assert string.contains(
    response,
    "\"code\":\"INVALID_MARKETING_ENGAGEMENT_ARGUMENTS\"",
  )
  assert !string.contains(response, "\"code\":\"CURRENCY_CODE_MISMATCH_INPUT\"")
  assert store.list_effective_marketing_engagements(engagement.store) == []
}

pub fn engagement_create_distinguishes_deleted_event_from_unknown_activity_test() {
  let activity_id = "gid://shopify/MarketingActivity/901"
  let event_id = "gid://shopify/MarketingEvent/901"
  let source =
    store.upsert_base_marketing_activities(store.new(), [
      activity(
        activity_id,
        "Deleted event activity",
        "deleted-event-remote",
        "2026-05-05T00:00:00Z",
      ),
    ])
    |> store.upsert_base_marketing_events([
      marketing_event(event_id, "deleted-event-remote"),
    ])
    |> store.stage_delete_marketing_event(event_id)

  let deleted_event =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(remoteId: \"deleted-event-remote\", marketingEngagement: { occurredOn: \"2026-04-27\", adSpend: { amount: \"10.00\", currencyCode: USD } }) { marketingEngagement { occurredOn } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let deleted_event_response = json.to_string(deleted_event.data)
  assert string.contains(
    deleted_event_response,
    "\"code\":\"MARKETING_EVENT_DOES_NOT_EXIST\"",
  )
  assert !string.contains(
    deleted_event_response,
    "\"code\":\"MARKETING_ACTIVITY_DOES_NOT_EXIST\"",
  )

  let missing_activity =
    marketing.process_mutation(
      source,
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(remoteId: \"missing-activity\", marketingEngagement: { occurredOn: \"2026-04-27\", adSpend: { amount: \"10.00\", currencyCode: USD } }) { marketingEngagement { occurredOn } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let missing_activity_response = json.to_string(missing_activity.data)
  assert string.contains(
    missing_activity_response,
    "\"code\":\"MARKETING_ACTIVITY_DOES_NOT_EXIST\"",
  )
  assert !string.contains(
    missing_activity_response,
    "\"code\":\"MARKETING_EVENT_DOES_NOT_EXIST\"",
  )
  assert store.list_effective_marketing_engagements(deleted_event.store) == []
  assert store.list_effective_marketing_engagements(missing_activity.store)
    == []
}

pub fn engagement_create_rejects_activity_currency_mismatch_by_id_test() {
  let created =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", budget: { budgetType: DAILY, total: { amount: \"100.00\", currencyCode: USD } }, urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let engagement =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(marketingActivityId: \"gid://shopify/MarketingActivity/1\", marketingEngagement: { occurredOn: \"2026-04-27\", adSpend: { amount: \"10.00\", currencyCode: EUR } }) { marketingEngagement { occurredOn adSpend { amount currencyCode } } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(engagement.data)
  assert string.contains(response, "\"marketingEngagement\":null")
  assert string.contains(response, "\"field\":[\"marketingEngagement\"]")
  assert string.contains(
    response,
    "\"code\":\"MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH\"",
  )
  assert store.list_effective_marketing_engagements(engagement.store) == []
}

pub fn engagement_create_rejects_activity_currency_mismatch_by_remote_id_test() {
  let created =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", adSpend: { amount: \"25.00\", currencyCode: USD }, urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let engagement =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(remoteId: \"remote-1\", marketingEngagement: { occurredOn: \"2026-04-27\", sales: { amount: \"30.00\", currencyCode: EUR } }) { marketingEngagement { occurredOn sales { amount currencyCode } } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(engagement.data)
  assert string.contains(response, "\"marketingEngagement\":null")
  assert string.contains(response, "\"field\":[\"marketingEngagement\"]")
  assert string.contains(
    response,
    "\"code\":\"MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH\"",
  )
  assert store.list_effective_marketing_engagements(engagement.store) == []
}

pub fn engagement_create_channel_handle_checks_input_currencies_only_test() {
  let created =
    marketing.process_mutation(
      registered_email_store(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", budget: { budgetType: DAILY, total: { amount: \"100.00\", currencyCode: USD } }, urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { message } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let accepted =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(channelHandle: \"email\", marketingEngagement: { occurredOn: \"2026-04-28\", adSpend: { amount: \"10.00\", currencyCode: EUR }, clicksCount: 3 }) { marketingEngagement { occurredOn channelHandle adSpend { amount currencyCode } clicksCount } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  assert string.contains(json.to_string(accepted.data), "\"userErrors\":[]")
  assert list.length(store.list_effective_marketing_engagements(accepted.store))
    == 1

  let rejected =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(channelHandle: \"email\", marketingEngagement: { occurredOn: \"2026-04-28\", adSpend: { amount: \"10.00\", currencyCode: USD }, sales: { amount: \"30.00\", currencyCode: EUR } }) { marketingEngagement { occurredOn channelHandle adSpend { amount currencyCode } sales { amount currencyCode } } userErrors { field message code } } }",
      empty_vars(),
      empty_upstream_context(),
    )
  let response = json.to_string(rejected.data)
  assert string.contains(response, "\"marketingEngagement\":null")
  assert string.contains(response, "\"code\":\"CURRENCY_CODE_MISMATCH_INPUT\"")
  assert store.list_effective_marketing_engagements(rejected.store) == []
}
