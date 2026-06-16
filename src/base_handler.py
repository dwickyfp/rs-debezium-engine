"""BasePythonChangeHandler — mirror of pydbzengine's handler interface."""


class BasePythonChangeHandler:
    """Base class for Python change event handlers.
    
    Subclass this and implement `handle_batch` to process CDC events.
    Drop-in replacement for pydbzengine's BasePythonChangeHandler.

    Example::

        from rs_debezium_engine import DebeziumEngine, BasePythonChangeHandler

        class PrintHandler(BasePythonChangeHandler):
            def handle_batch(self, records):
                for r in records:
                    print(f"[{r.destination()}] key={r.key()}")
                    print(f"  value={r.value()}")

        engine = DebeziumEngine(properties={...}, handler=PrintHandler())
        engine.run()
    """

    def handle_batch(self, records):
        """Process a batch of change events.

        Called by the engine for each batch of CDC events received from
        the Debezium connector. After this method returns, the engine
        automatically marks all records as processed.

        Args:
            records: List of ChangeEvent objects. Each has:
                - .key() / .key — Record key (JSON string or None)
                - .value() / .value — Record value (JSON string or None)
                - .destination() / .destination — Topic name (string or None)
                - .parsed_value() — Value parsed as Python dict
        """
        raise NotImplementedError(
            "Not implemented. Subclass BasePythonChangeHandler "
            "and implement handle_batch(self, records)."
        )

    def on_error(self, error):
        """Called when the engine encounters a fatal error.

        Args:
            error: Error message string.
        """
        import sys
        print(f"Debezium engine error: {error}", file=sys.stderr)
