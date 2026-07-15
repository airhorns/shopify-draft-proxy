use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn commit_staged_mutations(
        &mut self,
        commit_request: &Request,
    ) -> Response {
        let transport = Arc::clone(&self.commit_transport);
        let mut committed = 0usize;
        let mut failed = 0usize;
        let mut attempts = Vec::new();
        let mut id_map = BTreeMap::new();

        for index in 0..self.log_entries.len() {
            if self.log_entries[index].get("status") != Some(&json!("staged")) {
                continue;
            }

            let log_id = log_entry_id(&self.log_entries[index]);
            let path = log_entry_path(&self.log_entries[index]);
            let body = replay_body(&self.log_entries[index], &id_map);
            let replay = Request {
                method: "POST".to_string(),
                path: path.clone(),
                headers: commit_request.headers.clone(),
                body,
            };
            let outcome = transport(replay);
            let failed_reason = commit_failure_reason(&outcome, &log_id);

            if let Some(error) = failed_reason {
                failed += 1;
                set_log_status(&mut self.log_entries[index], "failed");
                attempts.push(json!({
                    "index": index,
                    "logId": log_id,
                    "status": "failed",
                    "request": {
                        "method": "POST",
                        "path": path
                    },
                    "response": {
                        "status": outcome.status,
                        "body": outcome.body
                    },
                    "error": error
                }));
                return Response {
                    status: 502,
                    headers: BTreeMap::new(),
                    body: json!({
                        "ok": false,
                        "committed": committed,
                        "failed": failed,
                        "stopIndex": index,
                        "attempts": attempts,
                        "error": error
                    }),
                };
            }

            let mapped_ids = record_authoritative_id_mappings(
                &mut id_map,
                &self.log_entries[index],
                &outcome.body,
            );
            committed += 1;
            set_log_status(&mut self.log_entries[index], "committed");
            attempts.push(json!({
                "index": index,
                "logId": log_id,
                "status": "committed",
                "request": {
                    "method": "POST",
                    "path": path
                },
                "response": {
                    "status": outcome.status,
                    "body": outcome.body
                },
                "mappedIds": mapped_ids
            }));
        }

        ok_json(json!({
            "ok": true,
            "committed": committed,
            "failed": failed,
            "stopIndex": Value::Null,
            "attempts": attempts
        }))
    }
}

fn log_entry_id(entry: &Value) -> String {
    entry
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn log_entry_path(entry: &Value) -> String {
    entry
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("/admin/api/2026-04/graphql.json")
        .to_string()
}

fn replay_body(entry: &Value, id_map: &BTreeMap<String, String>) -> String {
    let raw_body = entry
        .get("rawBody")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            let query = entry
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let variables = entry.get("variables").cloned().unwrap_or_else(|| json!({}));
            json!({ "query": query, "variables": variables }).to_string()
        });
    let marked_rewritten = id_map.iter().fold(raw_body, |body, (synthetic, upstream)| {
        body.replace(synthetic, upstream)
    });
    replay_canonical_aliases(&marked_rewritten, id_map)
}

fn replay_canonical_aliases(body: &str, id_map: &BTreeMap<String, String>) -> String {
    let aliases = canonical_alias_map(id_map);
    if aliases.is_empty() {
        return body.to_string();
    }

    let Ok(mut value) = serde_json::from_str::<Value>(body) else {
        return body.to_string();
    };
    let mut changed = false;
    if let Some(rewritten_query) = value
        .get("query")
        .and_then(Value::as_str)
        .and_then(|query| rewrite_graphql_string_literals(query, &aliases))
    {
        value["query"] = json!(rewritten_query);
        changed = true;
    }
    if let Some(variables) = value.get_mut("variables") {
        rewrite_json_string_values(variables, &aliases, &mut changed);
    }

    if changed {
        value.to_string()
    } else {
        body.to_string()
    }
}

fn canonical_alias_map(id_map: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    id_map
        .iter()
        .filter_map(|(synthetic, upstream)| {
            canonical_synthetic_gid_alias(synthetic).map(|alias| (alias, upstream.clone()))
        })
        .collect()
}

