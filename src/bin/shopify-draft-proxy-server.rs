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

    let mut proxy = DraftProxy::new(config);
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

fn config_from_env() -> Config {
    Config {
        read_mode: read_mode_from_env(),
        unsupported_mutation_mode: Some(unsupported_mutation_mode_from_env()),
        bulk_operation_run_mutation_max_input_file_size_bytes: env::var(
            "BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES",
        )
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .or(Some(104_857_600)),
        port: env::var("PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(3000),
        shopify_admin_origin: env::var("SHOPIFY_ADMIN_ORIGIN")
            .unwrap_or_else(|_| "https://shopify.com".to_string()),
        snapshot_path: env::var("SNAPSHOT_PATH")
            .ok()
            .filter(|value| !value.is_empty()),
    }
}

fn read_mode_from_env() -> ReadMode {
    match env::var("READ_MODE")
        .unwrap_or_else(|_| "snapshot".to_string())
        .as_str()
    {
        "live-hybrid" => ReadMode::LiveHybrid,
        "passthrough" | "live" => ReadMode::Live,
        _ => ReadMode::Snapshot,
    }
}

fn unsupported_mutation_mode_from_env() -> UnsupportedMutationMode {
    match env::var("UNSUPPORTED_MUTATION_MODE")
        .unwrap_or_else(|_| "passthrough".to_string())
        .as_str()
    {
        "reject" => UnsupportedMutationMode::Reject,
        _ => UnsupportedMutationMode::Passthrough,
    }
}

fn handle_connection(mut stream: TcpStream, proxy: &mut DraftProxy) -> std::io::Result<()> {
    let request = read_http_request(&mut stream)?;
    let response = proxy.process_request(request);
    write_http_response(&mut stream, response)
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
