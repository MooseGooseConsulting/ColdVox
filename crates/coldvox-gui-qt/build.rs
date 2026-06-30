// Build script for the CXX-Qt bridge.
//
// Regenerates the Rust↔C++ bridge from `src/bridge.rs` whenever it changes.
// When the `qt-ui` feature is OFF (the default), this exits immediately so the
// crate compiles as a lightweight stub with no Qt dependency — keeping
// workspace `cargo check` green on runners without Qt6 dev packages.
//
// Under `qt-ui`, `qml_module("ColdVox", 1, 0, "src/bridge.rs")` registers the
// bridge's `#[qml_element]`-annotated qobjects under the `ColdVox` QML URI,
// so QML can `import ColdVox 1.0` and instantiate `GuiBridge { id: bridge }`.

fn main() {
    println!("cargo:rerun-if-changed=src/bridge.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=qml/Main.qml");
    println!("cargo:rerun-if-changed=qml/SettingsWindow.qml");

    #[cfg(feature = "qt-ui")]
    {
        let builder = cxx_qt_build::CxxQtBuilder::new()
            .qml_module("ColdVox", 1, 0, "src/bridge.rs")
            .qt_module("Gui")
            .qt_module("Qml")
            .qt_module("Quick");
        builder.build();
    }
}
