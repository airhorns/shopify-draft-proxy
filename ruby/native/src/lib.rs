use std::cell::RefCell;
use std::collections::BTreeMap;

use magnus::{
    function, method, prelude::*, r_hash::ForEach, wrap, Error, RArray, RHash, RModule, RString,
    Ruby, Value,
};
use serde_json::{json, Map, Number};
use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ReadMode, Request, UnsupportedMutationMode,
    DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES,
};
use shopify_draft_proxy::upstream::HttpUpstreamClient;

#[wrap(class = "ShopifyDraftProxy::DraftProxy")]
struct NativeDraftProxy(RefCell<DraftProxy>);

impl NativeDraftProxy {
    fn new(options: RHash) -> Result<Self, Error> {
        let config = config_from_options(options)?;
        let upstream_client = HttpUpstreamClient::new(config.shopify_admin_origin.clone());
        let commit_client = upstream_client.clone();
        let proxy = DraftProxy::new(config)
            .with_upstream_transport(move |request| upstream_client.send(request))
            .with_commit_transport(move |request| commit_client.send(request));
        let native = Self(RefCell::new(proxy));

        if let Some(state) = ruby_hash_get(options, "state")? {
            if !state.is_nil() {
                native.restore_state(state)?;
            }
        }

        Ok(native)
    }

    fn process_request(&self, request: RHash) -> Result<RHash, Error> {
        let response = self
            .0
            .borrow_mut()
            .process_request(request_from_hash(request)?);
        response_to_hash(response.status, response.headers, response.body)
    }

    fn process_graphql_request(&self, body: Value, options: RHash) -> Result<RHash, Error> {
        let api_version = ruby_hash_get(options, "api_version")?
            .and_then(value_to_optional_string)
            .unwrap_or_else(|| "2025-01".to_string());
        let path = ruby_hash_get(options, "path")?
            .and_then(value_to_optional_string)
            .unwrap_or_else(|| format!("/admin/api/{api_version}/graphql.json"));
        let headers = ruby_hash_get(options, "headers")?
            .map(headers_from_value)
            .transpose()?
            .unwrap_or_default();
        let request = Request {
            method: "POST".to_string(),
            path,
            headers: content_type_headers(headers),
            body: json_string_from_value(body)?,
        };
        let response = self.0.borrow_mut().process_request(request);
        response_to_hash(response.status, response.headers, response.body)
    }

    fn reset(&self) -> Result<RHash, Error> {
        self.process_request(request_hash("POST", "/__meta/reset", None)?)
    }

    fn get_config(&self) -> Result<Value, Error> {
        self.process_request(request_hash("GET", "/__meta/config", None)?)?
            .aref("body")
    }

    fn get_log(&self) -> Result<Value, Error> {
        self.process_request(request_hash("GET", "/__meta/log", None)?)?
            .aref("body")
    }

    fn get_state(&self) -> Result<Value, Error> {
        self.process_request(request_hash("GET", "/__meta/state", None)?)?
            .aref("body")
    }

    fn dump_state(&self, options: RHash) -> Result<Value, Error> {
        let body = ruby_hash_get(options, "created_at")?
            .and_then(value_to_optional_string)
            .map(|created_at| json!({ "createdAt": created_at }));
        self.process_request(request_hash("POST", "/__meta/dump", body)?)?
            .aref("body")
    }

    fn restore_state(&self, dump: Value) -> Result<RHash, Error> {
        let request = Request {
            method: "POST".to_string(),
            path: "/__meta/restore".to_string(),
            headers: [("content-type".to_string(), "application/json".to_string())].into(),
            body: json_string_from_value(dump)?,
        };
        let response = self.0.borrow_mut().process_request(request);
        let status = response.status;
        let result = response_to_hash(response.status, response.headers, response.body)?;
        if status != 200 {
            return Err(Error::new(
                Ruby::get_with(result).exception_runtime_error(),
                format!("DraftProxy.restore_state failed with status {status}"),
            ));
        }
        Ok(result)
    }

    fn commit(&self, options: RHash) -> Result<Value, Error> {
        let headers = ruby_hash_get(options, "headers")?
            .map(headers_from_value)
            .transpose()?
            .unwrap_or_default();
        let request = Request {
            method: "POST".to_string(),
            path: "/__meta/commit".to_string(),
            headers,
            body: String::new(),
        };
        let response = self.0.borrow_mut().process_request(request);
        let body = json_to_ruby(response.body)?;
        if response.status != 200 || !body.funcall::<_, _, bool>("dig", ("ok",))? {
            return Err(Error::new(
                Ruby::get_with(body).exception_runtime_error(),
                format!("DraftProxy.commit failed with status {}", response.status),
            ));
        }
        Ok(body)
    }
}

