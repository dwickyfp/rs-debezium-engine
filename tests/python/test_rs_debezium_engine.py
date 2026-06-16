"""Tests for rs-debezium-engine Python bindings (PyO3)."""

import json
import os

import pytest

import rs_debezium_engine as rde
from rs_debezium_engine import (
    BasePythonChangeHandler,
    ChangeEvent,
    DebeziumEngine,
    DebeziumEngineError,
)


# ── 1. Module import & version ──────────────────────────────────────────


def test_module_imports():
    """All public names are importable."""
    assert hasattr(rde, "__version__")
    assert hasattr(rde, "ChangeEvent")
    assert hasattr(rde, "DebeziumEngine")
    assert hasattr(rde, "DebeziumEngineError")
    assert hasattr(rde, "BasePythonChangeHandler")


def test_version_is_semver():
    parts = rde.__version__.split(".")
    assert len(parts) == 3
    assert all(p.isdigit() for p in parts)


# ── 2. ChangeEvent creation & attribute/method access ───────────────────


def test_change_event_attributes():
    evt = ChangeEvent(key="k1", value='{"a":1}', destination="topic-1")
    # attribute access
    assert evt.key() == "k1"
    assert evt.value() == '{"a":1}'
    assert evt.destination() == "topic-1"
    # method access (identical)
    assert evt.key() == "k1"
    assert evt.value() == '{"a":1}'
    assert evt.destination() == "topic-1"


def test_change_event_none_fields():
    evt = ChangeEvent(key=None, value=None, destination=None)
    assert evt.key() is None
    assert evt.value() is None
    assert evt.destination() is None
    assert evt.key() is None
    assert evt.value() is None
    assert evt.destination() is None


def test_change_event_repr():
    evt = ChangeEvent(key="k", value='{"x":1}', destination="dest")
    r = repr(evt)
    assert "ChangeEvent" in r
    assert "dest" in r


def test_change_event_str():
    evt = ChangeEvent(key="k", value="{}", destination="mytopic")
    s = str(evt)
    assert "mytopic" in s


# ── 3. ChangeEvent.parsed_value() ──────────────────────────────────────


def test_parsed_value_returns_dict():
    payload = '{"before":null,"after":{"id":1,"name":"Alice"},"op":"c"}'
    evt = ChangeEvent(key="k", value=payload, destination="t")
    parsed = evt.parsed_value()
    assert isinstance(parsed, dict)
    assert parsed["op"] == "c"
    assert parsed["after"]["id"] == 1
    assert parsed["after"]["name"] == "Alice"
    assert parsed["before"] is None


def test_parsed_value_none_when_value_is_none():
    evt = ChangeEvent(key=None, value=None, destination=None)
    assert evt.parsed_value() is None


def test_parsed_value_nested_json():
    payload = json.dumps({"arr": [1, 2, 3], "nested": {"a": True}})
    evt = ChangeEvent(key="k", value=payload, destination="t")
    parsed = evt.parsed_value()
    assert parsed["arr"] == [1, 2, 3]
    assert parsed["nested"]["a"] is True


def test_parsed_value_invalid_json():
    evt = ChangeEvent(key="k", value="not-json", destination="t")
    with pytest.raises(ValueError, match="JSON parse error"):
        evt.parsed_value()


# ── 4. BasePythonChangeHandler subclassing ─────────────────────────────


def test_base_handler_subclass():
    class MyHandler(BasePythonChangeHandler):
        def __init__(self):
            self.received = []

        def handle_batch(self, records):
            self.received.extend(records)

    h = MyHandler()
    assert isinstance(h, BasePythonChangeHandler)
    assert h.received == []


def test_base_handler_handle_batch_not_implemented():
    """Calling handle_batch on the base class raises NotImplementedError."""
    h = BasePythonChangeHandler()
    with pytest.raises(NotImplementedError, match="handle_batch"):
        h.handle_batch([])


def test_base_handler_on_error_default(capsys):
    """on_error prints to stderr by default."""
    h = BasePythonChangeHandler()
    h.on_error("something broke")
    captured = capsys.readouterr()
    assert "something broke" in captured.err


# ── 5. DebeziumEngine construction validation ──────────────────────────


def test_engine_empty_properties_raises():
    """Empty properties dict must raise ValueError."""
    with pytest.raises(ValueError, match="Properties cannot be empty"):
        DebeziumEngine(properties={}, handler=BasePythonChangeHandler())


def test_engine_with_properties_succeeds():
    """Non-empty props → construction succeeds (no JVM needed yet)."""
    props = {"name": "test", "connector.class": "io.debezium.connector.postgresql.PostgresConnector"}
    engine = DebeziumEngine(properties=props, handler=BasePythonChangeHandler())
    assert engine is not None


# ── 6. DebeziumEngine repr ─────────────────────────────────────────────


def test_engine_repr():
    props = {"name": "test", "topic.prefix": "dbserver1"}
    engine = DebeziumEngine(properties=props, handler=BasePythonChangeHandler())
    r = repr(engine)
    assert "DebeziumEngine" in r
    assert "running=False" in r


# ── 7. Error handling ──────────────────────────────────────────────────


def test_debezium_engine_error_is_exception():
    """DebeziumEngineError is a proper Exception subclass."""
    assert issubclass(DebeziumEngineError, Exception)
    with pytest.raises(DebeziumEngineError):
        raise DebeziumEngineError("test error")


def test_engine_non_dict_properties_raises():
    """Passing non-dict should raise TypeError (PyO3 coercion)."""
    with pytest.raises(TypeError):
        DebeziumEngine(properties="not a dict", handler=BasePythonChangeHandler())


# ── Integration tests (require JVM + Debezium JARs) ────────────────────

requires_integration = pytest.mark.skipif(
    os.environ.get("DEBEZIUM_INTEGRATION_TEST") != "1",
    reason="Set DEBEZIUM_INTEGRATION_TEST=1 to run integration tests",
)


@requires_integration
def test_engine_run_and_close():
    """Full integration: engine runs and shuts down."""
    props = {
        "name": "integration-test",
        "connector.class": "io.debezium.connector.postgresql.PostgresConnector",
        "database.hostname": os.environ.get("DB_HOST", "localhost"),
        "database.port": os.environ.get("DB_PORT", "5432"),
        "database.user": os.environ.get("DB_USER", "postgres"),
        "database.password": os.environ.get("DB_PASS", "postgres"),
        "database.dbname": os.environ.get("DB_NAME", "testdb"),
        "topic.prefix": "test",
    }

    class CollectHandler(BasePythonChangeHandler):
        def __init__(self):
            self.batches = []

        def handle_batch(self, records):
            self.batches.append(records)

    handler = CollectHandler()
    engine = DebeziumEngine(properties=props, handler=handler)
    engine.run()
    engine.close()
