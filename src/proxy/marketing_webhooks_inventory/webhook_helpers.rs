use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn resolve_webhooks_graphql(
        &mut self,
        context: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            root_name: _,
            mode,
            ..
        } = context;
        match mode {
            LocalResolverMode::OverlayRead => {
                let Some(document) = parsed_document(query, variables) else {
                    return json_error(400, "Could not parse GraphQL operation");
                };
                let fields = match self.root_fields_or_error(query, variables) {
                    Ok(fields) => fields,
                    Err(response) => return response,
                };
                if let Some(error) = webhook_subscription_sort_key_validation_error(&document) {
                    ok_json(json!({ "errors": [error] }))
                } else {
                    ok_json(json!({
                        "data": self.webhook_subscriptions_query_data(&fields)
                    }))
                }
            }
            LocalResolverMode::StageLocally => self.webhook_mutation(request, query, variables),
        }
    }
}

pub(in crate::proxy) fn webhook_subscription_callback_url(uri: &str) -> Option<&str> {
    if uri.starts_with("arn:aws:events:") || uri.starts_with("pubsub://") {
        // The captured schema keeps this deprecated field non-null even though
        // Shopify omits it from cloud-delivery responses. Keep a private
        // executor placeholder; the GraphQL response adapter removes it after
        // non-null propagation has completed. `uri` and `endpoint` carry the
        // actual EventBridge/PubSub destination.
        Some("https://eventbridge.arn")
    } else {
        Some(uri)
    }
}

pub(in crate::proxy) fn webhook_endpoint(uri: &str) -> Value {
    if uri.starts_with("arn:aws:events:") {
        json!({ "__typename": "WebhookEventBridgeEndpoint", "arn": uri })
    } else if let Some(tail) = uri.strip_prefix("pubsub://") {
        let (project, topic) = tail.split_once(':').unwrap_or((tail, ""));
        json!({ "__typename": "WebhookPubSubEndpoint", "pubSubProject": project, "pubSubTopic": topic })
    } else {
        json!({ "__typename": "WebhookHttpEndpoint", "callbackUrl": uri })
    }
}

pub(in crate::proxy) fn webhook_subscription_string_field(record: &Value, field: &str) -> String {
    record[field]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub(in crate::proxy) fn valid_gcp_project_id(project: &str) -> bool {
    if project.chars().all(|ch| ch.is_ascii_digit()) {
        return !project.is_empty();
    }

    let len = project.len();
    (6..=30).contains(&len)
        && project
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        && project
            .chars()
            .last()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && project
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

pub(in crate::proxy) fn valid_gcp_pubsub_topic_id(topic: &str) -> bool {
    let Some(decoded_topic) = percent_decode_ascii_topic(topic) else {
        return false;
    };

    let len = decoded_topic.len();
    (3..=255).contains(&len)
        && !decoded_topic.starts_with("goog")
        && decoded_topic
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        && decoded_topic
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | '%'))
}

