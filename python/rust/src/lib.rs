#![allow(clippy::useless_conversion)]

use std::collections::BTreeMap;
use std::sync::Mutex;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};
use serde_json::{json, Map, Value};
use shopify_draft_proxy::proxy::{
    Config, DraftProxy as RustDraftProxy, ReadMode, Request, Response, UnsupportedMutationMode,
};

const DEFAULT_GRAPHQL_PATH: &str = "/admin/api/2025-01/graphql.json";
const STATE_DUMP_SCHEMA: &str = "shopify-draft-proxy-rust-state/v1";

#[pyclass(name = "DraftProxy")]
struct PyDraftProxy {
    inner: Mutex<RustDraftProxy>,
}

#[pymethods]
impl PyDraftProxy {
    #[new]
    #[pyo3(signature = (read_mode = "snapshot", shopify_admin_origin = "https://shopify.com", port = 4000, snapshot_path = None, unsupported_mutation_mode = "passthrough", state = None))]
    fn new(
        read_mode: &str,
        shopify_admin_origin: &str,
        port: u16,
        snapshot_path: Option<String>,
        unsupported_mutation_mode: &str,
        state: Option<&Bound<'_, PyAny>>,
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
        let mut proxy = RustDraftProxy::new(config);
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

    #[pyo3(signature = (body, *, path = DEFAULT_GRAPHQL_PATH, headers = None))]
    fn process_graphql_request(
        &self,
        py: Python<'_>,
        body: &Bound<'_, PyAny>,
        path: &str,
        headers: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        self.process_request(py, "POST", path, Some(body), headers)
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
fn create_draft_proxy(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<PyDraftProxy> {
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

    PyDraftProxy::new(
        &read_mode,
        &shopify_admin_origin,
        port,
        snapshot_path,
        &unsupported_mutation_mode,
        state.as_ref(),
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
