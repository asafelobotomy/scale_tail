// scale_tail's Tailscale page for uQ Settings (docs/settings-pages.md in the uQ repository),
// laid out like Settings' own pages (uq_api = 3): five tabs of cards. Everything runs through
// the commands declared in page.toml, never a shell, and all text goes through UqLabel (plain
// text only). The pure parts live in logic.js beside this file.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

SettingsPage {
    id: page

    property var ctx
    /// The search row to scroll to and flash (the [[setting]] ids of page.toml).
    property string highlight
    /// What the last reads said (see logic.js): null until the first answer, and when the last
    /// read failed.
    property var status: null
    property var config: null
    property string statusError: ""
    property bool loaded: false
    /// The key of the row whose change is running, "" when none. One change runs at a time.
    property string workingKey: ""
    /// What each change said, by row key: {message, failed, hint}.
    property var notes: ({})
    /// Each device's ping, by target: {running, percent, text, failed}.
    property var pings: ({})
    readonly property bool pingBusy: Object.keys(page.pings).some(k => page.pings[k].running)
    readonly property bool usable: Logic.canToggle(page.status) && page.workingKey === ""
    readonly property string signInUrl: Logic.signInUrl(page.status)
    // Set when the page is being destroyed: answers that arrive later are dropped.
    property bool dead: false
    property bool rereading: false
    property var openedWith: null

    pageId: "scale-tail"

    function noteFor(key) {
        return page.notes[key] || null;
    }

    function errorColor() {
        const colors = page.ctx && page.ctx.theme ? page.ctx.theme.colors : null;
        return (colors && colors.error) || "red";
    }

    function copyText(text) {
        clipboard.text = text;
        clipboard.selectAll();
        clipboard.copy();
    }

    function readStatus() {
        if (!page.ctx)
            return;
        page.ctx.command.run("status", [], result => {
            if (page.dead)
                return;
            // The same read was still running: read again after it.
            if (Logic.isBusyAnswer(result)) {
                page.rereading = true;
                return;
            }
            const next = Logic.parseStatus(result);
            const shown = next.ok ? next : null;
            // Only a real change replaces what is shown, so a poll that finds nothing new leaves
            // the picker (and a list that is open) alone.
            if (!Logic.same(page.status, shown))
                page.status = shown;
            page.statusError = next.ok ? "" : next.error;
            page.loaded = true;
            if (page.rereading) {
                page.rereading = false;
                page.readStatus();
            }
        });
    }

    /// Runs one change (a command that may ask first, and shows a plan when it has a preview),
    /// then reads again. A cancelled question changes nothing and shows nothing.
    function write(key, command, args, done) {
        if (page.workingKey !== "" || !page.ctx)
            return;
        const why = Logic.argsProblem(command, args);
        if (why !== "") {
            page.notes = Logic.withNote(page.notes, key, { message: why, failed: true, hint: "" });
            return;
        }
        page.notes = Logic.withNote(page.notes, key, null);
        page.workingKey = key;
        page.ctx.command.run(command, args, result => {
            if (page.dead)
                return;
            page.workingKey = "";
            const note = Logic.outcome(result, done);
            if (note === null)
                return;
            page.notes = Logic.withNote(page.notes, key, note);
            page.readStatus();
        });
    }

    /// Sets a switch: the careful ones ask first, the others just change.
    function setting(key, want) {
        const step = Logic.configStep(key, want);
        page.write("set:" + key, step.command, step.args, "");
    }

    function setPing(target, state) {
        const next = {};
        for (const k in page.pings)
            next[k] = page.pings[k];
        next[target] = state;
        page.pings = next;
    }

    /// Pings a device three times; the stream's lines move the bar, and its last line says how
    /// it went.
    function ping(target) {
        if (!page.ctx || page.pingBusy || Logic.argsProblem("ping", [target]) !== "")
            return;
        page.setPing(target, Logic.PING_START);
        page.ctx.command.stream("ping", [target], ev => {
            if (!page.dead)
                page.setPing(target, Logic.pingStep(page.pings[target] || Logic.PING_START, ev));
        }, result => {
            if (page.dead || !page.pings[target] || !page.pings[target].running)
                return;
            // The stream ended without its summary.
            const env = Logic.envelope(result);
            page.setPing(target, { running: false, percent: 100, text: env.error !== "" ? "No reply: " + env.error : "No reply", failed: true });
        });
    }

    onCtxChanged: {
        if (page.ctx && page.ctx !== page.openedWith) {
            page.openedWith = page.ctx;
            page.readStatus();
        }
    }
    Component.onDestruction: page.dead = true

    // tailscaled sends no change signal and its state file is root-only, so there is nothing to
    // watch: read every 5 s while the page is shown, and not while a change runs.
    Timer {
        interval: 5000
        repeat: true
        running: !!page.ctx && page.visible && Qt.application.state === Qt.ApplicationActive && page.workingKey === ""
        onTriggered: page.readStatus()
    }

    headerItems: [
        // Holds text for Copy; never shown. (Not a child of the page: only tabs belong there.)
        QQC2.TextField {
            id: clipboard
            visible: false
        },
        QQC2.Button {
            objectName: "refresh"
            text: "Refresh"
            onClicked: page.readStatus()
        },
        QQC2.Button {
            objectName: "openApp"
            text: "Open scale_tail"
            // Starts the tray, which also sends files (Taildrop); the tray stays off under uQ otherwise.
            onClicked: if (page.ctx) page.ctx.openApp("io.github.asafelobotomy.scale_tail.desktop")
        }
    ]

    banners: [
        ErrorCard {
            objectName: "readFailed"
            visible: page.statusError !== ""
            title: Logic.STRINGS.readFailed
            detail: page.statusError
            Layout.fillWidth: true
        },
        ErrorCard {
            objectName: "notRunning"
            visible: !!page.status && page.status.state === "unavailable"
            title: Logic.STRINGS.notRunning
            detail: Logic.STRINGS.notRunningDetail + (Logic.rawOf(page.status) !== "" ? "\n" + Logic.rawOf(page.status) : "")
            Layout.fillWidth: true
        },
        // A login is needed: show the link as text to select and copy. Tailscale is never turned
        // on from here in this state.
        ColumnLayout {
            objectName: "signIn"
            visible: !!page.status && page.status.state === "needs_login"
            spacing: 6
            Layout.fillWidth: true

            UqLabel {
                text: page.signInUrl !== "" ? Logic.STRINGS.signInLink : Logic.STRINGS.signInTerminal
                font.bold: true
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            RowLayout {
                visible: page.signInUrl !== ""
                Layout.fillWidth: true

                // The link is checked by logic.js (plain https only) before it gets here.
                QQC2.TextField {
                    id: urlField
                    objectName: "signInUrl"
                    readOnly: true
                    selectByMouse: true
                    text: page.signInUrl
                    Accessible.name: "Sign-in link"
                    Layout.fillWidth: true
                }
                QQC2.Button {
                    objectName: "copyUrl"
                    text: "Copy link"
                    onClicked: page.copyText(page.signInUrl)
                }
            }
        }
    ]

    SettingsTab {
        title: "Connection"
        iconName: "network-vpn"

        SettingsCard {
            title: "Tailscale"
            iconName: "network-vpn"
            help: "Turn your private network on or off for this computer."
            wide: true
            Layout.fillWidth: true

            ConnectionSection {
                host: page
                Layout.fillWidth: true
            }
        }
        SettingsCard {
            title: "Exit node"
            iconName: "network-wired"
            help: "Send all your internet traffic through one of your other devices."
            wide: true
            Layout.fillWidth: true

            ExitSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }

    SettingsTab {
        title: "Devices"
        iconName: "computer"

        SettingsCard {
            title: "Devices"
            iconName: "computer"
            help: "Everything else on your tailnet. Ping shows how fast a device answers."
            wide: true
            Layout.fillWidth: true

            DevicesSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }

    SettingsTab {
        title: "Network"
        iconName: "preferences-system-network"

        SettingsCard {
            title: "Names and routes"
            iconName: "network-wired"
            help: "How this computer finds the rest of your tailnet."
            Layout.fillWidth: true

            SwitchSetting {
                host: page
                settingKey: "accept_dns"
            }
            SwitchSetting {
                host: page
                settingKey: "accept_routes"
            }
            SwitchSetting {
                host: page
                settingKey: "lan_access"
            }
        }
        SettingsCard {
            title: "Who can reach this computer"
            iconName: "security-medium"
            help: "Each of these asks first and shows what it would change."
            Layout.fillWidth: true

            SwitchSetting {
                host: page
                settingKey: "shields_up"
            }
            SwitchSetting {
                host: page
                settingKey: "ssh"
            }
            SwitchSetting {
                host: page
                settingKey: "advertise_exit_node"
            }
        }
    }

    SettingsTab {
        title: "Account"
        iconName: "user-identity"

        SettingsCard {
            title: "Signed in as"
            iconName: "user-identity"
            help: "Signing out asks first and shows what it would change."
            Layout.fillWidth: true

            AccountSection {
                host: page
                part: "current"
                Layout.fillWidth: true
            }
        }
        SettingsCard {
            title: "Other accounts"
            iconName: "system-switch-user"
            help: "Accounts already signed in on this computer."
            Layout.fillWidth: true

            AccountSection {
                host: page
                part: "others"
                Layout.fillWidth: true
            }
        }
    }

    SettingsTab {
        title: "Preferences"
        iconName: "configure"

        SettingsCard {
            title: "scale_tail"
            iconName: "configure"
            help: "scale_tail's own settings, followed live."
            wide: true
            Layout.fillWidth: true

            PreferencesSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }
}
