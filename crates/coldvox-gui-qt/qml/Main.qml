import QtQuick 6.5
import QtQuick.Controls 6.5
import QtQuick.Layouts 6.5
import QtQuick.Window 6.5
import ColdVox 1.0
import Qt5Compat.GraphicalEffects
import Qt.labs.settings 1.1

// Main always-on-top transparent overlay for ColdVox (Qt backend).
// Collapsed: a small status pill. Expanded: live transcript + controls.
// The `bridge` object is the CXX-Qt GuiBridge registered from Rust; the
// `typeof bridge !== 'undefined'` guards let the QML render standalone during
// prototyping before the context-property wiring is finalized.
Window {
  id: root
  title: "ColdVox"
  visible: true
  flags: Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
  color: "transparent"
  property real scaleFactor: Screen.pixelDensity > 0 ? Screen.pixelDensity / 4.0 : 1.0

  // The CXX-Qt bridge qobject, registered via #[qml_element] under the
  // ColdVox URI. Instantiated here so every `bridge.*` binding below resolves
  // to the real runtime. The `typeof bridge !== 'undefined'` guards are kept
  // so the QML still loads standalone (e.g. in Qt Designer) without the import.
  GuiBridge { id: bridge }

  property bool expanded: settings.expanded
  // AppState enum: 0=Idle 1=Activating 2=Active 3=Paused 4=Stopping 5=Error
  property int st: typeof bridge !== 'undefined' ? bridge.state : 0
  property string partial_transcript: typeof bridge !== 'undefined' ? bridge.partial_transcript : ""
  property string final_transcript: typeof bridge !== 'undefined' ? bridge.final_transcript : ""

  Settings {
    id: settings
    category: "coldvox"
    property real x: (Screen.width - 240) / 2
    property real y: (Screen.height - 48) / 2
    property bool expanded: false
    property int expandedWidth: 600
    property int expandedHeight: 240
  }

  x: settings.x
  y: settings.y
  width: expanded ? settings.expandedWidth : 240
  height: expanded ? settings.expandedHeight : 48
  onXChanged: settings.x = x
  onYChanged: settings.y = y
  onExpandedChanged: settings.expanded = expanded
  onWidthChanged: if (expanded) settings.expandedWidth = width
  onHeightChanged: if (expanded) settings.expandedHeight = height

  property point dragStart
  function startDrag(mouse) { dragStart = Qt.point(mouse.x, mouse.y) }
  function doDrag(mouse) {
    if (!mouse.buttons) return
    root.x += mouse.x - dragStart.x
    root.y += mouse.y - dragStart.y
  }

  // Acrylic-ish background with a subtle drop shadow.
  Rectangle {
    id: bg
    anchors.fill: parent
    radius: expanded ? 16 : 24
    color: Qt.rgba(0.16, 0.16, 0.16, 0.30)
    border.width: expanded ? 1 : 0
    border.color: Qt.rgba(1, 1, 1, 0.10)
    layer.enabled: true
    layer.smooth: true
    layer.samples: 4
    Rectangle {
      anchors.fill: parent
      radius: parent.radius
      color: "transparent"
      layer.enabled: true
      layer.effect: DropShadow {
        horizontalOffset: 0
        verticalOffset: 6
        radius: 16
        samples: 17
        color: Qt.rgba(0, 0, 0, 0.35)
        source: bg
      }
    }
  }

  // ── Collapsed bar ────────────────────────────────────────────────────────
  Item {
    id: collapsedBar
    anchors.fill: parent
    visible: !expanded

    Rectangle {
      width: 8; height: 8; radius: 4
      color: st === 2 ? "#FF4D4D" : (st === 1 ? "#FFD24D" : "#00D084")
      anchors.verticalCenter: parent.verticalCenter
      anchors.horizontalCenter: parent.horizontalCenter
    }

    Text {
      text: "🎤"
      color: Qt.rgba(1,1,1, 0.70)
      font.pixelSize: 18
      anchors.verticalCenter: parent.verticalCenter
      anchors.left: parent.left
      anchors.leftMargin: 14
    }

    Text {
      id: gearIcon
      text: "⚙"
      color: Qt.rgba(1,1,1, 0.70)
      font.pixelSize: 18
      anchors.verticalCenter: parent.verticalCenter
      anchors.right: parent.right
      anchors.rightMargin: 14
      MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onEntered: gearIcon.opacity = 1.0
        onExited: gearIcon.opacity = 0.70
        onClicked: settingsWindow.visible = true
      }
    }

    MouseArea {
      anchors.fill: parent
      onPressed: root.startDrag(mouse)
      onPositionChanged: root.doDrag(mouse)
      onClicked: {
        expanded = true
        if (typeof bridge !== 'undefined' && bridge.cmd_start) bridge.cmd_start()
      }
    }
  }

  // ── Expanded panel ───────────────────────────────────────────────────────
  ColumnLayout {
    id: expandedPanel
    anchors.fill: parent
    spacing: 0
    visible: expanded

    // Activity / waveform area (draggable).
    Rectangle {
      Layout.fillWidth: true
      Layout.preferredHeight: 40
      color: "transparent"
      Row {
        id: bars
        anchors.fill: parent
        anchors.margins: 12
        spacing: 6
        Repeater {
          id: barsRepeater
          model: 24
          delegate: Rectangle {
            width: (bars.width - (bars.spacing * (barsRepeater.count - 1))) / barsRepeater.count
            radius: 2
            anchors.bottom: parent.bottom
            color: st === 1 ? "#FFD24D" : (st === 2 ? "#FF4D4D" : "#00D084")
            height: 8 + Math.abs(Math.sin((index + perfTimer.msec/50) / 2)) * 22
            Behavior on height { NumberAnimation { duration: 100 } }
          }
        }
      }
      MouseArea {
        anchors.fill: parent
        onPressed: root.startDrag(mouse)
        onPositionChanged: root.doDrag(mouse)
      }
    }

    // Transcript surface.
    Rectangle {
      Layout.fillWidth: true
      Layout.fillHeight: true
      color: "transparent"
      ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true
        ScrollBar.vertical.policy: ScrollBar.AsNeeded
        Column {
          width: scroll.availableWidth
          spacing: 6
          padding: 20
          Text {
            width: parent.width
            wrapMode: Text.WordWrap
            color: "#F5F5F5"
            font.pixelSize: 16
            font.bold: true
            lineHeight: 1.5
            text: root.final_transcript
            onTextChanged: scroll.scrollToBottom()
          }
          Text {
            width: parent.width
            wrapMode: Text.WordWrap
            color: Qt.rgba(0.8, 0.8, 0.8, 0.9)
            font.pixelSize: 16
            font.italic: true
            lineHeight: 1.5
            text: root.partial_transcript
            onTextChanged: scroll.scrollToBottom()
          }
        }
        function scrollToBottom() { contentItem.contentY = contentItem.contentHeight }
      }
    }

    // Control bar.
    Rectangle {
      Layout.fillWidth: true
      Layout.preferredHeight: 40
      color: Qt.rgba(0,0,0,0.25)
      RowLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 10
        ControlButton { label: "⏹"; onClicked: if (typeof bridge !== 'undefined' && bridge.cmd_stop) bridge.cmd_stop() }
        ControlButton {
          label: (typeof bridge !== 'undefined' && bridge.state === 3) ? "▶" : "⏸"
          onClicked: {
            if (typeof bridge !== 'undefined') {
              if (bridge.state === 2) bridge.cmd_pause()
              else if (bridge.state === 3) bridge.cmd_resume()
            }
          }
        }
        ControlButton { label: "🗑"; onClicked: if (typeof bridge !== 'undefined' && bridge.cmd_clear) bridge.cmd_clear() }
        Item { Layout.fillWidth: true }
        ControlButton { label: "⚙"; onClicked: settingsWindow.visible = true }
      }
    }
  }

  states: [
    State { name: "collapsed"; when: !expanded },
    State { name: "expanded"; when: expanded }
  ]
  transitions: [
    Transition {
      from: "collapsed"; to: "expanded"
      NumberAnimation { properties: "width,height"; duration: 300; easing.type: Easing.InOutQuad }
    },
    Transition {
      from: "expanded"; to: "collapsed"
      NumberAnimation { properties: "width,height"; duration: 300; easing.type: Easing.InOutQuad }
    }
  ]

  // Drive the waveform animation. `Timer` has no `msec` property, so we
  // maintain a counter incremented on each tick and read by the bar delegates.
  Timer {
    id: perfTimer
    interval: 33
    running: expanded
    repeat: true
    property int msec: 0
    onTriggered: msec += interval
  }

  SettingsWindow { id: settingsWindow }

  Shortcut {
    sequences: [ StandardKey.Cancel, "Ctrl+Shift+Space" ]
    onActivated: expanded = !expanded
  }
}

component ControlButton: Rectangle {
  id: btn
  implicitWidth: 40
  implicitHeight: 28
  radius: 6
  color: Qt.rgba(1,1,1,0.10)
  property alias label: lbl.text
  signal clicked()
  opacity: 0.60
  Behavior on opacity { NumberAnimation { duration: 100 } }
  Text {
    id: lbl
    anchors.centerIn: parent
    color: "#F5F5F5"
    font.pixelSize: 14
  }
  MouseArea {
    anchors.fill: parent
    hoverEnabled: true
    onEntered: btn.opacity = 1.0
    onExited: btn.opacity = 0.60
    onClicked: btn.clicked()
  }
}
