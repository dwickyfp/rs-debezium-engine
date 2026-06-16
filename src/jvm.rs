//! JVM lifecycle management and JNI bridge setup.

use crate::error::DebeziumError;
use crate::handler::ChangeHandler;
use crate::types::ChangeEvent;
use jni::objects::{GlobalRef, JClass, JObjectArray, JString, JValue};
use jni::sys::jlong;
use jni::{JNIEnv, NativeMethod};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

/// Singleton guard ensuring the JVM is started only once per process.
static JVM_INIT: Once = Once::new();

/// Global handle to the running JVM (set once during init).
static JVM_HANDLE: std::sync::OnceLock<jni::JavaVM> = std::sync::OnceLock::new();

/// Registry of handler pointers keyed by ID, to avoid fat-pointer casting issues.
/// The handler Box is stored here and its address used as the ID.
static HANDLER_REGISTRY: once_cell::sync::Lazy<Mutex<HashMap<u64, Box<Box<dyn ChangeHandler>>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

/// Default Debezium version to download.
pub const DEBEZIUM_VERSION: &str = "3.4.1.Final";

/// Find all `.jar` files in a directory, returning their absolute paths.
fn find_jars(dir: &Path) -> Result<Vec<String>, DebeziumError> {
    if !dir.exists() {
        return Err(DebeziumError::JarsNotFound {
            path: dir.display().to_string(),
        });
    }

    let mut jars = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "jar") {
            jars.push(path.to_string_lossy().into_owned());
        }
    }

    if jars.is_empty() {
        return Err(DebeziumError::JarsNotFound {
            path: dir.display().to_string(),
        });
    }

    jars.sort();
    Ok(jars)
}

/// Build the classpath string from a JAR directory.
fn build_classpath(jar_dir: &Path, extra_dirs: &[&Path]) -> Result<String, DebeziumError> {
    let mut jars = find_jars(jar_dir)?;
    for dir in extra_dirs {
        if dir.exists() {
            jars.push(dir.to_string_lossy().into_owned());
        }
    }
    Ok(jars.join(":"))
}

/// Locate the Debezium libs directory.
/// Checks in order: `DEBEZIUM_LIBS_DIR` env var, then `./debezium-libs/`.
pub fn find_debezium_libs() -> Result<PathBuf, DebeziumError> {
    // 1. Environment variable
    if let Ok(dir) = std::env::var("DEBEZIUM_LIBS_DIR") {
        let path = PathBuf::from(dir);
        if path.exists() {
            return Ok(path);
        }
    }

    // 2. Relative to current directory
    let local = PathBuf::from("debezium-libs");
    if local.exists() {
        return Ok(local);
    }

    // 3. Relative to CARGO_MANIFEST_DIR
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(manifest).join("debezium-libs");
        if path.exists() {
            return Ok(path);
        }
    }

    Err(DebeziumError::JarsNotFound {
        path: "debezium-libs/ (set DEBEZIUM_LIBS_DIR env var or run scripts/download-debezium.sh)"
            .to_string(),
    })
}

/// Locate the compiled Java bridge class directory.
fn find_bridge_class_dir() -> Result<PathBuf, DebeziumError> {
    // 1. Check target directory (compiled by build.rs)
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(manifest).join("target").join("java-classes");
        if path.join("io/rsdebezium/RsChangeConsumer.class").exists() {
            return Ok(path);
        }
    }

    // 2. Check java-classes/ directly
    let local = PathBuf::from("java-classes");
    if local.join("io/rsdebezium/RsChangeConsumer.class").exists() {
        return Ok(local);
    }

    // 3. Check if java source exists (will be compiled at runtime as fallback)
    if PathBuf::from("java/io/rsdebezium/RsChangeConsumer.java").exists() {
        compile_java_bridge()?;
        let path = PathBuf::from("java-classes");
        if path.join("io/rsdebezium/RsChangeConsumer.class").exists() {
            return Ok(path);
        }
    }

    Err(DebeziumError::Config(
        "Java bridge class not found. Run build.rs or compile manually.".to_string(),
    ))
}

/// Compile the Java bridge class using javac.
fn compile_java_bridge() -> Result<(), DebeziumError> {
    let java_home = std::env::var("JAVA_HOME").ok();
    let javac = if let Some(ref home) = java_home {
        PathBuf::from(home).join("bin/javac")
    } else {
        PathBuf::from("javac")
    };

    let out_dir = PathBuf::from("java-classes");
    std::fs::create_dir_all(&out_dir)?;

    let mut cmd = std::process::Command::new(&javac);
    cmd.arg("-d")
        .arg(&out_dir)
        .arg("-source")
        .arg("11")
        .arg("-target")
        .arg("11");

    // Add Debezium JARs to classpath if available
    if let Ok(libs) = find_debezium_libs() {
        let jars = find_jars(&libs)?;
        let cp = jars.join(":");
        if !cp.is_empty() {
            cmd.arg("-cp").arg(&cp);
        }
    }

    cmd.arg("java/io/rsdebezium/RsChangeConsumer.java");

    let output = cmd.output().map_err(|e| {
        DebeziumError::Config(format!("Failed to run javac: {}. Is JDK installed?", e))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DebeziumError::Config(format!("javac failed: {}", stderr)));
    }

    log::info!("Java bridge compiled to {}", out_dir.display());
    Ok(())
}