fn canonical_synthetic_gid_alias(synthetic: &str) -> Option<String> {
    if !is_synthetic_gid(synthetic) {
        return None;
    }
    let resource_type = shopify_gid_resource_type(synthetic)?;
    let tail = resource_id_tail(synthetic);
    if tail.is_empty() {
        return None;
    }
    let alias = shopify_gid(resource_type, tail);
    (alias != synthetic).then_some(alias)
}

fn rewrite_json_string_values(
    value: &mut Value,
    aliases: &BTreeMap<String, String>,
    changed: &mut bool,
) {
    match value {
        Value::String(value) => {
            if let Some(replacement) = aliases.get(value.as_str()) {
                *value = replacement.clone();
                *changed = true;
            }
        }
        Value::Array(values) => {
            for value in values {
                rewrite_json_string_values(value, aliases, changed);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                rewrite_json_string_values(value, aliases, changed);
            }
        }
        _ => {}
    }
}

fn rewrite_graphql_string_literals(
    query: &str,
    aliases: &BTreeMap<String, String>,
) -> Option<String> {
    let bytes = query.as_bytes();
    let mut output = String::with_capacity(query.len());
    let mut index = 0usize;
    let mut segment_start = 0usize;
    let mut changed = false;

    while index < bytes.len() {
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }

        if query[index..].starts_with("\"\"\"") {
            let content_start = index + 3;
            if let Some(relative_end) = query[content_start..].find("\"\"\"") {
                let content_end = content_start + relative_end;
                let content = &query[content_start..content_end];
                output.push_str(&query[segment_start..index]);
                output.push_str("\"\"\"");
                if let Some(replacement) = aliases.get(content) {
                    output.push_str(replacement);
                    changed = true;
                } else {
                    output.push_str(content);
                }
                output.push_str("\"\"\"");
                index = content_end + 3;
                segment_start = index;
                continue;
            }
            break;
        }

        let literal_start = index;
        index += 1;
        let content_start = index;
        let mut escaped = false;
        while index < bytes.len() {
            let byte = bytes[index];
            if escaped {
                escaped = false;
                index += 1;
                continue;
            }
            match byte {
                b'\\' => {
                    escaped = true;
                    index += 1;
                }
                b'"' => break,
                _ => index += 1,
            }
        }
        if index >= bytes.len() {
            break;
        }

        let content = &query[content_start..index];
        output.push_str(&query[segment_start..literal_start]);
        output.push('"');
        if let Some(decoded) = decode_graphql_string_literal_content(content) {
            if let Some(replacement) = aliases.get(decoded.as_str()) {
                output.push_str(&escape_graphql_string_literal_content(replacement));
                changed = true;
            } else {
                output.push_str(content);
            }
        } else {
            output.push_str(content);
        }
        output.push('"');
        index += 1;
        segment_start = index;
    }

    output.push_str(&query[segment_start..]);
    changed.then_some(output)
}

fn decode_graphql_string_literal_content(content: &str) -> Option<String> {
    if !content.contains('\\') {
        return Some(content.to_string());
    }
    let mut decoded = String::with_capacity(content.len());
    let mut chars = content.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }
        let escaped = chars.next()?;
        match escaped {
            '"' => decoded.push('"'),
            '\\' => decoded.push('\\'),
            '/' => decoded.push('/'),
            'b' => decoded.push('\u{0008}'),
            'f' => decoded.push('\u{000c}'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            _ => return None,
        }
    }
    Some(decoded)
}

