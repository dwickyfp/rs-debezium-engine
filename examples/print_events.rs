//! Example: Print CDC events from a PostgreSQL database.
//!
//! Usage:
//!   1. Download Debezium JARs: `bash scripts/download-debezium.sh`
//!   2. Run: `cargo run --example print_events`

use rs_debezium_engine::{ChangeEvent, ChangeHandler, DebeziumEngine};
use std::collections::HashMap;

/// Simple handler that prints all CDC events to stdout.
struct PrintHandler;

impl ChangeHandler for PrintHandler {
    fn handle_batch(&self, records: &[ChangeEvent]) {
        println!("━━━ Received batch of {} records ━━━", records.len());

        for (i, record) in records.iter().enumerate() {
            let dest = record.destination.as_deref().unwrap_or("?");
            let op = record
                .parsed_value()
                .and_then(|r| r.ok())
                .and_then(|e| e.op)
                .unwrap_or_else(|| "?".to_string());

            let op_label = match op.as_str() {
                "c" => "CREATE",
                "u" => "UPDATE",
                "d" => "DELETE",
                "r" => "READ (snapshot)",
                "t" => "TRUNCATE",
                _ => "UNKNOWN",
            };

            println!(
                "  [{}] {} | op={} | key={:?}",
                i + 1,
                dest,
                op_label,
                record.key.as_deref().unwrap_or("null"),
            );

            // Print the full event value (can be very verbose)
            if let Some(ref value) = record.value {
                // Truncate for display
                let display = if value.len() > 200 {
                    format!("{}...", &value[..200])
                } else {
                    value.clone()
                };
                println!("       value: {}", display);
            }
        }
    }

    fn on_error(&self, error: &str) {
        eprintln!("❌ Debezium engine error: {}", error);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // ── Debezium Configuration ──────────────────────────────────────
    //
    // These are standard Debezium connector properties.
    // See: https://debezium.io/documentation/reference/stable/connectors/postgresql.html
    //
    let props = HashMap::from([
        ("name".into(), "rust-pg-engine".into()),
        // Snapshot mode: "initial" = snapshot existing data then stream,
        //                "initial_only" = snapshot only, no streaming,
        //                "never" = stream only, no snapshot
        ("snapshot.mode".into(), "initial".into()),
        // PostgreSQL connector
        (
            "connector.class".into(),
            "io.debezium.connector.postgresql.PostgresConnector".into(),
        ),
        // Connection settings
        ("database.hostname".into(), "localhost".into()),
        ("database.port".into(), "5432".into()),
        ("database.user".into(), "postgres".into()),
        ("database.password".into(), "postgres".into()),
        ("database.dbname".into(), "testdb".into()),
        // Topic prefix (becomes: <prefix>.<schema>.<table>)
        ("topic.prefix".into(), "testdb".into()),
        // PostgreSQL specific: logical replication slot
        ("slot.name".into(), "rs_debezium_slot".into()),
        ("publication.name".into(), "rs_debezium_pub".into()),
        // Offset storage (file-based for embedded mode)
        (
            "offset.storage".into(),
            "org.apache.kafka.connect.storage.FileOffsetBackingStore".into(),
        ),
        (
            "offset.storage.file.filename".into(),
            "./debezium-offset.dat".into(),
        ),
        ("offset.flush.interval.ms".into(), "1000".into()),
    ]);

    println!("🚀 Starting rs-debezium-engine...");
    println!("   (Ensure PostgreSQL is running with wal_level=logical)");
    println!();

    let mut engine = DebeziumEngine::builder()
        .properties(props)
        .handler(PrintHandler)
        .build()?;

    println!("✅ Engine built. Starting event loop (Ctrl+C to stop)...");
    println!();

    engine.run()?;

    println!("👋 Engine stopped.");
    Ok(())
}
