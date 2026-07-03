use super::*;

const SUPPORTED_ADMIN_GRAPHQL_VERSIONS: &[&str] = &["2025-01", "2025-10", "2026-01", "2026-04"];

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
    supported_admin_graphql_version(version).then_some(version)
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
    SUPPORTED_ADMIN_GRAPHQL_VERSIONS.contains(&version)
}

pub(in crate::proxy) fn latest_supported_admin_graphql_version() -> Option<&'static str> {
    SUPPORTED_ADMIN_GRAPHQL_VERSIONS.last().copied()
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
    pub(in crate::proxy) variables: BTreeMap<String, ResolvedValue>,
}

pub(in crate::proxy) fn parse_graphql_request_body(body: &str) -> Option<GraphqlRequestBody> {
    let body = serde_json::from_str::<Value>(body).ok()?;
    let query = body.get("query")?.as_str()?.to_owned();
    let variables = match body.get("variables") {
        Some(Value::Object(fields)) => fields
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect(),
        _ => BTreeMap::new(),
    };

    Some(GraphqlRequestBody { query, variables })
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
