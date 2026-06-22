use super::*;

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

pub(in crate::proxy) fn webhook_subscription_matches_field_args(
    record: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if let Some(format) = resolved_string_arg(arguments, "format") {
        if !record["format"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(&format))
        {
            return false;
        }
    }

    if let Some(uri) = resolved_string_arg(arguments, "uri") {
        if record["uri"].as_str() != Some(uri.as_str())
            && record["callbackUrl"].as_str() != Some(uri.as_str())
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

    if let Some(query) = resolved_string_arg(arguments, "query") {
        if !webhook_subscription_matches_query(record, &query) {
            return false;
        }
    }

    true
}

pub(in crate::proxy) fn webhook_subscription_matches_query(record: &Value, query: &str) -> bool {
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
        let matches = webhook_subscription_matches_query_term(record, field, value);
        if matches == negated {
            return false;
        }
    }
    true
}

pub(in crate::proxy) fn webhook_subscription_matches_query_term(
    record: &Value,
    field: &str,
    value: &str,
) -> bool {
    let wanted = value.to_ascii_lowercase();
    match field.to_ascii_lowercase().as_str() {
        "id" => record["id"].as_str().is_some_and(|id| {
            id.eq_ignore_ascii_case(value)
                || webhook_subscription_legacy_id(id).eq_ignore_ascii_case(value)
        }),
        "topic" => webhook_subscription_string_field(record, "topic").contains(&wanted),
        "format" => webhook_subscription_string_field(record, "format") == wanted,
        "uri" | "callbackurl" => {
            webhook_subscription_string_field(record, "uri").contains(&wanted)
                || webhook_subscription_string_field(record, "callbackUrl").contains(&wanted)
        }
        _ => false,
    }
}