fn config_from_options(options: RHash) -> Result<Config, Error> {
    let read_mode = ruby_hash_get(options, "read_mode")?
        .and_then(value_to_optional_string)
        .map(|value| read_mode_from_string(&value))
        .unwrap_or(ReadMode::Snapshot);
    let unsupported_mutation_mode = ruby_hash_get(options, "unsupported_mutation_mode")?
        .and_then(value_to_optional_string)
        .map(|value| unsupported_mutation_mode_from_string(&value))
        .or(Some(UnsupportedMutationMode::Passthrough));
    let shopify_admin_origin = ruby_hash_get(options, "shopify_admin_origin")?
        .and_then(value_to_optional_string)
        .unwrap_or_else(|| "https://shopify.com".to_string());
    let snapshot_path = ruby_hash_get(options, "snapshot_path")?.and_then(value_to_optional_string);
    let port = ruby_hash_get(options, "port")?
        .and_then(value_to_optional_u64)
        .map(|value| value as u16)
        .unwrap_or(0);
    let max_size = ruby_hash_get(
        options,
        "bulk_operation_run_mutation_max_input_file_size_bytes",
    )?
    .and_then(value_to_optional_u64)
    .unwrap_or(DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES);

    Ok(Config {
        read_mode,
        unsupported_mutation_mode,
        bulk_operation_run_mutation_max_input_file_size_bytes: Some(max_size),
        port,
        shopify_admin_origin,
        snapshot_path,
    })
}

fn read_mode_from_string(value: &str) -> ReadMode {
    match value {
        "live-hybrid" => ReadMode::LiveHybrid,
        "passthrough" | "live" => ReadMode::Live,
        _ => ReadMode::Snapshot,
    }
}

fn unsupported_mutation_mode_from_string(value: &str) -> UnsupportedMutationMode {
    match value {
        "reject" => UnsupportedMutationMode::Reject,
        _ => UnsupportedMutationMode::Passthrough,
    }
}

fn request_from_hash(request: RHash) -> Result<Request, Error> {
    Ok(Request {
        method: ruby_hash_get(request, "method")?
            .and_then(value_to_optional_string)
            .unwrap_or_else(|| "GET".to_string()),
        path: ruby_hash_get(request, "path")?
            .and_then(value_to_optional_string)
            .unwrap_or_else(|| "/".to_string()),
        headers: ruby_hash_get(request, "headers")?
            .map(headers_from_value)
            .transpose()?
            .unwrap_or_default(),
        body: ruby_hash_get(request, "body")?
            .map(json_string_from_value)
            .transpose()?
            .unwrap_or_default(),
    })
}

fn request_hash(method: &str, path: &str, body: Option<serde_json::Value>) -> Result<RHash, Error> {
    let ruby = Ruby::get().expect("Ruby should be available on extension thread");
    let request = ruby.hash_new();
    request.aset("method", method)?;
    request.aset("path", path)?;
    if let Some(body) = body {
        request.aset("body", json_to_ruby(body)?)?;
    }
    Ok(request)
}

fn response_to_hash(
    status: u16,
    headers: BTreeMap<String, String>,
    body: serde_json::Value,
) -> Result<RHash, Error> {
    let ruby = Ruby::get().expect("Ruby should be available on extension thread");
    let hash = ruby.hash_new();
    hash.aset("status", status as i64)?;
    hash.aset("headers", json_to_ruby(json!(headers))?)?;
    hash.aset("body", json_to_ruby(body)?)?;
    Ok(hash)
}

fn content_type_headers(mut headers: BTreeMap<String, String>) -> BTreeMap<String, String> {
    if !headers
        .keys()
        .any(|key| key.eq_ignore_ascii_case("content-type"))
    {
        headers.insert("content-type".to_string(), "application/json".to_string());
    }
    headers
}

fn headers_from_value(value: Value) -> Result<BTreeMap<String, String>, Error> {
    let Some(hash) = RHash::from_value(value) else {
        return Ok(BTreeMap::new());
    };
    let mut headers = BTreeMap::new();
    hash.foreach(|key: Value, value: Value| {
        let Some(value) = value_to_optional_string(value) else {
            return Ok(ForEach::Continue);
        };
        headers.insert(value_to_string(key)?, value);
        Ok(ForEach::Continue)
    })?;
    Ok(headers)
}

fn ruby_hash_get(hash: RHash, key: &str) -> Result<Option<Value>, Error> {
    let ruby = Ruby::get_with(hash);
    let mut value: Value = hash.lookup(ruby.to_symbol(key))?;
    if value.is_nil() {
        value = hash.lookup(key)?;
    }
    Ok((!value.is_nil()).then_some(value))
}

fn value_to_optional_string(value: Value) -> Option<String> {
    if value.is_nil() {
        None
    } else {
        Some(value_to_string(value).ok()?)
    }
}

fn value_to_optional_u64(value: Value) -> Option<u64> {
    if value.is_nil() {
        None
    } else {
        value.funcall::<_, _, u64>("to_i", ()).ok()
    }
}

fn value_to_string(value: Value) -> Result<String, Error> {
    let string: RString = value.funcall("to_s", ())?;
    string.to_string()
}