fn percent_decode_ascii_topic(topic: &str) -> Option<String> {
    let bytes = topic.as_bytes();
    let mut decoded = String::with_capacity(topic.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex_value)?;
            let low = bytes.get(index + 2).copied().and_then(hex_value)?;
            let byte = high * 16 + low;
            if !byte.is_ascii() {
                return None;
            }
            decoded.push(char::from(byte));
            index += 3;
        } else {
            decoded.push(char::from(bytes[index]));
            index += 1;
        }
    }
    Some(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(in crate::proxy) fn eventbridge_arn_api_client_id(uri: &str) -> Option<&str> {
    let parts: Vec<&str> = uri.splitn(6, ':').collect();
    if parts.len() != 6
        || parts[0] != "arn"
        || parts[1] != "aws"
        || parts[2] != "events"
        || !valid_eventbridge_region(parts[3])
        || !parts[4].is_empty()
    {
        return None;
    }
    let resource = parts[5];
    let tail = resource
        .strip_prefix("event-source/aws.partner/shopify.com/")
        .or_else(|| resource.strip_prefix("event-source/aws.partner/shopify.com.test/"))?;
    let (api_client_id, event_source_name) = tail.split_once('/')?;
    if api_client_id.is_empty()
        || !api_client_id.chars().all(|ch| ch.is_ascii_digit())
        || event_source_name.is_empty()
    {
        return None;
    }
    Some(api_client_id)
}

fn valid_eventbridge_region(region: &str) -> bool {
    let mut parts = region.split('-');
    let Some(prefix) = parts.next() else {
        return false;
    };
    let Some(name) = parts.next() else {
        return false;
    };
    let Some(number) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && prefix.len() == 2
        && prefix.chars().all(|ch| ch.is_ascii_lowercase())
        && !name.is_empty()
        && name.chars().all(|ch| ch.is_ascii_lowercase())
        && !number.is_empty()
        && number.chars().all(|ch| ch.is_ascii_digit())
}

pub(in crate::proxy) fn webhook_uri_uses_disallowed_host(uri: &str) -> bool {
    let Some(host) = webhook_uri_host(uri) else {
        return false;
    };
    if host == "shopify.com"
        || host.ends_with(".shopify.com")
        || host.ends_with(".myshopify.com")
        || host.ends_with(".shopifypreview.com")
        || host.ends_with(".myshopify.dev")
        || host == "localhost"
    {
        return true;
    }
    if let Ok(std::net::IpAddr::V4(address)) = host.parse::<std::net::IpAddr>() {
        let octets = address.octets();
        return octets[0] == 0
            || octets[0] == 10
            || octets[0] == 127
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            || (octets[0] == 192 && octets[1] == 168);
    }
    false
}

pub(in crate::proxy) fn webhook_uri_host(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"))?;
    let host_with_port = rest.split('/').next().unwrap_or_default();
    Some(
        host_with_port
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn webhook_uri_unsupported_protocol(uri: &str) -> Option<&str> {
    if uri.trim().is_empty()
        || uri.starts_with("https://")
        || uri.starts_with("http://")
        || uri.starts_with("kafka://")
        || uri.starts_with("pubsub://")
        || uri.starts_with("arn:aws:events:")
    {
        return None;
    }

    let (scheme, _) = uri.split_once("://")?;
    (!scheme.is_empty()
        && scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')))
    .then_some(scheme)
}

fn webhook_https_uri_is_invalid(uri: &str) -> bool {
    if !uri.starts_with("https://") {
        return false;
    }

    url::Url::parse(uri)
        .ok()
        .filter(|parsed| parsed.scheme() == "https")
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .is_none_or(|host| host.is_empty())
}

pub(in crate::proxy) fn webhook_subscription_legacy_id(id: &str) -> String {
    resource_id_tail(id).to_string()
}

pub(in crate::proxy) fn webhook_subscription_numeric_id(record: &Value) -> u64 {
    record["id"]
        .as_str()
        .map(webhook_subscription_legacy_id)
        .and_then(|tail| tail.parse::<u64>().ok())
        .unwrap_or(0)
}

fn webhook_subscription_gid_tail_sort_value(record: &Value) -> StagedSortValue {
    resource_id_tail_sort_value(record.get("id").and_then(Value::as_str))
}

fn webhook_subscription_staged_sort_key(record: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let primary = match sort_key.unwrap_or("CREATED_AT") {
        "ID" => webhook_subscription_gid_tail_sort_value(record),
        // Shopify documents CREATED_AT as the default. Out-of-range values that
        // reach this adapter fall back to the default rather than gaining a
        // hidden local-only ordering.
        _ => StagedSortValue::String(
            record
                .get("createdAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
    };
    vec![primary, webhook_subscription_gid_tail_sort_value(record)]
}

pub(in crate::proxy) fn webhook_subscription_sort_key_validation_error(
    document: &ParsedDocument,
) -> Option<Value> {
    document.root_fields.iter().find_map(|field| {
        if field.name != "webhookSubscriptions" {
            return None;
        }
        let raw_sort_key = field.raw_arguments.get("sortKey")?;
        match raw_sort_key {
            RawArgumentValue::Enum(sort_key) | RawArgumentValue::String(sort_key)
                if !webhook_subscription_sort_key_is_valid(sort_key) =>
            {
                Some(json!({
                    "message": format!("Argument 'sortKey' on Field 'webhookSubscriptions' has an invalid value ({}). Expected type 'WebhookSubscriptionSortKeys'.", sort_key),
                    "locations": [{ "line": field.location.line, "column": field.location.column }],
                    "path": [document.operation_path.clone(), field.name.clone(), "sortKey"],
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "Field",
                        "argumentName": "sortKey"
                    }
                }))
            }
            RawArgumentValue::Variable {
                name,
                value: Some(ResolvedValue::String(sort_key)),
            } if !webhook_subscription_sort_key_is_valid(sort_key) => {
                let location = document
                    .variable_definitions
                    .get(name)
                    .map_or(field.location, |definition| definition.location);
                Some(json!({
                    "message": format!("Variable ${} of type WebhookSubscriptionSortKeys was provided invalid value", name),
                    "locations": [{ "line": location.line, "column": location.column }],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": sort_key,
                        "problems": [{
                            "path": [],
                            "explanation": format!("Expected \"{}\" to be one of: CREATED_AT, ID", sort_key)
                        }]
                    }
                }))
            }
            _ => None,
        }
    })
}

fn webhook_subscription_sort_key_is_valid(value: &str) -> bool {
    matches!(value, "CREATED_AT" | "ID")
}

pub(in crate::proxy) fn webhook_subscription_matches_field_args(
    record: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if let Some(format) = resolved_string_field(arguments, "format") {
        if !record["format"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(&format))
        {
            return false;
        }
    }

    if let Some(uri) = resolved_string_field(arguments, "uri") {
        if record["uri"].as_str() != Some(uri.as_str())
            && record["callbackUrl"].as_str() != Some(uri.as_str())
        {
            return false;
        }
    }

    if let Some(callback_url) = resolved_string_field(arguments, "callbackUrl") {
        if record["uri"].as_str() != Some(callback_url.as_str())
            && record["callbackUrl"].as_str() != Some(callback_url.as_str())
        {
            return false;
        }
    }

    let topics = resolved_string_list_arg(arguments, "topics");
    if !topics.is_empty()
        && !record["topic"].as_str().is_some_and(|topic| {
            topics
                .iter()
                .any(|wanted| topic.eq_ignore_ascii_case(wanted))
        })
    {
        return false;
    }

    true
}

pub(in crate::proxy) fn webhook_subscription_search_decision(
    record: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    for raw_token in query.split_whitespace() {
        let token = raw_token.trim();
        if token.is_empty() || token.eq_ignore_ascii_case("AND") || token.eq_ignore_ascii_case("OR")
        {
            continue;
        }
        let (negated, token) = token
            .strip_prefix('-')
            .map_or((false, token), |tail| (true, tail));
        let Some((field, value)) = token.split_once(':') else {
            continue;
        };
        let Some(matches) = webhook_subscription_matches_query_term(record, field, value) else {
            return StagedSearchDecision::Unsupported;
        };
        if matches == negated {
            return StagedSearchDecision::NoMatch;
        }
    }
    StagedSearchDecision::Match
}

pub(in crate::proxy) fn webhook_subscription_matches_query_term(
    record: &Value,
    field: &str,
    value: &str,
) -> Option<bool> {
    let wanted = value.to_ascii_lowercase();
    Some(match field.to_ascii_lowercase().as_str() {
        "id" => webhook_subscription_matches_id_query(record, value),
        "topic" => webhook_subscription_string_field(record, "topic").contains(&wanted),
        "format" => webhook_subscription_string_field(record, "format") == wanted,
        "uri" | "callbackurl" | "callback_url" => {
            webhook_subscription_string_field(record, "uri").contains(&wanted)
                || webhook_subscription_string_field(record, "callbackUrl").contains(&wanted)
        }
        "created_at" => webhook_subscription_matches_datetime_comparator(
            record.get("createdAt").and_then(Value::as_str),
            value,
        ),
        "updated_at" => webhook_subscription_matches_datetime_comparator(
            record.get("updatedAt").and_then(Value::as_str),
            value,
        ),
        _ => return None,
    })
}

fn webhook_subscription_matches_id_query(record: &Value, query_value: &str) -> bool {
    let query_value = query_value.trim_matches('"').trim_matches('\'');
    let (operator, expected) = search_comparator(query_value);
    if expected.is_empty() {
        return false;
    }
    if operator == "="
        && record["id"].as_str().is_some_and(|id| {
            id.eq_ignore_ascii_case(expected)
                || webhook_subscription_legacy_id(id).eq_ignore_ascii_case(expected)
        })
    {
        return true;
    }
    let Some(expected) = expected.parse::<u64>().ok() else {
        return false;
    };
    let actual = webhook_subscription_numeric_id(record);
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual == expected,
    }
}

fn webhook_subscription_matches_datetime_comparator(
    actual: Option<&str>,
    query_value: &str,
) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let query_value = query_value.trim_matches('"').trim_matches('\'');
    if query_value.is_empty() {
        return false;
    }
    let (operator, expected) = search_comparator(query_value);
    if expected.is_empty() {
        return false;
    }
    let actual = search_datetime_value(actual, expected);
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual.starts_with(expected),
    }
}

const WEBHOOK_FILTER_MAX_BYTE_SIZE: usize = 65_535;

impl DraftProxy {
    pub(in crate::proxy) fn webhook_subscriptions_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "webhookSubscription" => field
                    .arguments
                    .get("id")
                    .and_then(resolved_value_string)
                    .and_then(|id| self.store.staged.webhook_subscriptions.get(&id))
                    .map(|record| selected_json(record, &field.selection))
                    .unwrap_or(Value::Null),
                "webhookSubscriptions" => self.webhook_subscription_connection_field(field),
                "webhookSubscriptionsCount" => {
                    let records = self.webhook_subscription_records_for_filter_args(field);
                    let result = staged_connection_query(
                        records,
                        &field.arguments,
                        webhook_subscription_search_decision,
                        webhook_subscription_staged_sort_key,
                        value_id_cursor,
                    );
                    selected_json(
                        &snapshot_count_with_limit_precision(result.total_count, &field.arguments),
                        &field.selection,
                    )
                }
                _ => Value::Null,
            })
        })
    }

    pub(in crate::proxy) fn webhook_subscription_connection_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let records = self.webhook_subscription_records_for_filter_args(field);
        selected_staged_connection_with_args(
            records,
            &field.arguments,
            &field.selection,
            webhook_subscription_search_decision,
            webhook_subscription_staged_sort_key,
            selected_json,
            value_id_cursor,
        )
    }

    pub(in crate::proxy) fn webhook_subscription_records_for_filter_args(
        &self,
        field: &RootFieldSelection,
    ) -> Vec<Value> {
        self.store
            .staged
            .webhook_subscriptions
            .values()
            .filter(|record| webhook_subscription_matches_field_args(record, &field.arguments))
            .cloned()
            .collect()
    }

    /// Dispatch a webhook subscription mutation document. Iterates over every
    /// root field so aliased multi-mutation documents (e.g. several
    /// `webhookSubscriptionCreate` aliases in one request) all resolve, keyed by
    /// their response alias. Schema-level errors (invalid topic literal, missing
    /// required pub/sub fields) abort the whole operation with top-level errors,
    /// matching GraphQL execution semantics.
    pub(in crate::proxy) fn webhook_mutation(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let Some(fields) = self.execution_root_fields(query, variables) else {
            return json_error(400, "Operation has no root field");
        };
        let mut early_response = None;
        let data = root_payload_json(&fields, |field| {
            if early_response.is_some() {
                return None;
            }
            let required_errors = webhook_required_argument_errors(field, &document);
            if !required_errors.is_empty() {
                early_response = Some(ok_json(json!({ "errors": required_errors })));
                return None;
            }
            if let Some(error) = webhook_subscription_topic_coercion_error(field, Some(&document)) {
                early_response = Some(ok_json(json!({ "errors": [error] })));
                return None;
            }
            if let Some(error) =
                dedicated_pubsub_required_field_error(&field.name, field, &document)
            {
                early_response = Some(ok_json(json!({ "errors": [error] })));
                return None;
            }
            let payload = match field.name.as_str() {
                "webhookSubscriptionCreate"
                | "pubSubWebhookSubscriptionCreate"
                | "eventBridgeWebhookSubscriptionCreate" => {
                    self.webhook_subscription_create_field(field, request, query, variables)
                }
                "webhookSubscriptionUpdate"
                | "pubSubWebhookSubscriptionUpdate"
                | "eventBridgeWebhookSubscriptionUpdate" => {
                    self.webhook_subscription_update_field(field, request, query, variables)
                }
                "webhookSubscriptionDelete" => {
                    self.webhook_subscription_delete_field(field, request, query, variables)
                }
                other => {
                    early_response = Some(json_error(
                        501,
                        &format!("No Rust webhooks dispatcher implemented for root field: {other}"),
                    ));
                    return None;
                }
            };
            Some(payload)
        });
        if let Some(response) = early_response {
            return response;
        }
        ok_json(json!({ "data": data }))
    }

    fn webhook_subscription_create_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = self.next_proxy_synthetic_gid("WebhookSubscription");
        let api_client_id = request_header(request, API_CLIENT_ID_HEADER);
        let api_version = webhook_subscription_effective_api_version(request);
        let request_api_version = admin_graphql_version(&request.path);
        let record = self.webhook_subscription_record(
            &id,
            &field.arguments,
            None,
            api_client_id.as_deref(),
            api_version.as_deref(),
            request_api_version,
        );
        let errors =
            self.webhook_subscription_validation_errors(&field.name, &id, &record, request);
        if !errors.is_empty() {
            return self.webhook_subscription_payload(Value::Null, field.selection.clone(), errors);
        }
        self.store
            .staged
            .webhook_subscriptions
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, &field.name, vec![id]);
        self.webhook_subscription_payload(record, field.selection.clone(), Vec::new())
    }

    fn webhook_subscription_update_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.webhook_subscriptions.get(&id).cloned() else {
            return self.webhook_subscription_payload(
                Value::Null,
                field.selection.clone(),
                vec![user_error_omit_code(
                    ["id"],
                    "Webhook subscription does not exist",
                    None,
                )],
            );
        };
        let api_client_id = request_header(request, API_CLIENT_ID_HEADER);
        let api_version = webhook_subscription_effective_api_version(request);
        let request_api_version = admin_graphql_version(&request.path);
        let record = self.webhook_subscription_record(
            &id,
            &field.arguments,
            Some(existing),
            api_client_id.as_deref(),
            api_version.as_deref(),
            request_api_version,
        );
        let errors =
            self.webhook_subscription_validation_errors(&field.name, &id, &record, request);
        if !errors.is_empty() {
            return self.webhook_subscription_payload(Value::Null, field.selection.clone(), errors);
        }
        self.store
            .staged
            .webhook_subscriptions
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, &field.name, vec![id]);
        self.webhook_subscription_payload(record, field.selection.clone(), Vec::new())
    }

    fn webhook_subscription_delete_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let deleted_id = if self
            .store
            .staged
            .webhook_subscriptions
            .remove(&id)
            .is_some()
        {
            json!(id.clone())
        } else {
            Value::Null
        };
        if deleted_id != Value::Null {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "webhookSubscriptionDelete",
                vec![id],
            );
        }
        let payload = json!({
            "deletedWebhookSubscriptionId": deleted_id,
            "userErrors": if deleted_id == Value::Null {
                json!([user_error_omit_code(["id"], "Webhook subscription does not exist", None)])
            } else {
                json!([])
            }
        });
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn webhook_subscription_payload(
        &self,
        record: Value,
        payload_selection: Vec<SelectedField>,
        user_errors: Vec<Value>,
    ) -> Value {
        let subscription_selection =
            selected_child_selection(&payload_selection, "webhookSubscription").unwrap_or_default();
        let payload = json!({
            "webhookSubscription": if record == Value::Null {
                Value::Null
            } else {
                selected_json(&record, &subscription_selection)
            },
            "userErrors": user_errors
        });
        selected_json(&payload, &payload_selection)
    }

    pub(in crate::proxy) fn webhook_subscription_validation_errors(
        &self,
        root_field: &str,
        id: &str,
        record: &Value,
        request: &Request,
    ) -> Vec<Value> {
        let uri = record["uri"]
            .as_str()
            .or_else(|| record["callbackUrl"].as_str())
            .unwrap_or_default();
        let mut errors =
            Self::webhook_subscription_address_validation_errors(root_field, uri, request);
        errors.extend(self.webhook_subscription_record_validation_errors(id, record, uri));
        errors
    }

    fn webhook_subscription_address_validation_errors(
        root_field: &str,
        uri: &str,
        request: &Request,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        if uri.trim().is_empty() {
            errors.push(callback_error("Address can't be blank"));
        }
        if uri.starts_with("http://") {
            errors.push(callback_error("Address protocol http:// is not supported"));
        }
        if uri.starts_with("kafka://") {
            errors.push(callback_error("Address protocol kafka:// is not supported"));
            errors.push(callback_error("Address is not a valid kafka topic"));
        }
        let invalid_http_address = webhook_https_uri_is_invalid(uri)
            || (!uri.trim().is_empty()
                && !uri.starts_with("http://")
                && !uri.starts_with("kafka://")
                && !uri.starts_with("pubsub://")
                && !uri.starts_with("arn:aws:events:")
                && !uri.starts_with("https://"));
        if let Some(protocol) = webhook_uri_unsupported_protocol(uri) {
            let message = format!("Address protocol {protocol}:// is not supported");
            errors.push(webhook_address_error(root_field, &message));
        } else if invalid_http_address {
            errors.push(webhook_address_error(root_field, "Address is invalid"));
        }
        if uri.len() > 65_535 {
            errors.push(callback_error("Address is too big (maximum is 64 KB)"));
        }
        if webhook_uri_uses_disallowed_host(uri) {
            errors.push(callback_error(
                "Address cannot be a Shopify or an internal domain",
            ));
        }
        if let Some(pubsub_tail) = uri.strip_prefix("pubsub://") {
            let pubsub_parts = pubsub_tail.split_once(':');
            let (project, topic) = pubsub_parts.unwrap_or((pubsub_tail, ""));
            if pubsub_parts.is_none() || project.is_empty() || topic.is_empty() {
                errors.push(callback_error(
                    "Address protocol pubsub:// is not supported",
                ));
                errors.push(callback_error("Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic"));
            } else if root_field.starts_with("pubSubWebhookSubscription") {
                if !valid_gcp_project_id(project) {
                    errors.push(webhook_error(
                        ["webhookSubscription", "pubSubProject"],
                        "Google Cloud Pub/Sub project ID is not valid",
                    ));
                }
                if !valid_gcp_pubsub_topic_id(topic) {
                    errors.push(webhook_error(
                        ["webhookSubscription", "pubSubTopic"],
                        "Google Cloud Pub/Sub topic ID is not valid",
                    ));
                }
            } else if !valid_gcp_project_id(project) {
                errors.push(callback_error("Address is invalid"));
                errors.push(callback_error("Address is not a valid GCP project id."));
            } else if !valid_gcp_pubsub_topic_id(topic) {
                errors.push(callback_error("Address is invalid"));
                errors.push(callback_error("Address is not a valid GCP topic id."));
            }
        }
        if uri.starts_with("arn:aws:events:") {
            if let Some(arn_api_client_id) = eventbridge_arn_api_client_id(uri) {
                if let Some(caller_api_client_id) = request.headers.get(API_CLIENT_ID_HEADER) {
                    if arn_api_client_id != caller_api_client_id {
                        errors.push(webhook_address_error(root_field, "Address is invalid"));
                        let message = format!(
                            "Address is an AWS ARN and includes api_client_id '{}' instead of '{}'",
                            arn_api_client_id, caller_api_client_id
                        );
                        errors.push(webhook_address_error(root_field, &message));
                    }
                }
            } else {
                errors.push(webhook_address_error(root_field, "Address is invalid"));
                errors.push(webhook_address_error(
                    root_field,
                    "Address is not a valid AWS ARN",
                ));
            }
        }
        errors
    }

    fn webhook_subscription_record_validation_errors(
        &self,
        id: &str,
        record: &Value,
        uri: &str,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let topic = record["topic"].as_str().unwrap_or_default();
        let format = record["format"].as_str().unwrap_or_default();
        if (uri.starts_with("pubsub://") || uri.starts_with("arn:aws:events:"))
            && !format.eq_ignore_ascii_case("JSON")
        {
            errors.push(webhook_error(
                ["webhookSubscription", "format"],
                "Format can only be used with format: 'json'",
            ));
        } else if topic == "RETURNS_APPROVE" && format.eq_ignore_ascii_case("XML") {
            errors.push(webhook_error(
                ["webhookSubscription", "format"],
                "Format 'xml' is invalid for this webhook topic. Allowed formats: json",
            ));
        }
        if self
            .store
            .staged
            .webhook_subscriptions
            .iter()
            .any(|(existing_id, existing)| {
                existing_id != id
                    && existing["topic"].as_str() == Some(topic)
                    && existing["uri"]
                        .as_str()
                        .or_else(|| existing["callbackUrl"].as_str())
                        == Some(uri)
                    && existing["format"].as_str() == Some(format)
                    && webhook_subscription_optional_string_key(existing, "filter")
                        == webhook_subscription_optional_string_key(record, "filter")
                    && webhook_subscription_optional_string_key(existing, "apiPermissionId")
                        == webhook_subscription_optional_string_key(record, "apiPermissionId")
            })
        {
            errors.push(callback_error(
                "Address for this topic has already been taken",
            ));
        }
        if let Some(name) = record["name"].as_str() {
            if name.is_empty() {
                errors.push(webhook_error(
                    ["webhookSubscription", "name"],
                    "Name is too short (minimum is 1 character)",
                ));
            }
            if name.is_empty() || !token_chars_valid(name) {
                errors.push(webhook_error(["webhookSubscription", "name"], "Name name field can only contain alphanumeric characters, underscores, and hyphens"));
            }
            if name.chars().count() > 50 {
                errors.push(length_user_error(
                    ["webhookSubscription", "name"],
                    "Name",
                    LengthUserErrorBound::TooLong { maximum: 50 },
                ));
            }
            if self
                .store
                .staged
                .webhook_subscriptions
                .iter()
                .any(|(existing_id, existing)| {
                    existing_id != id
                        && existing["name"]
                            .as_str()
                            .is_some_and(|existing_name| existing_name.eq_ignore_ascii_case(name))
                })
            {
                errors.push(webhook_error(
                    ["webhookSubscription", "name"],
                    "Name already exists, no duplicate allowed",
                ));
            }
        }
        if let Some(filter) = record["filter"].as_str() {
            if webhook_filter_exceeds_byte_size_limit(filter) {
                errors.push(webhook_error(
                    ["webhookSubscription"],
                    "The specified filter exceeds the maximum allowed size.",
                ));
            } else if webhook_filter_is_invalid(filter) {
                errors.push(webhook_error(
                    ["webhookSubscription"],
                    "The specified filter is invalid, please ensure you specify the field(s) you wish to filter on.",
                ));
            }
        }
        errors
    }

    pub(in crate::proxy) fn webhook_subscription_record(
        &self,
        id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        existing: Option<Value>,
        api_client_id: Option<&str>,
        api_version_handle: Option<&str>,
        request_api_version: Option<&str>,
    ) -> Value {
        let webhook_input =
            resolved_object_field(arguments, "webhookSubscription").unwrap_or_default();
        let topic = resolved_string_field(arguments, "topic")
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["topic"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "ORDERS_CREATE".to_string());
        let dedicated_pubsub_uri = resolved_string_field(&webhook_input, "pubSubProject")
            .zip(resolved_string_field(&webhook_input, "pubSubTopic"))
            .map(|(project, topic)| format!("pubsub://{}:{}", project.trim(), topic.trim()));
        let uri = resolved_string_field(&webhook_input, "uri")
            .or_else(|| resolved_string_field(&webhook_input, "callbackUrl"))
            .or(dedicated_pubsub_uri)
            .or_else(|| resolved_string_field(&webhook_input, "arn"))
            .or_else(|| {
                existing.as_ref().and_then(|record| {
                    record["uri"]
                        .as_str()
                        .or_else(|| record["callbackUrl"].as_str())
                        .map(ToString::to_string)
                })
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        let callback_url = webhook_subscription_callback_url(&uri);
        let format = resolved_string_field(&webhook_input, "format")
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["format"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "JSON".to_string());
        let api_permission_id =
            resolved_string_field(&webhook_input, "apiPermissionId").or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["apiPermissionId"].as_str().map(ToString::to_string))
            });
        let name = resolved_string_field(&webhook_input, "name").or_else(|| {
            existing
                .as_ref()
                .and_then(|record| record["name"].as_str().map(ToString::to_string))
        });
        let include_fields = if webhook_input.contains_key("includeFields") {
            json!(list_string_field(&webhook_input, "includeFields"))
        } else {
            existing
                .as_ref()
                .map(|record| record["includeFields"].clone())
                .filter(Value::is_array)
                .unwrap_or_else(|| json!([]))
        };
        let metafield_namespaces = if webhook_input.contains_key("metafieldNamespaces") {
            json!(list_string_field(&webhook_input, "metafieldNamespaces")
                .into_iter()
                .map(|namespace| resolve_webhook_metafield_namespace(&namespace, api_client_id))
                .collect::<Vec<_>>())
        } else {
            existing
                .as_ref()
                .map(|record| record["metafieldNamespaces"].clone())
                .filter(Value::is_array)
                .unwrap_or_else(|| json!([]))
        };
        let metafields = if webhook_input.contains_key("metafields") {
            json!(resolved_object_list_field(&webhook_input, "metafields")
                .into_iter()
                .filter_map(|identifier| {
                    Some(json!({
                        "namespace": resolved_string_field(&identifier, "namespace")?,
                        "key": resolved_string_field(&identifier, "key")?
                    }))
                })
                .collect::<Vec<Value>>())
        } else {
            existing
                .as_ref()
                .map(|record| record["metafields"].clone())
                .filter(Value::is_array)
                .unwrap_or_else(|| json!([]))
        };
        let filter = match webhook_input.get("filter") {
            Some(ResolvedValue::String(value)) => json!(value),
            Some(ResolvedValue::Null) => Value::Null,
            Some(_) => Value::Null,
            None => existing
                .as_ref()
                .map(|record| record["filter"].clone())
                .unwrap_or(Value::Null),
        };
        let created_at = existing
            .as_ref()
            .and_then(|record| record["createdAt"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| self.next_product_timestamp());
        let updated_at = if existing.is_some() {
            self.next_product_timestamp()
        } else {
            created_at.clone()
        };
        let api_version = existing
            .as_ref()
            .and_then(|record| record.get("apiVersion"))
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| {
                webhook_subscription_api_version_record(api_version_handle, request_api_version)
            });
        let mut record = json!({
            "id": id,
            "legacyResourceId": webhook_subscription_legacy_id(id),
            "apiVersion": api_version,
            "topic": topic,
            "format": format,
            "uri": uri,
            "name": name,
            "apiPermissionId": api_permission_id,
            "includeFields": include_fields,
            "metafieldNamespaces": metafield_namespaces,
            "metafields": metafields,
            "filter": filter,
            "createdAt": created_at,
            "updatedAt": updated_at,
            "endpoint": webhook_endpoint(&uri)
        });
        if let Some(callback_url) = callback_url {
            record["callbackUrl"] = json!(callback_url);
        }
        record
    }
}

