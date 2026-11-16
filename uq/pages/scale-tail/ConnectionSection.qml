// The Connection card: whether Tailscale is on, with the tailnet and this computer's name, and
// the switch. A progress bar stands in until the first answer arrives.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var info: Logic.stateInfo(section.host ? section.host.status : null, section.host ? section.host.statusError !== "" : false)

    spacing: 8

    QQC2.ProgressBar {
        objectName: "skeleton"
        indeterminate: true
        visible: !!section.host && !section.host.loaded
        Layout.fillWidth: true
    }
    RowLayout {
        spacing: 12
        Layout.fillWidth: true

        UqAppIcon {
            name: "network-vpn"
            label: "Tailscale"
            size: 40
        }
        ColumnLayout {
            spacing: 0
            Layout.fillWidth: true

            UqLabel {
                objectName: "stateTitle"
                text: section.info.title
                font.bold: true
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            UqLabel {
                objectName: "stateDetail"
                text: section.info.detail
                visible: text !== ""
                wrapMode: Text.Wrap
                opacity: 0.7
                Layout.fillWidth: true
            }
        }
    }
    SwitchRow {
        objectName: "connection"
        settingId: "connection"
        label: "Tailscale connection"
        highlight: section.host ? section.host.highlight : ""
        enabled: !!section.host && section.host.usable
        value: !!section.host && !!section.host.status && section.host.status.connected
        onChosen: v => section.host.write("connection", v ? "connect" : "disconnect", [], v ? "Connected." : "Disconnected.")
        Layout.fillWidth: true
    }
    UqLabel {
        text: Logic.STRINGS.needsTailscale
        visible: !!section.host && section.host.loaded && !Logic.canToggle(section.host.status)
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    Outcome {
        host: section.host
        noteKey: "connection"
        Layout.fillWidth: true
    }
}
