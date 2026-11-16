// The Account cards' content: who is signed in, Sign out…, and the other accounts on this
// computer with Switch…. Both changes show a plan first (their commands in page.toml have
// `confirm` and `preview`).
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    /// "current" (who is signed in, sign out) or "others" (the switcher).
    property string part: "current"
    readonly property var others: Logic.otherAccounts(section.host ? section.host.status : null)

    spacing: 8

    SettingRow {
        visible: section.part === "current"
        settingId: "account"
        label: "Account"
        highlight: section.host ? section.host.highlight : ""
        Layout.fillWidth: true

        UqLabel {
            objectName: "accountLine"
            text: Logic.accountLine(section.host ? section.host.status : null)
        }
        QQC2.Button {
            objectName: "signOut"
            text: "Sign out…"
            visible: !!section.host && Logic.canToggle(section.host.status)
            enabled: !!section.host && section.host.workingKey === ""
            onClicked: section.host.write("logout", "logout", [], "Signed out.")
        }
    }
    UqLabel {
        text: section.host && section.host.status && section.host.status.tailnet !== "" ? "Tailnet: " + section.host.status.tailnet : ""
        visible: section.part === "current" && text !== ""
        opacity: 0.7
        Layout.fillWidth: true
    }
    Outcome {
        visible: section.part === "current"
        host: section.host
        noteKey: "logout"
        Layout.fillWidth: true
    }
    UqLabel {
        objectName: section.part === "others" ? "noOthers" : ""
        text: Logic.STRINGS.noOtherAccounts
        visible: section.part === "others" && section.others.length === 0
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    Repeater {
        model: section.part === "others" ? section.others : []

        delegate: ColumnLayout {
            id: account

            required property var modelData

            spacing: 2
            Layout.fillWidth: true

            SettingRow {
                settingId: ""
                label: Logic.accountLabel(account.modelData)
                Layout.fillWidth: true

                QQC2.Button {
                    objectName: "switch-" + account.modelData.id
                    text: "Switch…"
                    enabled: !!section.host && section.host.usable
                    onClicked: section.host.write("switch:" + account.modelData.id, "switch-account", [account.modelData.id], "Switched account.")
                }
            }
            Outcome {
                host: section.host
                noteKey: "switch:" + account.modelData.id
                Layout.fillWidth: true
            }
        }
    }
}