/// The ordered required (non-null) arguments for each webhook mutation root,
/// paired with the GraphQL type Shopify reports for them. The `webhookSubscription`
/// input type varies by delivery flavor (unified / Pub/Sub / EventBridge).
fn webhook_required_arguments(field_name: &str) -> Vec<(&'static str, &'static str)> {
    let input_type = if field_name.starts_with("pubSubWebhookSubscription") {
        "PubSubWebhookSubscriptionInput!"
    } else if field_name.starts_with("eventBridgeWebhookSubscription") {
        "EventBridgeWebhookSubscriptionInput!"
    } else {
        "WebhookSubscriptionInput!"
    };
    if field_name.ends_with("Create") {
        vec![
            ("topic", "WebhookSubscriptionTopic!"),
            ("webhookSubscription", input_type),
        ]
    } else if field_name.ends_with("Update") {
        vec![("id", "ID!"), ("webhookSubscription", input_type)]
    } else if field_name.ends_with("Delete") {
        vec![("id", "ID!")]
    } else {
        Vec::new()
    }
}

/// Static GraphQL validation for the webhook mutation roots: a required argument
/// that is entirely absent yields a `missingRequiredArguments` error, while one
/// present with a literal `null` yields an `argumentLiteralsIncompatible` error.
fn webhook_required_argument_errors(
    field: &RootFieldSelection,
    document: &ParsedDocument,
) -> Vec<Value> {
    let required = webhook_required_arguments(&field.name);
    if required.is_empty() {
        return Vec::new();
    }
    let mut errors = Vec::new();
    let mut missing = Vec::new();
    for (arg, type_display) in &required {
        match field.raw_arguments.get(*arg) {
            None => missing.push(*arg),
            Some(value) if value.is_literal_null() => {
                errors.push(json!({
                    "message": format!(
                        "Argument '{}' on Field '{}' has an invalid value (null). Expected type '{}'.",
                        arg, field.name, type_display
                    ),
                    "locations": [{ "line": field.location.line, "column": field.location.column }],
                    "path": [document.operation_path.clone(), field.name.clone(), *arg],
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "Field",
                        "argumentName": arg
                    }
                }));
            }
            Some(_) => {}
        }
    }
    if !missing.is_empty() {
        errors.insert(
            0,
            missing_required_arguments_error(
                &field.name,
                &missing.join(", "),
                field.location,
                vec![
                    json!(document.operation_path.clone()),
                    json!(field.name.clone()),
                ],
            ),
        );
    }
    errors
}

