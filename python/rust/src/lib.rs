#![allow(clippy::useless_conversion)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};
use serde_json::{json, Map, Value};
use shopify_draft_proxy::proxy::{
    Config, DraftProxy as RustDraftProxy, ReadMode, Request, Response, UnsupportedMutationMode,
};
use shopify_draft_proxy::upstream::{
    parse_upstream_body, prepare_upstream_request, upstream_error_response,
};

const DEFAULT_API_VERSION: &str = "2025-01";
const STATE_DUMP_SCHEMA: &str = "shopify-draft-proxy-rust-state/v1";

#[pyclass(name = "DraftProxy")]
struct PyDraftProxy {
    inner: Mutex<RustDraftProxy>,
}

/// Bridges the Rust commit/upstream transport seam into a host-language (Python)
/// callable. The actual HTTP request runs in Python — typically `urllib` in the
/// default transport — so Python releases the GIL during socket IO and
/// Python-level instrumentation (OpenTelemetry, responses, requests-mock, ...)
/// observes the request. Embedders can supply their own callable.
#[derive(Clone)]
struct PyTransport {
    origin: String,
    callable: Arc<Py<PyAny>>,
}

impl PyTransport {
    fn call(&self, request: Request) -> Response {
        Python::with_gil(|py| match self.invoke(py, request) {
            Ok(response) => response,
            Err(error) => upstream_error_response(&error.to_string()),
        })
    }

    fn invoke(&self, py: Python<'_>, request: Request) -> PyResult<Response> {
        let prepared = prepare_upstream_request(&self.origin, request)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let dict = PyDict::new_bound(py);
        dict.set_item("method", &prepared.method)?;
        dict.set_item("url", &prepared.url)?;
        let headers = PyDict::new_bound(py);
        for (name, value) in &prepared.headers {
            headers.set_item(name, value)?;
        }
        dict.set_item("headers", headers)?;
        dict.set_item("body", &prepared.body)?;

        let result = self.callable.bind(py).call1((dict,))?;
        response_from_py(&result)
    }
}

fn response_from_py(value: &Bound<'_, PyAny>) -> PyResult<Response> {
    let status: u16 = value
        .get_item("status")
        .map_err(|_| PyRuntimeError::new_err("transport response is missing 'status'"))?
        .extract()?;
    let headers = match value.get_item("headers") {
        Ok(headers) if !headers.is_none() => {
            let dict = headers
                .downcast::<PyDict>()
                .map_err(|_| PyRuntimeError::new_err("transport 'headers' must be a dict"))?;
            py_headers_to_map(Some(dict))?
        }
        _ => BTreeMap::new(),
    };
    let body = match value.get_item("body") {
        Ok(body) if !body.is_none() => match body.extract::<String>() {
            Ok(text) => parse_upstream_body(text),
            Err(_) => py_to_json(&body)?,
        },
        _ => Value::Null,
    };
    Ok(Response {
        status,
        headers,
        body,
    })
}

