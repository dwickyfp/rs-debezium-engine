//! Debezium Embedded Engine wrapper.
//!
//! Provides a builder API to configure and run a Debezium CDC engine
//! from Rust, with callbacks to a Rust `ChangeHandler`.

use crate::error::DebeziumError;
use crate::handler::ChangeHandler;
use crate::jvm;
use crate::types::EngineProperties;
use jni::objects::{GlobalRef, JObject, JValue};
use jni::JNIEnv;
use std::collections::HashMap;
use std::path::PathBuf;

/// Builder for constructing a [`DebeziumEngine`].
///
/// # Example
/// ```rust,no_run
/// use rs_debezium_engine::DebeziumEngine;
/// use std::collections::HashMap;
///
/// # fn main() -> Result<(), rs_debezium_engine::DebeziumError> {
/// let props = HashMap::from([
///     ("name".into(), "my-engine".into()),
///     ("snapshot.mode".into(), "initial_only".into()),
///     ("connector.class".into(), "io.debezium.connector.postgresql.PostgresConnector".into()),
///     ("database.hostname".into(), "localhost".into()),
///     ("database.port".into(), "5432".into()),
///     ("database.user".into(), "postgres".into()),
///     ("database.password".into(), "postgres".into()),
///     ("database.dbname".into(), "testdb".into()),
///     ("topic.prefix".into(), "testdb".into()),
/// ]);
///
/// let engine = DebeziumEngine::builder()
///     .properties(props)
///     .build()?;
/// # Ok(())
/// # }
/// ```
pub struct DebeziumEngineBuilder {
    properties: Option<EngineProperties>,
    jar_dir: Option<PathBuf>,
    handler: Option<Box<dyn ChangeHandler>>,
}

impl DebeziumEngineBuilder {
    pub fn new() -> Self {
        Self {
            properties: None,
            jar_dir: None,
            handler: None,
        }
    }

    /// Set Debezium connector configuration properties.
    ///
    /// These are the same properties you'd pass to a Debezium connector
    /// via Kafka Connect or the embedded engine API.
    ///
    /// Required properties vary by connector but typically include:
    /// - `name` — Engine/connector name
    /// - `connector.class` — Java class name of the connector
    /// - `snapshot.mode` — How to perform initial snapshot
    /// - Database connection properties (hostname, port, user, password, dbname)
    /// - `topic.prefix` — Prefix for CDC topic names
    pub fn properties(mut self, props: EngineProperties) -> Self {
        self.properties = Some(props);
        self
    }

    /// Set a single configuration property.
    pub fn property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value.into());
        self
    }

    /// Set the directory containing Debezium JAR files.
    ///
    /// Defaults to `./debezium-libs/` or the `DEBEZIUM_LIBS_DIR` env var.
    pub fn jar_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.jar_dir = Some(path.into());
        self
    }

    /// Set the change event handler.
    ///
    /// This handler's `handle_batch` method will be called for each
    /// batch of CDC events from the Debezium engine.
    pub fn handler<H: ChangeHandler>(mut self, handler: H) -> Self {
        self.handler = Some(Box::new(handler));
        self
    }

    /// Set the change event handler from a pre-built Box (used by PyO3 bindings).
    pub fn handler_box(mut self, handler: Box<dyn ChangeHandler>) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Build the [`DebeziumEngine`].
    ///
    /// This initializes the JVM (if not already started), creates the
    /// Java engine instance, but does NOT start processing events.
    /// Call [`DebeziumEngine::run`] to start.
    pub fn build(self) -> Result<DebeziumEngine, DebeziumError> {
        let properties = self.properties.ok_or(DebeziumError::Config(
            "Debezium properties are required. Call .properties() on the builder.".to_string(),
        ))?;

        let handler = self.handler.ok_or(DebeziumError::Config(
            "Change handler is required. Call .handler() on the builder.".to_string(),
        ))?;

        // Resolve JAR directory
        let jar_dir = match self.jar_dir {
            Some(d) => d,
            None => jvm::find_debezium_libs()?,
        };

        // Initialize JVM
        jvm::init_jvm(&jar_dir)?;

        // Register handler in global registry and get thin pointer ID
        let handler_id = jvm::register_handler(handler);

        Ok(DebeziumEngine {
            properties,
            handler_id,
            engine_ref: None,
        })
    }
}

/// The Debezium Embedded CDC Engine.
///
/// Wraps the Java `DebeziumEngine` to capture database change events
/// and deliver them to a Rust `ChangeHandler`.
///
/// # Lifecycle
/// 1. Create with [`DebeziumEngine::builder()`]
/// 2. Call [`run()`](Self::run) to start capturing events (blocks)
/// 3. Call [`close()`](Self::close) to stop
///
/// # Behavior
/// - Runs Debezium's initial snapshot (if configured) then streams changes
/// - Events are delivered in batches to the handler
/// - After the handler processes a batch, events are committed
/// - The engine runs until closed or interrupted
pub struct DebeziumEngine {
    properties: EngineProperties,
    handler_id: jni::sys::jlong,
    engine_ref: Option<GlobalRef>,
}