fn webhook_subscription_topic_coercion_error(
    field: &RootFieldSelection,
    document: Option<&ParsedDocument>,
) -> Option<Value> {
    let raw_topic = field.raw_arguments.get("topic")?;
    let topic = match raw_topic {
        RawArgumentValue::Enum(topic) => topic.as_str(),
        RawArgumentValue::Variable {
            value: Some(ResolvedValue::String(topic)),
            ..
        } => topic.as_str(),
        _ => return None,
    };
    if is_known_webhook_subscription_topic(topic) {
        return None;
    }
    Some(match raw_topic {
        RawArgumentValue::Enum(_) => json!({
            "message": format!("Argument 'topic' on Field '{}' has an invalid value ({}). Expected type 'WebhookSubscriptionTopic!'.", field.name, topic),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [
                document
                    .map(|document| document.operation_path.clone())
                    .unwrap_or_else(|| "mutation".to_string()),
                field.name.clone(),
                "topic"
            ],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "Field",
                "argumentName": "topic"
            }
        }),
        RawArgumentValue::Variable { name, .. } => {
            // Shopify anchors a coerced-variable error at the variable's
            // *definition* in the operation signature, not at the field.
            let location = document
                .and_then(|document| document.variable_definitions.get(name))
                .map_or(field.location, |definition| definition.location);
            json!({
                "message": format!("Variable ${} of type WebhookSubscriptionTopic! was provided invalid value", name),
                "locations": [{ "line": location.line, "column": location.column }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": topic,
                    "problems": [{
                        "path": [],
                        "explanation": format!("Expected \"{}\" to be one of: {}", topic, WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES)
                    }]
                }
            })
        }
        _ => unreachable!(),
    })
}

