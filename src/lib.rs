//! # rs-debezium-engine
//!
//! Embedded Debezium CDC engine for Rust — a Rust wrapper around the
//! Java Debezium Engine via JNI.
//!
//! This is the Rust equivalent of [pydbzengine](https://github.com/memiiso/pydbzengine)
//! (which uses JPype to embed Debezium in Python).
//!
//! ## Quick Start (Rust)
//!
//! ```rust,no_run
//! use rs_debezium_engine::{DebeziumEngine, ChangeEvent, ChangeHandler};
//! use std::collections::HashMap;
//!
//! struct PrintHandler;
//!
//! impl ChangeHandler for PrintHandler {
//!     fn handle_batch(&self, records: &[ChangeEvent]) {
//!         for r in records {
//!             println!("[{:?}] key={:?} value={:?}",
//!                 r.destination, r.key, r.value);
//!         }
//!     }
//! }
//!
//! fn main() -> Result<(), rs_debezium_engine::DebeziumError> {
//!     let props = HashMap::from([
//!         ("name".into(), "my-engine".into()),
//!         ("snapshot.mode".into(), "initial_only".into()),
//!         ("connector.class".into(),
//!          "io.debezium.connector.postgresql.PostgresConnector".into()),
//!         ("database.hostname".into(), "localhost".into()),
//!         ("database.port".into(), "5432".into()),
//!         ("database.user".into(), "postgres".into()),
//!         ("database.password".into(), "postgres".into()),
//!         ("database.dbname".into(), "testdb".into()),
//!         ("topic.prefix".into(), "testdb".into()),
//!     ]);
//!
//!     let mut engine = DebeziumEngine::builder()
//!         .properties(props)
//!         .handler(PrintHandler)
//!         .build()?;
//!
//!     engine.run()?;
//!     Ok(())
//! }
//! ```

pub mod engine;
pub mod error;
pub mod handler;
pub mod jvm;
pub mod types;

#[cfg(feature = "python")]
pub mod python;

// Re-exports for convenience
pub use engine::DebeziumEngine;
pub use error::DebeziumError;
pub use handler::ChangeHandler;
pub use types::{ChangeEvent, DebeziumEvent, EngineProperties, SourceInfo};
