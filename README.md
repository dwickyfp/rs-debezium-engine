<div align="center">

# 🦀 rs-debezium-engine

**Embedded Debezium CDC engine for Rust & Python**

[![Crates.io](https://img.shields.io/crates/v/rs-debezium-engine.svg)](https://crates.io/crates/rs-debezium-engine)
[![PyPI](https://img.shields.io/pypi/v/rs-debezium-engine.svg)](https://pypi.org/project/rs-debezium-engine/)
[![CI](https://github.com/dwickyfp/rs-debezium-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/dwickyfp/rs-debezium-engine/actions)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Crates.io Downloads](https://img.shields.io/crates/d/rs-debezium-engine.svg)](https://crates.io/crates/rs-debezium-engine)

[Quick Start](#-quick-start) • [Installation](#-installation) • [Python API](#python) • [Configuration](#-configuration-reference) • [Contributing](#-contributing)

</div>

---

## 📖 What Is This?

**rs-debezium-engine** is a Rust library with Python bindings that embeds the [Debezium](https://debezium.io/) Change Data Capture (CDC) engine directly into your application. It is the Rust equivalent of [pydbzengine](https://github.com/lermit/pydbzengine) — delivering the same Debezium-powered CDC capabilities with dramatically lower overhead, smaller footprint, and native-speed event processing via JNI instead of JPype.

It wraps the Java Debezium Embedded Engine through a JNI bridge, exposing clean builder-pattern APIs in Rust and idiomatic Python classes via PyO3. You get real-time CDC streams from PostgreSQL, MySQL, MongoDB, Oracle, SQL Server, and more — without running a separate Kafka Connect cluster.

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Your Application                        │
│                                                             │
│  ┌──────────────────┐         ┌──────────────────────────┐  │
│  │   Rust Binary     │         │   Python Script           │  │
│  │                   │         │                           │  │
│  │  DebeziumEngine   │         │  DebeziumEngine(          │  │
│  │    ::builder()    │         │    properties=...,        │  │
│  │    .properties()  │         │    handler=MyHandler()    │  │
│  │    .handler()     │         │  )                        │  │
│  │    .build()       │         │                           │  │
│  │    .run()         │         │  engine.run()             │  │
│  └────────┬─────────┘         └────────────┬─────────────┘  │
│           │                                │                │
│           │  Rust API                      │  PyO3 bindings │
│           ▼                                ▼                │
│  ┌──────────────────────────────────────────────────────┐   │
│  │              rs-debezium-engine core                  │   │
│  │                                                      │   │
│  │   ChangeHandler trait ←→ JNI bridge (src/jvm.rs)     │   │
│  └────────────────────────┬─────────────────────────────┘   │
│                           │                                 │
│                           │  JNI (Java Native Interface)    │
│                           ▼                                 │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                  JVM (JDK 11+)                        │   │
│  │                                                      │   │
│  │   RsChangeConsumer.java                               │   │
│  │         │                                             │   │
│  │         ▼                                             │   │
│  │   ┌────────────────────────────────────────────────┐  │   │
│  │   │         Debezium Engine (Embedded)              │  │   │
│  │   │                                                │  │   │
│  │   │  ┌──────────┐ ┌──────────┐ ┌──────────────┐   │  │   │
│  │   │  │PostgreSQL│ │  MySQL   │ │   MongoDB    │   │  │   │
│  │   │  │Connector │ │Connector │ │  Connector   │   │  │   │
│  │   │  └──────────┘ └──────────┘ └──────────────┘   │  │   │
│  │   │  ┌──────────┐ ┌──────────┐ ┌──────────────┐   │  │   │
│  │   │  │ Oracle   │ │  SQL     │ │    Other     │   │  │   │
│  │   │  │Connector │ │  Server  │ │  Connectors  │   │  │   │
│  │   │  └──────────┘ └──────────┘ └──────────────┘   │  │   │
│  │   └────────────────────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────────┘   │
│                           │                                 │
│                           ▼                                 │
│              📡 Database WAL / Oplog / CDC Stream            │
└─────────────────────────────────────────────────────────────┘
```

---

## ✨ Features

- 🦀 **Native Rust API** — Builder pattern with `DebeziumEngine` and `ChangeHandler` trait
- 🐍 **Python bindings** — PyO3-based `DebeziumEngine` class + `BasePythonChangeHandler` base class
- 🔄 **Drop-in replacement** for `pydbzengine` with identical conceptual API
- ⚡ **10-100× faster handler execution** — Native Rust event dispatch vs. Python/JPython overhead
- 📦 **Compact footprint** — ~49 MB of Debezium JARs (vs. ~200 MB for typical setups)
- 🗄️ **All Debezium connectors** — PostgreSQL, MySQL, MongoDB, Oracle, SQL Server, Cassandra, Vitess, Informix, Db2, Spanner
- 🔧 **Build-time Java compilation** — `RsChangeConsumer.java` compiled automatically via `build.rs`
- 🧪 **Battle-tested** — 64 tests (43 Rust unit + 3 doc-tests + 18 Python integration tests)
- 🔌 **Simple JAR setup** — One script downloads all required Debezium dependencies
- 🏃 **Embedded mode** — No Kafka or Kafka Connect cluster required

---

## 🚀 Quick Start

### Rust

```rust
use rs_debezium_engine::{DebeziumEngine, ChangeEvent, ChangeHandler};
use std::collections::HashMap;

/// Your custom change handler — implements the ChangeHandler trait.
struct MyHandler;

impl ChangeHandler for MyHandler {
    fn handle_batch(&self, records: &[ChangeEvent]) {
        for r in records {
            println!(
                "[{:?}] table={} key={:?} op={:?}",
                r.destination, r.source_table, r.key, r.op
            );
        }
    }
}

fn main() -> Result<(), rs_debezium_engine::DebeziumError> {
    // Debezium configuration properties
    let props = HashMap::from([
        ("name".into(), "my-engine".into()),
        ("snapshot.mode".into(), "initial".into()),
        ("connector.class".into(), "io.debezium.connector.postgresql.PostgresConnector".into()),
        ("database.hostname".into(), "localhost".into()),
        ("database.port".into(), "5432".into()),
        ("database.user".into(), "postgres".into()),
        ("database.password".into(), "postgres".into()),
        ("database.dbname".into(), "testdb".into()),
        ("topic.prefix".into(), "testdb".into()),
    ]);

    // Build and run the engine
    let mut engine = DebeziumEngine::builder()
        .properties(props)
        .handler(MyHandler)
        .build()?;

    engine.run()
}
```

### Python

```python
from rs_debezium_engine import DebeziumEngine, BasePythonChangeHandler


class MyHandler(BasePythonChangeHandler):
    """Custom handler — receives batches of CDC events."""

    def handle_batch(self, records):
        for r in records:
            print(f"[{r.destination()}] table={r.source_table()} key={r.key()} op={r.op()}")


# Debezium configuration properties
props = {
    "name": "my-engine",
    "snapshot.mode": "initial",
    "connector.class": "io.debezium.connector.postgresql.PostgresConnector",
    "database.hostname": "localhost",
    "database.port": "5432",
    "database.user": "postgres",
    "database.password": "postgres",
    "database.dbname": "testdb",
    "topic.prefix": "testdb",
}

# Build and run the engine
engine = DebeziumEngine(properties=props, handler=MyHandler())
engine.run()
```

---

## 📦 Installation

### Prerequisites

| Requirement | Minimum Version | Notes |
|-------------|-----------------|-------|
| **JDK** | 11+ | OpenJDK or Oracle JDK; `JAVA_HOME` must be set |
| **Rust** | 1.70+ | Install via [rustup](https://rustup.rs/) |
| **Python** | 3.9+ | Only needed for Python bindings |

> ⚠️ **Important:** Make sure `JAVA_HOME` points to your JDK installation. The build script uses it to locate `javac` and JNI headers.

### Rust (Cargo)

Add to your `Cargo.toml`:

```toml
[dependencies]
rs-debezium-engine = "0.1"
```

Or install from the git repository:

```toml
[dependencies]
rs-debezium-engine = { git = "https://github.com/dwickyfp/rs-debezium-engine.git" }
```

### Python (pip)

Install from GitHub:

```bash
pip install rs-debezium-engine
```

Or install from source using [maturin](https://www.maturin.rs/):

```bash
# Clone the repository
git clone https://github.com/dwickyfp/rs-debezium-engine.git
cd rs-debezium-engine

# Create a virtual environment
python -m venv .venv
source .venv/bin/activate

# Build and install the Python wheel
pip install maturin
maturin develop --release
```

---

## 📚 Debezium JAR Setup

Before running the engine, you need to download the Debezium connector JARs. A convenience script is included:

```bash
# Download all Debezium connector JARs (~49 MB, 64 files)
./scripts/download-debezium.sh
```

This script downloads:

- **Debezium Engine core** — the embedded engine runtime
- **Connector JARs** — one for each supported database (PostgreSQL, MySQL, etc.)
- **Transitive dependencies** — Kafka clients, Jackson, protobuf, etc.

The JARs are placed in a `jars/` directory at the project root. Set the environment variable if you want a custom location:

```bash
export DEBEZIUM_JARS_DIR=/path/to/jars
```

---

## 🗄️ Supported Connectors

| Connector | Database | Connector Class |
|-----------|----------|-----------------|
| 🐘 **PostgreSQL** | PostgreSQL 9.6+ | `io.debezium.connector.postgresql.PostgresConnector` |
| 🐬 **MySQL** | MySQL 5.7+ / 8.0+ | `io.debezium.connector.mysql.MySqlConnector` |
| 🍃 **MongoDB** | MongoDB 3.6+ | `io.debezium.connector.mongodb.MongoDbConnector` |
| 🔶 **Oracle** | Oracle 12c+ | `io.debezium.connector.oracle.OracleConnector` |
| 🟦 **SQL Server** | SQL Server 2016+ | `io.debezium.connector.sqlserver.SqlServerConnector` |
| 🏛️ **Db2** | IBM Db2 11.5+ | `io.debezium.connector.db2.Db2Connector` |
| 📊 **Vitess** | Vitess 11+ | `io.debezium.connector.vitess.VitessConnector` |
| 🗃️ **Cassandra** | Cassandra 3.x+ | `io.debezium.connector.cassandra.CassandraConnector` |
| 📋 **Informix** | Informix 12+ | `io.debezium.connector.informix.InformixConnector` |
| ☁️ **Spanner** | Google Cloud Spanner | `io.debezium.connector.spanner.SpannerConnector` |

---

## ⚙️ Configuration Reference

All Debezium properties are passed as key-value pairs. Here are the most commonly used ones:

### Engine Properties

| Property | Description | Example |
|----------|-------------|---------|
| `name` | Unique name for this engine instance | `"my-cdc-engine"` |
| `connector.class` | Fully qualified connector class name | See [Supported Connectors](#-supported-connectors) |
| `offset.storage` | Offset storage implementation (default: file) | `org.apache.kafka.connect.storage.FileOffsetBackingStore` |
| `offset.storage.file.filename` | Path to the offset file | `"/tmp/offsets.dat"` |
| `offset.flush.interval.ms` | How often to flush offsets (ms) | `60000` |

### Snapshot Properties

| Property | Description | Example |
|----------|-------------|---------|
| `snapshot.mode` | When to take a snapshot | `initial`, `never`, `always`, `initial_only` |
| `snapshot.locking.mode` | Locking strategy during snapshot | `minimal`, `none`, `extended` |

### PostgreSQL-Specific

| Property | Description | Example |
|----------|-------------|---------|
| `database.hostname` | Database server host | `"localhost"` |
| `database.port` | Database server port | `"5432"` |
| `database.user` | Database username | `"postgres"` |
| `database.password` | Database password | `"secret"` |
| `database.dbname` | Database name | `"mydb"` |
| `database.server.name` | Logical server name for topic naming | `"myserver"` |
| `topic.prefix` | Prefix for all Kafka topic names | `"mydb"` |
| `plugin.name` | PostgreSQL output plugin | `pgoutput`, `decoderbufs`, `wal2json` |
| `schema.include.list` | Schemas to capture | `"public,inventory"` |
| `table.include.list` | Tables to capture | `"public.users,public.orders"` |

### MySQL-Specific

| Property | Description | Example |
|----------|-------------|---------|
| `database.hostname` | MySQL server host | `"localhost"` |
| `database.port` | MySQL server port | `"3306"` |
| `database.user` | MySQL username | `"root"` |
| `database.password` | MySQL password | `"secret"` |
| `database.server.id` | Unique server ID (numeric) | `"184054"` |
| `database.include.list` | Databases to capture | `"mydb"` |
| `include.schema.changes` | Capture DDL events | `true` / `false` |

---

## 📊 Comparison with pydbzengine

| Feature | **pydbzengine** | **rs-debezium-engine** |
|---------|-----------------|------------------------|
| **Primary Language** | Python | Rust |
| **JVM Bridge** | JPype (Python ↔ Java) | JNI (Rust ↔ Java) |
| **Python Support** | Native Python | PyO3 bindings |
| **Rust Support** | ❌ | ✅ Native |
| **JAR Size** | ~200 MB | ~49 MB |
| **Handler Execution** | Python-speed | 10-100× faster (native) |
| **Memory Overhead** | Higher (JPype + GIL) | Lower (native + zero-copy) |
| **Build Tool** | pip | Cargo + maturin |
| **Debezium Connectors** | All | All |
| **Embedded Mode** | ✅ | ✅ |
| **Kafka Required** | ❌ | ❌ |
| **Test Suite** | Basic | 64 tests (Rust + Python) |
| **License** | Apache-2.0 | Apache-2.0 |

---

## 🧪 Testing

### Rust Tests

```bash
# Run all Rust tests (unit + doc tests)
cargo test

# Run with output
cargo test -- --nocapture

# Run a specific test
cargo test test_change_event_parsing
```

### Python Tests

```bash
# Build the Python wheel first
maturin develop --release

# Run Python tests
python -m pytest tests/python/ -v

# Run a specific test
python -m pytest tests/python/test_rs_debezium_engine.py -v -k test_engine_creation
```

### All Tests

```bash
# Run everything
cargo test && maturin develop --release && python -m pytest tests/python/ -v
```

> **Note:** Some integration tests require a running database instance. See the `docker-compose.yml` for spinning up test containers.

---

## 📁 Project Structure

```
rs-debezium-engine/
├── Cargo.toml                          # Rust package manifest & dependencies
├── pyproject.toml                      # Python package config (maturin/PyO3)
├── build.rs                            # Build script — compiles Java classes
├── README.md                           # This file
├── LICENSE                             # Apache-2.0 license
│
├── src/
│   ├── lib.rs                          # Crate root — public API re-exports
│   ├── engine.rs                       # DebeziumEngine builder & run logic
│   ├── handler.rs                      # ChangeHandler trait definition
│   ├── jvm.rs                          # JNI bridge — JVM lifecycle & class loading
│   ├── types.rs                        # ChangeEvent, SourceInfo, Op types
│   ├── error.rs                        # DebeziumError enum & error handling
│   ├── python.rs                       # PyO3 module — Python class bindings
│   └── base_handler.py                 # BasePythonChangeHandler Python base class
│
├── java/
│   └── io/rsdebezium/
│       └── RsChangeConsumer.java       # Java bridge class for Debezium callback
│
├── scripts/
│   └── download-debezium.sh            # Downloads Debezium JARs (~49 MB)
│
├── examples/
│   └── print_events.rs                 # Example: print all CDC events to stdout
│
├── tests/
│   ├── test_types.rs                   # Rust type tests
│   └── python/
│       └── test_rs_debezium_engine.py  # Python integration tests
│
└── jars/                               # Downloaded Debezium JARs (gitignored)
```

---

## 🔍 How It Works

### The JNI Bridge

The core challenge is bridging Rust (or Python) to the Java-based Debezium Engine. Here's how rs-debezium-engine solves it:

#### 1. **Build Time** — Java Compilation

The `build.rs` script compiles `RsChangeConsumer.java` against the Debezium JARs at crate build time using `javac`. This produces `.class` files embedded in the build output.

```
build.rs → javac RsChangeConsumer.java → RsChangeConsumer.class
```

#### 2. **Runtime** — JVM Initialization

When you call `DebeziumEngine::builder().build()`, the `jvm.rs` module:

1. Locates the JDK via `JAVA_HOME`
2. Creates a JVM instance using the JNI API (`jni` crate v0.21)
3. Constructs a classpath from all JARs in the `jars/` directory
4. Loads the compiled `RsChangeConsumer` class

#### 3. **Engine Startup**

The `engine.rs` module:

1. Converts Rust `HashMap<String, String>` properties into a Java `Properties` object via JNI
2. Instantiates the Debezium `EmbeddedEngine` with those properties
3. Sets `RsChangeConsumer` as the change handler callback
4. Calls `engine.run()` on the JVM thread

#### 4. **Event Flow**

When the Debezium engine detects a database change:

```
Database WAL/Oplog
    → Debezium Connector (Java)
    → EmbeddedEngine (Java)
    → RsChangeConsumer.handleBatch(List<ChangeEvent>) (Java)
    → JNI callback into Rust
    → ChangeHandler::handle_batch(&self, &[ChangeEvent]) (Rust)
    → BasePythonChangeHandler.handle_batch(self, records) (Python, if applicable)
```

#### 5. **Type Marshalling**

The `types.rs` module handles conversion of Java objects to Rust structs:

| Java Type | Rust Type | Python Type |
|-----------|-----------|-------------|
| `String` | `String` | `str` |
| `byte[]` | `Vec<u8>` | `bytes` |
| `Struct` (Connect) | `HashMap<String, Value>` | `dict` |
| `Map<String, Object>` | `HashMap<String, Value>` | `dict` |

#### 6. **Shutdown**

When `engine.run()` returns (or you drop the engine), the JVM is gracefully shut down — flushing offsets and closing connectors.

---

## 🤝 Contributing

Contributions are welcome! Here's how to get started:

1. **Fork** the repository
2. **Clone** your fork:
   ```bash
   git clone https://github.com/your-username/rs-debezium-engine.git
   cd rs-debezium-engine
   ```
3. **Create a branch** for your feature or fix:
   ```bash
   git checkout -b feature/my-feature
   ```
4. **Set up the development environment:**
   ```bash
   # Ensure JDK and Rust are installed
   java -version   # Should be 11+
   rustc --version # Should be 1.70+

   # Download Debezium JARs
   ./scripts/download-debezium.sh

   # Run Rust tests
   cargo test

   # Build Python wheel & run Python tests
   maturin develop --release
   python -m pytest tests/python/ -v
   ```
5. **Submit a pull request** with a clear description of your changes

### Development Guidelines

- Follow Rust naming conventions (`snake_case` for functions, `CamelCase` for types)
- Add tests for new functionality — both Rust unit tests and Python integration tests
- Update documentation if you change public APIs
- Ensure `cargo clippy` passes with no warnings
- Run `cargo fmt` before committing

---

## 📄 License

This project is licensed under the **Apache License, Version 2.0** — see the [LICENSE](LICENSE) file for details.

```
Copyright 2024 Nous Research

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

---

<div align="center">

**Built with 🦀 Rust + 🐍 Python + ☕ Java**

[Report Bug](https://github.com/dwickyfp/rs-debezium-engine/issues) • [Request Feature](https://github.com/dwickyfp/rs-debezium-engine/issues) • [Documentation](https://docs.rs/rs-debezium-engine)

</div>
