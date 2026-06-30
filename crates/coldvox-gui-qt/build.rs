// Build script for the CXX-Qt bridge.
//
// Regenerates the Rust↔C++ bridge from `src/bridge.rs` whenever it changes.
// When the `qt-ui` feature is OFF (the default), this exits immediately so the
// crate compiles as a lightweight stub with no Qt dependency — keeping
// workspace `cargo check` green on runners without Qt6 dev packages.

fn main() {
    println!("cargo:rerun-if-changed=src/bridge.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=qml/Main.qml");
    println!("cargo:rerun-if-changed=qml/SettingsWindow.qml");

    #[cfg(feature = "qt-ui")]
    {
        let builder = cxx_qt_build::CxxQtBuilder::new()
            .file("src/bridge.rs")
            .qt_module("Gui")
            .qt_module("Qml")
            .qt_module("Quick");
        builder.build();
    }
}
