// scale_tail's own settings, followed through the files they are kept in (`config`, watched from
// the first time the tab is shown), the version, and "Check for updates".
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    property var watching: null
    property bool checking: false
    property var update: null

    function show(result) {
        // The page was closed while the answer was on its way.
        if (!Logic || !section.host)
            return;
        if (Logic.isCancelled(result) || Logic.isBusyAnswer(result))
            return;
        section.host.config = Logic.parseConfig(result);
    }

    /// Follows the files from the first time the tab is shown, also when Settings opens the
    /// page on this tab.
    function autoload() {
        if (section.visible && section.host && section.host.ctx && section.watching === null)
            section.watching = section.host.ctx.command.watch("config", section.show);
    }

    function checkUpdates() {
        if (!section.host || !section.host.ctx || section.checking)
            return;
        section.checking = true;
        section.update = null;
        section.host.ctx.command.run("update-check", [], result => {
            if (!Logic)
                return;
            section.checking = false;
            const env = Logic.envelope(result);
            if (!env.ok) {
                section.update = { text: "Couldn't check: " + env.error, failed: true };
                return;
            }
            const d = env.doc;
            section.update = { text: d.available === true ? "Version " + Logic.cut(d.latest, 40) + " is out. Update through your package manager or AppImage launcher." : "You're up to date.", failed: false };
        });
    }

    onVisibleChanged: section.autoload()

    Connections {
        target: section.host
        function onCtxChanged() {
            section.autoload();
        }
    }

    spacing: 8

    UqLabel {
        text: section.host && section.host.config && !section.host.config.ok ? "Couldn't read scale_tail's settings: " + section.host.config.problem : ""
        visible: text !== ""
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    SwitchSetting {
        host: section.host
        settingKey: "autostart"
    }
    SwitchSetting {
        host: section.host
        settingKey: "auto_connect"
        anchor: false
    }
    SwitchSetting {
        host: section.host
        settingKey: "update_check"
        anchor: false
    }
    RowLayout {
        spacing: 12
        Layout.fillWidth: true

        UqLabel {
            objectName: "version"
            text: section.host && section.host.status && section.host.status.app.version !== "" ? "scale_tail " + section.host.status.app.version : "scale_tail"
            Layout.fillWidth: true
        }
        QQC2.Button {
            objectName: "checkUpdates"
            text: "Check for updates"
            enabled: !section.checking
            onClicked: section.checkUpdates()
        }
    }
    QQC2.ProgressBar {
        indeterminate: true
        visible: section.checking
        Layout.fillWidth: true
    }
    UqLabel {
        objectName: "updateText"
        text: section.update ? section.update.text : ""
        visible: text !== ""
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
}
