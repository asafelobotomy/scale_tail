// One on/off setting: its switch, a line of help and what the last change said. The switch shows
// what was read, so a refused change goes back by itself, with the reason under it. The careful
// settings show a plan and ask first (their command in page.toml has `confirm` and `preview`).
import QtQuick
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: row

    property var host
    property string settingKey
    /// False for all but the first row of a group that shares one search entry.
    property bool anchor: true
    readonly property var def: Logic.SETTINGS[row.settingKey] || ({ id: "", label: "", help: "" })
    readonly property var current: row.host ? Logic.settingValue(row.settingKey, row.host.status, row.host.config) : undefined

    spacing: 2
    Layout.fillWidth: true

    SwitchRow {
        objectName: "switch-" + row.settingKey
        settingId: row.anchor ? row.def.id : ""
        label: row.def.label
        highlight: row.host ? row.host.highlight : ""
        enabled: !!row.host && row.host.workingKey === "" && Logic.canChange(row.settingKey, row.host.status) && row.current !== undefined
        value: row.current === true
        onChosen: v => row.host.setting(row.settingKey, v)
        Layout.fillWidth: true
    }
    UqLabel {
        text: row.def.help
        visible: text !== ""
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    Outcome {
        host: row.host
        noteKey: "set:" + row.settingKey
        Layout.fillWidth: true
    }
}
