// Loads the uQ quick-settings widget (uq/widgets/scale-tail) with a fake ctx, as uQ's shell
// would, and checks the tile, the detail view and the commands it runs. Run with:
//   QT_QPA_PLATFORM=offscreen qmltestrunner -input tests/uq/tst_widget.qml
import QtQuick
import QtQuick.Layouts
import QtTest

Item {
    id: root
    width: 400; height: 400

    property var calls: []
    property var answers: ({})
    property var theme: ({ text: "white", textMuted: "gray", accent: "blue", accentText: "black",
                           overlay: "#333", surfaceAlt: "#222", error: "red", radius: 10 })
    Component { id: fakeText; Text { textFormat: Text.PlainText; elide: Text.ElideRight } }
    property bool popupOpen: false

    property var ctx: ({
        Theme: theme, theme: theme, UqText: fakeText, UqInput: null, slot: "quick",
        command: { run: function (id, args, cb) {
            root.calls.push({ id: id, args: args });
            var a = root.answers[id] || { code: 0, output: null };
            Qt.callLater(function () { cb(a); });
        } },
        openPopup: function () { root.popupOpen = true; },
        closePopup: function () { root.popupOpen = false; },
        fail: function (r) { throw new Error("failed: " + r); }
    })

    ColumnLayout {
        width: 300
        Loader {
            id: tile
            Layout.fillWidth: true
            Layout.preferredHeight: 32
            source: Qt.resolvedUrl("../../uq/widgets/scale-tail/Widget.qml")
            onLoaded: item.ctx = root.ctx
        }
        Loader {
            id: detail
            Layout.fillWidth: true
            active: root.popupOpen
            sourceComponent: tile.item ? tile.item.popup : null
            onLoaded: item.ctx = root.ctx
        }
    }

    TestCase {
        name: "ScaleTailWidget"
        when: windowShown

        function texts(item, out) {
            out = out || [];
            if (item.text !== undefined && typeof item.text === "string" && item.visible) out.push(item.text);
            for (var i = 0; i < item.children.length; i++) texts(item.children[i], out);
            return out;
        }

        function test_flow() {
            root.answers = { status: { code: 0, output: { version: 1, state: "running", connected: true, message: "",
                tailnet: "t", self: null, exit_node: { name: "vps", target: "vps.ts.net", online: true, active: true },
                exit_nodes: [ { name: "vps", target: "vps.ts.net", online: true, active: true },
                              { name: "home", target: "100.64.0.3", online: false, active: false } ] } } };
            tile.item.refresh();
            tryVerify(function () { return tile.item.connected; });
            tryVerify(function () { return texts(tile.item).indexOf("Tailscale · Exit node: vps") >= 0; }, 2000, JSON.stringify(texts(tile.item)));
            // open the detail
            mouseClick(tile.item, 20, 16);
            verify(root.popupOpen);
            tryVerify(function () { return detail.item !== null && tile.item.detailShown; });
            var t = texts(detail.item);
            verify(t.indexOf("home (offline)") >= 0, JSON.stringify(t));
            verify(t.indexOf("Use the suggested exit node") >= 0);
            // toggle off
            root.calls = [];
            root.answers.down = { code: 1, error: "exit 1: Access denied: prefs write access denied" };
            tile.item.toggle();
            tryVerify(function () { return !tile.item.busy; });
            compare(root.calls[0].id, "down");
            verify(tile.item.problem.indexOf("operator") >= 0, tile.item.problem);
            // pick an exit node
            root.calls = [];
            tile.item.act("exit-node", ["vps.ts.net"]);
            tryVerify(function () { return root.calls.length >= 2; });
            compare(root.calls[0].args, ["vps.ts.net"]);
            compare(root.calls[1].id, "status");
            // close detail
            mouseClick(tile.item, 20, 16);
            verify(!root.popupOpen);
            tryVerify(function () { return !tile.item.detailShown; });
            // unavailable
            root.answers.status = { error: "timed out" };
            tile.item.refresh();
            tryVerify(function () { return tile.item.status === null; });
            tryVerify(function () { return texts(tile.item).indexOf("Tailscale · Unavailable · !") >= 0; }, 2000, JSON.stringify(texts(tile.item)));
        }
    }
}