/// Initialize the JVM with Debezium JARs in classpath.
///
/// This function is safe to call multiple times — it only initializes
/// the JVM once (via `Once`).
pub fn init_jvm(jar_dir: &Path) -> Result<(), DebeziumError> {
    let mut init_result = Ok(());

    JVM_INIT.call_once(|| {
        // Build classpath: Debezium JARs + compiled bridge class dir
        let bridge_dir = match find_bridge_class_dir() {
            Ok(d) => d,
            Err(e) => {
                init_result = Err(e);
                return;
            }
        };

        let classpath = match build_classpath(jar_dir, &[&bridge_dir]) {
            Ok(cp) => cp,
            Err(e) => {
                init_result = Err(e);
                return;
            }
        };

        log::debug!(
            "JVM classpath has {} entries",
            classpath.matches(':').count() + 1
        );

        // Build JVM args using InitArgsBuilder
        let opt_cp = format!("-Djava.class.path={}", classpath);
        let opt_headless = "-Djava.awt.headless=true";
        let opt_log4j = format!(
            "-Dlog4j.configurationFile={}",
            find_log4j_config().display()
        );

        log::info!("Starting JVM...");

        let args = jni::InitArgsBuilder::new()
            .option(&opt_cp)
            .option(&*opt_headless)
            .option(&opt_log4j)
            .build();

        let jvm = match args {
            Ok(a) => jni::JavaVM::new(a),
            Err(e) => {
                init_result = Err(DebeziumError::Jvm(format!(
                    "Failed to build JVM args: {}",
                    e
                )));
                return;
            }
        };

        let jvm = match jvm {
            Ok(vm) => vm,
            Err(e) => {
                init_result = Err(DebeziumError::Jvm(format!(
                    "Failed to start JVM: {}. Ensure JAVA_HOME is set and JDK 11+ is installed.",
                    e
                )));
                return;
            }
        };

        // Register native methods for our bridge class
        if let Err(e) = register_native_methods(&jvm) {
            init_result = Err(e);
            return;
        }

        log::info!("JVM started successfully.");

        let _ = JVM_HANDLE.set(jvm);
    });

    init_result
}

/// Get a reference to the initialized JVM.
pub fn get_jvm() -> Result<&'static jni::JavaVM, DebeziumError> {
    JVM_HANDLE.get().ok_or(DebeziumError::Jvm(
        "JVM not initialized — call init_jvm() first".to_string(),
    ))
}

/// Find or create a minimal log4j2 config to suppress warnings.
fn find_log4j_config() -> PathBuf {
    let candidates = [
        PathBuf::from("config/log4j2.properties"),
        PathBuf::from("debezium-libs/config/log4j2.properties"),
    ];
    for c in &candidates {
        if c.exists() {
            return c.canonicalize().unwrap_or_else(|_| c.clone());
        }
    }
    PathBuf::from("log4j2.properties")
}

/// Register a handler in the global registry and return its ID.
///
/// The handler is stored as `Box<Box<dyn ChangeHandler>>` — double indirection
/// allows passing a thin pointer through JNI (jlong).
pub fn register_handler(handler: Box<dyn ChangeHandler>) -> jlong {
    let boxed: Box<Box<dyn ChangeHandler>> = Box::new(handler);
    let ptr = Box::into_raw(boxed);
    let id = ptr as jlong;

    // Reconstruct to move into the map
    let boxed = unsafe { Box::from_raw(ptr) };
    HANDLER_REGISTRY.lock().unwrap().insert(id as u64, boxed);

    id
}

/// Unregister a handler (called when engine is dropped).
pub fn unregister_handler(id: jlong) {
    HANDLER_REGISTRY.lock().unwrap().remove(&(id as u64));
}

/// Get a handler reference by ID.
///
/// # Safety
/// The handler must still be registered (not unregistered).
fn get_handler(id: jlong) -> &'static dyn ChangeHandler {
    let registry = HANDLER_REGISTRY.lock().unwrap();
    let boxed = registry
        .get(&(id as u64))
        .expect("Handler not registered — engine may have been dropped");
    // Safety: handler lives in global registry until explicitly removed.
    let handler: &Box<dyn ChangeHandler> = boxed.as_ref();
    let handler: &dyn ChangeHandler = handler.as_ref();
    unsafe { std::mem::transmute::<&dyn ChangeHandler, &'static dyn ChangeHandler>(handler) }
}

