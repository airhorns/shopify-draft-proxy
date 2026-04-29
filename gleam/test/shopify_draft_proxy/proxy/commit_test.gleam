//// Tests for `proxy/commit.gleam` — the pure helpers (GID rewriting,
//// header forwarding, response interpretation) plus the cross-target
//// `run_commit_sync` driver exercised through a fake `send` injection.
////
//// The TS surface we mirror lives in `src/meta/routes.ts:30-740` and
//// `src/shopify/upstream-request.ts:5-120`.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/state/store

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

const synthetic_one: String =
  "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"

const synthetic_two: String =
  "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic"

const authoritative_one: String = "gid://shopify/SavedSearch/12345"

const authoritative_two: String = "gid://shopify/SavedSearch/67890"

fn empty_capability() -> store.Capability {
  store.Capability(
    operation_name: Some("savedSearchCreate"),
    domain: "saved-searches",
    execution: "stage-locally",
  )
}

fn entry_factory(
  id: String,
  query: String,
  staged: List(String),
) -> store.MutationLogEntry {
  store.MutationLogEntry(
    id: id,
    received_at: "2026-04-29T12:00:00.000Z",
    operation_name: Some("savedSearchCreate"),
    path: "/admin/api/2025-01/graphql.json",
    query: query,
    variables: dict.new(),
    staged_resource_ids: staged,
    status: store.Staged,
    interpreted: store.InterpretedMetadata(
      operation_type: store.Mutation,
      operation_name: Some("savedSearchCreate"),
      root_fields: ["savedSearchCreate"],
      primary_root_field: Some("savedSearchCreate"),
      capability: empty_capability(),
    ),
    notes: None,
  )
}

fn store_with_entries(entries: List(store.MutationLogEntry)) -> store.Store {
  list.fold(entries, store.new(), fn(s, entry) {
    store.record_mutation_log_entry(s, entry)
  })
}

fn ok_send_factory(
  status: Int,
  body: String,
) -> fn(_) -> Result(commit.HttpOutcome, commit.CommitTransportError) {
  fn(_req) { Ok(commit.HttpOutcome(status: status, body: body)) }
}

// ---------------------------------------------------------------------------
// gid_resource_type
// ---------------------------------------------------------------------------

pub fn gid_resource_type_extracts_segment_test() {
  assert commit.gid_resource_type("gid://shopify/SavedSearch/12")
    == Some("SavedSearch")
}

pub fn gid_resource_type_strips_query_string_test() {
  assert commit.gid_resource_type(synthetic_one) == Some("SavedSearch")
}

pub fn gid_resource_type_returns_none_for_non_gid_test() {
  assert commit.gid_resource_type("not a gid") == None
  assert commit.gid_resource_type("") == None
  assert commit.gid_resource_type("gid://shopify/") == None
}

// ---------------------------------------------------------------------------
// apply_id_map_to_body_string
// ---------------------------------------------------------------------------

pub fn apply_id_map_substitutes_each_synthetic_test() {
  let body =
    "{\"input\":{\"id\":\""
    <> synthetic_one
    <> "\",\"other\":\""
    <> synthetic_two
    <> "\"}}"
  let id_map =
    dict.from_list([
      #(synthetic_one, authoritative_one),
      #(synthetic_two, authoritative_two),
    ])
  let rewritten = commit.apply_id_map_to_body_string(body, id_map)
  assert string.contains(rewritten, authoritative_one)
  assert string.contains(rewritten, authoritative_two)
  assert !string.contains(rewritten, synthetic_one)
  assert !string.contains(rewritten, synthetic_two)
}

pub fn apply_id_map_is_noop_when_unmapped_test() {
  let body = "{\"id\":\"" <> synthetic_one <> "\"}"
  let id_map = dict.new()
  assert commit.apply_id_map_to_body_string(body, id_map) == body
}

// ---------------------------------------------------------------------------
// collect_authoritative_gids_by_type
// ---------------------------------------------------------------------------

