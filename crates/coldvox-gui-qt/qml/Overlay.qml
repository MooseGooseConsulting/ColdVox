// ColdVox Qt overlay shell
// Mirrors the visual contract of the Tauri/React frontend:
//   - Collapsed state: compact 336×68 bar
//   - Expanded state:  720×448 panel with transcript and controls
//
// The OverlayBridge QObject is registered as a QML element by cxx-qt-build.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Window 2.15
import QtQuick.Layouts 1.15
import ColdVoxOverlay 1.0

Window {
    id: root

    // -----------------------------------------------------------------------
    // Window chrome — transparent, always-on-top, frameless overlay
    // -----------------------------------------------------------------------
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool
    color: "transparent"

    width:  overlay.expanded ? 720 : 336
    height: overlay.expanded ? 448 : 68

    // Smooth resize animation.
    Behavior on width  { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    Behavior on height { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }

    visible: true
    title: "ColdVox"

    // -----------------------------------------------------------------------
    // Overlay bridge instance — created by cxx-qt-build as a QML element.
    // -----------------------------------------------------------------------
    OverlayBridge {
        id: overlay
    }

    // -----------------------------------------------------------------------
    // Demo timer — advances the demo sequence one step every 350 ms.
    // The OverlayBridge.demo_tick() invokable is safe to call when idle;
    // it is a no-op unless a demo session is active.
    // -----------------------------------------------------------------------
    Timer {
        id: demoTimer
        interval: 350
        repeat: true
        running: false

        onTriggered: {
            overlay.demo_tick()
            // Stop the timer once the demo finishes (status → ready).
            if (overlay.status === "ready" || overlay.status === "idle") {
                demoTimer.stop()
            }
        }
    }

    // -----------------------------------------------------------------------
    // Status colour mapping
    // -----------------------------------------------------------------------
    function statusColor(st) {
        switch (st) {
            case "idle":       return "#6b7280"   // grey-500
            case "listening":  return "#3b82f6"   // blue-500
            case "processing": return "#f59e0b"   // amber-500
            case "ready":      return "#10b981"   // emerald-500
            case "error":      return "#ef4444"   // red-500
            default:           return "#6b7280"
        }
    }

    // -----------------------------------------------------------------------
    // Collapsed state — compact bar, click to expand
    // -----------------------------------------------------------------------
    Rectangle {
        id: collapsedCard
        visible: !overlay.expanded
        anchors.fill: parent
        radius: 8
        color: "#111827"   // grey-900
        border.color: "#374151"
        border.width: 1
        opacity: 0.95

        MouseArea {
            anchors.fill: parent
            onClicked: overlay.set_expanded(true)
        }

        RowLayout {
            anchors { fill: parent; margins: 12 }
            spacing: 10

            // Colour accent stripe
            Rectangle {
                width: 4; height: 36; radius: 2
                color: statusColor(overlay.status)
                Behavior on color { ColorAnimation { duration: 200 } }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                Text {
                    text: "ColdVox overlay ready"
                    color: "#f9fafb"
                    font { pixelSize: 13; weight: Font.Medium }
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                Text {
                    text: overlay.partialTranscript !== ""
                          ? overlay.partialTranscript
                          : (overlay.finalTranscript !== ""
                             ? overlay.finalTranscript
                             : overlay.statusDetail)
                    color: "#9ca3af"
                    font.pixelSize: 11
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }

            // Status pill
            Rectangle {
                width: statusLabel.implicitWidth + 16
                height: 22; radius: 11
                color: statusColor(overlay.status)

                Text {
                    id: statusLabel
                    anchors.centerIn: parent
                    text: overlay.status.charAt(0).toUpperCase() + overlay.status.slice(1)
                    color: "white"
                    font { pixelSize: 11; weight: Font.Medium }
                }
            }

            Text {
                text: "Expand"
                color: "#9ca3af"
                font.pixelSize: 11
            }
        }
    }

    // -----------------------------------------------------------------------
    // Expanded state — full transcript panel
    // -----------------------------------------------------------------------
    Rectangle {
        id: expandedCard
        visible: overlay.expanded
        anchors.fill: parent
        radius: 10
        color: "#111827"
        border.color: "#374151"
        border.width: 1
        opacity: 0.96

        ColumnLayout {
            anchors { fill: parent; margins: 16 }
            spacing: 0

            // ---- Header ----
            RowLayout {
                Layout.fillWidth: true
                spacing: 12

                // Drag region
                MouseArea {
                    Layout.fillWidth: true
                    height: 56
                    property point pressPos
                    onPressed: pressPos = Qt.point(mouseX, mouseY)
                    onPositionChanged: {
                        if (pressed) {
                            root.x += (mouseX - pressPos.x)
                            root.y += (mouseY - pressPos.y)
                        }
                    }

                    ColumnLayout {
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        Text {
                            text: "Windows-first transparent overlay"
                            color: "#6b7280"
                            font { pixelSize: 10; capitalization: Font.AllUppercase; letterSpacing: 1 }
                        }
                        Text {
                            text: "ColdVox"
                            color: "#f9fafb"
                            font { pixelSize: 22; weight: Font.Bold }
                        }
                        Text {
                            text: overlay.statusDetail
                            color: "#9ca3af"
                            font.pixelSize: 12
                            elide: Text.ElideRight
                        }
                    }
                }

                ColumnLayout {
                    spacing: 4
                    Layout.alignment: Qt.AlignTop

                    // Status pill
                    Rectangle {
                        width: expStatusLabel.implicitWidth + 16
                        height: 24; radius: 12
                        color: statusColor(overlay.status)
                        Behavior on color { ColorAnimation { duration: 200 } }

                        Text {
                            id: expStatusLabel
                            anchors.centerIn: parent
                            text: overlay.status.charAt(0).toUpperCase() + overlay.status.slice(1)
                            color: "white"
                            font { pixelSize: 12; weight: Font.Medium }
                        }
                    }

                    Button {
                        text: "Collapse"
                        onClicked: overlay.set_expanded(false)
                        flat: true

                        contentItem: Text {
                            text: parent.text
                            color: "#9ca3af"
                            font.pixelSize: 11
                            horizontalAlignment: Text.AlignHCenter
                        }
                        background: Rectangle {
                            color: parent.pressed ? "#374151" : "transparent"
                            radius: 4
                        }
                    }
                }
            }

            // ---- Divider ----
            Rectangle { Layout.fillWidth: true; height: 1; color: "#1f2937"; Layout.topMargin: 4 }

            // ---- Body: signal rail + transcript ----
            RowLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.topMargin: 12
                spacing: 16

                // Signal rail
                ColumnLayout {
                    width: 120
                    Layout.alignment: Qt.AlignTop
                    spacing: 12

                    ColumnLayout {
                        spacing: 4
                        Text { text: "Phase"; color: "#6b7280"; font { pixelSize: 10; capitalization: Font.AllUppercase; letterSpacing: 1 } }
                        RowLayout {
                            spacing: 4
                            Repeater {
                                model: 4
                                Rectangle {
                                    width: 20
                                    height: 6 + index * 4
                                    radius: 2
                                    color: statusColor(overlay.status)
                                    opacity: (index + 1) / 4.0
                                }
                            }
                        }
                    }

                    ColumnLayout {
                        spacing: 4
                        Text { text: "Contract"; color: "#6b7280"; font { pixelSize: 10; capitalization: Font.AllUppercase; letterSpacing: 1 } }
                        Text {
                            text: "Commands resize the window and update state. Signals flow from Rust to QML."
                            color: "#9ca3af"; font.pixelSize: 11
                            wrapMode: Text.Wrap
                            width: 120
                        }
                    }
                }

                // Transcript panel
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 8

                    RowLayout {
                        Layout.fillWidth: true
                        ColumnLayout {
                            spacing: 2
                            Text { text: "Transcript"; color: "#6b7280"; font { pixelSize: 10; capitalization: Font.AllUppercase; letterSpacing: 1 } }
                            Text { text: "Committed words stay dominant"; color: "#f9fafb"; font { pixelSize: 14; weight: Font.SemiBold } }
                        }
                        Item { Layout.fillWidth: true }
                        Rectangle {
                            visible: overlay.errorMessage !== ""
                            color: "#7f1d1d"; radius: 4
                            width: errText.implicitWidth + 16; height: 26
                            Text {
                                id: errText
                                anchors.centerIn: parent
                                text: overlay.errorMessage
                                color: "#fca5a5"; font.pixelSize: 11
                            }
                        }
                    }

                    Flickable {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        contentHeight: transcriptCol.implicitHeight
                        clip: true

                        ColumnLayout {
                            id: transcriptCol
                            width: parent.width
                            spacing: 8

                            // Final transcript block
                            Rectangle {
                                Layout.fillWidth: true
                                height: Math.max(finalCol.implicitHeight + 16, 60)
                                color: "#1f2937"; radius: 6

                                ColumnLayout {
                                    id: finalCol
                                    anchors { fill: parent; margins: 10 }
                                    spacing: 4
                                    Text { text: "Final text"; color: "#6b7280"; font { pixelSize: 10; capitalization: Font.AllUppercase } }
                                    Text {
                                        text: overlay.finalTranscript || "Final transcript will appear here once the demo commits it."
                                        color: overlay.finalTranscript ? "#f9fafb" : "#4b5563"
                                        font.pixelSize: 13
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }
                            }

                            // Partial transcript block
                            Rectangle {
                                Layout.fillWidth: true
                                height: Math.max(partialCol.implicitHeight + 16, 48)
                                color: "#1a2336"; radius: 6

                                ColumnLayout {
                                    id: partialCol
                                    anchors { fill: parent; margins: 10 }
                                    spacing: 4
                                    Text { text: "Live partials"; color: "#6b7280"; font { pixelSize: 10; capitalization: Font.AllUppercase } }
                                    Text {
                                        text: overlay.partialTranscript || "Listening state keeps provisional text visible here."
                                        color: overlay.partialTranscript ? "#93c5fd" : "#4b5563"
                                        font.pixelSize: 13
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Footer controls ----
            Rectangle { Layout.fillWidth: true; height: 1; color: "#1f2937"; Layout.topMargin: 8 }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 10
                spacing: 6

                // Primary controls
                RowLayout {
                    spacing: 6

                    Button {
                        visible: overlay.status === "idle"
                                 || overlay.status === "ready"
                                 || overlay.status === "error"
                        text: "Run demo"
                        onClicked: {
                            overlay.start_pipeline()
                            demoTimer.start()
                        }

                        contentItem: Text { text: parent.text; color: "white"; font.pixelSize: 12; horizontalAlignment: Text.AlignHCenter }
                        background: Rectangle { color: parent.pressed ? "#1d4ed8" : "#2563eb"; radius: 5 }
                    }

                    Repeater {
                        model: [
                            { label: "Stop",    action: function() { overlay.stop_pipeline(); demoTimer.stop() } },
                            { label: overlay.paused ? "Resume" : "Pause", action: function() { overlay.toggle_pause() } },
                            { label: "Clear",   action: function() { overlay.clear_transcript(); demoTimer.stop() } },
                        ]
                        Button {
                            required property var modelData
                            text: modelData.label
                            onClicked: modelData.action()

                            contentItem: Text { text: parent.text; color: "#d1d5db"; font.pixelSize: 12; horizontalAlignment: Text.AlignHCenter }
                            background: Rectangle { color: parent.pressed ? "#374151" : "#1f2937"; radius: 5; border.color: "#374151"; border.width: 1 }
                        }
                    }
                }

                Item { Layout.fillWidth: true }

                Button {
                    text: "Settings"
                    onClicked: overlay.open_settings()

                    contentItem: Text { text: parent.text; color: "#9ca3af"; font.pixelSize: 12; horizontalAlignment: Text.AlignHCenter }
                    background: Rectangle { color: parent.pressed ? "#374151" : "transparent"; radius: 5; border.color: "#374151"; border.width: 1 }
                }
            }
        }
    }
}
