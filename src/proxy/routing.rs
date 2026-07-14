use super::*;
use crate::admin_graphql::AdminApiVersion;

const SUPPORTED_STOREFRONT_GRAPHQL_VERSIONS: &[&str] = &["2025-01", "2026-04"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::proxy) enum Route {
    Health,
    MetaConfig,
    MetaLog,
    MetaState,
    MetaReset,
    MetaDump,
    MetaRestore,
    MetaCommit,
    BulkOperationResult { artifact_id: String },
    Graphql,
    StorefrontGraphql,
    NotFound,
    MethodNotAllowed,
}

pub(in crate::proxy) fn route(request: &Request) -> Route {
    let method = request.method.to_ascii_uppercase();
    match request.path.as_str() {
        "/__meta/health" => only_method("GET", &method, Route::Health),
        "/__meta/config" => only_method("GET", &method, Route::MetaConfig),
        "/__meta/log" => only_method("GET", &method, Route::MetaLog),
        "/__meta/state" => only_method("GET", &method, Route::MetaState),
        "/__meta/reset" => only_method("POST", &method, Route::MetaReset),
        "/__meta/dump" => only_method("POST", &method, Route::MetaDump),
        "/__meta/restore" => only_method("POST", &method, Route::MetaRestore),
        "/__meta/commit" => only_method("POST", &method, Route::MetaCommit),
        path if path.starts_with("/__meta/bulk-operations/") && path.ends_with("/result.jsonl") => {
            only_method(
                "GET",
                &method,
                Route::BulkOperationResult {
                    artifact_id: bulk_operation_result_artifact_id(path),
                },
            )
        }
        path if admin_graphql_version(path).is_some() => {
            only_method("POST", &method, Route::Graphql)
        }
        path if storefront_graphql_version(path).is_some() => {
            only_method("POST", &method, Route::StorefrontGraphql)
        }
        _ => Route::NotFound,
    }
}

fn bulk_operation_result_artifact_id(path: &str) -> String {
    path.trim_start_matches("/__meta/bulk-operations/")
        .trim_end_matches("/result.jsonl")
        .to_string()
}

pub(in crate::proxy) fn only_method(expected: &str, actual: &str, route: Route) -> Route {
    if actual == expected {
        route
    } else {
        Route::MethodNotAllowed
    }
}

pub(in crate::proxy) fn admin_graphql_version(path: &str) -> Option<&str> {
    let version = admin_graphql_path_version(path)?;
    supported_admin_graphql_version(version).then_some(version)
}

fn admin_graphql_path_version(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some(""), Some("admin"), Some("api"), Some(version), Some("graphql.json"), None) => {
            Some(version)
        }
        _ => None,
    }
}

pub(in crate::proxy) fn storefront_graphql_version(path: &str) -> Option<&str> {
    let version = storefront_graphql_path_version(path)?;
    supported_storefront_graphql_version(version).then_some(version)
}

fn storefront_graphql_path_version(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some(""), Some("api"), Some(version), Some("graphql.json"), None) => Some(version),
        _ => None,
    }
}

pub(in crate::proxy) fn supported_admin_graphql_version(version: &str) -> bool {
    AdminApiVersion::parse(version).is_some()
}

pub(in crate::proxy) fn supported_storefront_graphql_version(version: &str) -> bool {
    SUPPORTED_STOREFRONT_GRAPHQL_VERSIONS.contains(&version)
}

pub(in crate::proxy) fn latest_supported_admin_graphql_version() -> Option<&'static str> {
    AdminApiVersion::ALL
        .last()
        .copied()
        .map(AdminApiVersion::as_str)
}

pub(in crate::proxy) fn version_at_least(
    version: &str,
    minimum_year: u16,
    minimum_month: u8,
) -> bool {
    let Some((year, month)) = parse_year_month_version(version) else {
        return false;
    };
    (year, month) >= (minimum_year, minimum_month)
}

fn parse_year_month_version(version: &str) -> Option<(u16, u8)> {
    let (year, month) = version.split_once('-')?;
    let year = year.parse().ok()?;
    let month = month.parse().ok()?;
    matches!(month, 1 | 4 | 7 | 10).then_some((year, month))
}

pub(in crate::proxy) fn request_header(request: &Request, header_name: &str) -> Option<String> {
    request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(header_name))
        .map(|(_, value)| value.clone())
}

pub(in crate::proxy) fn request_access_token(request: &Request) -> Option<String> {
    request_header(request, "X-Shopify-Access-Token").or_else(|| {
        request_header(request, "Authorization").map(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .unwrap_or(&value)
                .to_string()
        })
    })
}

pub(in crate::proxy) fn ok_json(body: Value) -> Response {
    Response {
        status: 200,
        headers: BTreeMap::new(),
        body,
    }
}

