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
    id_map.iter().fold(raw_body, |body, (synthetic, upstream)| {
        body.replace(synthetic, upstream)
    })
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