/// Register JNI native methods for `RsChangeConsumer`.
fn register_native_methods(jvm: &jni::JavaVM) -> Result<(), DebeziumError> {
    let mut env = jvm
        .attach_current_thread()
        .map_err(|e| DebeziumError::Jni(format!("Failed to attach thread: {}", e)))?;

    let methods = [
        NativeMethod {
            name: "nativeHandleBatch".into(),
            sig: "(J[Ljava/lang/String;[Ljava/lang/String;[Ljava/lang/String;)V".into(),
            fn_ptr: native_handle_batch as *mut std::ffi::c_void,
        },
        NativeMethod {
            name: "nativeOnError".into(),
            sig: "(JLjava/lang/String;)V".into(),
            fn_ptr: native_on_error as *mut std::ffi::c_void,
        },
    ];

    env.register_native_methods("io/rsdebezium/RsChangeConsumer", &methods)
        .map_err(|e| DebeziumError::Jni(format!("RegisterNatives failed: {}", e)))?;

    log::debug!("Native methods registered for RsChangeConsumer.");
    Ok(())
}

// ── JNI Native Method Implementations ────────────────────────────────────

/// Extract a Java String[] element to Rust Option<String>.
fn jstring_at(env: &mut JNIEnv<'_>, array: &JObjectArray<'_>, index: i32) -> Option<String> {
    let element = env.get_object_array_element(array, index).ok()?;
    if element.is_null() {
        return None;
    }
    let jstr = JString::from(element);
    env.get_string(&jstr)
        .ok()
        .map(|s| s.to_string_lossy().into_owned())
}

/// Extract all elements of a Java String[] to Vec<Option<String>>.
fn jstring_array_to_vec(env: &mut JNIEnv<'_>, array: &JObjectArray<'_>) -> Vec<Option<String>> {
    let length = env.get_array_length(array).unwrap_or(0);
    (0..length).map(|i| jstring_at(env, array, i)).collect()
}

/// JNI callback: Debezium has a batch of events for us.
///
/// This is called from the JVM thread. It extracts the records,
/// calls the Rust user handler, then returns to Java for commit.
extern "system" fn native_handle_batch(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handler_id: jlong,
    keys: JObjectArray<'_>,
    values: JObjectArray<'_>,
    destinations: JObjectArray<'_>,
) {
    let handler = get_handler(handler_id);

    let keys_vec = jstring_array_to_vec(&mut env, &keys);
    let values_vec = jstring_array_to_vec(&mut env, &values);
    let destinations_vec = jstring_array_to_vec(&mut env, &destinations);

    let records: Vec<ChangeEvent> = keys_vec
        .into_iter()
        .zip(values_vec)
        .zip(destinations_vec)
        .map(|((key, value), destination)| ChangeEvent {
            key,
            value,
            destination,
        })
        .collect();

    handler.handle_batch(&records);
}

/// JNI callback: Engine encountered a fatal error.
extern "system" fn native_on_error(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handler_id: jlong,
    error: JString<'_>,
) {
    let handler = get_handler(handler_id);

    let error_msg = env
        .get_string(&error)
        .ok()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown error".to_string());

    handler.on_error(&error_msg);
}

// ── High-level JNI Helpers ───────────────────────────────────────────────

/// Create a Java Properties object from a Rust HashMap.
pub fn create_java_properties(
    env: &mut JNIEnv<'_>,
    props: &HashMap<String, String>,
) -> Result<GlobalRef, DebeziumError> {
    let properties = env
        .new_object("java/util/Properties", "()V", &[])
        .map_err(|e| DebeziumError::Jni(format!("Failed to create Properties: {}", e)))?;

    for (key, value) in props {
        let jkey = env.new_string(key).map_err(|e| {
            DebeziumError::Jni(format!("Failed to create Java string: {}", e))
        })?;
        let jvalue = env.new_string(value).map_err(|e| {
            DebeziumError::Jni(format!("Failed to create Java string: {}", e))
        })?;

        env.call_method(
            &properties,
            "setProperty",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&jkey.into()), JValue::Object(&jvalue.into())],
        )
        .map_err(|e| DebeziumError::Jni(format!("setProperty failed: {}", e)))?;
    }

    env.new_global_ref(&properties)
        .map_err(|e| DebeziumError::Jni(format!("Failed to create global ref: {}", e)))
}

/// Create an RsChangeConsumer Java instance wrapping a Rust handler.
pub fn create_consumer(
    env: &mut JNIEnv<'_>,
    handler_id: jlong,
) -> Result<GlobalRef, DebeziumError> {
    let consumer = env
        .new_object(
            "io/rsdebezium/RsChangeConsumer",
            "(J)V",
            &[JValue::Long(handler_id)],
        )
        .map_err(|e| DebeziumError::Jni(format!("Failed to create RsChangeConsumer: {}", e)))?;

    env.new_global_ref(&consumer)
        .map_err(|e| DebeziumError::Jni(format!("Failed to create global ref: {}", e)))
}
