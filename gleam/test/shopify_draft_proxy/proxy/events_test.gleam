import gleam/json
import shopify_draft_proxy/proxy/events

fn handle(query: String) -> String {
  let assert Ok(data) = events.handle_events_query(query)
  json.to_string(data)
}

pub fn empty_event_field_returns_null_test() {
  // Single-event lookups always miss in the proxy — the response is
  // just `{ event: null }`.
  let result = handle("{ event(id: \"gid://shopify/Event/1\") { id } }")
  assert result == "{\"event\":null}"
}

pub fn events_connection_returns_empty_shape_test() {
  let result =
    handle(
      "{ events(first: 10) { edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }",
    )
  assert result
    == "{\"events\":{\"edges\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}}"
}

pub fn events_connection_with_only_nodes_test() {
  // The proxy only fills in selections the client asks for.
  let result = handle("{ events(first: 5) { nodes { id } } }")
  assert result == "{\"events\":{\"nodes\":[]}}"
}

pub fn events_count_returns_exact_zero_test() {
  let result = handle("{ eventsCount { count precision } }")
  assert result == "{\"eventsCount\":{\"count\":0,\"precision\":\"EXACT\"}}"
}

pub fn events_count_unknown_subfield_is_null_test() {
  let result = handle("{ eventsCount { count whatever } }")
  assert result == "{\"eventsCount\":{\"count\":0,\"whatever\":null}}"
}

pub fn unknown_root_field_is_null_test() {
  let result = handle("{ event(id: \"x\") { id } whatever }")
  assert result == "{\"event\":null,\"whatever\":null}"
}

pub fn alias_is_used_as_response_key_test() {
  let result = handle("{ myEvent: event(id: \"x\") { id } }")
  assert result == "{\"myEvent\":null}"
}

pub fn process_wraps_in_data_envelope_test() {
  let assert Ok(envelope) = events.process("{ event(id: \"x\") { id } }")
  assert json.to_string(envelope) == "{\"data\":{\"event\":null}}"
}

pub fn parse_failure_propagates_test() {
  let assert Error(events.ParseFailed(_)) =
    events.handle_events_query("{ events(")
}
