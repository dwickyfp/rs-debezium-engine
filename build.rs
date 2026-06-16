//! Build script for rs-debezium-engine.
//!
//! Compiles the Java bridge class `RsChangeConsumer.java` into `.class`
//! files at `target/java-classes/`. Requires JDK to be installed.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let java_src = manifest_dir.join("java/io/rsdebezium/RsChangeConsumer.java");
    let out_dir = manifest_dir.join("target/java-classes");

    // Create output directory
    std::fs::create_dir_all(&out_dir).expect("Failed to create java-classes output dir");

    // Find JAVA_HOME for javac
    let java_home = env::var("JAVA_HOME").ok().or_else(|| {
        // Try /usr/libexec/java_home on macOS
        #[cfg(target_os = "macos")]
        {
            Command::new("/usr/libexec/java_home")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    });

    let javac = if let Some(ref home) = java_home {
        PathBuf::from(home).join("bin/javac")
    } else {
        PathBuf::from("javac")
    };

    // Find Debezium API JAR for classpath (needed to compile the bridge)
    let debezium_libs = manifest_dir.join("debezium-libs");
    let mut classpath_jars = Vec::new();
    if debezium_libs.exists() {
        for entry in std::fs::read_dir(&debezium_libs).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "jar") {
                classpath_jars.push(path);
            }
        }
    }

    let classpath = classpath_jars
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(":");

    // Compile Java source
    let mut cmd = Command::new(&javac);
    cmd.arg("-d")
        .arg(&out_dir)
        .arg("-source")
        .arg("11")
        .arg("-target")
        .arg("11");

    if !classpath.is_empty() {
        cmd.arg("-cp").arg(&classpath);
    }

    cmd.arg(&java_src);

    println!("cargo:warning=Compiling Java bridge: {} → {}", java_src.display(), out_dir.display());

    let output = cmd.output().expect("Failed to run javac. Ensure JDK 11+ is installed.");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        // If no Debezium JARs, skip compilation (will fail at runtime)
        if !debezium_libs.exists() || classpath_jars.is_empty() {
            println!(
                "cargo:warning=Debezium JARs not found at {}. \
                 Java bridge not compiled — run scripts/download-debezium.sh first.",
                debezium_libs.display()
            );
            return;
        }

        panic!(
            "javac failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
    }

    println!("cargo:warning=Java bridge compiled successfully.");

    // Re-run if Java source changes
    println!("cargo:rerun-if-changed={}", java_src.display());
    println!("cargo:rerun-if-changed={}", debezium_libs.display());
}
