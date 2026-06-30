use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .file("src/overlay_bridge.rs")
        .qml_module(QmlModule {
            uri: "ColdVoxOverlay",
            version_major: 1,
            version_minor: 0,
            qml_files: &["qml/Overlay.qml"],
            rust_files: &["src/overlay_bridge.rs"],
        })
        .build();
}