fn dedicated_pubsub_required_field_error(
    root_field: &str,
    field: &RootFieldSelection,
    document: &ParsedDocument,
) -> Option<Value> {
    if !root_field.starts_with("pubSubWebhookSubscription") {
        return None;
    }
    match field.raw_arguments.get("webhookSubscription")? {
        RawArgumentValue::Variable {
            name,
            value: Some(ResolvedValue::Object(value)),
        } => dedicated_pubsub_variable_required_field_error(name, value, field, document),
        RawArgumentValue::Object(value) => {
            dedicated_pubsub_inline_required_field_error(value, field)
        }
        _ => None,
    }
}

fn dedicated_pubsub_variable_required_field_error(
    variable_name: &str,
    value: &BTreeMap<String, ResolvedValue>,
    field: &RootFieldSelection,
    document: &ParsedDocument,
) -> Option<Value> {
    let missing = missing_pubsub_resolved_fields(value);
    if missing.is_empty() {
        return None;
    }
    // Shopify anchors a coerced-variable error at the variable's *definition*
    // in the operation signature, not at the field where it is used.
    let location = document
        .variable_definitions
        .get(variable_name)
        .map_or(field.location, |definition| definition.location);
    let message_detail = missing
        .iter()
        .map(|key| format!("{key} (Expected value to not be null)"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(json!({
        "message": format!("Variable ${} of type PubSubWebhookSubscriptionInput! was provided invalid value for {}", variable_name, message_detail),
        "locations": [{ "line": location.line, "column": location.column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(&ResolvedValue::Object(value.clone())),
            "problems": missing
                .iter()
                .map(|key| json!({
                    "path": [key],
                    "explanation": "Expected value to not be null"
                }))
                .collect::<Vec<_>>()
        }
    }))
}

fn dedicated_pubsub_inline_required_field_error(
    value: &BTreeMap<String, RawArgumentValue>,
    field: &RootFieldSelection,
) -> Option<Value> {
    let missing = ["pubSubProject", "pubSubTopic"]
        .into_iter()
        .filter(|key| {
            !value.contains_key(*key)
                || value
                    .get(*key)
                    .is_some_and(RawArgumentValue::is_literal_null)
        })
        .collect::<Vec<_>>();
    let first_missing = missing.first()?;
    Some(json!({
        "message": format!("Argument '{}' on InputObject 'PubSubWebhookSubscriptionInput' is required. Expected type String!", first_missing),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "path": ["mutation", field.name.clone(), "webhookSubscription", first_missing],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": first_missing,
            "argumentType": "String!",
            "inputObjectType": "PubSubWebhookSubscriptionInput"
        }
    }))
}

fn missing_pubsub_resolved_fields(value: &BTreeMap<String, ResolvedValue>) -> Vec<&'static str> {
    ["pubSubProject", "pubSubTopic"]
        .into_iter()
        .filter(|key| {
            !value.contains_key(*key) || matches!(value.get(*key), Some(ResolvedValue::Null))
        })
        .collect()
}

fn webhook_address_error(root_field: &str, message: &str) -> Value {
    if root_field.starts_with("eventBridgeWebhookSubscription") {
        webhook_error(["webhookSubscription", "arn"], message)
    } else {
        callback_error(message)
    }
}
fn webhook_error(field: impl Into<UserErrorField>, message: &str) -> Value {
    user_error_omit_code(field, message, None)
}
fn callback_error(message: &str) -> Value {
    webhook_error(["webhookSubscription", "callbackUrl"], message)
}
fn webhook_subscription_optional_string_key(record: &Value, key: &str) -> Option<String> {
    record[key].as_str().map(ToString::to_string)
}

fn webhook_subscription_effective_api_version(request: &Request) -> Option<String> {
    request_header(request, "x-shopify-draft-proxy-api-version")
        .or_else(|| admin_graphql_version(&request.path).map(|version| version.trim().to_string()))
}

fn webhook_subscription_api_version_record(
    handle: Option<&str>,
    request_api_version: Option<&str>,
) -> Value {
    let handle = match handle.map(str::trim).filter(|handle| !handle.is_empty()) {
        Some(handle) => handle.to_string(),
        None => latest_supported_admin_graphql_version()
            .unwrap_or("2026-04")
            .to_string(),
    };
    let request_api_version = request_api_version
        .filter(|version| supported_admin_graphql_version(version))
        .map(str::to_string)
        .or_else(|| latest_supported_admin_graphql_version().map(str::to_string));
    let future_release = request_api_version
        .as_deref()
        .is_some_and(|request_version| {
            supported_admin_graphql_version(&handle) && handle.as_str() > request_version
        });
    let (display_name, supported) = if future_release {
        (format!("{handle} (Release candidate)"), false)
    } else if supported_admin_graphql_version(&handle) {
        if Some(handle.as_str()) == request_api_version.as_deref() {
            (format!("{handle} (Latest)"), true)
        } else {
            (handle.clone(), true)
        }
    } else {
        match handle.as_str() {
            "2026-07" => ("2026-07 (Release candidate)".to_string(), false),
            "unstable" => ("unstable".to_string(), false),
            _ => (handle.clone(), false),
        }
    };
    json!({
        "handle": handle,
        "displayName": display_name,
        "supported": supported
    })
}

/// Resolve an app-reserved metafield namespace shorthand. Shopify expands
/// `$app:NAME` to `app--<api_client_id>--NAME` (and bare `$app` to
/// `app--<api_client_id>`) using the requesting app's client id. Namespaces
/// that are already fully qualified (e.g. `app--999999999999--kept`) or
/// unrelated (e.g. `custom`) are returned unchanged.
fn resolve_webhook_metafield_namespace(namespace: &str, api_client_id: Option<&str>) -> String {
    let Some(client_id) = api_client_id else {
        return namespace.to_string();
    };
    if let Some(rest) = namespace.strip_prefix("$app:") {
        format!("app--{client_id}--{rest}")
    } else if namespace == "$app" {
        format!("app--{client_id}")
    } else {
        namespace.to_string()
    }
}

/// A webhook filter is a search-query string where every non-boolean term must
/// reference a field via `field:value` syntax. A non-empty filter containing any
/// bare/default term (e.g. `customer_id:123 bareword`) is rejected by Shopify.
/// Empty/blank filters mean "no filter" and are accepted.
fn webhook_filter_exceeds_byte_size_limit(filter: &str) -> bool {
    filter.len() > WEBHOOK_FILTER_MAX_BYTE_SIZE
}

fn webhook_filter_is_invalid(filter: &str) -> bool {
    let trimmed = filter.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut saw_field_term = false;
    for token in trimmed.split_whitespace() {
        if token.eq_ignore_ascii_case("AND") || token.eq_ignore_ascii_case("OR") {
            continue;
        }

        let term = token.strip_prefix('-').unwrap_or(token);
        let Some((field, _)) = term.split_once(':') else {
            return true;
        };
        if field.is_empty() || !field.chars().all(graphql_name_char) {
            return true;
        }

        saw_field_term = true;
    }

    !saw_field_term
}

fn is_known_webhook_subscription_topic(topic: &str) -> bool {
    WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES
        .split(", ")
        .any(|known| known == topic)
}

const WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES: &str = "TAX_SUMMARIES_CREATE, APP_UNINSTALLED, APP_SCOPES_UPDATE, CARTS_CREATE, CARTS_UPDATE, CHANNELS_DELETE, CHECKOUTS_CREATE, CHECKOUTS_DELETE, CHECKOUTS_UPDATE, CUSTOMER_PAYMENT_METHODS_CREATE, CUSTOMER_PAYMENT_METHODS_UPDATE, CUSTOMER_PAYMENT_METHODS_REVOKE, COLLECTION_LISTINGS_ADD, COLLECTION_LISTINGS_REMOVE, COLLECTION_LISTINGS_UPDATE, COLLECTION_PUBLICATIONS_CREATE, COLLECTION_PUBLICATIONS_DELETE, COLLECTION_PUBLICATIONS_UPDATE, COLLECTIONS_CREATE, COLLECTIONS_DELETE, COLLECTIONS_UPDATE, CUSTOMER_GROUPS_CREATE, CUSTOMER_GROUPS_DELETE, CUSTOMER_GROUPS_UPDATE, CUSTOMERS_CREATE, CUSTOMERS_DELETE, CUSTOMERS_DISABLE, CUSTOMERS_ENABLE, CUSTOMERS_UPDATE, CUSTOMERS_PURCHASING_SUMMARY, CUSTOMERS_MARKETING_CONSENT_UPDATE, CUSTOMER_TAGS_ADDED, CUSTOMER_TAGS_REMOVED, CUSTOMERS_EMAIL_MARKETING_CONSENT_UPDATE, DISPUTES_CREATE, DISPUTES_UPDATE, DRAFT_ORDERS_CREATE, DRAFT_ORDERS_DELETE, DRAFT_ORDERS_UPDATE, FULFILLMENT_EVENTS_CREATE, FULFILLMENT_EVENTS_DELETE, FULFILLMENTS_CREATE, FULFILLMENTS_UPDATE, ATTRIBUTED_SESSIONS_FIRST, ATTRIBUTED_SESSIONS_LAST, ORDER_TRANSACTIONS_CREATE, ORDERS_CANCELLED, ORDERS_CREATE, ORDERS_DELETE, ORDERS_EDITED, ORDERS_FULFILLED, ORDERS_PAID, ORDERS_PARTIALLY_FULFILLED, ORDERS_UPDATED, ORDERS_LINK_REQUESTED, FULFILLMENT_ORDERS_MOVED, FULFILLMENT_ORDERS_HOLD_RELEASED, FULFILLMENT_ORDERS_SCHEDULED_FULFILLMENT_ORDER_READY, FULFILLMENT_HOLDS_RELEASED, FULFILLMENT_ORDERS_ORDER_ROUTING_COMPLETE, FULFILLMENT_ORDERS_CANCELLED, FULFILLMENT_ORDERS_FULFILLMENT_SERVICE_FAILED_TO_COMPLETE, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_REJECTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_SUBMITTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_ACCEPTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_REJECTED, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_SUBMITTED, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_ACCEPTED, FULFILLMENT_HOLDS_ADDED, FULFILLMENT_ORDERS_LINE_ITEMS_PREPARED_FOR_LOCAL_DELIVERY, FULFILLMENT_ORDERS_PLACED_ON_HOLD, FULFILLMENT_ORDERS_MERGED, FULFILLMENT_ORDERS_SPLIT, FULFILLMENT_ORDERS_PROGRESS_REPORTED, FULFILLMENT_ORDERS_MANUALLY_REPORTED_PROGRESS_STOPPED, PRODUCT_LISTINGS_ADD, PRODUCT_LISTINGS_REMOVE, PRODUCT_LISTINGS_UPDATE, SCHEDULED_PRODUCT_LISTINGS_ADD, SCHEDULED_PRODUCT_LISTINGS_UPDATE, SCHEDULED_PRODUCT_LISTINGS_REMOVE, PRODUCT_PUBLICATIONS_CREATE, PRODUCT_PUBLICATIONS_DELETE, PRODUCT_PUBLICATIONS_UPDATE, PRODUCTS_CREATE, PRODUCTS_DELETE, PRODUCTS_UPDATE, REFUNDS_CREATE, SEGMENTS_CREATE, SEGMENTS_DELETE, SEGMENTS_UPDATE, SHIPPING_ADDRESSES_CREATE, SHIPPING_ADDRESSES_UPDATE, SHOP_UPDATE, TAX_PARTNERS_UPDATE, TAX_SERVICES_CREATE, TAX_SERVICES_UPDATE, THEMES_CREATE, THEMES_DELETE, THEMES_PUBLISH, THEMES_UPDATE, VARIANTS_IN_STOCK, VARIANTS_OUT_OF_STOCK, INVENTORY_LEVELS_CONNECT, INVENTORY_LEVELS_UPDATE, INVENTORY_LEVELS_DISCONNECT, INVENTORY_ITEMS_CREATE, INVENTORY_ITEMS_UPDATE, INVENTORY_ITEMS_DELETE, LOCATIONS_ACTIVATE, LOCATIONS_DEACTIVATE, LOCATIONS_CREATE, LOCATIONS_UPDATE, LOCATIONS_DELETE, TENDER_TRANSACTIONS_CREATE, APP_PURCHASES_ONE_TIME_UPDATE, APP_SUBSCRIPTIONS_APPROACHING_CAPPED_AMOUNT, APP_SUBSCRIPTIONS_UPDATE, LOCALES_CREATE, LOCALES_UPDATE, LOCALES_DESTROY, DOMAINS_CREATE, DOMAINS_UPDATE, DOMAINS_DESTROY, SUBSCRIPTION_CONTRACTS_CREATE, SUBSCRIPTION_CONTRACTS_UPDATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_CREATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_UPDATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_DELETE, PROFILES_CREATE, PROFILES_UPDATE, PROFILES_DELETE, SUBSCRIPTION_BILLING_ATTEMPTS_SUCCESS, SUBSCRIPTION_BILLING_ATTEMPTS_FAILURE, SUBSCRIPTION_BILLING_ATTEMPTS_CHALLENGED, RETURNS_CANCEL, RETURNS_CLOSE, RETURNS_REOPEN, RETURNS_REQUEST, RETURNS_APPROVE, RETURNS_UPDATE, RETURNS_PROCESS, RETURNS_DECLINE, REVERSE_DELIVERIES_ATTACH_DELIVERABLE, REVERSE_FULFILLMENT_ORDERS_DISPOSE, PAYMENT_TERMS_CREATE, PAYMENT_TERMS_DELETE, PAYMENT_TERMS_UPDATE, PAYMENT_SCHEDULES_DUE, SELLING_PLAN_GROUPS_CREATE, SELLING_PLAN_GROUPS_UPDATE, SELLING_PLAN_GROUPS_DELETE, BULK_OPERATIONS_FINISH, PRODUCT_FEEDS_CREATE, PRODUCT_FEEDS_UPDATE, PRODUCT_FEEDS_INCREMENTAL_SYNC, PRODUCT_FEEDS_FULL_SYNC, PRODUCT_FEEDS_FULL_SYNC_FINISH, MARKETS_CREATE, MARKETS_UPDATE, MARKETS_DELETE, ORDERS_RISK_ASSESSMENT_CHANGED, ORDERS_SHOPIFY_PROTECT_ELIGIBILITY_CHANGED, FINANCE_KYC_INFORMATION_UPDATE, FULFILLMENT_ORDERS_RESCHEDULED, PUBLICATIONS_DELETE, AUDIT_EVENTS_ADMIN_API_ACTIVITY, FULFILLMENT_ORDERS_LINE_ITEMS_PREPARED_FOR_PICKUP, COMPANIES_CREATE, COMPANIES_UPDATE, COMPANIES_DELETE, COMPANY_LOCATIONS_CREATE, COMPANY_LOCATIONS_UPDATE, COMPANY_LOCATIONS_DELETE, COMPANY_CONTACTS_CREATE, COMPANY_CONTACTS_UPDATE, COMPANY_CONTACTS_DELETE, CUSTOMERS_MERGE, INVENTORY_TRANSFERS_ADD_ITEMS, INVENTORY_TRANSFERS_UPDATE_ITEM_QUANTITIES, INVENTORY_TRANSFERS_REMOVE_ITEMS, INVENTORY_TRANSFERS_READY_TO_SHIP, INVENTORY_TRANSFERS_CANCEL, INVENTORY_TRANSFERS_COMPLETE, INVENTORY_SHIPMENTS_DELETE, INVENTORY_SHIPMENTS_CREATE, INVENTORY_SHIPMENTS_MARK_IN_TRANSIT, INVENTORY_SHIPMENTS_UPDATE_TRACKING, INVENTORY_SHIPMENTS_ADD_ITEMS, INVENTORY_SHIPMENTS_UPDATE_ITEM_QUANTITIES, INVENTORY_SHIPMENTS_REMOVE_ITEMS, INVENTORY_SHIPMENTS_RECEIVE_ITEMS, CUSTOMER_ACCOUNT_SETTINGS_UPDATE, CUSTOMER_JOINED_SEGMENT, CUSTOMER_LEFT_SEGMENT, COMPANY_CONTACT_ROLES_ASSIGN, COMPANY_CONTACT_ROLES_REVOKE, SUBSCRIPTION_CONTRACTS_ACTIVATE, SUBSCRIPTION_CONTRACTS_PAUSE, SUBSCRIPTION_CONTRACTS_CANCEL, SUBSCRIPTION_CONTRACTS_FAIL, SUBSCRIPTION_CONTRACTS_EXPIRE, SUBSCRIPTION_BILLING_CYCLES_SKIP, SUBSCRIPTION_BILLING_CYCLES_UNSKIP, METAOBJECTS_CREATE, METAOBJECTS_UPDATE, METAOBJECTS_DELETE, FINANCE_APP_STAFF_MEMBER_GRANT, FINANCE_APP_STAFF_MEMBER_REVOKE, FINANCE_APP_STAFF_MEMBER_DELETE, FINANCE_APP_STAFF_MEMBER_UPDATE, DISCOUNTS_CREATE, DISCOUNTS_UPDATE, DISCOUNTS_DELETE, DISCOUNTS_REDEEMCODE_ADDED, DISCOUNTS_REDEEMCODE_REMOVED, METAFIELD_DEFINITIONS_CREATE, METAFIELD_DEFINITIONS_UPDATE, METAFIELD_DEFINITIONS_DELETE, DELIVERY_PROMISE_SETTINGS_UPDATE, MARKETS_BACKUP_REGION_UPDATE, CHECKOUT_AND_ACCOUNTS_CONFIGURATIONS_UPDATE";
