#!/usr/bin/env bash
#
# download-debezium.sh — Download Debezium Embedded Engine JARs from Maven Central.
#
# Usage:
#   ./scripts/download-debezium.sh [VERSION] [DEST_DIR]
#
# Examples:
#   ./scripts/download-debezium.sh                           # Latest: 3.4.1.Final → ./debezium-libs/
#   ./scripts/download-debezium.sh 3.4.1.Final               # Specific version
#   ./scripts/download-debezium.sh 3.4.1.Final /opt/debezium # Custom directory
#
# Requires: curl
#
set -euo pipefail

DEBEZIUM_VERSION="${1:-3.4.1.Final}"
DEST_DIR="${2:-$(dirname "$0")/../debezium-libs}"
MAVEN_BASE="https://repo1.maven.org/maven2"

mkdir -p "$DEST_DIR"

download_jar() {
    local group_path="$1"
    local artifact="$2"
    local version="$3"
    local classifier="${4:-}"

    local filename="${artifact}-${version}"
    if [ -n "$classifier" ]; then
        filename="${filename}-${classifier}"
    fi
    filename="${filename}.jar"

    local url="${MAVEN_BASE}/${group_path}/${artifact}/${version}/${filename}"
    local dest="${DEST_DIR}/${filename}"

    if [ -f "$dest" ]; then
        echo "  ✓ $filename (exists)"
        return 0
    fi

    echo "  ↓ $filename"
    if ! curl -sSfL -o "$dest" "$url" 2>/dev/null; then
        echo "  ✗ FAILED: $url"
        rm -f "$dest"
        return 1
    fi
}

echo "=== rs-debezium-engine: Downloading Debezium ${DEBEZIUM_VERSION} JARs ==="
echo "    Destination: ${DEST_DIR}"
echo ""

# ── Debezium Core ────────────────────────────────────────────────────────
echo "[1/7] Debezium Core & Embedded"
download_jar "io/debezium" "debezium-api" "$DEBEZIUM_VERSION"
download_jar "io/debezium" "debezium-core" "$DEBEZIUM_VERSION"
download_jar "io/debezium" "debezium-embedded" "$DEBEZIUM_VERSION"
download_jar "io/debezium" "debezium-ddl-parser" "$DEBEZIUM_VERSION"
download_jar "io/debezium" "debezium-storage-file" "$DEBEZIUM_VERSION"
download_jar "io/debezium" "debezium-storage-jdbc" "$DEBEZIUM_VERSION"
download_jar "io/debezium" "debezium-common" "$DEBEZIUM_VERSION"
download_jar "io/debezium" "debezium-sink" "$DEBEZIUM_VERSION"

# ── Debezium Connectors (add/remove as needed) ───────────────────────────
echo "[2/7] Debezium Connectors"
download_jar "io/debezium" "debezium-connector-postgres" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-mysql" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-mongodb" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-sqlserver" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-oracle" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-binlog" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-db2" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-cassandra-5" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-cassandra-core" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-vitess" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-connector-spanner" "$DEBEZIUM_VERSION" || true

# ── Kafka Connect ────────────────────────────────────────────────────────
echo "[3/7] Kafka Connect"
KAFKA_VERSION="3.7.1"
download_jar "org/apache/kafka" "connect-api" "$KAFKA_VERSION"
download_jar "org/apache/kafka" "connect-runtime" "$KAFKA_VERSION"
download_jar "org/apache/kafka" "connect-json" "$KAFKA_VERSION"
download_jar "org/apache/kafka" "connect-file" "$KAFKA_VERSION"
download_jar "org/apache/kafka" "connect-transforms" "$KAFKA_VERSION"
download_jar "org/apache/kafka" "kafka-clients" "$KAFKA_VERSION"
download_jar "org/apache/kafka" "kafka-metadata" "$KAFKA_VERSION"
download_jar "org/apache/kafka" "kafka-server-common" "$KAFKA_VERSION"
download_jar "org/apache/kafka" "kafka-streams" "$KAFKA_VERSION" || true
download_jar "org/apache/kafka" "kafka-raft" "$KAFKA_VERSION" || true