pub(in crate::proxy) fn json_error(status: u16, message: &str) -> Response {
    Response {
        status,
        headers: BTreeMap::new(),
        body: json!({ "errors": [{ "message": message }] }),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::proxy) struct GraphqlRequestBody {
    pub(in crate::proxy) query: String,
    pub(in crate::proxy) operation_name: Option<String>,
    pub(in crate::proxy) variables: BTreeMap<String, ResolvedValue>,
    pub(in crate::proxy) variable_input_orders: BTreeMap<Vec<String>, Vec<String>>,
}

pub(in crate::proxy) fn parse_graphql_request_body(body: &str) -> Option<GraphqlRequestBody> {
    let variable_input_orders = ordered_variable_input_fields(body).unwrap_or_default();
    let body = serde_json::from_str::<Value>(body).ok()?;
    let query = body.get("query")?.as_str()?.to_owned();
    let operation_name = match body.get("operationName") {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) | None => None,
        Some(_) => None,
    };
    let variables = match body.get("variables") {
        Some(Value::Object(fields)) => fields
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect(),
        _ => BTreeMap::new(),
    };

    Some(GraphqlRequestBody {
        query,
        operation_name,
        variables,
        variable_input_orders,
    })
}

#[derive(Debug)]
enum OrderedJsonValue {
    Scalar,
    Array(Vec<OrderedJsonValue>),
    Object(Vec<(String, OrderedJsonValue)>),
}

impl<'de> Deserialize<'de> for OrderedJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OrderedJsonVisitor;

        impl<'de> serde::de::Visitor<'de> for OrderedJsonVisitor {
            type Value = OrderedJsonValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON value")
            }

            fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Scalar)
            }

            fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Scalar)
            }

            fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Scalar)
            }

            fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Scalar)
            }

            fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(OrderedJsonValue::Scalar)
            }

            fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Scalar)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Scalar)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Scalar)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_any(self)
            }

            fn visit_seq<A>(self, mut values: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some(value) = values.next_element()? {
                    entries.push(value);
                }
                Ok(OrderedJsonValue::Array(entries))
            }

            fn visit_map<A>(self, mut values: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some((name, value)) = values.next_entry()? {
                    entries.push((name, value));
                }
                Ok(OrderedJsonValue::Object(entries))
            }
        }

        deserializer.deserialize_any(OrderedJsonVisitor)
    }
}

fn ordered_variable_input_fields(body: &str) -> Option<BTreeMap<Vec<String>, Vec<String>>> {
    let OrderedJsonValue::Object(root) = serde_json::from_str::<OrderedJsonValue>(body).ok()?
    else {
        return None;
    };
    let OrderedJsonValue::Object(variables) = root
        .into_iter()
        .find_map(|(name, value)| (name == "variables").then_some(value))?
    else {
        return None;
    };
    let mut orders = BTreeMap::new();
    for (variable_name, value) in variables {
        collect_ordered_object_fields(&value, vec![variable_name], &mut orders);
    }
    Some(orders)
}

fn collect_ordered_object_fields(
    value: &OrderedJsonValue,
    path: Vec<String>,
    orders: &mut BTreeMap<Vec<String>, Vec<String>>,
) {
    match value {
        OrderedJsonValue::Scalar => {}
        OrderedJsonValue::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                let mut item_path = path.clone();
                item_path.push(index.to_string());
                collect_ordered_object_fields(value, item_path, orders);
            }
        }
        OrderedJsonValue::Object(fields) => {
            orders.insert(
                path.clone(),
                fields.iter().map(|(name, _)| name.clone()).collect(),
            );
            for (name, value) in fields {
                let mut field_path = path.clone();
                field_path.push(name.clone());
                collect_ordered_object_fields(value, field_path, orders);
            }
        }
    }
}

pub(in crate::proxy) fn resolved_value_from_json(value: &Value) -> ResolvedValue {
    match value {
        Value::Null => ResolvedValue::Null,
        Value::Bool(value) => ResolvedValue::Bool(*value),
        Value::Number(number) => number
            .as_i64()
            .map(ResolvedValue::Int)
            .or_else(|| number.as_f64().map(ResolvedValue::Float))
            .unwrap_or(ResolvedValue::Null),
        Value::String(value) => ResolvedValue::String(value.clone()),
        Value::Array(values) => {
            ResolvedValue::List(values.iter().map(resolved_value_from_json).collect())
        }
        Value::Object(fields) => ResolvedValue::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphql_request_preserves_nested_variable_input_field_order() {
        let request = parse_graphql_request_body(
            r#"{"query":"mutation($input: ExampleInput!) { example(input: $input) }","variables":{"input":{"z":1,"a":{"second":2,"first":1},"m":3}}}"#,
        )
        .expect("request should parse");

        assert_eq!(
            request
                .variable_input_orders
                .get(&vec!["input".to_string()]),
            Some(&vec!["z".to_string(), "a".to_string(), "m".to_string()])
        );
        assert_eq!(
            request
                .variable_input_orders
                .get(&vec!["input".to_string(), "a".to_string()]),
            Some(&vec!["second".to_string(), "first".to_string()])
        );
    }
}
