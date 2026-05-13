use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    // The real injection helper apps are Linux desktop fixtures. Avoid building
    // them for Windows/macOS when the feature is accidentally enabled.
    if env::var("CARGO_FEATURE_REAL_INJECTION_TESTS").is_ok()
        && env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
    {
        build_gtk_test_app();
        build_terminal_test_app();
    }
}

fn build_gtk_test_app() {
    println!("cargo:rerun-if-changed=test-apps/gtk_test_app.c");

    let out_dir = env::var("OUT_DIR").unwrap();
    let executable_path = Path::new(&out_dir).join("gtk_test_app");

    // Check if pkg-config and GTK3 are available before attempting to build.
    let check_pkg_config = Command::new("pkg-config")
        .arg("--atleast-version=3.0")
        .arg("gtk+-3.0")
        .status();

    if check_pkg_config.is_err() || !check_pkg_config.unwrap().success() {
        println!("cargo:warning=Skipping GTK test app build: GTK+ 3.0 not found by pkg-config.");
        return;
    }

    // Get compiler flags from pkg-config.
    let cflags_output = Command::new("pkg-config")
        .arg("--cflags")
        .arg("gtk+-3.0")
        .output()
        .expect("Failed to run pkg-config for cflags");

    // Get linker flags from pkg-config.
    let libs_output = Command::new("pkg-config")
        .arg("--libs")
        .arg("gtk+-3.0")
        .output()
        .expect("Failed to run pkg-config for libs");

    let cflags = String::from_utf8(cflags_output.stdout).unwrap();
    let libs = String::from_utf8(libs_output.stdout).unwrap();

    // Compile the test app using gcc.
    let status = Command::new("gcc")
        .arg("test-apps/gtk_test_app.c")
        .arg("-o")
        .arg(&executable_path)
        .args(cflags.split_whitespace())
        .args(libs.split_whitespace())
        .status()
        .expect("Failed to execute gcc");

    if !status.success() {
        println!("cargo:warning=Failed to compile GTK test app. Real injection tests against GTK may fail.");
    }
}

fn build_terminal_test_app() {
    println!("cargo:rerun-if-changed=test-apps/terminal-test-app/src/main.rs");
    println!("cargo:rerun-if-changed=test-apps/terminal-test-app/Cargo.toml");
    println!("cargo:rerun-if-changed=test-apps/terminal-test-app/Cargo.lock");

    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir).join("terminal-test-app-target");

    // Build the terminal test app using `cargo build`.
    // Note: the test app is NOT part of the main workspace, so we must specify --manifest-path.
    let status = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        .arg("build")
        .arg("--manifest-path")
        .arg("test-apps/terminal-test-app/Cargo.toml")
        .arg("--release") // Build in release mode for faster startup.
        .arg("--locked")
        .arg("--target-dir")
        .arg(&target_dir)
        .status()
        .expect("Failed to execute cargo build for terminal-test-app");

    if !status.success() {
        println!(
            "cargo:warning=Failed to build the terminal test app. Real injection tests may fail."
        );
    } else {
        // Copy the executable to a known location in OUT_DIR for the tests to find easily.
        let src_path = target_dir.join("release/terminal-test-app");
        let dest_path = Path::new(&out_dir).join("terminal-test-app");
        if let Err(e) = std::fs::copy(&src_path, &dest_path) {
            println!(
                "cargo:warning=Failed to copy terminal test app executable from {:?} to {:?}: {}",
                src_path, dest_path, e
            );
        }
    }
}
