// What a change said, under its row: a progress bar while it runs, then the answer in plain
// words (the failure in the error colour). A cancelled question shows nothing.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings

ColumnLayout {
    id: out

    /// The page (Page.qml): it holds the notes and which change is running.
    property var host
    /// The row's key in the page's notes.
    property string noteKey
    readonly property var note: out.host ? out.host.noteFor(out.noteKey) : null
    readonly property bool working: !!out.host && out.host.workingKey === out.noteKey

    visible: out.working || (out.note !== null && out.note.message !== "")
    spacing: 2

    QQC2.ProgressBar {
        objectName: "working"
        indeterminate: true
        visible: out.working
        Layout.fillWidth: true
    }
    UqLabel {
        id: message
        text: out.working ? "Working…" : (out.note ? out.note.message : "")
        visible: text !== ""
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    Binding {
        target: message
        property: "color"
        value: out.host ? out.host.errorColor() : "red"
        when: !out.working && out.note !== null && out.note.failed
    }
}