fn json_string_from_value(value: Value) -> Result<String, Error> {
    if let Some(string) = value_to_optional_string(value) {
        if RString::from_value(value).is_some() {
            return Ok(string);
        }
    }
    let json_module: RModule = Ruby::get_with(value).eval("JSON")?;
    json_module.funcall("generate", (value,))
}

fn json_to_ruby(value: serde_json::Value) -> Result<Value, Error> {
    match value {
        serde_json::Value::Null => Ok(Ruby::get().unwrap().qnil().as_value()),
        serde_json::Value::Bool(value) => Ok(Ruby::get().unwrap().into_value(value)),
        serde_json::Value::Number(number) => Ok(number_to_ruby(number)),
        serde_json::Value::String(value) => Ok(Ruby::get().unwrap().str_new(&value).as_value()),
        serde_json::Value::Array(values) => {
            let ruby = Ruby::get().unwrap();
            let array = ruby.ary_new_capa(values.len());
            for value in values {
                array.push(json_to_ruby(value)?)?;
            }
            Ok(array.as_value())
        }
        serde_json::Value::Object(fields) => {
            let ruby = Ruby::get().unwrap();
            let hash = ruby.hash_new();
            for (key, value) in fields {
                hash.aset(key, json_to_ruby(value)?)?;
            }
            Ok(hash.as_value())
        }
    }
}

fn number_to_ruby(number: Number) -> Value {
    let ruby = Ruby::get().unwrap();
    if let Some(value) = number.as_i64() {
        ruby.into_value(value)
    } else if let Some(value) = number.as_u64() {
        ruby.into_value(value)
    } else {
        ruby.into_value(number.as_f64().unwrap_or_default())
    }
}

fn ruby_to_json(value: Value) -> Result<serde_json::Value, Error> {
    if value.is_nil() {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(boolean) =
        value.funcall::<_, _, bool>("is_a?", (Ruby::get_with(value).class_true_class(),))
    {
        if boolean {
            return Ok(serde_json::Value::Bool(true));
        }
    }
    if let Ok(boolean) =
        value.funcall::<_, _, bool>("is_a?", (Ruby::get_with(value).class_false_class(),))
    {
        if boolean {
            return Ok(serde_json::Value::Bool(false));
        }
    }
    if let Some(hash) = RHash::from_value(value) {
        let mut fields = Map::new();
        hash.foreach(|key: Value, value: Value| {
            fields.insert(value_to_string(key)?, ruby_to_json(value)?);
            Ok(ForEach::Continue)
        })?;
        return Ok(serde_json::Value::Object(fields));
    }
    if let Some(array) = RArray::from_value(value) {
        let mut values = Vec::with_capacity(array.len());
        for index in 0..array.len() {
            values.push(ruby_to_json(array.entry(index as isize)?)?);
        }
        return Ok(serde_json::Value::Array(values));
    }
    if let Ok(integer) = value.funcall::<_, _, i64>("to_int", ()) {
        return Ok(serde_json::Value::Number(integer.into()));
    }
    if let Ok(float) = value.funcall::<_, _, f64>("to_f", ()) {
        if let Some(number) = Number::from_f64(float) {
            return Ok(serde_json::Value::Number(number));
        }
    }
    Ok(serde_json::Value::String(value_to_string(value)?))
}

fn parse_json_string(value: Value) -> Result<Value, Error> {
    let json_module: RModule = Ruby::get_with(value).eval("JSON")?;
    json_module.funcall("parse", (value,))
}

fn native_json_round_trip(value: Value) -> Result<Value, Error> {
    json_to_ruby(ruby_to_json(value)?)
}

#[magnus::init(name = "shopify_draft_proxy_native")]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("ShopifyDraftProxy")?;
    let class = module.define_class("DraftProxy", ruby.class_object())?;
    class.define_singleton_method("new", function!(NativeDraftProxy::new, 1))?;
    class.define_method(
        "process_request",
        method!(NativeDraftProxy::process_request, 1),
    )?;
    class.define_method(
        "process_graphql_request",
        method!(NativeDraftProxy::process_graphql_request, 2),
    )?;
    class.define_method("reset", method!(NativeDraftProxy::reset, 0))?;
    class.define_method("get_config", method!(NativeDraftProxy::get_config, 0))?;
    class.define_method("get_log", method!(NativeDraftProxy::get_log, 0))?;
    class.define_method("get_state", method!(NativeDraftProxy::get_state, 0))?;
    class.define_method("dump_state", method!(NativeDraftProxy::dump_state, 1))?;
    class.define_method("restore_state", method!(NativeDraftProxy::restore_state, 1))?;
    class.define_method("commit", method!(NativeDraftProxy::commit, 1))?;
    module.define_module_function("parse_json_string", function!(parse_json_string, 1))?;
    module.define_module_function(
        "native_json_round_trip",
        function!(native_json_round_trip, 1),
    )?;
    Ok(())
}