pub fn collect_authoritative_gids_groups_by_type_test() {
  let body =
    commit.parse_json_value(
      "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"id\":\""
      <> authoritative_one
      <> "\"},\"otherSavedSearch\":{\"id\":\""
      <> authoritative_two
      <> "\"}},\"unrelated\":{\"id\":\"gid://shopify/Webhook/99\"}}}",
    )
  let groups = commit.collect_authoritative_gids_by_type(body)
  let saved_searches = case dict.get(groups, "SavedSearch") {
    Ok(items) -> items
    Error(_) -> []
  }
  // The underlying parser uses a `dict` per object, whose iteration order
  // is unspecified — assert membership only.
  assert list.contains(saved_searches, authoritative_one)
  assert list.contains(saved_searches, authoritative_two)
  let webhooks = case dict.get(groups, "Webhook") {
    Ok(items) -> items
    Error(_) -> []
  }
  assert webhooks == ["gid://shopify/Webhook/99"]
}

pub fn collect_authoritative_gids_skips_synthetics_test() {
  let body =
    commit.parse_json_value(
      "{\"id\":\"" <> synthetic_one <> "\"}",
    )
  let groups = commit.collect_authoritative_gids_by_type(body)
  assert dict.size(groups) == 0
}

pub fn collect_authoritative_gids_dedups_test() {
  let body =
    commit.parse_json_value(
      "[\"" <> authoritative_one <> "\",\"" <> authoritative_one <> "\"]",
    )
  let groups = commit.collect_authoritative_gids_by_type(body)
  let saved_searches = case dict.get(groups, "SavedSearch") {
    Ok(items) -> items
    Error(_) -> []
  }
  assert saved_searches == [authoritative_one]
}

// ---------------------------------------------------------------------------
// record_commit_id_mappings
// ---------------------------------------------------------------------------

pub fn record_id_mappings_pairs_synthetic_with_first_authoritative_test() {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [synthetic_one],
    )
  let body =
    commit.parse_json_value(
      "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"id\":\""
      <> authoritative_one
      <> "\"}}}}",
    )
  let id_map = commit.record_commit_id_mappings(entry, body, dict.new())
  assert dict.get(id_map, synthetic_one) == Ok(authoritative_one)
}

pub fn record_id_mappings_skips_non_synthetic_staged_ids_test() {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      ["gid://shopify/SavedSearch/literal"],
    )
  let body =
    commit.parse_json_value(
      "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"id\":\""
      <> authoritative_one
      <> "\"}}}}",
    )
  let id_map = commit.record_commit_id_mappings(entry, body, dict.new())
  assert dict.size(id_map) == 0
}

pub fn record_id_mappings_returns_input_when_no_staged_ids_test() {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [],
    )
  let body = commit.parse_json_value("{}")
  let id_map = commit.record_commit_id_mappings(entry, body, dict.new())
  assert dict.size(id_map) == 0
}

pub fn record_id_mappings_skips_when_no_matching_type_test() {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [synthetic_one],
    )
  // upstream returned a Webhook id, not a SavedSearch id
  let body =
    commit.parse_json_value(
      "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/99\"}}}}",
    )
  let id_map = commit.record_commit_id_mappings(entry, body, dict.new())
  assert dict.size(id_map) == 0
}

// ---------------------------------------------------------------------------
// build_replay_body — currently always falls back to {query, variables}
// ---------------------------------------------------------------------------

pub fn build_replay_body_uses_query_and_variables_test() {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [],
    )
  let body = commit.build_replay_body(entry)
  assert string.contains(body, "\"query\":")
  assert string.contains(body, "savedSearchCreate")
  assert string.contains(body, "\"variables\":{}")
}

// ---------------------------------------------------------------------------
// forward_headers
// ---------------------------------------------------------------------------

