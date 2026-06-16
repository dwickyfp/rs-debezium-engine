//! Comprehensive unit tests for rs-debezium-engine types, errors, and handlers.
//!
//! These tests exercise the public API without requiring a JVM or external
//! dependencies — pure Rust data-structure and trait tests.

use rs_debezium_engine::{
    ChangeEvent, ChangeHandler, DebeziumError, DebeziumEvent, SourceInfo,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Helper: a ChangeHandler that records every call for later assertions.
// ---------------------------------------------------------------------------

/// Thread-safe test handler that captures batch calls and error messages.
struct RecordingHandler {
    batches: Mutex<Vec<Vec<ChangeEvent>>>,
    errors: Mutex<Vec<String>>,
}

impl RecordingHandler {
    fn new() -> Self {
        Self {
            batches: Mutex::new(Vec::new()),
            errors: Mutex::new(Vec::new()),
        }
    }

    fn all_records(&self) -> Vec<ChangeEvent> {
        self.batches
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .cloned()
            .collect()
    }

    fn batch_count(&self) -> usize {
        self.batches.lock().unwrap().len()
    }

    fn error_count(&self) -> usize {
        self.errors.lock().unwrap().len()
    }

    fn last_error(&self) -> Option<String> {
        self.errors.lock().unwrap().last().cloned()
    }
}

impl ChangeHandler for RecordingHandler {
    fn handle_batch(&self, records: &[ChangeEvent]) {
        self.batches
            .lock()
            .unwrap()
            .push(records.to_vec());
    }

    fn on_error(&self, error: &str) {
        self.errors.lock().unwrap().push(error.to_string());
    }
}

/// Minimal handler that relies on the default `on_error` implementation.
struct DefaultErrorHandler {
    batches: Mutex<Vec<Vec<ChangeEvent>>>,
}

impl DefaultErrorHandler {
    fn new() -> Self {
        Self {
            batches: Mutex::new(Vec::new()),
        }
    }
}

impl ChangeHandler for DefaultErrorHandler {
    fn handle_batch(&self, records: &[ChangeEvent]) {
        self.batches.lock().unwrap().push(records.to_vec());
    }
    // on_error intentionally omitted — tests the default impl
}

// ===========================================================================
// 1. types.rs — ChangeEvent
// ===========================================================================

#[test]
fn change_event_creation_with_all_fields() {
    let event = ChangeEvent {
        key: Some(r#"{"id":1}"#.to_string()),
        value: Some(r#"{"after":{"id":1,"name":"Alice"}}"#.to_string()),
        destination: Some("mydb.public.users".to_string()),
    };

    assert_eq!(event.key.as_deref(), Some(r#"{"id":1}"#));
    assert_eq!(
        event.value.as_deref(),
        Some(r#"{"after":{"id":1,"name":"Alice"}}"#)
    );
    assert_eq!(event.destination.as_deref(), Some("mydb.public.users"));
}

#[test]
fn change_event_tombstone_all_none() {
    // A Debezium tombstone event has all fields set to None.
    let tombstone = ChangeEvent {
        key: None,
        value: None,
        destination: None,
    };

    assert!(tombstone.key.is_none());
    assert!(tombstone.value.is_none());
    assert!(tombstone.destination.is_none());

    // parsed_value returns None (no value string to parse).
    assert!(tombstone.parsed_value().is_none());
}

#[test]
fn change_event_tombstone_key_only() {
    // Tombstones sometimes carry a key but no value.
    let tombstone = ChangeEvent {
        key: Some(r#"{"id":42}"#.to_string()),
        value: None,
        destination: Some("mydb.public.orders".to_string()),
    };

    assert!(tombstone.key.is_some());
    assert!(tombstone.value.is_none());
    assert!(tombstone.parsed_value().is_none());
}

#[test]
fn change_event_json_serialization_roundtrip() {
    let original = ChangeEvent {
        key: Some(r#"{"id":10}"#.to_string()),
        value: Some(r#"{"op":"c","after":{"id":10}}"#.to_string()),
        destination: Some("db.schema.table".to_string()),
    };

    let json_str = serde_json::to_string(&original).expect("serialize");
    let deserialized: ChangeEvent =
        serde_json::from_str(&json_str).expect("deserialize");

    assert_eq!(original.key, deserialized.key);
    assert_eq!(original.value, deserialized.value);
    assert_eq!(original.destination, deserialized.destination);
}

#[test]
fn change_event_json_roundtrip_all_none() {
    let original = ChangeEvent {
        key: None,
        value: None,
        destination: None,
    };

    let json_str = serde_json::to_string(&original).expect("serialize");
    let deserialized: ChangeEvent =
        serde_json::from_str(&json_str).expect("deserialize");

    assert_eq!(original.key, deserialized.key);
    assert_eq!(original.value, deserialized.value);
    assert_eq!(original.destination, deserialized.destination);
}

#[test]
fn change_event_from_raw_json() {
    // Simulate deserializing a ChangeEvent from a JSON string (e.g. from a
    // message queue or file).
    let raw = r#"{
        "key": "{\"id\":5}",
        "value": "{\"after\":{\"id\":5,\"name\":\"Bob\"}}",
        "destination": "pg.public.users"
    }"#;

    let event: ChangeEvent = serde_json::from_str(raw).unwrap();
    assert_eq!(event.key.as_deref(), Some(r#"{"id":5}"#));
    assert_eq!(event.destination.as_deref(), Some("pg.public.users"));
}

// ===========================================================================
// 1b. types.rs — ChangeEvent::parsed_value()
// ===========================================================================

#[test]
fn parsed_value_returns_some_on_valid_json() {
    let event = ChangeEvent {
        key: Some(r#"{"id":1}"#.to_string()),
        value: Some(
            r#"{
                "before": null,
                "after": {"id": 1, "name": "Alice"},
                "source": {"version": "3.4.1", "connector": "postgres", "db": "mydb", "schema": "public", "table": "users"},
                "op": "c",
                "ts_ms": 1687000000000
            }"#
            .to_string(),
        ),
        destination: Some("mydb.public.users".to_string()),
    };

    let parsed = event.parsed_value();
    assert!(parsed.is_some(), "parsed_value should be Some for a valid value string");

    let debezium_event = parsed.unwrap().expect("JSON should parse without error");
    assert_eq!(debezium_event.op.as_deref(), Some("c"));
    assert_eq!(debezium_event.ts_ms, Some(1687000000000));
    assert!(debezium_event.before.is_none());
    assert!(debezium_event.after.is_some());
}

#[test]
fn parsed_value_returns_none_when_value_is_none() {
    let event = ChangeEvent {
        key: None,
        value: None,
        destination: None,
    };
    assert!(event.parsed_value().is_none());
}

#[test]
fn parsed_value_returns_err_on_invalid_json() {
    let event = ChangeEvent {
        key: None,
        value: Some("this is not valid json {{{".to_string()),
        destination: None,
    };

    let result = event.parsed_value();
    assert!(result.is_some(), "should wrap the parse attempt in Some");
    assert!(
        result.unwrap().is_err(),
        "invalid JSON should produce a serde_json error"
    );
}

// ===========================================================================
// 1c. types.rs — DebeziumEvent deserialization
// ===========================================================================

#[test]
fn debezium_event_create_operation() {
    let json_str = r#"{
        "before": null,
        "after": {"id": 1, "name": "Alice", "email": "alice@example.com"},
        "source": {
            "version": "3.4.1",
            "connector": "postgres",
            "name": "my-connector",
            "ts_ms": 1687000000000,
            "snapshot": "false",
            "db": "mydb",
            "schema": "public",
            "table": "users",
            "txId": 560,
            "lsn": 23456789,
            "xmin": null
        },
        "op": "c",
        "ts_ms": 1687000000000,
        "transaction": null
    }"#;

    let event: DebeziumEvent = serde_json::from_str(json_str).unwrap();

    // Create op: before is null, after is present
    assert!(event.before.is_none());
    assert!(event.after.is_some());
    assert_eq!(event.op.as_deref(), Some("c"));
    assert_eq!(event.ts_ms, Some(1687000000000));

    let after = event.after.unwrap();
    assert_eq!(after["id"], json!(1));
    assert_eq!(after["name"], json!("Alice"));
    assert_eq!(after["email"], json!("alice@example.com"));

    // Source info
    let source = event.source.unwrap();
    assert_eq!(source.version.as_deref(), Some("3.4.1"));
    assert_eq!(source.connector.as_deref(), Some("postgres"));
    assert_eq!(source.db.as_deref(), Some("mydb"));
    assert_eq!(source.schema.as_deref(), Some("public"));
    assert_eq!(source.table.as_deref(), Some("users"));

    // Extra flattened fields (LSN, txId, snapshot, etc.)
    assert_eq!(source.extra.get("txId"), Some(&json!(560)));
    assert_eq!(source.extra.get("lsn"), Some(&json!(23456789)));
    assert_eq!(source.extra.get("snapshot"), Some(&json!("false")));
    // The "name" field in source is not in the struct — it lands in extra
    assert_eq!(source.extra.get("name"), Some(&json!("my-connector")));
}

#[test]
fn debezium_event_update_operation() {
    let json_str = r#"{
        "before": {"id": 1, "name": "Alice"},
        "after": {"id": 1, "name": "Bob"},
        "source": {
            "version": "3.4.1",
            "connector": "postgres",
            "db": "mydb",
            "schema": "public",
            "table": "users"
        },
        "op": "u",
        "ts_ms": 1687000060000
    }"#;

    let event: DebeziumEvent = serde_json::from_str(json_str).unwrap();

    // Update op: both before and after are present
    assert!(event.before.is_some());
    assert!(event.after.is_some());
    assert_eq!(event.op.as_deref(), Some("u"));
    assert_eq!(event.ts_ms, Some(1687000060000));

    assert_eq!(event.before.unwrap()["name"], json!("Alice"));
    assert_eq!(event.after.unwrap()["name"], json!("Bob"));
}

#[test]
fn debezium_event_delete_operation() {
    let json_str = r#"{
        "before": {"id": 42, "name": "Charlie"},
        "after": null,
        "source": {
            "version": "3.4.1",
            "connector": "postgres",
            "db": "mydb",
            "schema": "public",
            "table": "users"
        },
        "op": "d",
        "ts_ms": 1687000120000
    }"#;

    let event: DebeziumEvent = serde_json::from_str(json_str).unwrap();

    // Delete op: before is present, after is null
    assert!(event.before.is_some());
    assert!(event.after.is_none());
    assert_eq!(event.op.as_deref(), Some("d"));
    assert_eq!(event.ts_ms, Some(1687000120000));

    let before = event.before.unwrap();
    assert_eq!(before["id"], json!(42));
    assert_eq!(before["name"], json!("Charlie"));
}

#[test]
fn debezium_event_read_snapshot_operation() {
    let json_str = r#"{
        "before": null,
        "after": {"id": 10, "status": "active"},
        "source": {
            "version": "3.4.1",
            "connector": "postgres",
            "db": "mydb",
            "schema": "public",
            "table": "items",
            "snapshot": "true"
        },
        "op": "r",
        "ts_ms": 1687000000000
    }"#;

    let event: DebeziumEvent = serde_json::from_str(json_str).unwrap();
    assert_eq!(event.op.as_deref(), Some("r"));

    let source = event.source.unwrap();
    assert_eq!(source.extra.get("snapshot"), Some(&json!("true")));
}

#[test]
fn debezium_event_all_fields_none() {
    let json_str = r#"{}"#;
    let event: DebeziumEvent = serde_json::from_str(json_str).unwrap();

    assert!(event.before.is_none());
    assert!(event.after.is_none());
    assert!(event.source.is_none());
    assert!(event.op.is_none());
    assert!(event.ts_ms.is_none());
    assert!(event.transaction.is_none());
}

#[test]
fn debezium_event_serialization_roundtrip() {
    let original = DebeziumEvent {
        before: Some(json!({"id": 1, "name": "Alice"})),
        after: Some(json!({"id": 1, "name": "Bob"})),
        source: Some(SourceInfo {
            version: Some("3.4.1".into()),
            connector: Some("postgres".into()),
            db: Some("mydb".into()),
            schema: Some("public".into()),
            table: Some("users".into()),
            extra: HashMap::from([("lsn".into(), json!(12345))]),
        }),
        op: Some("u".into()),
        ts_ms: Some(1687000060000),
        transaction: None,
    };

    let json_str = serde_json::to_string(&original).unwrap();
    let deserialized: DebeziumEvent = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.op, original.op);
    assert_eq!(deserialized.ts_ms, original.ts_ms);
    assert_eq!(deserialized.before, original.before);
    assert_eq!(deserialized.after, original.after);
    assert!(deserialized.transaction.is_none());

    let src = deserialized.source.unwrap();
    assert_eq!(src.version.as_deref(), Some("3.4.1"));
    assert_eq!(src.extra.get("lsn"), Some(&json!(12345)));
}

// ===========================================================================
// 1d. types.rs — SourceInfo with extra / flatten fields
// ===========================================================================

#[test]
fn source_info_extra_fields_captured_via_flatten() {
    let json_str = r#"{
        "version": "3.4.1",
        "connector": "mysql",
        "db": "inventory",
        "schema": null,
        "table": "products",
        "server_id": 223344,
        "file": "mysql-bin.000003",
        "pos": 46728,
        "row": 0,
        "thread": 100,
        "query": null
    }"#;

    let source: SourceInfo = serde_json::from_str(json_str).unwrap();

    assert_eq!(source.version.as_deref(), Some("3.4.1"));
    assert_eq!(source.connector.as_deref(), Some("mysql"));
    assert_eq!(source.db.as_deref(), Some("inventory"));
    assert!(source.schema.is_none());
    assert_eq!(source.table.as_deref(), Some("products"));

    // Verify the flattened extras
    assert_eq!(source.extra.get("server_id"), Some(&json!(223344)));
    assert_eq!(source.extra.get("file"), Some(&json!("mysql-bin.000003")));
    assert_eq!(source.extra.get("pos"), Some(&json!(46728)));
    assert_eq!(source.extra.get("row"), Some(&json!(0)));
    assert_eq!(source.extra.get("thread"), Some(&json!(100)));
    assert_eq!(source.extra.get("query"), Some(&json!(null)));
}

#[test]
fn source_info_empty_extra() {
    let json_str = r#"{
        "version": "2.5.0",
        "connector": "postgres",
        "db": "testdb",
        "schema": "public",
        "table": "events"
    }"#;

    let source: SourceInfo = serde_json::from_str(json_str).unwrap();
    assert!(source.extra.is_empty());
}

#[test]
fn source_info_serialization_roundtrip_with_extras() {
    let original = SourceInfo {
        version: Some("3.4.1".into()),
        connector: Some("postgres".into()),
        db: Some("mydb".into()),
        schema: Some("public".into()),
        table: Some("orders".into()),
        extra: HashMap::from([
            ("txId".into(), json!(9001)),
            ("lsn".into(), json!(99887766)),
            ("snapshot".into(), json!("false")),
        ]),
    };

    let json_str = serde_json::to_string(&original).unwrap();
    let deserialized: SourceInfo = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.version, original.version);
    assert_eq!(deserialized.connector, original.connector);
    assert_eq!(deserialized.db, original.db);
    assert_eq!(deserialized.schema, original.schema);
    assert_eq!(deserialized.table, original.table);
    assert_eq!(deserialized.extra.len(), 3);
    assert_eq!(deserialized.extra.get("txId"), Some(&json!(9001)));
    assert_eq!(deserialized.extra.get("lsn"), Some(&json!(99887766)));
    assert_eq!(deserialized.extra.get("snapshot"), Some(&json!("false")));
}

#[test]
fn source_info_all_none() {
    let json_str = r#"{}"#;
    let source: SourceInfo = serde_json::from_str(json_str).unwrap();

    assert!(source.version.is_none());
    assert!(source.connector.is_none());
    assert!(source.db.is_none());
    assert!(source.schema.is_none());
    assert!(source.table.is_none());
    assert!(source.extra.is_empty());
}

// ===========================================================================
// 2. error.rs — DebeziumError Display messages
// ===========================================================================

#[test]
fn error_display_jvm() {
    let err = DebeziumError::Jvm("out of memory".into());
    let msg = format!("{}", err);
    assert!(msg.contains("JVM error"), "got: {}", msg);
    assert!(msg.contains("out of memory"), "got: {}", msg);
}

#[test]
fn error_display_jni() {
    let err = DebeziumError::Jni("null pointer".into());
    let msg = format!("{}", err);
    assert!(msg.contains("JNI error"), "got: {}", msg);
    assert!(msg.contains("null pointer"), "got: {}", msg);
}

#[test]
fn error_display_config() {
    let err = DebeziumError::Config("missing database.hostname".into());
    let msg = format!("{}", err);
    assert!(msg.contains("Configuration error"), "got: {}", msg);
    assert!(msg.contains("missing database.hostname"), "got: {}", msg);
}

#[test]
fn error_display_not_built() {
    let err = DebeziumError::NotBuilt;
    let msg = format!("{}", err);
    assert!(msg.contains("not built"), "got: {}", msg);
    assert!(msg.contains("build()"), "got: {}", msg);
}

#[test]
fn error_display_already_running() {
    let err = DebeziumError::AlreadyRunning;
    let msg = format!("{}", err);
    assert!(msg.contains("already running"), "got: {}", msg);
}

#[test]
fn error_display_jvm_already_started() {
    let err = DebeziumError::JvmAlreadyStarted;
    let msg = format!("{}", err);
    assert!(msg.contains("JVM already started"), "got: {}", msg);
}

#[test]
fn error_display_java_exception() {
    let err = DebeziumError::JavaException {
        class: "java.lang.NullPointerException".into(),
        message: "Cannot invoke method on null".into(),
    };
    let msg = format!("{}", err);

    assert!(msg.contains("Java exception"), "got: {}", msg);
    assert!(
        msg.contains("java.lang.NullPointerException"),
        "got: {}",
        msg
    );
    assert!(msg.contains("Cannot invoke method on null"), "got: {}", msg);
    // The format is "Java exception: {class}: {message}"
    assert!(
        msg.contains("NullPointerException: Cannot invoke method"),
        "expected class: message format, got: {}",
        msg
    );
}

#[test]
fn error_display_io() {
    // Create an io::Error to embed
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let err = DebeziumError::Io(io_err);
    let msg = format!("{}", err);
    assert!(msg.contains("IO error"), "got: {}", msg);
    assert!(msg.contains("file missing"), "got: {}", msg);
}

#[test]
fn error_display_jars_not_found_includes_path() {
    let err = DebeziumError::JarsNotFound {
        path: "/opt/debezium/jars".into(),
    };
    let msg = format!("{}", err);

    assert!(msg.contains("JARs not found"), "got: {}", msg);
    assert!(
        msg.contains("/opt/debezium/jars"),
        "path should appear in message, got: {}",
        msg
    );
    assert!(
        msg.contains("download-debezium"),
        "should hint at the download script, got: {}",
        msg
    );
}

#[test]
fn error_display_handler() {
    let err = DebeziumError::Handler("callback panicked".into());
    let msg = format!("{}", err);
    assert!(msg.contains("Handler error"), "got: {}", msg);
    assert!(msg.contains("callback panicked"), "got: {}", msg);
}

#[test]
fn error_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DebeziumError>();
}

#[test]
fn error_debug_format() {
    let err = DebeziumError::AlreadyRunning;
    let dbg = format!("{:?}", err);
    assert_eq!(dbg, "AlreadyRunning");
}

#[test]
fn error_java_exception_debug_includes_fields() {
    let err = DebeziumError::JavaException {
        class: "java.io.IOException".into(),
        message: "Connection refused".into(),
    };
    let dbg = format!("{:?}", err);
    assert!(dbg.contains("JavaException"), "got: {}", dbg);
    assert!(dbg.contains("java.io.IOException"), "got: {}", dbg);
    assert!(dbg.contains("Connection refused"), "got: {}", dbg);
}

#[test]
fn error_io_from_conversion() {
    // Verify the #[from] derive on the Io variant works
    let io_err: std::io::Error =
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let err: DebeziumError = io_err.into();
    assert!(matches!(err, DebeziumError::Io(_)));
    assert!(format!("{}", err).contains("denied"));
}

// ===========================================================================
// 3. handler.rs — ChangeHandler trait
// ===========================================================================

#[test]
fn handler_receives_correct_batch() {
    let handler = RecordingHandler::new();

    let records = vec![
        ChangeEvent {
            key: Some(r#"{"id":1}"#.into()),
            value: Some(r#"{"after":{"id":1}}"#.into()),
            destination: Some("db.public.t1".into()),
        },
        ChangeEvent {
            key: Some(r#"{"id":2}"#.into()),
            value: Some(r#"{"after":{"id":2}}"#.into()),
            destination: Some("db.public.t1".into()),
        },
    ];

    handler.handle_batch(&records);

    assert_eq!(handler.batch_count(), 1);
    let all = handler.all_records();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].key, Some(r#"{"id":1}"#.into()));
    assert_eq!(all[1].key, Some(r#"{"id":2}"#.into()));
}

#[test]
fn handler_multiple_batches_recorded() {
    let handler = RecordingHandler::new();

    let batch1 = vec![ChangeEvent {
        key: Some("k1".into()),
        value: Some("v1".into()),
        destination: Some("dest1".into()),
    }];

    let batch2 = vec![
        ChangeEvent {
            key: Some("k2".into()),
            value: Some("v2".into()),
            destination: Some("dest2".into()),
        },
        ChangeEvent {
            key: Some("k3".into()),
            value: Some("v3".into()),
            destination: Some("dest3".into()),
        },
    ];

    handler.handle_batch(&batch1);
    handler.handle_batch(&batch2);

    assert_eq!(handler.batch_count(), 2);
    assert_eq!(handler.all_records().len(), 3);
}

#[test]
fn handler_empty_batch() {
    let handler = RecordingHandler::new();

    handler.handle_batch(&[]);

    assert_eq!(handler.batch_count(), 1);
    assert_eq!(handler.all_records().len(), 0);
}

#[test]
fn handler_on_error_records_message() {
    let handler = RecordingHandler::new();

    handler.on_error("something went wrong");
    handler.on_error("another failure");

    assert_eq!(handler.error_count(), 2);
    assert_eq!(
        handler.last_error(),
        Some("another failure".to_string())
    );
}

#[test]
fn handler_is_send_and_sync() {
    fn assert_handler<T: ChangeHandler>() {}

    assert_handler::<RecordingHandler>();
    assert_handler::<DefaultErrorHandler>();
}

#[test]
fn handler_trait_works_through_arc() {
    // Typical usage: handler is shared via Arc across threads.
    let handler = Arc::new(RecordingHandler::new());

    let records = vec![ChangeEvent {
        key: Some("k".into()),
        value: Some("v".into()),
        destination: Some("d".into()),
    }];

    // Simulate usage from another thread
    let handler_clone = Arc::clone(&handler);
    let handle = std::thread::spawn(move || {
        handler_clone.handle_batch(&records);
    });

    handle.join().unwrap();

    assert_eq!(handler.all_records().len(), 1);
    assert_eq!(handler.all_records()[0].key, Some("k".into()));
}

#[test]
fn handler_on_error_default_implementation_is_callable() {
    // DefaultErrorHandler does NOT override on_error, so calling it exercises
    // the default implementation (which just calls log::error!).
    // We verify that it doesn't panic — the log message goes to the logger.
    let handler = DefaultErrorHandler::new();
    handler.on_error("test default error handler");

    // No crash means the default impl ran successfully.
    // (The default impl only calls log::error!, which is a no-op without a
    // logger backend.)
}

// ===========================================================================
// Integration-style: ChangeEvent -> parsed_value -> DebeziumEvent
// ===========================================================================

#[test]
fn end_to_end_change_event_to_debezium_event() {
    // Simulate a full create event flowing through ChangeEvent -> DebeziumEvent
    let value_json = serde_json::json!({
        "before": null,
        "after": {
            "id": 99,
            "product": "Widget",
            "price": 19.99
        },
        "source": {
            "version": "3.4.1",
            "connector": "postgres",
            "db": "shopdb",
            "schema": "public",
            "table": "products",
            "txId": 1001,
            "lsn": 55443322
        },
        "op": "c",
        "ts_ms": 1700000000000_u64
    });

    let event = ChangeEvent {
        key: Some(r#"{"id":99}"#.to_string()),
        value: Some(serde_json::to_string(&value_json).unwrap()),
        destination: Some("shopdb.public.products".to_string()),
    };

    // Parse through the public API
    let debezium = event
        .parsed_value()
        .expect("value should be present")
        .expect("value should be valid JSON");

    assert_eq!(debezium.op.as_deref(), Some("c"));
    assert_eq!(debezium.ts_ms, Some(1700000000000));

    let after = debezium.after.expect("create has after");
    assert_eq!(after["id"], json!(99));
    assert_eq!(after["product"], json!("Widget"));
    assert!((after["price"].as_f64().unwrap() - 19.99).abs() < f64::EPSILON);

    let src = debezium.source.unwrap();
    assert_eq!(src.db.as_deref(), Some("shopdb"));
    assert_eq!(src.table.as_deref(), Some("products"));
    assert_eq!(src.extra.get("txId"), Some(&json!(1001)));
    assert_eq!(src.extra.get("lsn"), Some(&json!(55443322)));
}

#[test]
fn end_to_end_full_update_lifecycle() {
    // Create a "before" snapshot and an "after" snapshot representing an update
    let value_json = serde_json::json!({
        "before": {"id": 1, "status": "draft"},
        "after": {"id": 1, "status": "published"},
        "source": {
            "version": "3.4.1",
            "connector": "postgres",
            "db": "cms",
            "schema": "public",
            "table": "articles"
        },
        "op": "u",
        "ts_ms": 1700000060000_u64
    });

    let event = ChangeEvent {
        key: Some(r#"{"id":1}"#.to_string()),
        value: Some(serde_json::to_string(&value_json).unwrap()),
        destination: Some("cms.public.articles".to_string()),
    };

    let debezium = event.parsed_value().unwrap().unwrap();

    assert_eq!(debezium.op.as_deref(), Some("u"));
    assert_eq!(debezium.before.unwrap()["status"], json!("draft"));
    assert_eq!(debezium.after.unwrap()["status"], json!("published"));
}

#[test]
fn end_to_end_full_delete_lifecycle() {
    let value_json = serde_json::json!({
        "before": {"id": 7, "name": "obsolete"},
        "after": null,
        "source": {
            "version": "3.4.1",
            "connector": "postgres",
            "db": "appdb",
            "schema": "public",
            "table": "configs"
        },
        "op": "d",
        "ts_ms": 1700000120000_u64
    });

    let event = ChangeEvent {
        key: Some(r#"{"id":7}"#.to_string()),
        value: Some(serde_json::to_string(&value_json).unwrap()),
        destination: Some("appdb.public.configs".to_string()),
    };

    let debezium = event.parsed_value().unwrap().unwrap();

    assert_eq!(debezium.op.as_deref(), Some("d"));
    assert_eq!(debezium.before.unwrap()["name"], json!("obsolete"));
    assert!(debezium.after.is_none());
}
