package io.rsdebezium;

import io.debezium.engine.ChangeEvent;
import io.debezium.engine.DebeziumEngine;
import java.util.List;

/**
 * Rust-side consumer for Debezium Embedded Engine.
 * 
 * This class implements Debezium's ChangeConsumer interface and bridges
 * batch callbacks from the Java Debezium Engine into Rust via JNI native
 * methods registered at runtime (no System.loadLibrary needed).
 * 
 * Architecture:
 *   Debezium Engine (Java)
 *     → RsChangeConsumer.handleBatch() (Java)
 *       → nativeHandleBatch() (JNI → Rust)
 *         → ChangeHandler::handle_batch() (Rust user code)
 *       ← return
 *     ← markProcessed + markBatchFinished
 */
public class RsChangeConsumer implements DebeziumEngine.ChangeConsumer<ChangeEvent<String, String>> {

    /**
     * Pointer to the Rust ChangeHandler trait object.
     * Passed from Rust during construction. Used to route callbacks to the
     * correct handler instance.
     */
    private final long handlerId;

    /**
     * @param handlerId Raw pointer to Rust handler, cast to long.
     *                  Rust must ensure the handler lives until the engine stops.
     */
    public RsChangeConsumer(long handlerId) {
        this.handlerId = handlerId;
    }

    // ── Native methods (registered at runtime via JNI RegisterNatives) ──

    /**
     * Called when a batch of change events is ready.
     * 
     * @param handlerId     Pointer to Rust handler
     * @param keys          Record keys (may contain nulls)
     * @param values        Record values (JSON strings, may contain nulls for tombstones)
     * @param destinations  Topic/table destinations
     */
    private static native void nativeHandleBatch(
        long handlerId,
        String[] keys,
        String[] values,
        String[] destinations
    );

    /**
     * Called when the engine encounters a fatal error.
     * 
     * @param handlerId Pointer to Rust handler
     * @param error     Error message
     */
    private static native void nativeOnError(long handlerId, String error);

    // ── ChangeConsumer interface implementation ──

    @Override
    public void handleBatch(
        List<ChangeEvent<String, String>> records,
        DebeziumEngine.RecordCommitter<ChangeEvent<String, String>> committer
    ) throws InterruptedException {
        int size = records.size();
        String[] keys = new String[size];
        String[] values = new String[size];
        String[] destinations = new String[size];

        for (int i = 0; i < size; i++) {
            ChangeEvent<String, String> record = records.get(i);
            keys[i] = record.key();
            values[i] = record.value();
            destinations[i] = record.destination();
        }

        try {
            // Dispatch to Rust
            nativeHandleBatch(handlerId, keys, values, destinations);

            // After Rust returns, mark all records as processed
            for (ChangeEvent<String, String> record : records) {
                committer.markProcessed(record);
            }
            committer.markBatchFinished();
        } catch (Exception e) {
            nativeOnError(handlerId, e.getMessage() != null ? e.getMessage() : e.getClass().getName());
            Thread.currentThread().interrupt();
        }
    }

    @Override
    public boolean supportsTombstoneEvents() {
        return true;
    }
}