#[pymethods]
impl PyDraftProxy {
    #[new]
    #[pyo3(signature = (read_mode = "snapshot", shopify_admin_origin = "https://shopify.com", port = 4000, snapshot_path = None, unsupported_mutation_mode = "passthrough", state = None, transport = None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        read_mode: &str,
        shopify_admin_origin: &str,
        port: u16,
        snapshot_path: Option<String>,
        unsupported_mutation_mode: &str,
        state: Option<&Bound<'_, PyAny>>,
        transport: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let config = Config {
            read_mode: parse_read_mode(read_mode)?,
            unsupported_mutation_mode: Some(parse_unsupported_mutation_mode(
                unsupported_mutation_mode,
            )?),
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port,
            shopify_admin_origin: shopify_admin_origin.to_string(),
            snapshot_path,
        };
        let callable = resolve_transport(py, transport)?;
        let transport = PyTransport {
            origin: config.shopify_admin_origin.clone(),
            callable: Arc::new(callable),
        };
        let commit_transport = transport.clone();
        let mut proxy = RustDraftProxy::new(config)
            .with_upstream_transport(move |request| transport.call(request))
            .with_commit_transport(move |request| commit_transport.call(request));
        if let Some(state) = state {
            restore_native_state(&mut proxy, state)?;
        }
        Ok(Self {
            inner: Mutex::new(proxy),
        })
    }

    #[pyo3(signature = (method, path, body = None, headers = None))]
    fn process_request(
        &self,
        py: Python<'_>,
        method: &str,
        path: &str,
        body: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let request = Request {
            method: method.to_string(),
            path: path.to_string(),
            headers: py_headers_to_map(headers)?,
            body: py_body_to_string(body)?,
        };
        let response = self.with_proxy(|proxy| proxy.process_request(request))?;
        response_to_py(py, response)
    }

    #[pyo3(signature = (body, *, api_version = DEFAULT_API_VERSION, path = None, headers = None))]
    fn process_graphql_request(
        &self,
        py: Python<'_>,
        body: &Bound<'_, PyAny>,
        api_version: &str,
        path: Option<String>,
        headers: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let path = path.unwrap_or_else(|| format!("/admin/api/{api_version}/graphql.json"));
        let request = Request {
            method: "POST".to_string(),
            path,
            headers: content_type_headers(py_headers_to_map(headers)?),
            body: py_body_to_string(Some(body))?,
        };
        let response = self.with_proxy(|proxy| proxy.process_request(request))?;
        response_to_py(py, response)
    }

    fn get_config(&self, py: Python<'_>) -> PyResult<PyObject> {
        let value = self.with_proxy(|proxy| proxy.get_config_snapshot())?;
        value_to_py(py, &value)
    }

    fn get_log(&self, py: Python<'_>) -> PyResult<PyObject> {
        let value = self.with_proxy(|proxy| proxy.get_log_snapshot())?;
        value_to_py(py, &value)
    }

    fn get_state(&self, py: Python<'_>) -> PyResult<PyObject> {
        let value = self.with_proxy(|proxy| proxy.get_state_snapshot())?;
        value_to_py(py, &value)
    }

    #[pyo3(signature = (created_at = None))]
    fn dump_state(&self, py: Python<'_>, created_at: Option<String>) -> PyResult<PyObject> {
        let request = Request {
            method: "POST".to_string(),
            path: "/__meta/dump".to_string(),
            headers: BTreeMap::new(),
            body: json!({ "createdAt": created_at }).to_string(),
        };
        let response = self.with_proxy(|proxy| proxy.process_request(request))?;
        if response.status != 200 {
            return Err(PyRuntimeError::new_err(format!(
                "DraftProxy.dump_state failed with status {}: {}",
                response.status, response.body
            )));
        }
        value_to_py(py, &response.body)
    }

    fn restore_state(&self, state: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = py_to_json(state)?;
        let request = Request {
            method: "POST".to_string(),
            path: "/__meta/restore".to_string(),
            headers: BTreeMap::new(),
            body: value.to_string(),
        };
        let response = self.with_proxy(|proxy| proxy.process_request(request))?;
        if response.status != 200 {
            return Err(PyValueError::new_err(format!(
                "DraftProxy.restore_state failed with status {}: {}",
                response.status, response.body
            )));
        }
        Ok(())
    }

    fn reset(&self) -> PyResult<()> {
        let request = Request {
            method: "POST".to_string(),
            path: "/__meta/reset".to_string(),
            headers: BTreeMap::new(),
            body: String::new(),
        };
        let response = self.with_proxy(|proxy| proxy.process_request(request))?;
        if response.status != 200 {
            return Err(PyRuntimeError::new_err(format!(
                "DraftProxy.reset failed with status {}: {}",
                response.status, response.body
            )));
        }
        Ok(())
    }

    #[pyo3(signature = (headers = None))]
    fn commit(&self, py: Python<'_>, headers: Option<&Bound<'_, PyDict>>) -> PyResult<PyObject> {
        let request = Request {
            method: "POST".to_string(),
            path: "/__meta/commit".to_string(),
            headers: py_headers_to_map(headers)?,
            body: String::new(),
        };
        let response = self.with_proxy(|proxy| proxy.process_request(request))?;
        let status = response.status;
        let body = response.body;
        if status != 200 || !body.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return Err(PyRuntimeError::new_err(format!(
                "DraftProxy.commit failed with status {status}: {body}"
            )));
        }
        value_to_py(py, &body)
    }

    fn dispose(&self) {}

    fn origin(&self) -> Option<String> {
        None
    }
}

