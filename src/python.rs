//! PyO3 Python bindings for rs-debezium-engine.
//!
//! Provides a Python API that mirrors pydbzengine's interface,
//! making it a drop-in replacement.

use crate::engine::DebeziumEngineBuilder;
use crate::handler::ChangeHandler;
use crate::types::ChangeEvent as RustChangeEvent;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;

// ── Custom Python Exception ──────────────────────────────────────────────

pyo3::create_exception!(
    rs_debezium_engine,
    DebeziumEngineError,
    pyo3::exceptions::PyException,
    "Error from the Debezium CDC engine"
);

// ── PyChangeEvent ────────────────────────────────────────────────────────

/// Python representation of a Debezium change event.
#[pyclass(name = "ChangeEvent")]
#[derive(Clone, Debug)]
pub struct PyChangeEvent {
    pub key: Option<String>,
    pub value: Option<String>,
    pub destination: Option<String>,
}

#[pymethods]
impl PyChangeEvent {
    #[new]
    #[pyo3(signature = (key=None, value=None, destination=None))]
    fn new(key: Option<String>, value: Option<String>, destination: Option<String>) -> Self {
        PyChangeEvent { key, value, destination }
    }

    fn key(&self) -> Option<String> {
        self.key.clone()
    }
    fn value(&self) -> Option<String> {
        self.value.clone()
    }
    fn destination(&self) -> Option<String> {
        self.destination.clone()
    }

    /// Parse the value JSON into a Python dict.
    fn parsed_value(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match &self.value {
            Some(v) => {
                let val: serde_json::Value = serde_json::from_str(v)
                    .map_err(|e| PyValueError::new_err(format!("JSON parse error: {}", e)))?;
                Ok(Some(json_to_python(py, &val)?))
            }
            None => Ok(None),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ChangeEvent(key={:?}, destination={:?}, value_len={})",
            self.key,
            self.destination,
            self.value.as_ref().map_or(0, |v| v.len())
        )
    }

    fn __str__(&self) -> String {
        format!(
            "[{}] key={:?} value_len={}",
            self.destination.as_deref().unwrap_or("?"),
            self.key,
            self.value.as_ref().map_or(0, |v| v.len())
        )
    }
}

impl pyo3::ToPyObject for PyChangeEvent {
    fn to_object(&self, py: Python<'_>) -> PyObject {
        Py::new(py, self.clone())
            .expect("Failed to create PyChangeEvent")
            .into_py(py)
    }
}

impl From<&RustChangeEvent> for PyChangeEvent {
    fn from(event: &RustChangeEvent) -> Self {
        PyChangeEvent {
            key: event.key.clone(),
            value: event.value.clone(),
            destination: event.destination.clone(),
        }
    }
}

// ── JSON → Python conversion ─────────────────────────────────────────────

fn json_to_python(py: Python<'_>, val: &serde_json::Value) -> PyResult<PyObject> {
    Ok(match val {
        serde_json::Value::Null => py.None(),
        serde_json::Value::Bool(b) => b.to_object(py),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_object(py)
            } else if let Some(f) = n.as_f64() {
                f.to_object(py)
            } else {
                n.to_string().to_object(py)
            }
        }
        serde_json::Value::String(s) => s.to_object(py),
        serde_json::Value::Array(arr) => {
            let items: Vec<PyObject> = arr
                .iter()
                .map(|v| json_to_python(py, v))
                .collect::<PyResult<_>>()?;
            items.to_object(py)
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in map {
                dict.set_item(k, json_to_python(py, v)?)?;
            }
            dict.to_object(py)
        }
    })
}

// ── PyChangeHandler bridge ───────────────────────────────────────────────

/// Wraps a Python handler object as a Rust `ChangeHandler`.
struct PyChangeHandlerBridge {
    handler: PyObject,
}

impl ChangeHandler for PyChangeHandlerBridge {
    fn handle_batch(&self, records: &[RustChangeEvent]) {
        Python::with_gil(|py| {
            let py_records: Vec<PyChangeEvent> = records.iter().map(PyChangeEvent::from).collect();

            let py_list = match PyList::new_bound(py, &py_records) {
                l => l,
            };

            if let Err(e) = self.handler.call_method1(py, "handle_batch", (py_list,)) {
                log::error!("Python handle_batch error: {}", e);
                e.print_and_set_sys_last_vars(py);
            }
        });
    }

    fn on_error(&self, error: &str) {
        Python::with_gil(|py| {
            if let Err(_) = self.handler.call_method1(py, "on_error", (error,)) {
                log::error!("Debezium engine error: {}", error);
            }
        });
    }
}

// ── PyDebeziumEngine ─────────────────────────────────────────────────────

/// Python wrapper for the Debezium Embedded Engine.
#[pyclass(name = "DebeziumEngine")]
pub struct PyDebeziumEngine {
    properties: HashMap<String, String>,
    handler: PyObject,
    jar_dir: Option<String>,
    engine: Option<crate::engine::DebeziumEngine>,
}

#[pymethods]
impl PyDebeziumEngine {
    /// Create a new Debezium engine.
    #[new]
    #[pyo3(signature = (properties, handler, jar_dir=None))]
    fn new(
        properties: HashMap<String, String>,
        handler: PyObject,
        jar_dir: Option<String>,
    ) -> PyResult<Self> {
        if properties.is_empty() {
            return Err(PyValueError::new_err(
                "Properties cannot be empty. Provide Debezium configuration.",
            ));
        }

        Ok(PyDebeziumEngine {
            properties,
            handler,
            jar_dir,
            engine: None,
        })
    }

    /// Start the engine and process CDC events. **Blocking**.
    fn run(&mut self, py: Python<'_>) -> PyResult<()> {
        let handler_ref = self.handler.clone_ref(py);
        py.allow_threads(|| {
            let bridge = PyChangeHandlerBridge {
                handler: handler_ref,
            };

            let mut builder = DebeziumEngineBuilder::new();
            builder = builder.properties(self.properties.clone());
            builder = builder.handler_box(Box::new(bridge));

            if let Some(ref dir) = self.jar_dir {
                builder = builder.jar_dir(dir);
            }

            let mut engine = builder
                .build()
                .map_err(|e| DebeziumEngineError::new_err(format!("{}", e)))?;

            engine
                .run()
                .map_err(|e| DebeziumEngineError::new_err(format!("{}", e)))?;

            self.engine = Some(engine);
            Ok(())
        })
    }

    /// Close the engine gracefully.
    fn close(&mut self) {
        if let Some(ref mut engine) = self.engine {
            engine.close();
        }
    }

    fn __repr__(&self) -> String {
        let running = if self.engine.is_some() { "True" } else { "False" };
        format!(
            "DebeziumEngine(properties={:?}, running={})",
            self.properties.keys().collect::<Vec<_>>(),
            running
        )
    }
}

// ── Module Definition ────────────────────────────────────────────────────

#[pymodule]
fn rs_debezium_engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyChangeEvent>()?;
    m.add_class::<PyDebeziumEngine>()?;
    m.add(
        "DebeziumEngineError",
        m.py().get_type_bound::<DebeziumEngineError>(),
    )?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Inject BasePythonChangeHandler from embedded Python file
    let py = m.py();
    let code = include_str!("base_handler.py");
    let locals = PyDict::new_bound(py);
    py.run_bound(code, None, Some(&locals))?;
    if let Some(cls) = locals.get_item("BasePythonChangeHandler")? {
        m.add("BasePythonChangeHandler", cls)?;
    }

    Ok(())
}
