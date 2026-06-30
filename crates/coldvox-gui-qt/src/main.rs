mod demo;
mod overlay_bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    // Qt requires argc/argv; cxx-qt-lib manages the C++ side.
    let mut app = QGuiApplication::new();

    let mut engine = QQmlApplicationEngine::new();
    engine.pin_mut().load(&QUrl::from(
        "qrc:/qt/qml/ColdVoxOverlay/qml/Overlay.qml",
    ));

    app.pin_mut().exec();
}
