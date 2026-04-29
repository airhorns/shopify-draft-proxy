//// Read-path tests for the minimal `proxy/marketing` stub. Every
//// singular root returns null, every connection root returns the
//// empty-connection shape — this guards that contract on both
//// compile targets.

import gleam/json
import shopify_draft_proxy/proxy/marketing

fn run(query: String) -> String {
  let assert Ok(data) = marketing.handle_marketing_query(query)
  json.to_string(data)
}

pub fn is_marketing_query_root_test() {
  assert marketing.is_marketing_query_root("marketingActivity")
  assert marketing.is_marketing_query_root("marketingActivities")
  assert marketing.is_marketing_query_root("marketingEvent")
  assert marketing.is_marketing_query_root("marketingEvents")
  assert !marketing.is_marketing_query_root("marketingEngagementCreate")
  assert !marketing.is_marketing_query_root("shop")
}

pub fn marketing_activity_returns_null_test() {
  let result =
    run(
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { id } }",
    )
  assert result == "{\"marketingActivity\":null}"
}

pub fn marketing_event_returns_null_test() {
  let result =
    run("{ marketingEvent(id: \"gid://shopify/MarketingEvent/1\") { id } }")
  assert result == "{\"marketingEvent\":null}"
}

pub fn marketing_activities_returns_empty_connection_test() {
  let result =
    run(
      "{ marketingActivities(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
    )
  assert result
    == "{\"marketingActivities\":{\"nodes\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}"
}

pub fn marketing_events_returns_empty_connection_test() {
  let result =
    run("{ marketingEvents(first: 10) { nodes { id } edges { cursor } } }")
  assert result == "{\"marketingEvents\":{\"nodes\":[],\"edges\":[]}}"
}

pub fn process_wraps_in_data_envelope_test() {
  let assert Ok(data) =
    marketing.process(
      "{ marketingActivity(id: \"gid://shopify/MarketingActivity/1\") { id } }",
    )
  assert json.to_string(data) == "{\"data\":{\"marketingActivity\":null}}"
}
