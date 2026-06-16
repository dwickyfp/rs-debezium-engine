//! Error types for rs-debezium-engine.

/// Errors that can occur in the Debezium engine.
#[derive(Debug, thiserror::Error)]
pub enum DebeziumError {
    #[error("JVM error: {0}")]
    Jvm(String),

    #[error("JNI error: {0}")]
    Jni(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Engine not built — call build() before run()")]
    NotBuilt,

    #[error("Engine already running")]
    AlreadyRunning,

    #[error("JVM already started by another library")]
    JvmAlreadyStarted,

    #[error("Java exception: {class}: {message}")]
    JavaException { class: String, message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Debezium JARs not found at {path} — run scripts/download-debezium.sh first")]
    JarsNotFound { path: String },

    #[error("Handler error: {0}")]
    Handler(String),
}

impl DebeziumError {
    /// Create from a JNI exception that may be pending on the JVM.
    /// Takes `&mut` JNIEnv because JNI methods require mutable access.
    pub fn from_jni_exception(env: &mut jni::JNIEnv<'_>) -> Self {
        if let Ok(true) = env.exception_check() {
            if let Ok(exception) = env.exception_occurred() {
                // Extract class name
                let class = env
                    .call_method(&exception, "getClass", "()Ljava/lang/Class;", &[])
                    .ok()
                    .and_then(|c| c.l().ok())
                    .and_then(|c| {
                        env.call_method(c, "getName", "()Ljava/lang/String;", &[])
                            .ok()
                    })
                    .and_then(|n| n.l().ok())
                    .map(|n| {
                        let js = jni::objects::JString::from(n);
                        env.get_string(&js)
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|_| "Unknown".to_string())
                    })
                    .unwrap_or_else(|| "Unknown".to_string());

                // Extract message
                let message = env
                    .call_method(&exception, "getMessage", "()Ljava/lang/String;", &[])
                    .ok()
                    .and_then(|m| m.l().ok())
                    .map(|m| {
                        let js = jni::objects::JString::from(m);
                        env.get_string(&js)
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();

                let _ = env.exception_clear();
                return DebeziumError::JavaException { class, message };
            }
        }
        DebeziumError::Jni("Unknown JNI error".to_string())
    }
}
