//// Read and mutation tests for the Gleam Marketing domain port.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None}
import gleam/string
import shopify_draft_proxy/proxy/marketing
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  MarketingBool, MarketingObject, MarketingRecord, MarketingString,
}

fn empty_vars() {
  dict.new()
}

fn run(source: store.Store, query: String) -> String {
  let assert Ok(data) =
    marketing.handle_marketing_query(source, query, empty_vars())
  json.to_string(data)
}

fn activity(id: String, title: String, remote_id: String, created_at: String) {
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
            #("campaign", MarketingString("spring")),
            #("source", MarketingString("email")),
            #("medium", MarketingString("newsletter")),
          ]),
        ),
      ),
      #("marketingEvent", MarketingObject(event)),
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

pub fn external_activity_create_update_delete_stages_locally_test() {
  let assert Ok(created) =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id title remoteId marketingEvent { id remoteId channelHandle } } userErrors { field message code } } }",
      empty_vars(),
    )
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

  let assert Ok(updated) =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityUpdateExternal(remoteId: \"remote-1\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, input: { title: \"Launch updated\", status: INACTIVE }) { marketingActivity { id title status marketingEvent { endedAt } } userErrors { message } } }",
      empty_vars(),
    )
  let update_response = json.to_string(updated.data)
  assert string.contains(update_response, "\"title\":\"Launch updated\"")
  assert string.contains(update_response, "\"status\":\"INACTIVE\"")

  let assert Ok(deleted) =
    marketing.process_mutation(
      updated.store,
      updated.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityDeleteExternal(remoteId: \"remote-1\") { deletedMarketingActivityId userErrors { message } } }",
      empty_vars(),
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

pub fn native_activity_validation_update_and_log_test() {
  let assert Ok(missing_extension) =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreate(input: { marketingActivityTitle: \"Native\" }) { userErrors { field message code } } }",
      empty_vars(),
    )
  assert string.contains(
    json.to_string(missing_extension.data),
    "Could not find the marketing extension",
  )
  assert store.get_log(missing_extension.store) == []

  let assert Ok(created) =
    marketing.process_mutation(
      missing_extension.store,
      missing_extension.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreate(input: { marketingActivityTitle: \"Native\", marketingActivityExtensionId: \"gid://shopify/MarketingActivityExtension/abc\" }) { userErrors { message } } }",
      empty_vars(),
    )
  assert created.staged_resource_ids == ["gid://shopify/MarketingActivity/1"]
  let assert Ok(updated) =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityUpdate(input: { id: \"gid://shopify/MarketingActivity/1\", marketingActivityTitle: \"Native updated\", status: PAUSED }) { marketingActivity { id title status statusLabel } redirectPath userErrors { message } } }",
      empty_vars(),
    )
  assert string.contains(
    json.to_string(updated.data),
    "\"marketingActivity\":{\"id\":\"gid://shopify/MarketingActivity/1\",\"title\":\"Native updated\",\"status\":\"PAUSED\",\"statusLabel\":\"Paused\"}",
  )
  assert list.length(store.get_log(updated.store)) == 2
}

pub fn engagement_create_and_delete_stages_metric_records_test() {
  let assert Ok(created) =
    marketing.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingActivityCreateExternal(input: { title: \"Launch\", remoteId: \"remote-1\", urlParameterValue: \"utm_campaign=launch\", utm: { campaign: \"launch\", source: \"email\", medium: \"newsletter\" }, channelHandle: \"email\" }) { marketingActivity { id } userErrors { message } } }",
      empty_vars(),
    )
  let assert Ok(engagement) =
    marketing.process_mutation(
      created.store,
      created.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(remoteId: \"remote-1\", marketingEngagement: { occurredOn: \"2026-04-27\", impressionsCount: 10, adSpend: { amount: \"4.50\", currencyCode: USD }, orders: \"2.0\" }) { marketingEngagement { occurredOn impressionsCount adSpend { amount currencyCode } orders marketingActivity { id } } userErrors { message } } }",
      empty_vars(),
    )
  let engagement_response = json.to_string(engagement.data)
  assert string.contains(engagement_response, "\"impressionsCount\":10")
  assert string.contains(engagement_response, "\"orders\":\"2.0\"")
  assert list.length(store.list_effective_marketing_engagements(
      engagement.store,
    ))
    == 1

  let assert Ok(channel_engagement) =
    marketing.process_mutation(
      engagement.store,
      engagement.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementCreate(channelHandle: \"email\", marketingEngagement: { occurredOn: \"2026-04-28\", clicksCount: 3 }) { marketingEngagement { occurredOn channelHandle clicksCount } userErrors { message } } }",
      empty_vars(),
    )
  assert list.length(store.list_effective_marketing_engagements(
      channel_engagement.store,
    ))
    == 2

  let assert Ok(deleted) =
    marketing.process_mutation(
      channel_engagement.store,
      channel_engagement.identity,
      "/admin/api/2026-04/graphql.json",
      "mutation { marketingEngagementsDelete(channelHandle: \"email\") { result userErrors { message } } }",
      empty_vars(),
    )
  assert json.to_string(deleted.data)
    == "{\"data\":{\"marketingEngagementsDelete\":{\"result\":\"Engagement data marked for deletion for 1 channel(s)\",\"userErrors\":[]}}}"
  assert list.length(store.list_effective_marketing_engagements(deleted.store))
    == 1
}
