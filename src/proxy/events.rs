use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn events_query_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        if self.config.read_mode == ReadMode::LiveHybrid && self.store.staged.events.is_empty() {
            let response = (self.upstream_transport)(request.clone());
            if response.status < 400 {
                self.hydrate_events_from_upstream(&response.body);
            }
            return response;
        }

        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        ok_json(json!({ "data": self.events_query_data(&fields) }))
    }

    pub(in crate::proxy) fn events_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        root_payload_json(fields, |field| match field.name.as_str() {
            "event" => Some(self.event_by_id_field(field)),
            "events" => Some(self.events_connection_field(field)),
            "eventsCount" => Some(self.events_count_field(field)),
            "node" => Some(self.event_node_field(field)),
            "nodes" => Some(self.event_nodes_field(field)),
            _ => None,
        })
    }

    pub(in crate::proxy) fn hydrate_events_from_upstream(&mut self, body: &Value) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        for value in data.values() {
            self.observe_event_values(value);
        }
    }

    pub(in crate::proxy) fn stage_basic_event(
        &mut self,
        action: &str,
        subject_id: &str,
        message: impl Into<String>,
    ) {
        if subject_id.is_empty() {
            return;
        }
        let Some(subject_type) = event_subject_type(subject_id) else {
            return;
        };
        let event = json!({
            "__typename": "BasicEvent",
            "id": self.next_proxy_synthetic_gid("BasicEvent"),
            "action": action,
            "appTitle": "shopify-draft-proxy",
            "attributeToApp": false,
            "attributeToUser": false,
            "createdAt": self.next_mutation_timestamp(),
            "criticalAlert": false,
            "message": message.into(),
            "additionalContent": "null",
            "additionalData": "null",
            "arguments": [],
            "author": "shopify-draft-proxy",
            "hasAdditionalContent": false,
            "secondaryMessage": Value::Null,
            "subjectId": subject_id,
            "subjectType": subject_type,
        });
        self.store.stage_event(event);
    }

    fn observe_event_values(&mut self, value: &Value) {
        if event_id(value).is_some() {
            self.store.observe_event(value.clone());
        }
        if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
            for node in nodes {
                self.observe_event_values(node);
            }
        }
        if let Some(edges) = value.get("edges").and_then(Value::as_array) {
            for edge in edges {
                if let Some(node) = edge.get("node") {
                    self.observe_event_values(node);
                }
            }
        }
    }

    fn event_by_id_field(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            return Value::Null;
        };
        self.store
            .event_by_id(&id)
            .map(|event| selected_json(event, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn events_connection_field(&self, field: &RootFieldSelection) -> Value {
        selected_staged_connection_with_args(
            self.store.events(),
            &field.arguments,
            &field.selection,
            event_search_decision,
            event_staged_sort_key,
            selected_json,
            event_cursor,
        )
    }

    fn events_count_field(&self, field: &RootFieldSelection) -> Value {
        let count = if field.arguments.contains_key("query") {
            staged_connection_query(
                self.store.events(),
                &field.arguments,
                event_search_decision,
                event_staged_sort_key,
                event_cursor,
            )
            .total_count
        } else {
            self.store.event_count()
        };
        selected_json(
            &staged_count_with_limit_precision(count, &field.arguments),
            &field.selection,
        )
    }

    fn event_node_field(&self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.local_node_value_by_id(&id, &field.selection)
            .unwrap_or(Value::Null)
    }

    fn event_nodes_field(&self, field: &RootFieldSelection) -> Value {
        field
            .arguments
            .get("ids")
            .map(resolved_string_list)
            .unwrap_or_default()
            .into_iter()
            .map(|id| {
                self.local_node_value_by_id(&id, &field.selection)
                    .unwrap_or(Value::Null)
            })
            .collect::<Vec<_>>()
            .into()
    }
}

fn event_id(event: &Value) -> Option<&str> {
    let id = event.get("id").and_then(Value::as_str)?;
    matches!(
        shopify_gid_resource_type(id),
        Some("Event" | "BasicEvent" | "CommentEvent")
    )
    .then_some(id)
}

fn event_cursor(event: &Value) -> String {
    event_id(event).unwrap_or_default().to_string()
}

fn event_subject_type(subject_id: &str) -> Option<String> {
    let resource_type = shopify_gid_resource_type(subject_id)?;
    let mut output = String::new();
    for (index, ch) in resource_type.chars().enumerate() {
        if ch.is_ascii_uppercase() && index > 0 {
            output.push('_');
        }
        output.push(ch.to_ascii_uppercase());
    }
    Some(output)
}

fn event_staged_sort_key(event: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let primary = match sort_key.unwrap_or("ID") {
        "CREATED_AT" => event_string_sort_value(event, "createdAt"),
        "RELEVANCE" => event_string_sort_value(event, "message"),
        _ => event_gid_sort_value(event),
    };
    vec![primary, event_gid_sort_value(event)]
}

fn event_gid_sort_value(event: &Value) -> StagedSortValue {
    event_id(event).map_or(StagedSortValue::Null, |id| {
        resource_id_tail(id)
            .parse::<i64>()
            .map(StagedSortValue::I64)
            .unwrap_or_else(|_| StagedSortValue::String(id.to_ascii_lowercase()))
    })
}

fn event_string_sort_value(event: &Value, field: &str) -> StagedSortValue {
    event
        .get(field)
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn event_search_decision(event: &Value, query: Option<&str>) -> StagedSearchDecision {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    for term in search_terms(query) {
        if term.eq_ignore_ascii_case("AND") {
            continue;
        }
        match event_search_term_decision(event, &term) {
            StagedSearchDecision::Match => {}
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }
    StagedSearchDecision::Match
}

fn event_search_term_decision(event: &Value, term: &str) -> StagedSearchDecision {
    if term.is_empty() {
        return StagedSearchDecision::Match;
    }
    let Some((field, value)) = term.split_once(':') else {
        return StagedSearchDecision::from_bool(event_free_text_matches(event, term));
    };
    let value = trim_search_value(value);
    if value.is_empty() {
        return StagedSearchDecision::Unsupported;
    }
    match field {
        "action" => StagedSearchDecision::from_bool(
            event.get("action").and_then(Value::as_str) == Some(value),
        ),
        "comments" => StagedSearchDecision::from_bool(event_comments_match(event, value)),
        "created_at" => StagedSearchDecision::from_bool(event_created_at_matches(event, value)),
        "id" => StagedSearchDecision::from_bool(
            event_id(event).is_some_and(|id| id == value || resource_id_tail(id) == value),
        ),
        "subject_type" => StagedSearchDecision::from_bool(
            event.get("subjectType").and_then(Value::as_str) == Some(&value.to_ascii_uppercase()),
        ),
        _ => StagedSearchDecision::Unsupported,
    }
}

fn event_free_text_matches(event: &Value, term: &str) -> bool {
    let needle = trim_search_value(term)
        .trim_end_matches('*')
        .to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    [
        "id",
        "action",
        "message",
        "secondaryMessage",
        "additionalContent",
        "additionalData",
        "author",
        "appTitle",
        "subjectId",
        "subjectType",
    ]
    .iter()
    .filter_map(|field| event.get(*field).and_then(Value::as_str))
    .any(|value| value.to_ascii_lowercase().contains(&needle))
}

fn event_comments_match(event: &Value, value: &str) -> bool {
    let needle = trim_search_value(value)
        .trim_end_matches('*')
        .to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    ["message", "secondaryMessage", "additionalContent"]
        .iter()
        .filter_map(|field| event.get(*field).and_then(Value::as_str))
        .any(|comment| comment.to_ascii_lowercase().contains(&needle))
}

fn event_created_at_matches(event: &Value, value: &str) -> bool {
    let Some(created_at) = event.get("createdAt").and_then(Value::as_str) else {
        return false;
    };
    let (operator, expected) = split_search_comparison(value);
    if expected.is_empty() {
        return false;
    }
    match operator {
        ">" => created_at > expected,
        ">=" => created_at >= expected,
        "<" => created_at < expected,
        "<=" => created_at <= expected,
        "=" => created_at == expected || created_at.starts_with(expected),
        _ => created_at.starts_with(expected),
    }
}

fn split_search_comparison(value: &str) -> (&str, &str) {
    let value = trim_search_value(value);
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, trim_search_value(rest));
        }
    }
    ("", value)
}

fn search_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in query.chars() {
        match ch {
            '"' | '\'' if quote == Some(ch) => {
                quote = None;
                current.push(ch);
            }
            '"' | '\'' if quote.is_none() => {
                quote = Some(ch);
                current.push(ch);
            }
            ch if ch.is_whitespace() && quote.is_none() => {
                if !current.trim().is_empty() {
                    terms.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        terms.push(current.trim().to_string());
    }
    terms
}

fn trim_search_value(value: &str) -> &str {
    value.trim().trim_matches('"').trim_matches('\'')
}
