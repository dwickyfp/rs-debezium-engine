//! User-facing handler trait for processing CDC events.

use crate::types::ChangeEvent;

/// Trait for processing batches of Debezium change events.
///
/// Implement this trait to define custom CDC event processing logic.
/// The engine calls [`handle_batch`] for each batch of events received
/// from the Debezium connector.
///
/// # Example
/// ```rust
/// use rs_debezium_engine::{ChangeHandler, ChangeEvent};
///
/// struct PrintHandler;
///
/// impl ChangeHandler for PrintHandler {
///     fn handle_batch(&self, records: &[ChangeEvent]) {
///         for record in records {
///             println!("[{}] key={:?} value={:?}",
///                 record.destination.as_deref().unwrap_or("?"),
///                 record.key,
///                 record.value,
///             );
///         }
///     }
/// }
/// ```
pub trait ChangeHandler: Send + Sync + 'static {
    /// Process a batch of change events.
    ///
    /// This method is called by the Debezium engine (potentially from
    /// a JVM worker thread) whenever a batch of CDC events is ready.
    ///
    /// After this method returns, the engine automatically marks all
    /// records in the batch as processed and signals batch completion.
    /// If this method panics, the engine will interrupt the JVM thread.
    ///
    /// # Arguments
    /// * `records` — Slice of change events in this batch.
    fn handle_batch(&self, records: &[ChangeEvent]);

    /// Called when the engine encounters a fatal error.
    ///
    /// Default implementation logs the error.
    fn on_error(&self, error: &str) {
        log::error!("Debezium engine error: {}", error);
    }
}
