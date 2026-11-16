// The devices on the tailnet: each with its system's icon, whether it is online, its address
// (Copy), a Ping that streams its progress, and "Use as exit node" where it offers one. Names
// and addresses come from outside, so they are shown as plain text only (UqLabel).
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property bool ready: !!section.host && !!section.host.status && Logic.canToggle(section.host.status)
    readonly property var shown: section.ready ? Logic.filterPeers(section.host.status.peers, filter.text) : []

    spacing: 8

    QQC2.ProgressBar {
        indeterminate: true
        visible: !!section.host && !section.host.loaded
        Layout.fillWidth: true
    }
    SettingRow {
        settingId: "peers"
        label: "Devices on your tailnet"
        highlight: section.host ? section.host.highlight : ""
        visible: section.ready
        Layout.fillWidth: true

        QQC2.TextField {
            id: filter
            objectName: "filter"
            placeholderText: "Search devices"
            implicitWidth: 200
            Accessible.name: "Search devices"
        }
    }
    UqLabel {
        text: Logic.STRINGS.needsTailscale
        visible: !!section.host && section.host.loaded && !section.ready
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    UqLabel {
        objectName: "noDevices"
        text: filter.text.trim() !== "" ? "No device matches that." : Logic.STRINGS.noDevices
        visible: section.ready && section.shown.length === 0
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    Outcome {
        host: section.host
        noteKey: "exit"
        Layout.fillWidth: true
    }
    Repeater {
        model: section.shown

        delegate: ColumnLayout {
            id: device

            required property var modelData
            readonly property var ping: section.host.pings[device.modelData.target] || null
            readonly property string exitText: Logic.exitButton(device.modelData, section.host.status)

            spacing: 4
            Layout.fillWidth: true

            RowLayout {
                spacing: 12
                Layout.fillWidth: true

                UqAppIcon {
                    name: Logic.osIcon(device.modelData.os)
                    label: device.modelData.name
                    size: 32
                    opacity: device.modelData.online ? 1 : 0.5
                }
                ColumnLayout {
                    spacing: 0
                    Layout.fillWidth: true

                    UqLabel {
                        text: device.modelData.name
                        font.bold: true
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    UqLabel {
                        text: Logic.peerDetail(device.modelData) + (device.modelData.active ? " · Exit node" : "")
                        elide: Text.ElideRight
                        opacity: 0.7
                        Layout.fillWidth: true
                    }
                }
                QQC2.Button {
                    objectName: "copy-" + device.modelData.name
                    text: "Copy address"
                    visible: device.modelData.ip !== ""
                    onClicked: section.host.copyText(device.modelData.ip)
                }
                QQC2.Button {
                    objectName: "ping-" + device.modelData.name
                    text: "Ping"
                    visible: device.modelData.target !== ""
                    enabled: device.modelData.online && !section.host.pingBusy
                    onClicked: section.host.ping(device.modelData.target)
                }
                QQC2.Button {
                    objectName: "exit-" + device.modelData.name
                    text: device.exitText
                    visible: device.exitText !== ""
                    enabled: section.host.usable
                    onClicked: {
                        if (device.modelData.active)
                            section.host.write("exit", "exit-node-none", [], "Your internet traffic goes out directly again.");
                        else
                            section.host.write("exit", "exit-node", [device.modelData.target], "Exit node changed.");
                    }
                }
            }
            RowLayout {
                visible: device.ping !== null
                spacing: 8
                Layout.fillWidth: true
                Layout.leftMargin: 44

                QQC2.ProgressBar {
                    objectName: "pingBar-" + device.modelData.name
                    from: 0
                    to: 100
                    value: device.ping ? device.ping.percent : 0
                    visible: !!device.ping && device.ping.running
                    Layout.preferredWidth: 160
                }
                UqLabel {
                    id: pingText
                    objectName: "pingText-" + device.modelData.name
                    text: device.ping ? device.ping.text : ""
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                Binding {
                    target: pingText
                    property: "color"
                    value: section.host.errorColor()
                    when: !!device.ping && device.ping.failed
                }
            }
        }
    }
    UqLabel {
        text: Logic.STRINGS.morePeers
        visible: section.ready && section.host.status.total > section.host.status.peers.length
        opacity: 0.7
        Layout.fillWidth: true
    }
}