impl PyDraftProxy {
    fn with_proxy<T>(&self, f: impl FnOnce(&mut RustDraftProxy) -> T) -> PyResult<T> {
        let mut proxy = self
            .inner
            .lock()
            .map_err(|_| PyRuntimeError::new_err("DraftProxy lock was poisoned"))?;
        Ok(f(&mut proxy))
    }
}

#[pyfunction]
#[pyo3(signature = (**kwargs))]
fn create_draft_proxy(
    py: Python<'_>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyDraftProxy> {
    let read_mode = optional_string(kwargs, "read_mode")?.unwrap_or_else(|| "snapshot".to_string());
    let shopify_admin_origin = optional_string(kwargs, "shopify_admin_origin")?
        .unwrap_or_else(|| "https://shopify.com".to_string());
    let port = optional_u16(kwargs, "port")?.unwrap_or(4000);
    let snapshot_path = optional_string(kwargs, "snapshot_path")?;
    let unsupported_mutation_mode = optional_string(kwargs, "unsupported_mutation_mode")?
        .unwrap_or_else(|| "passthrough".to_string());
    let state = kwargs
        .and_then(|dict| dict.get_item("state").transpose())
        .transpose()?;
    let transport = kwargs
        .and_then(|dict| dict.get_item("transport").transpose())
        .transpose()?;

    PyDraftProxy::new(
        py,
        &read_mode,
        &shopify_admin_origin,
        port,
        snapshot_path,
        &unsupported_mutation_mode,
        state.as_ref(),
        transport.as_ref(),
    )
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyDraftProxy>()?;
    module.add_function(wrap_pyfunction!(create_draft_proxy, module)?)?;
    module.add("DRAFT_PROXY_STATE_DUMP_SCHEMA", STATE_DUMP_SCHEMA)?;
    Ok(())
}

fn restore_native_state(proxy: &mut RustDraftProxy, state: &Bound<'_, PyAny>) -> PyResult<()> {
    let value = py_to_json(state)?;
    let response = proxy.process_request(Request {
        method: "POST".to_string(),
        path: "/__meta/restore".to_string(),
        headers: BTreeMap::new(),
        body: value.to_string(),
    });
    if response.status != 200 {
        return Err(PyValueError::new_err(format!(
            "DraftProxy state restore failed with status {}: {}",
            response.status, response.body
        )));
    }
    Ok(())
}

/// Resolve the transport callable to install. When the embedder passes one we
/// use it verbatim; otherwise we fall back to the stdlib `urllib` transport
/// exported from the Python package.
fn resolve_transport(py: Python<'_>, transport: Option<&Bound<'_, PyAny>>) -> PyResult<Py<PyAny>> {
    match transport {
        Some(callable) if !callable.is_none() => Ok(callable.clone().unbind()),
        _ => default_transport(py),
    }
}

fn default_transport(py: Python<'_>) -> PyResult<Py<PyAny>> {
    Ok(py
        .import_bound("shopify_draft_proxy")?
        .getattr("default_http_transport")?
        .unbind())
}

fn parse_read_mode(value: &str) -> PyResult<ReadMode> {
    match value {
        "snapshot" => Ok(ReadMode::Snapshot),
        "live-hybrid" | "live_hybrid" => Ok(ReadMode::LiveHybrid),
        "passthrough" | "live" => Ok(ReadMode::Live),
        _ => Err(PyValueError::new_err(format!(
            "Unsupported read_mode {value:?}; expected snapshot, live-hybrid, or passthrough"
        ))),
    }
}