impl DebeziumEngine {
    /// Create a new builder for configuring the engine.
    pub fn builder() -> DebeziumEngineBuilder {
        DebeziumEngineBuilder::new()
    }

    /// Start the Debezium engine and process events.
    ///
    /// This is a **blocking** call that runs until the engine is stopped.
    /// The handler's `handle_batch` method will be called on a JVM worker
    /// thread for each batch of CDC events.
    pub fn run(&mut self) -> Result<(), DebeziumError> {
        let jvm_handle = jvm::get_jvm()?;
        let mut env = jvm_handle.attach_current_thread().map_err(|e| {
            DebeziumError::Jni(format!("Failed to attach thread: {}", e))
        })?;

        // Create Java Properties (global ref to prevent GC)
        let java_props = jvm::create_java_properties(&mut env, &self.properties)?;

        // Create RsChangeConsumer (global ref to prevent GC)
        let consumer = jvm::create_consumer(&mut env, self.handler_id)?;

        // Build DebeziumEngine via Java builder API
        let engine = self.build_java_engine(&mut env, &java_props, &consumer)?;

        // Store engine reference to prevent GC
        let engine_global = env.new_global_ref(&engine).map_err(|e| {
            DebeziumError::Jni(format!("Failed to create global ref: {}", e))
        })?;
        self.engine_ref = Some(engine_global);

        log::info!("Starting Debezium engine...");

        // engine.run() — this blocks until the engine stops
        if let Err(e) = env.call_method(&engine, "run", "()V", &[]) {
            return Err(DebeziumError::Jni(format!("engine.run() failed: {}", e)));
        }

        // Check for Java exception
        if env.exception_check().unwrap_or(false) {
            return Err(DebeziumError::from_jni_exception(&mut env));
        }

        log::info!("Debezium engine stopped.");
        Ok(())
    }

    /// Build the Java DebeziumEngine via the builder API.
    ///
    /// Equivalent to:
    /// ```java
    /// DebeziumEngine.create(Json.class)
    ///     .using(java_props)
    ///     .notifying(consumer)
    ///     .build()
    /// ```
    fn build_java_engine<'local>(
        &self,
        env: &mut JNIEnv<'local>,
        java_props: &GlobalRef,
        consumer: &GlobalRef,
    ) -> Result<JObject<'local>, DebeziumError> {
        // Load Json format class
        let json_class = env
            .find_class("io/debezium/engine/format/Json")
            .map_err(|e| DebeziumError::Jni(format!(
                "Cannot find Json format class: {}. Are Debezium JARs in classpath?", e
            )))?;

        // DebeziumEngine.create(Json.class)
        let builder = env
            .call_static_method(
                "io/debezium/engine/DebeziumEngine",
                "create",
                "(Ljava/lang/Class;)Lio/debezium/engine/DebeziumEngine$Builder;",
                &[JValue::Object(&json_class)],
            )
            .map_err(|e| DebeziumError::Jni(format!("DebeziumEngine.create() failed: {}", e)))?
            .l()
            .map_err(|e| DebeziumError::Jni(format!("Expected object return: {}", e)))?;

        // .using(java_props)
        let builder = env
            .call_method(
                &builder,
                "using",
                "(Ljava/util/Properties;)Lio/debezium/engine/DebeziumEngine$Builder;",
                &[JValue::Object(java_props.as_obj())],
            )
            .map_err(|e| DebeziumError::Jni(format!(".using() failed: {}", e)))?
            .l()
            .map_err(|e| DebeziumError::Jni(format!("Expected object return: {}", e)))?;

        // .notifying(consumer)
        let builder = env
            .call_method(
                &builder,
                "notifying",
                "(Lio/debezium/engine/DebeziumEngine$ChangeConsumer;)Lio/debezium/engine/DebeziumEngine$Builder;",
                &[JValue::Object(consumer.as_obj())],
            )
            .map_err(|e| DebeziumError::Jni(format!(".notifying() failed: {}", e)))?
            .l()
            .map_err(|e| DebeziumError::Jni(format!("Expected object return: {}", e)))?;

        // .build()
        let engine = env
            .call_method(
                &builder,
                "build",
                "()Lio/debezium/engine/DebeziumEngine;",
                &[],
            )
            .map_err(|e| DebeziumError::Jni(format!(".build() failed: {}", e)))?
            .l()
            .map_err(|e| DebeziumError::Jni(format!("Expected object return: {}", e)))?;

        // Check for Java exception
        if env.exception_check().unwrap_or(false) {
            return Err(DebeziumError::from_jni_exception(env));
        }

        Ok(engine)
    }

    /// Stop the engine gracefully.
    pub fn close(&mut self) {
        if let Some(ref engine) = self.engine_ref {
            if let Ok(jvm_handle) = jvm::get_jvm() {
                if let Ok(mut env) = jvm_handle.attach_current_thread() {
                    let _ = env.call_method(engine.as_obj(), "close", "()V", &[]);
                    log::info!("Debezium engine close() called.");
                }
            }
        }
    }
}

impl Drop for DebeziumEngine {
    fn drop(&mut self) {
        self.close();
        // Unregister handler from global registry
        jvm::unregister_handler(self.handler_id);
    }
}
