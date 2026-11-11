// scale_tail's Tailscale page for uQ Settings (docs/settings-pages.md in the uQ repository).
// Everything runs through the commands declared in page.toml, never a shell, and all text goes
// through UqLabel (plain text only). The pure parts live in logic.js beside this file.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

Item {
    id: page

    property var ctx
    /// The search row to scroll to and flash (the [[setting]] ids of page.toml).
    property string highlight

    // What the last reads said (see logic.js): null until the first answer.
    property var status: null
    property var peers: null
    property var prefs: null
    property string statusError: ""
    // A change is running: the controls wait for it.
    property bool busy: false
    // Why the last change failed, or the operator hint; "" when there is nothing to say.
    property string problem: ""
    property bool detailsShown: false
    // Set when the page is being destroyed: answers that arrive later are dropped.
    property bool dead: false
    // A read found another one still running: read everything again after the next answer.
    property bool wantAll: false

    readonly property var exitInfo: Logic.exitChoices(page.status)
    readonly property string signInUrl: Logic.signInUrl(page.status)
    readonly property bool usable: Logic.canToggle(page.status) && !page.busy
    // The raw text behind a failed or unusual state, shown only under Advanced details.
    readonly property string rawText: page.statusError !== "" ? page.statusError : Logic.rawOf(page.status)
    readonly property var colors: page.ctx && page.ctx.theme && page.ctx.theme.colors ? page.ctx.theme.colors : ({})

    function readStatus(everything) {
        if (!page.ctx)
            return;
        page.ctx.command.run("status", [], function (result) {
            if (page.dead || !Logic)
                return;
            page.gotStatus(result, everything);
        });
    }

    function gotStatus(result, everything) {
        // The same read was still running: keep what is shown and read again after the next one.
        if (Logic.isBusyAnswer(result)) {
            page.wantAll = page.wantAll || everything;
            return;
        }
        const next = Logic.parseStatus(result);
        const reread = everything || page.wantAll || Logic.stateChanged(page.status, next);
        page.wantAll = false;
        // Only a real change replaces what is shown, so a poll that finds nothing new leaves
        // the picker (and a list that is open) alone.
        const shown = next.ok ? next : null;
        if (!Logic.same(page.status, shown))
            page.status = shown;
        page.statusError = next.ok ? "" : next.error;
        if (reread && Logic.readsDetail(next)) {
            page.readPeers();
            page.readPrefs();
        }
    }

    function readPeers() {
        page.ctx.command.run("peers", [], function (result) {
            if (page.dead || !Logic || Logic.isBusyAnswer(result))
                return;
            const next = Logic.parsePeers(result);
            if (!Logic.same(page.peers, next))
                page.peers = next;
        });
    }

    function readPrefs() {
        page.ctx.command.run("prefs", [], function (result) {
            if (page.dead || !Logic || Logic.isBusyAnswer(result))
                return;
            const next = Logic.parsePrefs(result);
            if (!Logic.same(page.prefs, next))
                page.prefs = next;
        });
    }

    // Run one change, then read everything again. A switch or the picker keeps showing what was
    // last read until then, so a refused change goes back, with the reason shown below.
    function act(id, args) {
        // Nothing changes while Tailscale can't be switched (no login, not running), whatever
        // the controls say: `up` in particular would wait for a browser login.
        if (!page.ctx || page.busy || !Logic.canToggle(page.status))
            return;
        page.busy = true;
        page.problem = "";
        page.ctx.command.run(id, args, function (result) {
            if (page.dead || !Logic)
                return;
            page.busy = false;
            page.problem = Logic.writeProblem(result);
            // Cancelling a question changes nothing, so there is nothing to read again.
            if (!(result && result.cancelled))
                page.readStatus(true);
        });
    }

    function pickExit(index) {
        // Back to the value read until the change is confirmed.
        exitBox.currentIndex = Qt.binding(() => page.exitInfo.index);
        if (index === page.exitInfo.index)
            return;
        if (index === 0)
            page.act("exit-node-off", []);
        else if (Logic.checkTarget(page.exitInfo.targets[index]))
            page.act("exit-node", [page.exitInfo.targets[index]]);
    }

    function reveal(id) {
        const rows = { "connection": connectionRow, "exit-node": exitRow, "routes": routesRow, "lan-access": lanRow, "peers": peersRow, "account": accountRow };
        const row = rows[id];
        if (!row)
            return;
        const y = row.mapToItem(column, 0, 0).y;
        flick.contentY = Math.max(0, Math.min(y - 8, flick.contentHeight - flick.height));
    }

    Component.onDestruction: page.dead = true

    onHighlightChanged: Qt.callLater(() => page.reveal(page.highlight))

    // Read when the context arrives, and every 5 s while the page is shown: tailscaled sends no
    // change signal and its state file is root-only, so there is nothing to watch.
    onCtxChanged: page.readStatus(true)

    Timer {
        interval: 5000
        repeat: true
        running: !!page.ctx && page.visible && Qt.application.state === Qt.ApplicationActive && !page.busy
        onTriggered: page.readStatus(false)
    }

    Flickable {
        id: flick
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight + 32
        clip: true
        QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

        ColumnLayout {
            id: column
            x: 16
            y: 16
            width: flick.width - 32
            spacing: 12

            UqLabel {
                visible: page.statusError === ""
                text: Logic.stateLine(page.status)
                font.bold: true
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            ErrorCard {
                visible: page.statusError !== ""
                title: Logic.STRINGS.readFailed
                detail: Logic.STRINGS.readFailedDetail
                Layout.fillWidth: true
            }
            QQC2.Button {
                objectName: "advanced"
                visible: page.rawText !== ""
                text: "Advanced details"
                onClicked: page.detailsShown = !page.detailsShown
            }
            UqLabel {
                visible: page.detailsShown && page.rawText !== ""
                text: page.rawText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            UqLabel {
                visible: page.busy
                text: Logic.STRINGS.working
                Layout.fillWidth: true
            }
            UqLabel {
                visible: Logic.canToggle(page.status) && ((!!page.peers && page.peers.ok === false) || (!!page.prefs && page.prefs.ok === false))
                text: Logic.STRINGS.detailFailed
                color: page.colors.error || "red"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            UqLabel {
                visible: page.problem !== ""
                text: page.problem
                color: page.colors.error || "red"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // A login is needed: show the link as text to select and copy. Tailscale is never
            // turned on from here in this state.
            ColumnLayout {
                visible: !!page.status && page.status.state === "needs_login"
                spacing: 6
                Layout.fillWidth: true

                UqLabel {
                    text: page.signInUrl !== "" ? Logic.STRINGS.signInLink : Logic.STRINGS.signInTerminal
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
                        onClicked: {
                            urlField.selectAll();
                            urlField.copy();
                        }
                    }
                }
            }

            SwitchRow {
                id: connectionRow
                objectName: "connection"
                settingId: "connection"
                label: "Tailscale connection"
                highlight: page.highlight
                enabled: page.usable
                value: !!page.status && page.status.connected
                onChosen: v => page.act(v ? "up" : "down", [])
                Layout.fillWidth: true
            }

            SettingRow {
                id: exitRow
                settingId: "exit-node"
                label: "Exit node"
                highlight: page.highlight
                Layout.fillWidth: true

                UqComboBox {
                    id: exitBox
                    objectName: "exitBox"
                    enabled: page.usable
                    model: page.exitInfo.labels
                    currentIndex: page.exitInfo.index
                    // A new list resets the index to the first row: put the one in use back.
                    onModelChanged: currentIndex = Qt.binding(() => page.exitInfo.index)
                    implicitWidth: 220
                    onActivated: index => page.pickExit(index)
                }
            }

            SwitchRow {
                id: routesRow
                objectName: "routes"
                settingId: "routes"
                label: "Use routes from other devices"
                highlight: page.highlight
                enabled: page.usable && !!page.prefs && page.prefs.ok === true
                value: !!page.prefs && page.prefs.routes === true
                onChosen: v => page.act(v ? "routes-on" : "routes-off", [])
                Layout.fillWidth: true
            }

            SwitchRow {
                id: lanRow
                objectName: "lan"
                settingId: "lan-access"
                label: "Reach the local network while using an exit node"
                highlight: page.highlight
                enabled: page.usable && !!page.prefs && page.prefs.ok === true
                value: !!page.prefs && page.prefs.lan === true
                onChosen: v => page.act(v ? "lan-on" : "lan-off", [])
                Layout.fillWidth: true
            }

            SettingRow {
                id: accountRow
                settingId: "account"
                label: "Account"
                highlight: page.highlight
                Layout.fillWidth: true

                UqLabel {
                    text: Logic.accountLine(page.status, page.peers)
                }
                QQC2.Button {
                    objectName: "signOut"
                    visible: Logic.canToggle(page.status)
                    enabled: !page.busy
                    text: "Sign out…"
                    // Settings asks first (page.toml `confirm`); this runs only on Confirm.
                    onClicked: page.act("logout", [])
                }
            }

            SettingRow {
                id: peersRow
                settingId: "peers"
                label: "Devices"
                highlight: page.highlight
                Layout.fillWidth: true

                QQC2.Button {
                    text: "Refresh"
                    enabled: !page.busy
                    onClicked: page.readStatus(true)
                }
            }
            UqLabel {
                visible: !!page.peers && page.peers.ok === true && page.peers.peers.length === 0
                text: Logic.STRINGS.noDevices
                Layout.fillWidth: true
            }
            Repeater {
                model: page.peers && page.peers.ok ? page.peers.peers : []

                delegate: RowLayout {
                    required property var modelData
                    spacing: 12
                    Layout.fillWidth: true

                    UqLabel {
                        text: modelData.name
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    UqLabel {
                        text: Logic.presence(modelData)
                        color: modelData.online ? (page.colors.text || "black") : (page.colors.text_muted || "gray")
                    }
                    UqLabel {
                        text: modelData.os
                        color: page.colors.text_muted || "gray"
                        Layout.preferredWidth: 90
                        elide: Text.ElideRight
                    }
                }
            }
            UqLabel {
                visible: !!page.peers && page.peers.ok === true && page.peers.more
                text: Logic.STRINGS.morePeers
                color: page.colors.text_muted || "gray"
                Layout.fillWidth: true
            }
        }
    }
}
