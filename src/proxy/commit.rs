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
