// The exit node card: the picker, "use suggested", and what is in use. Choosing a device shows
// the plan first (page.toml: `exit-node` has `confirm` and `preview`); the picker goes back to
// the device in use until the change is confirmed.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var choices: Logic.exitChoices(section.host ? section.host.status : null)
    readonly property var active: Logic.activeExit(section.host ? section.host.status : null)

    function pick(index) {
        // Back to the value read until the change is confirmed.
        exitBox.currentIndex = Qt.binding(() => section.choices.index);
        if (index === section.choices.index)
            return;
        if (index === 0)
            section.host.write("exit", "exit-node-none", [], "Your internet traffic goes out directly again.");
        else
            section.host.write("exit", "exit-node", [section.choices.targets[index]], "Exit node changed.");
    }

    spacing: 8

    SettingRow {
        settingId: "exit-node"
        label: "Exit node"
        highlight: section.host ? section.host.highlight : ""
        Layout.fillWidth: true

        UqComboBox {
            id: exitBox
            objectName: "exitBox"
            enabled: !!section.host && section.host.usable
            model: section.choices.labels
            currentIndex: section.choices.index
            // A new list resets the index to the first row: put the one in use back.
            onModelChanged: currentIndex = Qt.binding(() => section.choices.index)
            implicitWidth: 240
            onActivated: index => section.pick(index)
        }
        QQC2.Button {
            objectName: "useSuggested"
            text: "Use suggested"
            enabled: !!section.host && section.host.usable && section.choices.labels.length > 1
            onClicked: section.host.write("exit", "exit-node-suggested", [], "Using the suggested exit node.")
        }
    }
    UqLabel {
        objectName: "exitSummary"
        text: section.active ? "All your internet traffic leaves through " + section.active.name + "." : "Your internet traffic goes out directly."
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    Outcome {
        host: section.host
        noteKey: "exit"
        Layout.fillWidth: true
    }
}
