use std::collections::BTreeMap;
use std::fmt;

use reqwest::blocking::{Client, Request as ReqwestRequest};
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Method, Url};
use serde_json::Value;

use crate::proxy::{Request, Response};

#[derive(Clone)]
pub struct HttpUpstreamClient {
    origin: String,
    client: Client,
}

impl HttpUpstreamClient {
    pub fn new(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            client: Client::new(),
        }
    }

    pub fn send(&self, request: Request) -> Response {
        match self.send_result(request) {
            Ok(response) => response,
            Err(error) => Response {
                status: 502,
                headers: BTreeMap::new(),
                body: serde_json::json!({ "errors": [{ "message": format!("upstream network error: {error}") }] }),
            },
        }
    }

    pub fn send_result(&self, request: Request) -> Result<Response, UpstreamError> {
        let upstream_request = self.build_request(request)?;
        let upstream_response = self.client.execute(upstream_request)?;
        let status = upstream_response.status().as_u16();
        let headers = upstream_response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_ascii_lowercase(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let text = upstream_response.text()?;
        let body = match serde_json::from_str::<Value>(&text) {
            Ok(body) => body,
            Err(_) => Value::String(text),
        };

        Ok(Response {
            status,
            headers,
            body,
        })
    }

    pub fn build_request(&self, request: Request) -> Result<ReqwestRequest, UpstreamError> {
        let method = Method::from_bytes(request.method.as_bytes())
            .map_err(|error| UpstreamError::InvalidMethod(error.to_string()))?;
        let url = build_upstream_url(&self.origin, &request.path)?;
        let mut builder = self.client.request(method, url);

        for (name, value) in request.headers {
            if !should_forward_header(&name) {
                continue;
            }
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|error| UpstreamError::InvalidHeaderName(error.to_string()))?;
            let header_value = HeaderValue::from_str(&value)
                .map_err(|error| UpstreamError::InvalidHeaderValue(error.to_string()))?;
            builder = builder.header(header_name, header_value);
        }

        builder
            .body(request.body)
            .build()
            .map_err(UpstreamError::Http)
    }
}

#[derive(Debug)]
pub enum UpstreamError {
    InvalidOrigin(String),
    InvalidScheme(String),
    InvalidMethod(String),
    InvalidHeaderName(String),
    InvalidHeaderValue(String),
    Http(reqwest::Error),
}

impl fmt::Display for UpstreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOrigin(error) => write!(formatter, "invalid upstream origin: {error}"),
            Self::InvalidScheme(scheme) => {
                write!(formatter, "unsupported upstream origin scheme: {scheme}")
            }
            Self::InvalidMethod(error) => write!(formatter, "invalid upstream method: {error}"),
            Self::InvalidHeaderName(error) => {
                write!(formatter, "invalid upstream header name: {error}")
            }
            Self::InvalidHeaderValue(error) => {
                write!(formatter, "invalid upstream header value: {error}")
            }
            Self::Http(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for UpstreamError {}

impl From<reqwest::Error> for UpstreamError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

fn build_upstream_url(origin: &str, path: &str) -> Result<Url, UpstreamError> {
    let mut url =
        Url::parse(origin).map_err(|error| UpstreamError::InvalidOrigin(error.to_string()))?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(UpstreamError::InvalidScheme(scheme.to_string())),
    }

    let (path, query) = path
        .split_once('?')
        .map_or((path, None), |(path, query)| (path, Some(query)));
    url.set_path(if path.is_empty() { "/" } else { path });
    url.set_query(query);
    Ok(url)
}

fn should_forward_header(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "content-length"
            | "host"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HOST};
    use serde_json::json;

    use super::*;

    #[test]
    fn build_request_preserves_https_origin_versioned_path_and_body() {
        let client = HttpUpstreamClient::new("https://example.myshopify.com");
        let body = json!({ "query": "{ shop { name } }" }).to_string();

        let request = client
            .build_request(Request {
                method: "POST".to_string(),
                path: "/admin/api/2026-04/graphql.json".to_string(),
                headers: BTreeMap::new(),
                body: body.clone(),
            })
            .expect("request should build");

        assert_eq!(request.url().scheme(), "https");
        assert_eq!(request.url().host_str(), Some("example.myshopify.com"));
        assert_eq!(request.url().path(), "/admin/api/2026-04/graphql.json");
        assert_eq!(
            request.body().and_then(|body| body.as_bytes()),
            Some(body.as_bytes())
        );
    }

    #[test]
    fn build_request_forwards_auth_headers_and_filters_hop_by_hop_headers() {
        let client = HttpUpstreamClient::new("https://example.myshopify.com");
        let request = client
            .build_request(Request {
                method: "POST".to_string(),
                path: "/admin/api/2026-04/graphql.json".to_string(),
                headers: [
                    ("authorization".to_string(), "Bearer unchanged".to_string()),
                    (
                        "x-shopify-access-token".to_string(),
                        "shpat_unchanged".to_string(),
                    ),
                    ("content-type".to_string(), "application/json".to_string()),
                    ("host".to_string(), "wrong-host".to_string()),
                    ("content-length".to_string(), "999".to_string()),
                    ("connection".to_string(), "keep-alive".to_string()),
                    ("transfer-encoding".to_string(), "chunked".to_string()),
                ]
                .into(),
                body: "{}".to_string(),
            })
            .expect("request should build");

        let headers = request.headers();
        assert_eq!(
            headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer unchanged")
        );
        assert_eq!(
            headers
                .get("x-shopify-access-token")
                .and_then(|value| value.to_str().ok()),
            Some("shpat_unchanged")
        );
        assert_eq!(
            headers
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        assert!(headers.get(HOST).is_none());
        assert!(headers.get(CONTENT_LENGTH).is_none());
        assert!(headers.get("connection").is_none());
        assert!(headers.get("transfer-encoding").is_none());
    }

    #[test]
    fn build_request_preserves_query_string_when_present() {
        let client = HttpUpstreamClient::new("https://example.myshopify.com/admin");
        let request = client
            .build_request(Request {
                method: "POST".to_string(),
                path: "/admin/api/2026-04/graphql.json?debug=true".to_string(),
                headers: BTreeMap::new(),
                body: "{}".to_string(),
            })
            .expect("request should build");

        assert_eq!(
            request.url().as_str(),
            "https://example.myshopify.com/admin/api/2026-04/graphql.json?debug=true"
        );
    }
}
