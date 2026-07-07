use std::cell::RefCell;
use std::collections::BTreeMap;

use magnus::{
    function, method, prelude::*, r_hash::ForEach, value::Opaque, wrap, Error, RArray, RHash,
    RModule, RString, Ruby, Value,
};
use serde_json::{json, Map, Number};
use shopify_draft_proxy::proxy::{
    Config, DraftProxy, ReadMode, Request, Response, UnsupportedMutationMode,
    DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES,
};
use shopify_draft_proxy::upstream::{
    parse_upstream_body, prepare_upstream_request, upstream_error_response,
};

#[wrap(class = "ShopifyDraftProxy::DraftProxy")]
struct NativeDraftProxy(RefCell<DraftProxy>);

/// Bridges the Rust commit/upstream transport seam back into a host-language
/// (Ruby) callable. The actual HTTP request is performed in Ruby — typically by
/// `Net::HTTP` in the default transport — which means Ruby releases the GVL
/// during socket IO and Ruby-level instrumentation (tracing, mocking) sees the
/// request. The embedding app can supply its own callable instead.
///
/// `Opaque<Value>` makes the callable `Send + Sync` so it satisfies the
/// transport bound; it is only ever dereferenced via `Ruby::get()` on the
/// GVL-holding thread that drives the proxy, and it is GC-pinned for the
/// process lifetime when the proxy is constructed.
#[derive(Clone)]
struct RubyTransport {
    origin: String,
    callable: Opaque<Value>,
}

impl RubyTransport {
    fn call(&self, request: Request) -> Response {
        match self.invoke(request) {
            Ok(response) => response,
            Err(message) => upstream_error_response(&message),
        }
    }

    fn invoke(&self, request: Request) -> Result<Response, String> {
        let prepared =
            prepare_upstream_request(&self.origin, request).map_err(|error| error.to_string())?;
        let ruby = Ruby::get().map_err(|_| "transport invoked without the GVL".to_string())?;
        let callable = ruby.get_inner(self.callable);

        let request_hash = ruby.hash_new();
        let assign = |key: &str, value: Value| -> Result<(), String> {
            request_hash
                .aset(key, value)
                .map_err(|error| error.to_string())
        };
        assign("method", ruby.str_new(&prepared.method).as_value())?;
        assign("url", ruby.str_new(&prepared.url).as_value())?;
        let headers = ruby.hash_new();
        for (name, value) in &prepared.headers {
            headers
                .aset(name.as_str(), value.as_str())
                .map_err(|error| error.to_string())?;
        }
        assign("headers", headers.as_value())?;
        assign("body", ruby.str_new(&prepared.body).as_value())?;

        let result: Value = callable
            .funcall("call", (request_hash,))
            .map_err(|error| error.to_string())?;
        response_from_ruby(result)
    }
}

fn response_from_ruby(value: Value) -> Result<Response, String> {
    let hash = RHash::from_value(value)
        .ok_or_else(|| "transport must return a Hash with :status and :body".to_string())?;
    let status = ruby_hash_get(hash, "status")
        .map_err(|error| error.to_string())?
        .and_then(value_to_optional_u64)
        .ok_or_else(|| "transport response is missing an integer :status".to_string())?
        as u16;
    let headers = ruby_hash_get(hash, "headers")
        .map_err(|error| error.to_string())?
        .map(headers_from_value)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let body = match ruby_hash_get(hash, "body").map_err(|error| error.to_string())? {
        None => serde_json::Value::Null,
        Some(value) => match RString::from_value(value) {
            Some(string) => parse_upstream_body(string.to_string().map_err(|e| e.to_string())?),
            None => ruby_to_json(value).map_err(|error| error.to_string())?,
        },
    };
    Ok(Response {
        status,
        headers,
        body,
    })
}

impl NativeDraftProxy {
    fn new(options: RHash) -> Result<Self, Error> {
        let ruby = Ruby::get().expect("Ruby should be available on extension thread");
        let config = config_from_options(options)?;
        let origin = config.shopify_admin_origin.clone();

        let callable = ruby_hash_get(options, "transport")?.filter(|value| !value.is_nil());
        let callable = callable.ok_or_else(|| {
            Error::new(
                ruby.exception_arg_error(),
                "ShopifyDraftProxy requires a `transport` callable (responding to #call)",
            )
        })?;
        // Pin the callable so the transport closure can resurrect it across GC
        // cycles; the closure only stores a Send+Sync Opaque handle to it.
        magnus::gc::register_mark_object(callable);

        let transport = RubyTransport {
            origin,
            callable: Opaque::from(callable),
        };
        let commit_transport = transport.clone();
        let proxy = DraftProxy::new(config)
            .with_upstream_transport(move |request| transport.call(request))
            .with_commit_transport(move |request| commit_transport.call(request));
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
        // The core always answers with a well-formed commit result body — `ok:
        // true` (HTTP 200) on full success, or an inspectable `ok: false` (HTTP
        // 502) carrying `committed`/`failed`/`stopIndex`/`attempts`/`error` when a
        // staged mutation's replay fails. Return that body verbatim rather than
        // raising on the non-2xx result: the Ruby wrapper decides success vs
        // failure from `ok` and raises a typed `CommitError` that keeps the full
        // result, so the upstream cause is never discarded at the binding (this
        // is what the JS binding does in `runtime.ts`; the old Ruby binding threw
        // the detail away).
        let response = self.0.borrow_mut().process_request(request);
        json_to_ruby(response.body)
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
    // Mark this extension Ractor-safe so a DraftProxy can be constructed and driven
    // from a non-main Ractor: each proxy instance owns its state (RefCell<DraftProxy>)
    // and lives entirely within one Ractor, so there is no shared mutable state to
    // guard. Without this, magnus defines its C methods Ractor-unsafe and any native
    // call from a worker Ractor raises Ractor::UnsafeError. magnus 0.8 does not wrap
    // rb_ext_ractor_safe, so call the raw C API via rb-sys. It must run before any
    // define_method below, since the flag applies to methods defined afterward.
    unsafe { rb_sys::rb_ext_ractor_safe(true) };
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
