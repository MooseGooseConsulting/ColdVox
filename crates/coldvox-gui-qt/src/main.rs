// Entry point for the ColdVox Qt GUI backend.
//
// Under the `qt-ui` feature this launches a QGuiApplication, loads the QML
// overlay (`qml/Main.qml`), and runs the Qt event loop. Without the feature
// (the workspace default), it prints a stub message so `cargo check` succeeds
// on runners without Qt6 dev packages.

#[cfg(feature = "qt-ui")]
mod bridge;

#[cfg(feature = "qt-ui")]
fn main() {
    use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

    // Initialize logging so the bridge's `tracing` calls are visible.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    // Load the overlay QML. CARGO_MANIFEST_DIR resolves to this crate's root,
    // so qml/Main.qml is found during dev. For production, embed via Qt's qrc
    // resource system (follow-up).
    let qml_path = format!("{}/qml/Main.qml", env!("CARGO_MANIFEST_DIR"));
    let url = QUrl::from_local_file(&QString::from(&qml_path));
    if let Some(engine_pin) = engine.as_mut() {
        engine_pin.load(&url);
    }

    // Run the Qt event loop until the window closes or quit() is called.
    if let Some(app_pin) = app.as_mut() {
        let _ = app_pin.exec();
    }
}

#[cfg(not(feature = "qt-ui"))]
fn main() {
    println!("ColdVox Qt GUI groundwork ready (stub build).");
    println!("Build with: cargo run -p coldvox-gui-qt --features qt-ui");
    println!("Requires Qt6 dev packages (Gui/Qml/Quick).");
}
