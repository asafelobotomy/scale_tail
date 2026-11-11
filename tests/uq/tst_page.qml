// The Tailscale settings page with the stub org.uq.settings module and a fake ctx
// (the uQ settings page test kit, uQ v0.3.0-rc.1). Run from the repository root:
//   QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq
import QtQuick
import QtTest

Item {
    id: root
    width: 700
    height: 900

    readonly property url pageUrl: Qt.resolvedUrl("../../uq/pages/scale-tail/Page.qml")
    readonly property string operatorHint: "Tailscale needs you to be its operator once. In a terminal, run: sudo tailscale set --operator=$USER"
    readonly property string loginUrl: "https://login.tailscale.com/a/abc123"

    function status(state, extra) {
        const base = {
            version: 1, state: state, connected: state === "running", message: "", tailnet: "example.com",
            self: { name: "laptop", dns: "laptop.tail1234.ts.net", ip: "100.64.0.1" },
            exit_node: null,
            exit_nodes: [
                { name: "vps", target: "vps.tail1234.ts.net", online: true, active: false },
                { name: "nas", target: "100.64.0.9", online: false, active: false }
            ]
        };
        return { code: 0, output: Object.assign(base, extra || {}) };
    }

    readonly property var peers: ({
            code: 0,
            output: {
                BackendState: "Running",
                Self: { UserID: 42, HostName: "laptop" },
                User: { "42": { LoginName: "me@example.com", DisplayName: "Me" } },
                Peer: {
                    k1: { HostName: "<b>x</b>", DNSName: "x.tail.ts.net.", Online: true, OS: "linux" },
                    k2: { HostName: "‮evil", DNSName: "e.tail.ts.net.", Online: false, OS: "windows" }
                }
            }
        })

    function freshCommands() {
        return {
            status: root.status("stopped"),
            peers: root.peers,
            prefs: { code: 0, output: { RouteAll: false, ExitNodeAllowLANAccess: false } },
            up: { code: 0, output: null },
            down: { code: 0, output: null },
            "exit-node": { code: 0, output: null },
            "exit-node-off": { code: 0, output: null },
            "routes-on": { code: 0, output: null },
            "routes-off": { code: 0, output: null },
            "lan-on": { code: 0, output: null },
            "lan-off": { code: 0, output: null },
            logout: { code: 0, output: null }
        };
    }

    FakeContext {
        id: fake
        commands: root.freshCommands()
        confirmed: ["logout"]
    }

    Loader {
        id: loader
        anchors.fill: parent
    }

    function find(item, match) {
        if (match(item))
            return item;
        for (let i = 0; i < item.children.length; ++i) {
            const hit = find(item.children[i], match);
            if (hit)
                return hit;
        }
        return null;
    }
    function findLabel(text) {
        return find(loader.item, it => it.text === text && it.visible);
    }
    function named(name) {
        return find(loader.item, it => it.objectName === name);
    }
    function ran(id) {
        return fake.calls.filter(c => c[0] === "run" && c[1] === id);
    }

    TestCase {
        name: "TailscalePage"
        when: windowShown

        function init() {
            fake.calls = [];
            fake.confirms = true;
            fake.commands = root.freshCommands();
            loader.source = "";
            loader.source = root.pageUrl;
            verify(loader.status === Loader.Ready, "the page loads");
            loader.item.ctx = fake.ctx;
        }

        function flip(row) {
            row.toggle.toggle();
            row.toggle.toggled();
        }

        function test_reads_on_open() {
            tryVerify(() => root.findLabel("Off") !== null);
            tryVerify(() => root.findLabel("me@example.com") !== null);
            compare(fake.calls[0], ["run", "status", []]);
            verify(root.ran("peers").length === 1);
            verify(root.ran("prefs").length === 1);
            compare(root.ran("up").length, 0, "nothing changes on open");
        }

        function test_switch_up_down() {
            const sw = root.named("connection");
            tryVerify(() => sw.enabled && sw.value === false && root.findLabel("Off") !== null);
            fake.commands.up = () => {
                fake.commands.status = root.status("running");
                return { code: 0, output: null };
            };
            flip(sw);
            tryVerify(() => root.ran("up").length === 1);
            compare(root.ran("up")[0], ["run", "up", []]);
            tryVerify(() => sw.value === true);
            tryVerify(() => root.findLabel("Connected") !== null);

            fake.commands.down = () => {
                fake.commands.status = root.status("stopped");
                return { code: 0, output: null };
            };
            flip(sw);
            tryVerify(() => root.ran("down").length === 1);
            tryVerify(() => sw.value === false);
        }

        function test_exit_node_pick() {
            fake.commands.status = root.status("running");
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            const box = root.named("exitBox");
            tryVerify(() => box.model.length === 3);
            compare(box.model, ["None", "vps", "nas (offline)"]);
            compare(box.currentIndex, 0);
            fake.commands["exit-node"] = () => {
                fake.commands.status = root.status("running", {
                    exit_node: { name: "vps", target: "vps.tail1234.ts.net", online: true, active: true },
                    exit_nodes: [
                        { name: "vps", target: "vps.tail1234.ts.net", online: true, active: true },
                        { name: "nas", target: "100.64.0.9", online: false, active: false }
                    ]
                });
                return { code: 0, output: null };
            };
            box.activated(1);
            tryVerify(() => root.ran("exit-node").length === 1);
            compare(root.ran("exit-node")[0], ["run", "exit-node", ["vps.tail1234.ts.net"]]);
            tryVerify(() => box.currentIndex === 1);

            fake.commands["exit-node-off"] = () => {
                fake.commands.status = root.status("running");
                return { code: 0, output: null };
            };
            box.activated(0);
            tryVerify(() => root.ran("exit-node-off").length === 1);
            compare(root.ran("exit-node-off")[0], ["run", "exit-node-off", []]);
            tryVerify(() => box.currentIndex === 0);
        }

        function test_failed_exit_node_goes_back() {
            fake.commands.status = root.status("running");
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            const box = root.named("exitBox");
            tryVerify(() => box.model.length === 3);
            fake.commands["exit-node"] = { code: 1, error: "exit 1: no such peer" };
            box.activated(2);
            tryVerify(() => root.findLabel("Couldn't change that: exit 1: no such peer") !== null);
            tryVerify(() => box.currentIndex === 0);
        }

        function test_routes_and_lan_switches() {
            fake.commands.status = root.status("running");
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            const routes = root.named("routes");
            const lan = root.named("lan");
            tryVerify(() => routes.enabled && lan.enabled);
            flip(routes);
            tryVerify(() => root.ran("routes-on").length === 1);
            // The switch shows what Tailscale says, not what was clicked, until it is read again.
            tryVerify(() => routes.enabled);
            compare(routes.value, false);
            fake.commands.prefs = { code: 0, output: { RouteAll: true, ExitNodeAllowLANAccess: true } };
            flip(lan);
            tryVerify(() => root.ran("lan-on").length === 1);
            tryVerify(() => lan.value === true && routes.value === true);
            tryVerify(() => lan.enabled);
            flip(lan);
            tryVerify(() => root.ran("lan-off").length === 1);
            tryVerify(() => routes.enabled);
            flip(routes);
            tryVerify(() => root.ran("routes-off").length === 1);
        }

        function test_operator_hint() {
            const sw = root.named("connection");
            tryVerify(() => sw.enabled && root.findLabel("Off") !== null);
            verify(root.findLabel(root.operatorHint) === null, "no hint before a refusal");
            fake.commands.up = { code: 1, error: "exit 1: Access denied: prefs write access denied" };
            flip(sw);
            tryVerify(() => root.findLabel(root.operatorHint) !== null);
            compare(sw.value, false, "the switch goes back");
            // A refusal for another reason shows its own message, not the hint.
            fake.commands.up = { code: 1, error: "timed out" };
            flip(sw);
            tryVerify(() => root.findLabel("Couldn't change that: timed out") !== null);
            verify(root.findLabel(root.operatorHint) === null);
        }

        function test_logout_confirm_only() {
            tryVerify(() => root.named("signOut") !== null && root.findLabel("me@example.com") !== null);
            compare(root.ran("logout").length, 0, "nothing signs out on its own");
            const button = root.named("signOut");
            compare(button.text, "Sign out…");
            const before = fake.calls.length;
            fake.confirms = false;
            button.clicked();
            wait(100);
            // Cancel changes nothing and shows nothing: no read after it, no message.
            compare(fake.calls.length, before + 1);
            verify(root.find(loader.item, it => typeof it.text === "string" && it.text.indexOf("Couldn't") === 0 && it.visible) === null);
            fake.confirms = true;
            fake.commands.logout = () => {
                fake.commands.status = root.status("needs_login", { message: root.loginUrl });
                return { code: 0, output: null };
            };
            button.clicked();
            tryVerify(() => root.findLabel("Sign in to Tailscale") !== null);
        }

        function test_sign_in_shows_link_and_never_runs_up() {
            fake.commands.status = root.status("needs_login", { message: root.loginUrl });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.findLabel("Sign in to Tailscale") !== null);
            const field = root.named("signInUrl");
            verify(field !== null);
            compare(field.text, root.loginUrl);
            verify(field.readOnly);
            verify(root.named("copyUrl") !== null);
            const sw = root.named("connection");
            verify(!sw.enabled, "the switch is off while a login is needed");
            flip(sw);
            wait(50);
            compare(root.ran("up").length, 0);
            compare(root.ran("peers").length, 0, "no device list without a login");
        }

        function test_not_running_is_not_an_error_card() {
            fake.commands.status = root.status("unavailable", { message: "failed to connect to local tailscaled" });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.findLabel("Tailscale isn't running.") !== null);
            verify(!root.named("connection").enabled);
        }

        function test_a_failed_read_shows_the_error() {
            fake.commands.status = { error: "timed out" };
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.findLabel("Couldn't read Tailscale's state") !== null);
            tryVerify(() => root.findLabel("timed out") !== null);
        }

        function test_untrusted_peer_name_plain() {
            fake.commands.status = root.status("running");
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.findLabel("<b>x</b>") !== null);
            tryVerify(() => root.findLabel("‮evil") !== null);
            // The text is exactly the name and the label never reads markup.
            compare(root.findLabel("<b>x</b>").textFormat, Text.PlainText);
            compare(root.findLabel("‮evil").textFormat, Text.PlainText);
        }
    }
}