# ── Jackson (JSON) ───────────────────────────────────────────────────────
echo "[4/7] Jackson JSON"
JACKSON_VERSION="2.17.0"
download_jar "com/fasterxml/jackson/core" "jackson-core" "$JACKSON_VERSION"
download_jar "com/fasterxml/jackson/core" "jackson-databind" "$JACKSON_VERSION"
download_jar "com/fasterxml/jackson/core" "jackson-annotations" "$JACKSON_VERSION"
download_jar "com/fasterxml/jackson/datatype" "jackson-datatype-jsr310" "$JACKSON_VERSION"
download_jar "com/fasterxml/jackson/datatype" "jackson-datatype-jdk8" "$JACKSON_VERSION"
download_jar "com/fasterxml/jackson/dataformat" "jackson-dataformat-yaml" "$JACKSON_VERSION"
download_jar "com/fasterxml/jackson/module" "jackson-module-blackbird" "$JACKSON_VERSION"
download_jar "com/fasterxml/jackson/jaxrs" "jackson-jaxrs-json-provider" "$JACKSON_VERSION" || true

# ── Database Drivers ─────────────────────────────────────────────────────
echo "[5/7] Database Drivers"
download_jar "org/postgresql" "postgresql" "42.7.3"
download_jar "com/mysql" "mysql-connector-j" "8.3.0" || true
download_jar "org/mongodb" "mongodb-driver-sync" "5.0.1" || true
download_jar "org/mongodb" "bson" "5.0.1" || true
download_jar "com/microsoft/sqlserver" "mssql-jdbc" "12.6.1.jre11" || true
download_jar "com/oracle/database/jdbc" "ojdbc11" "23.3.0.23.09" || true

# ── Google/Guava ─────────────────────────────────────────────────────────
echo "[6/7] Google Guava & Common"
download_jar "com/google/guava" "guava" "33.2.0-jre"
download_jar "com/google/guava" "failureaccess" "1.0.2"
download_jar "com/google/guava" "listenablefuture" "9999.0-empty-to-avoid-conflict-with-guava"
download_jar "com/google/code/findbugs" "jsr305" "3.0.2"
download_jar "com/google/errorprone" "error_prone_annotations" "2.28.0"
download_jar "com/google/j2objc" "j2objc-annotations" "2.8"
download_jar "org/checkerframework" "checker-qual" "3.42.0"

# ── Logging & Misc ───────────────────────────────────────────────────────
echo "[7/7] Logging & Misc"
download_jar "org/slf4j" "slf4j-api" "2.0.13"
download_jar "org/slf4j" "slf4j-simple" "2.0.13" || true
download_jar "org/apache/logging/log4j" "log4j-api" "2.23.1"
download_jar "org/apache/logging/log4j" "log4j-core" "2.23.1"
download_jar "org/apache/logging/log4j" "log4j-slf4j2-impl" "2.23.1"
download_jar "org/antlr" "antlr4-runtime" "4.13.1"
download_jar "commons-io" "commons-io" "2.16.1"
download_jar "org/apache/commons" "commons-lang3" "3.14.0"
download_jar "org/apache/commons" "commons-collections4" "4.4"
download_jar "com/anthropic" "anthropic-java" "0.0.1" 2>/dev/null || true  # placeholder
download_jar "com/github/ben-manes/caffeine" "caffeine" "2.9.3"
download_jar "com/zaxxer" "HikariCP" "5.1.0" || true
download_jar "io/debezium" "debezium-openlineage-api" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-storage-redis" "$DEBEZIUM_VERSION" || true
download_jar "io/debezium" "debezium-storage-s3" "$DEBEZIUM_VERSION" || true

# ── Count results ────────────────────────────────────────────────────────
echo ""
JAR_COUNT=$(find "$DEST_DIR" -name "*.jar" | wc -l | tr -d ' ')
TOTAL_SIZE=$(du -sh "$DEST_DIR" | cut -f1)
echo "=== Done! ${JAR_COUNT} JARs (${TOTAL_SIZE}) in ${DEST_DIR} ==="
echo ""
echo "Note: Some connectors may have additional transitive dependencies."
echo "If you hit ClassNotFoundException at runtime, check the Debezium"
echo "connector documentation for required JARs."
echo ""
echo "For production use, consider resolving dependencies with Maven:"
echo "  mvn dependency:copy-dependencies -DoutputDirectory=./debezium-libs"
