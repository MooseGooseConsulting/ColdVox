import QtQuick 6.5
import QtQuick.Controls 6.5
import QtQuick.Layouts 6.5

// Settings window for the ColdVox Qt overlay. Most controls are UI scaffolding
// backed by Qt.labs.settings in Main.qml; backend wiring lands in a follow-up.
Window {
  id: win
  title: "ColdVox Settings"
  visible: false
  property real opacityValue: 0.3
  width: 480
  height: 600
  minimumWidth: 400
  minimumHeight: 500
  flags: Qt.Dialog | Qt.WindowStaysOnTopHint

  Rectangle { anchors.fill: parent; color: Qt.rgba(0.12, 0.12, 0.12, 0.95) }

  ScrollView {
    anchors.fill: parent
    contentWidth: availableWidth
    clip: true
    ColumnLayout {
      width: parent.width
      spacing: 16
      padding: 20

      GroupBox {
        title: "Audio Input"
        Layout.fillWidth: true
        ColumnLayout {
          anchors.margins: 8
          anchors.fill: parent
          RowLayout {
            Layout.fillWidth: true
            Label { text: "Device"; Layout.preferredWidth: 120 }
            ComboBox { Layout.fillWidth: true; model: ["Default", "Device 1", "Device 2"] }
          }
        }
      }

      GroupBox {
        title: "Hotkey"
        Layout.fillWidth: true
        ColumnLayout {
          anchors.margins: 8
          anchors.fill: parent
          RowLayout {
            Layout.fillWidth: true
            Label { text: "Activation"; Layout.preferredWidth: 120 }
            TextField { Layout.fillWidth: true; placeholderText: "Ctrl+Shift+Space" }
          }
          Label { text: "Global hotkeys require platform backend integration."; color: "#BBBBBB" }
        }
      }

      GroupBox {
        title: "Appearance"
        Layout.fillWidth: true
        ColumnLayout {
          anchors.margins: 8
          anchors.fill: parent
          RowLayout {
            Layout.fillWidth: true
            Label { text: "Transparency"; Layout.preferredWidth: 120 }
            Slider { Layout.fillWidth: true; from: 0.1; to: 0.8; value: win.opacityValue; onMoved: win.opacityValue = value }
          }
          RowLayout {
            Layout.fillWidth: true
            Label { text: "Theme"; Layout.preferredWidth: 120 }
            ComboBox { Layout.fillWidth: true; model: ["Auto", "Light", "Dark"] }
          }
        }
      }

      GroupBox {
        title: "Transcription"
        Layout.fillWidth: true
        ColumnLayout {
          anchors.margins: 8
          anchors.fill: parent
          CheckBox { text: "Auto punctuation"; checked: true }
        }
      }

      RowLayout {
        Layout.fillWidth: true
        spacing: 12
        Item { Layout.fillWidth: true }
        Button { text: "Close"; onClicked: win.visible = false }
      }
    }
  }
}
