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
        let Some(resource_type) = gid_resource_type(&synthetic_id) else {
            continue;
        };
        let mut authoritative_ids = Vec::new();
        collect_authoritative_ids(response_body, resource_type, &mut authoritative_ids);
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
    collect_synthetic_ids(&entry["stagedResourceIds"], &mut ids);
    ids
}

fn collect_synthetic_ids(value: &Value, ids: &mut Vec<String>) {
    match value {
        Value::String(id) if is_synthetic_gid(id) => ids.push(id.clone()),
        Value::Array(values) => {
            for value in values {
                collect_synthetic_ids(value, ids);
            }
        }
        Value::Object(fields) => {
            for value in fields.values() {
                collect_synthetic_ids(value, ids);
            }
        }
        _ => {}
    }
}

fn collect_authoritative_ids(value: &Value, resource_type: &str, ids: &mut Vec<String>) {
    match value {
        Value::String(id)
            if gid_resource_type(id) == Some(resource_type) && !is_synthetic_gid(id) =>
        {
            ids.push(id.clone());
        }
        Value::Array(values) => {
            for value in values {
                collect_authoritative_ids(value, resource_type, ids);
            }
        }
        Value::Object(fields) => {
            for value in fields.values() {
                collect_authoritative_ids(value, resource_type, ids);
            }
        }
        _ => {}
    }
}

fn is_synthetic_gid(id: &str) -> bool {
    id.starts_with("gid://shopify/") && id.contains("shopify-draft-proxy=synthetic")
}

fn gid_resource_type(id: &str) -> Option<&str> {
    let rest = id.strip_prefix("gid://shopify/")?;
    rest.split(['/', '?'])
        .next()
        .filter(|part| !part.is_empty())
}