fn escape_graphql_string_literal_content(content: &str) -> String {
    let mut escaped = String::with_capacity(content.len());
    for ch in content.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{000c}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn commit_failure_reason(response: &Response, log_id: &str) -> Option<String> {
    if response.status >= 400 {
        return Some(format!(
            "Upstream commit failed for {log_id} with status {}",
            response.status
        ));
    }
    if has_graphql_errors(&response.body) {
        return Some(format!(
            "Upstream commit failed for {log_id} with GraphQL errors"
        ));
    }
    None
}

fn has_graphql_errors(body: &Value) -> bool {
    match body.get("errors") {
        Some(Value::Array(errors)) => !errors.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

fn record_authoritative_id_mappings(
    id_map: &mut BTreeMap<String, String>,
    entry: &Value,
    response_body: &Value,
) -> Value {
    let mut mapped = serde_json::Map::new();
    for synthetic_id in staged_synthetic_ids(entry) {
        if id_map.contains_key(&synthetic_id) {
            continue;
        }
        let Some(resource_type) = shopify_gid_resource_type(&synthetic_id) else {
            continue;
        };
        let mut authoritative_ids = Vec::new();
        collect_ids_matching(response_body, &mut authoritative_ids, &|id| {
            shopify_gid_resource_type(id) == Some(resource_type) && !is_synthetic_gid(id)
        });
        if let Some(authoritative_id) = authoritative_ids
            .into_iter()
            .find(|candidate| !id_map.values().any(|mapped_id| mapped_id == candidate))
        {
            id_map.insert(synthetic_id.clone(), authoritative_id.clone());
            mapped.insert(synthetic_id, json!(authoritative_id));
        }
    }
    Value::Object(mapped)
}

fn staged_synthetic_ids(entry: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    collect_ids_matching(&entry["stagedResourceIds"], &mut ids, &is_synthetic_gid);
    ids
}

fn collect_ids_matching(value: &Value, ids: &mut Vec<String>, matches_id: &impl Fn(&str) -> bool) {
    match value {
        Value::String(id) if matches_id(id) => ids.push(id.clone()),
        Value::Array(values) => {
            for value in values {
                collect_ids_matching(value, ids, matches_id);
            }
        }
        Value::Object(fields) => {
            for value in fields.values() {
                collect_ids_matching(value, ids, matches_id);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{json, Value};

    use super::*;

    const SYNTHETIC_ONE: &str = "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic";
    const SYNTHETIC_TWO: &str = "gid://shopify/SavedSearch/2?shopify-draft-proxy=synthetic";
    const CANONICAL_ONE: &str = "gid://shopify/SavedSearch/1";
    const CANONICAL_TWO: &str = "gid://shopify/SavedSearch/2";
    const AUTHORITATIVE_ONE: &str = "gid://shopify/SavedSearch/12345";
    const AUTHORITATIVE_TWO: &str = "gid://shopify/SavedSearch/67890";

    #[test]
    fn commit_replay_body_prefers_raw_body_and_rewrites_all_mapped_synthetic_ids() {
        let entry = json!({
            "query": "mutation { ignored }",
            "variables": { "ignored": true },
            "rawBody": json!({
                "query": "mutation UpdateSavedSearches($ids: [ID!]!) { savedSearchUpdate(ids: $ids) { savedSearch { id } } }",
                "variables": { "ids": [SYNTHETIC_ONE, SYNTHETIC_TWO] }
            }).to_string()
        });
        let id_map = BTreeMap::from([
            (SYNTHETIC_ONE.to_string(), AUTHORITATIVE_ONE.to_string()),
            (SYNTHETIC_TWO.to_string(), AUTHORITATIVE_TWO.to_string()),
        ]);

        let body = replay_body(&entry, &id_map);

        assert!(body.contains(AUTHORITATIVE_ONE));
        assert!(body.contains(AUTHORITATIVE_TWO));
        assert!(!body.contains(SYNTHETIC_ONE));
        assert!(!body.contains(SYNTHETIC_TWO));
        assert!(!body.contains("ignored"));
    }

    #[test]
    fn commit_replay_body_rewrites_canonical_aliases_in_variables_and_inline_literals() {
        let query = format!(
            r#"
            mutation {{
              # GraphQL text should keep {CANONICAL_ONE}
              savedSearchUpdate(input: {{ id: "{CANONICAL_ONE}", name: "Updated", query: "status:open" }}) {{
                savedSearch {{ id }}
                userErrors {{ field message }}
              }}
              second: savedSearchUpdate(input: {{ id: "{CANONICAL_TWO}", name: "Updated two", query: "status:closed" }}) {{
                savedSearch {{ id }}
                userErrors {{ field message }}
              }}
            }}
            "#
        );
        let entry = json!({
            "rawBody": json!({
                "query": query,
                "variables": {
                    "id": CANONICAL_ONE,
                    "nested": {
                        "ids": [CANONICAL_TWO, SYNTHETIC_ONE],
                        "text": format!("note mentions {CANONICAL_ONE} but is not an ID value")
                    }
                }
            }).to_string()
        });
        let id_map = BTreeMap::from([
            (SYNTHETIC_ONE.to_string(), AUTHORITATIVE_ONE.to_string()),
            (SYNTHETIC_TWO.to_string(), AUTHORITATIVE_TWO.to_string()),
        ]);

        let body = replay_body(&entry, &id_map);
        let parsed = serde_json::from_str::<Value>(&body).expect("replayed body should be JSON");
        let query = parsed["query"].as_str().expect("query should be preserved");

        assert!(query.contains(&format!(
            "savedSearchUpdate(input: {{ id: \"{AUTHORITATIVE_ONE}\""
        )));
        assert!(query.contains(&format!(
            "second: savedSearchUpdate(input: {{ id: \"{AUTHORITATIVE_TWO}\""
        )));
        assert!(query.contains(&format!("# GraphQL text should keep {CANONICAL_ONE}")));
        assert_eq!(parsed["variables"]["id"], json!(AUTHORITATIVE_ONE));
        assert_eq!(
            parsed["variables"]["nested"]["ids"],
            json!([AUTHORITATIVE_TWO, AUTHORITATIVE_ONE])
        );
        assert_eq!(
            parsed["variables"]["nested"]["text"],
            json!(format!(
                "note mentions {CANONICAL_ONE} but is not an ID value"
            ))
        );
    }

    #[test]
    fn commit_replay_body_preserves_nonmatching_canonical_type_and_tail_values() {
        let wrong_type_same_tail = "gid://shopify/Product/1";
        let same_type_wrong_tail = "gid://shopify/SavedSearch/10";
        let query = format!(
            r#"
            mutation {{
              wrongType: savedSearchUpdate(input: {{ id: "{wrong_type_same_tail}", name: "Wrong type" }}) {{ savedSearch {{ id }} }}
              wrongTail: savedSearchUpdate(input: {{ id: "{same_type_wrong_tail}", name: "Wrong tail" }}) {{ savedSearch {{ id }} }}
              longer: savedSearchUpdate(input: {{ id: "{CANONICAL_ONE}0", name: "Longer tail" }}) {{ savedSearch {{ id }} }}
            }}
            "#
        );
        let entry = json!({
            "rawBody": json!({
                "query": query,
                "variables": {
                    "wrongType": wrong_type_same_tail,
                    "wrongTail": same_type_wrong_tail,
                    "longer": format!("{CANONICAL_ONE}0")
                }
            }).to_string()
        });
        let id_map = BTreeMap::from([(SYNTHETIC_ONE.to_string(), AUTHORITATIVE_ONE.to_string())]);

        let body = replay_body(&entry, &id_map);
        let parsed = serde_json::from_str::<Value>(&body).expect("replayed body should be JSON");
        let query = parsed["query"].as_str().expect("query should be preserved");

        assert!(query.contains(wrong_type_same_tail));
        assert!(query.contains(same_type_wrong_tail));
        assert!(query.contains(&format!("{CANONICAL_ONE}0")));
        assert_eq!(
            parsed["variables"]["wrongType"],
            json!(wrong_type_same_tail)
        );
        assert_eq!(
            parsed["variables"]["wrongTail"],
            json!(same_type_wrong_tail)
        );
        assert_eq!(
            parsed["variables"]["longer"],
            json!(format!("{CANONICAL_ONE}0"))
        );
        assert!(!body.contains(AUTHORITATIVE_ONE));
    }

    #[test]
    fn commit_replay_body_falls_back_to_query_and_variables_for_legacy_log_entries() {
        let entry = json!({
            "query": "mutation LegacyCommit($input: SavedSearchCreateInput!) { savedSearchCreate(input: $input) { savedSearch { id } } }",
            "variables": { "input": { "name": "Open orders", "query": "status:open" } }
        });

        let body = replay_body(&entry, &BTreeMap::new());
        let parsed = serde_json::from_str::<Value>(&body).expect("fallback body should be JSON");

        assert_eq!(parsed["query"], entry["query"]);
        assert_eq!(parsed["variables"], entry["variables"]);
    }

    #[test]
    fn commit_graphql_error_detection_matches_top_level_error_semantics() {
        assert!(has_graphql_errors(
            &json!({ "errors": [{ "message": "boom" }] })
        ));
        assert!(has_graphql_errors(
            &json!({ "errors": { "message": "boom" } })
        ));
        assert!(!has_graphql_errors(&json!({ "errors": [] })));
        assert!(!has_graphql_errors(&json!({ "errors": null })));
        assert!(!has_graphql_errors(&json!({ "data": { "ok": true } })));
    }

    #[test]
    fn commit_authoritative_id_mapping_pairs_multiple_synthetics_with_distinct_ids() {
        let entry = json!({
            "stagedResourceIds": [
                SYNTHETIC_ONE,
                { "nested": [SYNTHETIC_TWO, "gid://shopify/SavedSearch/non-synthetic"] }
            ]
        });
        let response = json!({
            "data": {
                "savedSearchCreate": {
                    "savedSearches": [
                        { "id": AUTHORITATIVE_ONE },
                        { "id": AUTHORITATIVE_ONE },
                        { "id": SYNTHETIC_ONE },
                        { "id": AUTHORITATIVE_TWO }
                    ],
                    "userErrors": []
                }
            }
        });
        let mut id_map = BTreeMap::new();

        let mapped = record_authoritative_id_mappings(&mut id_map, &entry, &response);

        assert_eq!(
            id_map.get(SYNTHETIC_ONE).map(String::as_str),
            Some(AUTHORITATIVE_ONE)
        );
        assert_eq!(
            id_map.get(SYNTHETIC_TWO).map(String::as_str),
            Some(AUTHORITATIVE_TWO)
        );
        assert_eq!(mapped[SYNTHETIC_ONE], json!(AUTHORITATIVE_ONE));
        assert_eq!(mapped[SYNTHETIC_TWO], json!(AUTHORITATIVE_TWO));
        assert_eq!(
            mapped
                .as_object()
                .expect("mapped ids should be an object")
                .len(),
            2
        );
    }

    #[test]
    fn commit_authoritative_id_mapping_skips_non_synthetic_and_wrong_type_ids() {
        let entry = json!({
            "stagedResourceIds": [
                "gid://shopify/SavedSearch/ordinary",
                SYNTHETIC_ONE
            ]
        });
        let response = json!({
            "data": {
                "webhookSubscriptionCreate": {
                    "webhookSubscription": {
                        "id": "gid://shopify/WebhookSubscription/99"
                    }
                }
            }
        });
        let mut id_map = BTreeMap::new();

        let mapped = record_authoritative_id_mappings(&mut id_map, &entry, &response);

        assert!(id_map.is_empty());
        assert_eq!(mapped, json!({}));
    }

    #[test]
    fn commit_authoritative_id_mapping_does_not_overwrite_existing_mappings() {
        let entry = json!({ "stagedResourceIds": [SYNTHETIC_ONE] });
        let response = json!({ "id": AUTHORITATIVE_TWO });
        let mut id_map =
            BTreeMap::from([(SYNTHETIC_ONE.to_string(), AUTHORITATIVE_ONE.to_string())]);

        let mapped = record_authoritative_id_mappings(&mut id_map, &entry, &response);

        assert_eq!(
            id_map.get(SYNTHETIC_ONE).map(String::as_str),
            Some(AUTHORITATIVE_ONE)
        );
        assert_eq!(mapped, json!({}));
    }
}
