// scale_tail's quick-settings tile for uQ (docs/integrating.md in the uQ repository).
// The row shows Tailscale's state and the current exit node, with an on/off switch; a click on
// the row opens the detail view with the exit-node picker. Everything goes through the commands
// declared in widget.toml; all text through ctx.UqText (plain text only).
import QtQuick
import QtQuick.Layouts

Item {
    id: widget

    property var ctx
    // `scale_tail status --json`, or null before the first answer.
    property var status: null
    // Why the last status read or the last action failed; shown in the detail view.
    property string statusError: ""
    property string actionError: ""
    readonly property string problem: actionError !== "" ? actionError : statusError
    // A command is running: the switch and the picker wait for it.
    property bool busy: false
    property bool detailShown: false

    readonly property bool connected: status !== null && status.connected === true
    readonly property var exitNodes: status && Array.isArray(status.exit_nodes) ? status.exit_nodes : []
    readonly property string exitName: status && status.exit_node ? String(status.exit_node.name) : ""

    function stateLine() {
        if (status === null)
            return problem !== "" ? "Unavailable" : "Checking…";
        switch (status.state) {
        case "running":
            return exitName !== "" ? "Exit node: " + exitName : "Connected";
        case "stopped":
            return "Off";
        case "needs_login":
            return "Needs login";
        case "unavailable":
            return "Not running";
        default:
            return String(status.message || status.state);
        }
    }

    function refresh() {
        if (!ctx)
            return;
        ctx.command.run("status", [], function (r) {
            // A non-zero exit still carries the JSON when there is one.
            var out = r.output;
            if (out && typeof out === "object" && out.version === 1) {
                widget.status = out;
                widget.statusError = "";
            } else {
                widget.status = null;
                widget.statusError = String(r.error || "scale_tail gave no status");
            }
        });
    }

    // Run a declared command, then read the state again.
    function act(id, args) {
        if (!ctx || busy)
            return;
        busy = true;
        actionError = "";
        ctx.command.run(id, args, function (r) {
            widget.busy = false;
            if (r.error !== undefined) {
                var e = String(r.error);
                widget.actionError = /access denied|operator/i.test(e)
                    ? e + " (run once as root: tailscale set --operator=$USER)" : e;
            }
            widget.refresh();
        });
    }

    function toggle() {
        if (status === null || status.state === "unavailable")
            return;
        act(connected ? "down" : "up", []);
    }

    Component.onCompleted: refresh()

    Timer {
        interval: widget.detailShown ? 5000 : 15000
        running: widget.ctx !== undefined && widget.ctx !== null
        repeat: true
        onTriggered: widget.refresh()
    }

    // One line of plain text through the shell's UqText. Inline components don't see the
    // ids of this file, so `uq` carries the ctx.
    component Line: Loader {
        property var uq: null
        property string line: ""
        property color tint: uq ? uq.Theme.text : "white"
        property bool strong: false
        sourceComponent: uq ? uq.UqText : null
        onLoaded: {
            item.text = Qt.binding(() => line);
            item.color = Qt.binding(() => tint);
            item.font.bold = Qt.binding(() => strong);
        }
    }

    // A clickable row in the detail view.
    component Choice: Rectangle {
        id: choice
        property var uq: null
        property string line: ""
        property bool checked: false
        property bool usable: true
        property bool busy: false
        signal picked()
        Layout.fillWidth: true
        implicitHeight: 28
        radius: uq ? uq.Theme.radius - 2 : 4
        color: hover.containsMouse && usable && uq ? uq.Theme.overlay : "transparent"
        opacity: usable ? 1 : 0.5
        Accessible.role: Accessible.RadioButton
        Accessible.checkable: true
        Accessible.checked: checked
        Accessible.name: line
        Accessible.onPressAction: if (usable && !busy) picked()
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 8
            Rectangle {
                implicitWidth: 10
                implicitHeight: 10
                radius: 5
                color: choice.checked && choice.uq ? choice.uq.Theme.accent : "transparent"
                border.width: 1
                border.color: choice.uq ? choice.uq.Theme.textMuted : "gray"
            }
            Line {
                Layout.fillWidth: true
                uq: choice.uq
                line: choice.line
            }
        }
        MouseArea {
            id: hover
            anchors.fill: parent
            hoverEnabled: true
            enabled: choice.usable && !choice.busy
            onClicked: choice.picked()
        }
    }

    // The detail view under the tile: the exit-node picker and the last error.
    property Component popup: Component {
        ColumnLayout {
            property var ctx
            spacing: 2
            Component.onCompleted: widget.detailShown = true
            Component.onDestruction: widget.detailShown = false

            Line {
                Layout.fillWidth: true
                uq: widget.ctx
                visible: widget.problem !== ""
                line: widget.problem
                tint: widget.ctx ? widget.ctx.Theme.error : "red"
            }
            Line {
                Layout.fillWidth: true
                uq: widget.ctx
                line: widget.connected ? "Exit node" : "Turn Tailscale on to pick an exit node"
                tint: widget.ctx ? widget.ctx.Theme.textMuted : "gray"
                strong: widget.connected
            }
            Choice {
                uq: widget.ctx
                busy: widget.busy
                visible: widget.connected
                line: "None"
                checked: widget.exitName === ""
                onPicked: widget.act("exit-node-none", [])
            }
            Choice {
                uq: widget.ctx
                busy: widget.busy
                visible: widget.connected && widget.exitNodes.length > 0
                line: "Use the suggested exit node"
                onPicked: widget.act("exit-node-suggested", [])
            }
            Repeater {
                model: widget.connected ? widget.exitNodes : []
                delegate: Choice {
                    required property var modelData
                    uq: widget.ctx
                    busy: widget.busy
                    line: String(modelData.name) + (modelData.online ? "" : " (offline)")
                    checked: modelData.active === true
                    usable: modelData.online === true
                    onPicked: widget.act("exit-node", [String(modelData.target)])
                }
            }
            Line {
                Layout.fillWidth: true
                uq: widget.ctx
                visible: widget.connected && widget.exitNodes.length === 0
                line: "No exit nodes on this tailnet"
                tint: widget.ctx ? widget.ctx.Theme.textMuted : "gray"
            }
        }
    }

    // The tile: a full-width 32 px row, like the toggle rows above it.
    Rectangle {
        anchors.fill: parent
        radius: widget.ctx ? widget.ctx.Theme.radius - 2 : 4
        color: widget.ctx ? (rowMouse.containsMouse ? widget.ctx.Theme.overlay : widget.ctx.Theme.surfaceAlt)
                          : "transparent"
    }
    MouseArea {
        id: rowMouse
        anchors.fill: parent
        hoverEnabled: true
        onClicked: {
            if (widget.detailShown)
                widget.ctx.closePopup();
            else
                widget.ctx.openPopup();
        }
    }
    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 10
        anchors.rightMargin: 10
        spacing: 8
        Line {
            Layout.fillWidth: true
            uq: widget.ctx
            line: "Tailscale · " + widget.stateLine() + (widget.problem !== "" ? " · !" : "")
        }
        // The on/off switch, drawn like the shell's toggle rows.
        Rectangle {
            id: knob
            implicitWidth: 30
            implicitHeight: 16
            radius: 8
            opacity: widget.busy || widget.status === null || widget.status.state === "unavailable" ? 0.5 : 1
            color: widget.ctx ? (widget.connected ? widget.ctx.Theme.accent : widget.ctx.Theme.overlay) : "gray"
            Accessible.role: Accessible.CheckBox
            Accessible.checkable: true
            Accessible.checked: widget.connected
            Accessible.name: "Tailscale"
            Accessible.onPressAction: widget.toggle()
            Accessible.onToggleAction: widget.toggle()
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                x: widget.connected ? parent.width - width - 2 : 2
                width: 12
                height: 12
                radius: 6
                color: widget.ctx ? (widget.connected ? widget.ctx.Theme.accentText : widget.ctx.Theme.textMuted) : "white"
            }
            MouseArea {
                anchors.fill: parent
                anchors.margins: -6
                enabled: !widget.busy
                onClicked: widget.toggle()
            }
        }
    }
}