fn parse_unsupported_mutation_mode(value: &str) -> PyResult<UnsupportedMutationMode> {
    match value {
        "passthrough" => Ok(UnsupportedMutationMode::Passthrough),
        "reject" => Ok(UnsupportedMutationMode::Reject),
        _ => Err(PyValueError::new_err(format!(
            "Unsupported unsupported_mutation_mode {value:?}; expected passthrough or reject"
        ))),
    }
}

fn optional_string(kwargs: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<Option<String>> {
    kwargs
        .and_then(|dict| dict.get_item(key).transpose())
        .transpose()?
        .map(|value| value.extract::<String>())
        .transpose()
}

fn optional_u16(kwargs: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<Option<u16>> {
    kwargs
        .and_then(|dict| dict.get_item(key).transpose())
        .transpose()?
        .map(|value| value.extract::<u16>())
        .transpose()
}

fn py_headers_to_map(headers: Option<&Bound<'_, PyDict>>) -> PyResult<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let Some(headers) = headers else {
        return Ok(out);
    };
    for item in headers.iter() {
        let key: String = item.0.extract()?;
        let value: String = item.1.extract()?;
        out.insert(key, value);
    }
    Ok(out)
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

fn py_body_to_string(body: Option<&Bound<'_, PyAny>>) -> PyResult<String> {
    match body {
        None => Ok(String::new()),
        Some(value) => {
            if let Ok(text) = value.extract::<String>() {
                Ok(text)
            } else {
                Ok(py_to_json(value)?.to_string())
            }
        }
    }
}

fn response_to_py(py: Python<'_>, response: Response) -> PyResult<PyObject> {
    let dict = PyDict::new_bound(py);
    dict.set_item("status", response.status)?;
    if !response.headers.is_empty() {
        let headers = PyDict::new_bound(py);
        for (key, value) in response.headers {
            headers.set_item(key, value)?;
        }
        dict.set_item("headers", headers)?;
    }
    dict.set_item("body", value_to_py(py, &response.body)?)?;
    Ok(dict.into())
}

fn value_to_py(py: Python<'_>, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(value) => Ok(value.into_py(py)),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(value.into_py(py))
            } else if let Some(value) = number.as_u64() {
                Ok(value.into_py(py))
            } else if let Some(value) = number.as_f64() {
                Ok(value.into_py(py))
            } else {
                Ok(py.None())
            }
        }
        Value::String(value) => Ok(value.into_py(py)),
        Value::Array(values) => {
            let list = PyList::empty_bound(py);
            for value in values {
                list.append(value_to_py(py, value)?)?;
            }
            Ok(list.into())
        }
        Value::Object(fields) => {
            let dict = PyDict::new_bound(py);
            for (key, value) in fields {
                dict.set_item(key, value_to_py(py, value)?)?;
            }
            Ok(dict.into())
        }
    }
}

fn py_to_json(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_none() {
        Ok(Value::Null)
    } else if let Ok(value) = value.extract::<bool>() {
        Ok(Value::Bool(value))
    } else if let Ok(value) = value.extract::<i64>() {
        Ok(Value::Number(value.into()))
    } else if let Ok(value) = value.extract::<u64>() {
        Ok(Value::Number(value.into()))
    } else if let Ok(value) = value.extract::<f64>() {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| PyValueError::new_err("Cannot serialize non-finite float to JSON"))
    } else if let Ok(value) = value.extract::<String>() {
        Ok(Value::String(value))
    } else if let Ok(dict) = value.downcast::<PyDict>() {
        let mut out = Map::new();
        for item in dict.iter() {
            let key: String = item.0.extract()?;
            out.insert(key, py_to_json(&item.1)?);
        }
        Ok(Value::Object(out))
    } else if let Ok(sequence) = value.extract::<Vec<Bound<'_, PyAny>>>() {
        sequence
            .iter()
            .map(py_to_json)
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array)
    } else {
        Err(PyValueError::new_err(format!(
            "Cannot convert Python value of type {} to JSON",
            value.get_type().name()?
        )))
    }
}
