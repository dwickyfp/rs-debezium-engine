//! Core types for CDC change events.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single Change Data Capture event from Debezium.
///
/// Mirrors the Java `ChangeEvent<String, String>` where key/value are
/// JSON-serialized strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// Record key (typically the primary key, JSON-encoded).
    /// May be `None` for tombstone events.
    pub key: Option<String>,

    /// Record value (the full Debezium change event as JSON string).
    /// Contains `before`, `after`, `source`, `op`, `ts_ms` fields.
    /// May be `None` for tombstone events.
    pub value: Option<String>,

    /// Destination topic/table name.
    /// Format: `<topic.prefix>.<schema>.<table>`
    pub destination: Option<String>,
}

impl ChangeEvent {
    /// Parse the value as a [`DebeziumEvent`] for structured access.
    pub fn parsed_value(&self) -> Option<serde_json::Result<DebeziumEvent>> {
        self.value.as_ref().map(|v| serde_json::from_str(v))
    }
}

/// Structured representation of a Debezium change event JSON payload.
///
/// Example JSON:
/// ```json
/// {
///   "before": { "id": 1, "name": "Alice" },
///   "after": { "id": 1, "name": "Bob" },
///   "source": { "version": "3.4.1", "connector": "postgres", ... },
///   "op": "u",
///   "ts_ms": 1687000000000
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebeziumEvent {
    /// State of the row before the change. `None` for inserts.
    pub before: Option<serde_json::Value>,

    /// State of the row after the change. `None` for deletes.
    pub after: Option<serde_json::Value>,

    /// Source metadata (connector, database, LSN, etc.)
    pub source: Option<SourceInfo>,

    /// Operation type: "c" (create), "u" (update), "d" (delete), "r" (read/snapshot), "t" (truncate)
    pub op: Option<String>,

    /// Timestamp in milliseconds since epoch.
    pub ts_ms: Option<u64>,

    /// Transaction metadata (if enabled).
    pub transaction: Option<serde_json::Value>,
}

/// Source metadata from a Debezium change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Debezium version.
    pub version: Option<String>,

    /// Connector name (e.g., "postgres", "mysql").
    pub connector: Option<String>,

    /// Database name.
    pub db: Option<String>,

    /// Schema name (for relational DBs).
    pub schema: Option<String>,

    /// Table name.
    pub table: Option<String>,

    /// LSN (PostgreSQL) or binlog position (MySQL).
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Engine configuration properties.
///
/// These map directly to Debezium connector configuration properties.
/// See: https://debezium.io/documentation/reference/stable/connectors/
pub type EngineProperties = HashMap<String, String>;
