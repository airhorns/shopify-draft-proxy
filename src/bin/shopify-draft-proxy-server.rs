use std::collections::BTreeMap;
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use serde_json::Value;
use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ReadMode, Request, Response, UnsupportedMutationMode,
};
use shopify_draft_proxy::upstream::HttpUpstreamClient;

fn main() -> std::io::Result<()> {
    let config = config_from_env();
    let upstream_origin = config.shopify_admin_origin.clone();
    let bind_addr = format!("127.0.0.1:{}", config.port);
    let listener = TcpListener::bind(&bind_addr)?;
    println!(
        "{}",
        serde_json::json!({
            "level": "info",
            "msg": "shopify-draft-proxy rust runtime listening",
            "port": config.port,
            "url": format!("http://localhost:{}", config.port)
        })
    );

    let upstream_client = HttpUpstreamClient::new(upstream_origin);
    let commit_client = upstream_client.clone();
    let mut proxy = DraftProxy::new(config)
        .with_upstream_transport(move |request| upstream_client.send(request))
        .with_commit_transport(move |request| commit_client.send(request));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_connection(stream, &mut proxy) {
                    eprintln!(
                        "{}",
                        serde_json::json!({
                            "level": "error",
                            "msg": "failed to handle request",
                            "error": error.to_string()
                        })
                    );
                }
            }
            Err(error) => eprintln!(
                "{}",
                serde_json::json!({
                    "level": "error",
                    "msg": "failed to accept connection",
                    "error": error.to_string()
                })
            ),
        }
    }

    Ok(())
}

fn env_value(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| env::var(name).ok())
}

fn config_from_env() -> Config {
    Config {
        read_mode: read_mode_from_env(),
        unsupported_mutation_mode: Some(unsupported_mutation_mode_from_env()),
        bulk_operation_run_mutation_max_input_file_size_bytes: env_value(&[
            "BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES",
            "SHOPIFY_DRAFT_PROXY_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES",
        ])
        .and_then(|value| value.parse::<u64>().ok())
        .or(Some(104_857_600)),
        port: env_value(&["PORT"])
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(3000),
        shopify_admin_origin: env_value(&["SHOPIFY_ADMIN_ORIGIN"])
            .unwrap_or_else(|| "https://shopify.com".to_string()),
        snapshot_path: env_value(&["SNAPSHOT_PATH", "SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH"])
            .filter(|value| !value.is_empty()),
    }
}

fn read_mode_from_env() -> ReadMode {
    match env_value(&["READ_MODE", "SHOPIFY_DRAFT_PROXY_READ_MODE"])
        .unwrap_or_else(|| "snapshot".to_string())
        .as_str()
    {
        "live-hybrid" => ReadMode::LiveHybrid,
        "passthrough" | "live" => ReadMode::Live,
        _ => ReadMode::Snapshot,
    }
}

fn unsupported_mutation_mode_from_env() -> UnsupportedMutationMode {
    match env_value(&[
        "UNSUPPORTED_MUTATION_MODE",
        "SHOPIFY_DRAFT_PROXY_UNSUPPORTED_MUTATION_MODE",
    ])
    .unwrap_or_else(|| "passthrough".to_string())
    .as_str()
    {
        "reject" => UnsupportedMutationMode::Reject,
        _ => UnsupportedMutationMode::Passthrough,
    }
}

fn handle_connection(mut stream: TcpStream, proxy: &mut DraftProxy) -> std::io::Result<()> {
    let request = read_http_request(&mut stream)?;
    let response = match staged_upload_response(&request) {
        Some(response) => response,
        None => proxy.process_request(request),
    };
    write_http_response(&mut stream, response)
}

fn staged_upload_response(request: &Request) -> Option<Response> {
    let rest = request.path.strip_prefix("/staged-uploads/")?;
    let (encoded_target_id, encoded_filename) = rest.split_once('/')?;
    let method = request.method.to_ascii_uppercase();
    if method != "POST" && method != "PUT" {
        return Some(Response {
            status: 405,
            headers: BTreeMap::new(),
            body: serde_json::json!({ "errors": [{ "message": "Method not allowed" }] }),
        });
    }
    let target_id = percent_decode(encoded_target_id);
    let filename = percent_decode(encoded_filename);
    Some(Response {
        status: 201,
        headers: BTreeMap::new(),
        body: serde_json::json!({
            "ok": true,
            "key": format!("shopify-draft-proxy/{target_id}/{filename}")
        }),
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut offset = 0usize;
    while offset < bytes.len() {
        if bytes[offset] == b'%' && offset + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[offset + 1..offset + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    decoded.push(byte);
                    offset += 3;
                    continue;
                }
            }
        }
        decoded.push(bytes[offset]);
        offset += 1;
    }
    String::from_utf8_lossy(&decoded).to_string()
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/").to_string();

    let mut headers = BTreeMap::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let key = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if key == "content-length" {
                content_length = value.parse::<usize>().unwrap_or(0);
            }
            headers.insert(key, value);
        }
    }

    let mut body = vec![0_u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok(Request {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn write_http_response(stream: &mut TcpStream, response: Response) -> std::io::Result<()> {
    let body = body_to_bytes(&response.body);
    let reason = reason_phrase(response.status);
    write!(stream, "HTTP/1.1 {} {}\r\n", response.status, reason)?;
    let mut has_content_type = false;
    for (name, value) in response.headers {
        if name.eq_ignore_ascii_case("content-length")
            || name.eq_ignore_ascii_case("connection")
            || name.eq_ignore_ascii_case("transfer-encoding")
        {
            continue;
        }
        if name.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        write!(stream, "{}: {}\r\n", name, value)?;
    }
    if !has_content_type {
        write!(stream, "content-type: application/json; charset=utf-8\r\n")?;
    }
    write!(stream, "content-length: {}\r\n", body.len())?;
    write!(stream, "connection: close\r\n\r\n")?;
    stream.write_all(&body)?;
    stream.flush()
}

fn body_to_bytes(body: &Value) -> Vec<u8> {
    match body {
        Value::String(value) => value.as_bytes().to_vec(),
        other => serde_json::to_vec(other).unwrap_or_else(|_| b"null".to_vec()),
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        _ => "OK",
    }
}
