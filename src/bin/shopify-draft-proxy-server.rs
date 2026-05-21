use std::collections::BTreeMap;
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use serde_json::Value;
use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ReadMode, Request, Response, UnsupportedMutationMode,
};

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

    let passthrough_origin = upstream_origin.clone();
    let commit_origin = upstream_origin;
    let mut proxy = DraftProxy::new(config)
        .with_upstream_transport(move |request| send_upstream_request(&passthrough_origin, request))
        .with_commit_transport(move |request| send_upstream_request(&commit_origin, request));
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

fn send_upstream_request(origin: &str, request: Request) -> Response {
    match send_upstream_request_result(origin, request) {
        Ok(response) => response,
        Err(error) => Response {
            status: 502,
            headers: BTreeMap::new(),
            body: serde_json::json!({ "errors": [{ "message": format!("upstream network error: {error}") }] }),
        },
    }
}

fn send_upstream_request_result(origin: &str, request: Request) -> std::io::Result<Response> {
    let target = parse_http_origin(origin)?;
    let mut stream = TcpStream::connect((&*target.host, target.port))?;
    let body = request.body;
    let mut request_text = format!(
        "{} {} HTTP/1.1\r\nhost: {}\r\ncontent-length: {}\r\nconnection: close\r\n",
        request.method,
        request.path,
        target.host_header,
        body.as_bytes().len()
    );
    for (name, value) in request.headers {
        if name.eq_ignore_ascii_case("host")
            || name.eq_ignore_ascii_case("content-length")
            || name.eq_ignore_ascii_case("connection")
        {
            continue;
        }
        request_text.push_str(&format!("{name}: {value}\r\n"));
    }
    request_text.push_str("\r\n");
    request_text.push_str(&body);
    stream.write_all(request_text.as_bytes())?;
    stream.flush()?;
    read_http_response(&mut stream)
}

struct HttpOrigin {
    host: String,
    port: u16,
    host_header: String,
}

fn parse_http_origin(origin: &str) -> std::io::Result<HttpOrigin> {
    let Some(rest) = origin.strip_prefix("http://") else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Rust stdlib transport currently supports http:// upstream origins",
        ));
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse::<u16>().unwrap_or(80)),
        None => (authority.to_string(), 80),
    };
    Ok(HttpOrigin {
        host: host.clone(),
        port,
        host_header: authority.to_string(),
    })
}

fn read_http_response(stream: &mut TcpStream) -> std::io::Result<Response> {
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(502);
    let mut headers = BTreeMap::new();
    let mut content_length = None;
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
                content_length = value.parse::<usize>().ok();
            }
            headers.insert(key, value);
        }
    }
    let mut body = Vec::new();
    match content_length {
        Some(length) => {
            body.resize(length, 0);
            reader.read_exact(&mut body)?;
        }
        None => {
            reader.read_to_end(&mut body)?;
        }
    }
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
    {
        body = decode_chunked_body(&body)?;
        headers.remove("transfer-encoding");
    }
    let text = String::from_utf8_lossy(&body);
    let body =
        serde_json::from_str::<Value>(&text).unwrap_or_else(|_| Value::String(text.to_string()));
    Ok(Response {
        status,
        headers,
        body,
    })
}

fn decode_chunked_body(body: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut decoded = Vec::new();
    let mut offset = 0usize;
    loop {
        let Some(line_end) = body[offset..]
            .windows(2)
            .position(|window| window == b"\r\n")
        else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "malformed chunked upstream body",
            ));
        };
        let size_text = String::from_utf8_lossy(&body[offset..offset + line_end]);
        let size_hex = size_text.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_hex, 16).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid chunk size: {error}"),
            )
        })?;
        offset += line_end + 2;
        if size == 0 {
            break;
        }
        if offset + size > body.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "chunked upstream body ended early",
            ));
        }
        decoded.extend_from_slice(&body[offset..offset + size]);
        offset += size + 2;
    }
    Ok(decoded)
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