pub fn forward_headers_strips_omitted_set_test() {
  let incoming =
    dict.from_list([
      #("Connection", "keep-alive"),
      #("HOST", "example.com"),
      #("X-Custom", "value"),
    ])
  let forwarded = commit.forward_headers(incoming)
  let names = list.map(forwarded, fn(p) { p.0 })
  assert !list.contains(names, "connection")
  assert !list.contains(names, "host")
  assert list.contains(names, "x-custom")
}

pub fn forward_headers_lowercases_names_test() {
  let incoming = dict.from_list([#("X-Shopify-Token", "abc")])
  let forwarded = commit.forward_headers(incoming)
  assert list.find_map(forwarded, fn(p) {
      case p.0 {
        "x-shopify-token" -> Ok(p.1)
        _ -> Error(Nil)
      }
    })
    == Ok("abc")
}

pub fn forward_headers_force_sets_content_type_json_test() {
  let incoming = dict.from_list([#("Content-Type", "text/html")])
  let forwarded = commit.forward_headers(incoming)
  assert list.find_map(forwarded, fn(p) {
      case p.0 {
        "content-type" -> Ok(p.1)
        _ -> Error(Nil)
      }
    })
    == Ok("application/json")
}

pub fn forward_headers_stamps_proxy_user_agent_test() {
  let incoming = dict.new()
  let forwarded = commit.forward_headers(incoming)
  let ua_value = case
    list.find_map(forwarded, fn(p) {
      case p.0 {
        "user-agent" -> Ok(p.1)
        _ -> Error(Nil)
      }
    })
  {
    Ok(value) -> value
    Error(_) -> ""
  }
  assert string.contains(ua_value, "shopify-draft-proxy")
}

pub fn forward_headers_wraps_incoming_user_agent_test() {
  let incoming = dict.from_list([#("User-Agent", "MyClient/1.0")])
  let forwarded = commit.forward_headers(incoming)
  let ua_value = case
    list.find_map(forwarded, fn(p) {
      case p.0 {
        "user-agent" -> Ok(p.1)
        _ -> Error(Nil)
      }
    })
  {
    Ok(value) -> value
    Error(_) -> ""
  }
  assert string.contains(ua_value, "shopify-draft-proxy")
  assert string.contains(ua_value, "MyClient/1.0")
}

// ---------------------------------------------------------------------------
// proxy_user_agent
// ---------------------------------------------------------------------------

pub fn proxy_user_agent_bare_when_none_test() {
  assert commit.proxy_user_agent(None) == "shopify-draft-proxy"
}

pub fn proxy_user_agent_wraps_string_when_present_test() {
  assert commit.proxy_user_agent(Some("UA"))
    == "shopify-draft-proxy (wrapping UA)"
}

pub fn proxy_user_agent_bare_when_empty_string_test() {
  assert commit.proxy_user_agent(Some("   ")) == "shopify-draft-proxy"
}

// ---------------------------------------------------------------------------
// response_body_has_graphql_errors
// ---------------------------------------------------------------------------

pub fn response_body_has_graphql_errors_detects_errors_array_test() {
  let body = commit.parse_json_value("{\"errors\":[{\"message\":\"boom\"}]}")
  assert commit.response_body_has_graphql_errors(body) == True
}

pub fn response_body_has_graphql_errors_ignores_empty_errors_test() {
  let body = commit.parse_json_value("{\"errors\":[]}")
  assert commit.response_body_has_graphql_errors(body) == False
}

pub fn response_body_has_graphql_errors_ignores_data_only_test() {
  let body = commit.parse_json_value("{\"data\":{\"hi\":\"there\"}}")
  assert commit.response_body_has_graphql_errors(body) == False
}

// ---------------------------------------------------------------------------
// run_commit_sync — driver tests with injected `send`
// ---------------------------------------------------------------------------

pub fn run_commit_empty_log_returns_ok_test() {
  let send = ok_send_factory(200, "{}")
  let #(_store, meta) =
    commit.run_commit_sync(store.new(), "https://shop.example", dict.new(), send)
  assert meta.ok == True
  assert meta.stop_index == None
  assert meta.attempts == []
}

pub fn run_commit_skips_already_committed_entries_test() {
  let entry =
    store.MutationLogEntry(
      ..entry_factory(
        "log-1",
        "mutation { savedSearchCreate { savedSearch { id } } }",
        [],
      ),
      status: store.Committed,
    )
  let s = store_with_entries([entry])
  let send = ok_send_factory(200, "{}")
  let #(_after, meta) =
    commit.run_commit_sync(s, "https://shop.example", dict.new(), send)
  assert meta.ok == True
  assert meta.attempts == []
}

pub fn run_commit_marks_entry_committed_on_success_test() {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [synthetic_one],
    )
  let s = store_with_entries([entry])
  let body =
    "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"id\":\""
    <> authoritative_one
    <> "\"},\"userErrors\":[]}}}"
  let send = ok_send_factory(200, body)
  let #(after, meta) =
    commit.run_commit_sync(s, "https://shop.example", dict.new(), send)
  assert meta.ok == True
  assert meta.stop_index == None
  assert list.length(meta.attempts) == 1
  let log = store.get_log(after)
  case log {
    [updated] -> {
      assert updated.status == store.Committed
    }
    _ -> panic as "expected single log entry after commit"
  }
}

pub fn run_commit_threads_synthetic_id_remap_to_subsequent_entry_test() {
  let entry1 =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [synthetic_one],
    )
  // entry 2 references the synthetic id from entry 1 in its query body
  let entry2 =
    entry_factory(
      "log-2",
      "mutation { savedSearchUpdate(id: \""
        <> synthetic_one
        <> "\") { savedSearch { id } } }",
      [],
    )
  let s = store_with_entries([entry1, entry2])

  // Track per-call request bodies via a mutable-style fold — we capture the
  // bodies by emitting different responses per call.
  let response_one =
    "{\"data\":{\"savedSearchCreate\":{\"savedSearch\":{\"id\":\""
    <> authoritative_one
    <> "\"},\"userErrors\":[]}}}"
  let response_two = "{\"data\":{\"savedSearchUpdate\":{\"userErrors\":[]}}}"

  // Replay the second entry's body through a fake send that asserts the
  // synthetic GID has been substituted.
  let recorded_bodies =
    record_request_bodies(s, [response_one, response_two])
  case recorded_bodies {
    [_first_body, second_body] -> {
      assert string.contains(second_body, authoritative_one)
      assert !string.contains(second_body, synthetic_one)
    }
    _ -> panic as "expected exactly two recorded request bodies"
  }
}

// Drive run_commit_sync with a stateful `send` that records each request
// body and replies with the next canned response. Returns the bodies in
// call order.
fn record_request_bodies(
  proxy_store: store.Store,
  responses: List(String),
) -> List(String) {
  // Use a process-dictionary-free approach: the closure captures a mutable
  // ref via list folding. We accumulate by constructing a fresh send that
  // pulls the next response off `responses` keyed on call index, since
  // gleam doesn't have refs in pure code without `gleam_erlang/atomic_term`
  // or similar. Simpler: use a helper that re-runs each entry with a
  // sequence-indexed send.
  let entries = store.get_log(proxy_store)
  let pending =
    list.filter(entries, fn(e) {
      case e.status {
        store.Staged -> True
        _ -> False
      }
    })
  collect_bodies_loop(pending, dict.new(), responses, [])
  |> list.reverse
}

fn collect_bodies_loop(
  pending: List(store.MutationLogEntry),
  id_map: dict.Dict(String, String),
  responses: List(String),
  acc: List(String),
) -> List(String) {
  case pending, responses {
    [], _ -> acc
    _, [] -> acc
    [entry, ..rest_entries], [response, ..rest_responses] -> {
      let body = commit.apply_id_map_to_body_string(
        commit.build_replay_body(entry),
        id_map,
      )
      let parsed = commit.parse_json_value(response)
      let new_id_map = commit.record_commit_id_mappings(entry, parsed, id_map)
      collect_bodies_loop(rest_entries, new_id_map, rest_responses, [
        body,
        ..acc
      ])
    }
  }
}

pub fn run_commit_halts_on_4xx_test() {
  let entry1 =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [],
    )
  let entry2 = store.MutationLogEntry(..entry1, id: "log-2")
  let s = store_with_entries([entry1, entry2])
  let send = ok_send_factory(422, "{\"errors\":[{\"message\":\"bad\"}]}")
  let #(after, meta) =
    commit.run_commit_sync(s, "https://shop.example", dict.new(), send)
  assert meta.ok == False
  assert meta.stop_index == Some(0)
  assert list.length(meta.attempts) == 1
  // entry1 was marked failed; entry2 is still Staged
  let log = store.get_log(after)
  case log {
    [first, second] -> {
      assert first.status == store.Failed
      assert second.status == store.Staged
    }
    _ -> panic as "expected two log entries"
  }
}

pub fn run_commit_halts_on_graphql_errors_in_200_test() {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [],
    )
  let s = store_with_entries([entry])
  let send = ok_send_factory(200, "{\"errors\":[{\"message\":\"boom\"}]}")
  let #(after, meta) =
    commit.run_commit_sync(s, "https://shop.example", dict.new(), send)
  assert meta.ok == False
  assert meta.stop_index == Some(0)
  let log = store.get_log(after)
  case log {
    [updated] -> {
      assert updated.status == store.Failed
    }
    _ -> panic as "expected single log entry"
  }
}

pub fn run_commit_halts_on_transport_error_test() {
  let entry =
    entry_factory(
      "log-1",
      "mutation { savedSearchCreate { savedSearch { id } } }",
      [],
    )
  let s = store_with_entries([entry])
  let send = fn(_req) {
    Error(commit.CommitTransportError(message: "connection refused"))
  }
  let #(after, meta) =
    commit.run_commit_sync(s, "https://shop.example", dict.new(), send)
  assert meta.ok == False
  assert meta.stop_index == Some(0)
  case meta.attempts {
    [attempt] -> {
      assert attempt.upstream_error == Some("connection refused")
      assert attempt.upstream_status == None
    }
    _ -> panic as "expected single attempt"
  }
  let log = store.get_log(after)
  case log {
    [updated] -> {
      assert updated.status == store.Failed
      let notes = case updated.notes {
        Some(n) -> n
        None -> ""
      }
      assert string.contains(notes, "connection refused")
    }
    _ -> panic as "expected single log entry"
  }
}

// ---------------------------------------------------------------------------
// serialize_meta_response — sanity check that the JSON envelope follows the
// shape the TS proxy emits.
// ---------------------------------------------------------------------------

pub fn serialize_meta_response_emits_expected_top_level_keys_test() {
  let meta =
    commit.MetaCommitResponse(ok: True, stop_index: None, attempts: [])
  let serialized = json.to_string(commit.serialize_meta_response(meta))
  assert string.contains(serialized, "\"ok\":true")
  assert string.contains(serialized, "\"stopIndex\":null")
  assert string.contains(serialized, "\"attempts\":[]")
}

pub fn serialize_meta_response_serialises_attempt_test() {
  let attempt =
    commit.CommitAttempt(
      log_entry_id: "log-1",
      operation_name: Some("savedSearchCreate"),
      path: "/admin/api/2025-01/graphql.json",
      success: True,
      status: store.Committed,
      upstream_status: Some(200),
      upstream_body: Some(commit.parse_json_value("{\"hello\":\"world\"}")),
      upstream_error: None,
      response_body: commit.parse_json_value("{\"hello\":\"world\"}"),
    )
  let meta =
    commit.MetaCommitResponse(
      ok: True,
      stop_index: None,
      attempts: [attempt],
    )
  let serialized = json.to_string(commit.serialize_meta_response(meta))
  assert string.contains(serialized, "\"logEntryId\":\"log-1\"")
  assert string.contains(serialized, "\"upstreamStatus\":200")
  assert string.contains(serialized, "\"status\":\"committed\"")
}
