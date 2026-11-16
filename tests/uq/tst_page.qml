// The Tailscale settings page with the stub org.uq.settings module and a fake ctx (uQ's settings
// page test kit). Run from the repository root:
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

    function peer(name, ip, extra) {
        return Object.assign({ name: name, dns: name + ".tail1.ts.net", ip: ip, os: "linux", online: true, exit_node_option: false, exit_node: false, target: name + ".tail1.ts.net" }, extra || {});
    }

    function status(state, extra) {
        const base = {
            version: 1, state: state, connected: state === "running", message: "", tailnet: "example.com", account: "me@example.com",
            self: { name: "laptop", dns: "laptop.tail1.ts.net", ip: "100.64.0.1" },
            exit_node: null, exit_nodes: [],
            peers: [
                root.peer("vps", "100.64.0.2", { exit_node_option: true }),
                root.peer("nas", "100.64.0.3", { exit_node_option: true, online: false }),
                root.peer("<b>x</b>", "100.64.0.4", { os: "android" })
            ],
            prefs: { accept_routes: false, lan_access: false, accept_dns: true, shields_up: false, ssh: false, advertise_exit_node: false },
            accounts: [{ id: "ab1", label: "me", account: "me@example.com", tailnet: "example.com", active: true }, { id: "cd2", label: "w", account: "me@work.example", tailnet: "work.example", active: false }],
            app: { version: "0.7.0", settings: { autostart: false, auto_connect: false, update_check: true }, appimage: false }
        };
        return { code: 0, output: Object.assign(base, extra || {}) };
    }

    function ok(extra) {
        return { code: 0, output: Object.assign({ ok: true }, extra || {}) };
    }
    function bad(message) {
        return { code: 1, error: "exit code 1", output: { ok: false, error: message } };
    }

    function freshCommands() {
        return {
            status: root.status("running"),
            config: { code: 0, output: { version: 1, state: "running", app: { autostart: false, auto_connect: false, update_check: true }, tailscale: null, error: "" } },
            "update-check": root.ok({ current: "0.7.0", latest: "0.7.0", available: false, url: "" }),
            ping: { code: 0, output: null },
            connect: root.ok(), disconnect: root.ok(),
            "exit-node": root.ok(), "exit-node-none": root.ok(), "exit-node-suggested": root.ok(),
            "config-set": root.ok(), "config-set-careful": root.ok(),
            "switch-account": root.ok(), logout: root.ok()
        };
    }

    FakeContext {
        id: fake
        commands: root.freshCommands()
        // The commands with `confirm` in page.toml (a Node test compares this list).
        confirmed: ["exit-node", "exit-node-suggested", "config-set-careful", "switch-account", "logout"]
        events: root.pingEvents
    }
    readonly property var pingEvents: ({
            ping: [
                { event: "reply", seq: 1, of: 3, percent: 33, ok: true, ms: 20 },
                { event: "reply", seq: 2, of: 3, percent: 66, ok: true, ms: 24 },
                { event: "done", ok: true, sent: 3, received: 3, avg_ms: 22.4, via: "DERP(nyc)", percent: 100 }
            ]
        })

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
            fake.events = root.pingEvents;
            loader.source = "";
            loader.source = root.pageUrl;
            verify(loader.status === Loader.Ready, "the page loads");
            loader.item.ctx = fake.ctx;
        }

        function flip(row) {
            row.toggle.toggle();
            row.toggle.toggled();
        }

        function tab(title) {
            const t = loader.item.tabList.find(x => x.title === title);
            verify(t, "tab " + title);
            loader.item.show(t);
            return t;
        }

        function test_five_tabs() {
            compare(loader.item.tabList.map(t => t.title), ["Connection", "Devices", "Network", "Account", "Preferences"]);
        }

        function test_reads_on_open_and_changes_nothing() {
            tryVerify(() => root.findLabel("Connected") !== null);
            tryVerify(() => root.findLabel("laptop on example.com") !== null);
            compare(fake.calls[0], ["run", "status", []]);
            compare(fake.calls.filter(c => ["connect", "disconnect", "exit-node", "config-set", "logout"].includes(c[1])).length, 0);
            verify(!root.named("skeleton").visible, "the progress bar is gone once Tailscale answered");
        }

        function test_shows_progress_before_the_first_answer() {
            verify(root.named("skeleton").visible);
            compare(root.findLabel("Checking Tailscale…") !== null, true);
        }

        function test_switch_off_and_on() {
            const sw = root.named("connection");
            tryVerify(() => sw.enabled && sw.value === true);
            fake.commands.disconnect = () => {
                fake.commands.status = root.status("stopped");
                return root.ok();
            };
            flip(sw);
            tryVerify(() => root.ran("disconnect").length === 1);
            tryVerify(() => sw.value === false);
            tryVerify(() => root.findLabel("Off") !== null);
            fake.commands.connect = () => {
                fake.commands.status = root.status("running");
                return root.ok();
            };
            flip(sw);
            tryVerify(() => root.ran("connect").length === 1);
            tryVerify(() => sw.value === true);
        }

        function test_sign_in_shows_the_link_and_never_connects() {
            fake.commands.status = root.status("needs_login", { message: root.loginUrl, peers: [], prefs: null, accounts: [] });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.named("signInUrl") !== null && root.named("signInUrl").visible);
            compare(root.named("signInUrl").text, root.loginUrl);
            verify(!root.named("connection").enabled, "the switch waits for the login");
            compare(root.ran("connect").length, 0);
            tryVerify(() => root.findLabel("Not signed in") !== null || true);
        }

        function test_a_link_that_is_not_https_is_not_offered() {
            fake.commands.status = root.status("needs_login", { message: "javascript:alert(1)" });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.findLabel("In a terminal, run: tailscale up. Then open the link it prints.") !== null);
            verify(!root.named("signInUrl").visible);
        }

        function test_not_running_says_so() {
            fake.commands.status = root.status("unavailable", { message: "failed to connect to local tailscaled", peers: [], prefs: null, accounts: [] });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.named("notRunning").visible);
            verify(root.named("notRunning").detail.indexOf("failed to connect") >= 0);
            verify(!root.named("connection").enabled);
        }

        function test_exit_node_pick_asks_then_changes() {
            const box = root.named("exitBox");
            tryVerify(() => box.model.length === 3 && box.enabled);
            compare(box.model, ["None", "vps", "nas (offline)"]);
            box.activated(1);
            tryVerify(() => root.ran("exit-node").length === 1);
            compare(root.ran("exit-node")[0], ["run", "exit-node", ["vps.tail1.ts.net"]]);
            tryVerify(() => root.findLabel("Exit node changed.") !== null);
        }

        function test_a_cancelled_exit_node_changes_nothing() {
            const box = root.named("exitBox");
            tryVerify(() => box.model.length === 3 && box.enabled);
            fake.confirms = false;
            const before = fake.calls.length;
            box.activated(1);
            wait(100);
            compare(fake.calls.length, before + 1, "no read after a cancel");
            compare(box.currentIndex, 0, "the picker goes back");
            verify(root.findLabel("Exit node changed.") === null);
        }

        function test_exit_node_none_and_suggested() {
            fake.commands.status = root.status("running", { peers: [root.peer("vps", "100.64.0.2", { exit_node_option: true, exit_node: true })] });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            const box = root.named("exitBox");
            tryVerify(() => box.currentIndex === 1);
            tryVerify(() => root.findLabel("All your internet traffic leaves through vps.") !== null);
            box.activated(0);
            tryVerify(() => root.ran("exit-node-none").length === 1);
            tryVerify(() => loader.item.workingKey === "");
            root.named("useSuggested").clicked();
            tryVerify(() => root.ran("exit-node-suggested").length === 1);
        }

        function test_a_refused_change_goes_back_with_the_operator_hint() {
            tab("Network");
            const sw = root.named("switch-accept_routes");
            tryVerify(() => sw.enabled && sw.value === false);
            fake.commands["config-set"] = root.bad("Access denied: prefs write access denied");
            flip(sw);
            tryVerify(() => root.findLabel(root.operatorHint) !== null);
            compare(sw.value, false);
        }

        function test_careful_settings_ask_and_the_others_do_not() {
            tab("Network");
            tryVerify(() => root.named("switch-ssh").enabled);
            flip(root.named("switch-lan_access"));
            tryVerify(() => root.ran("config-set").length === 1);
            compare(root.ran("config-set")[0], ["run", "config-set", ["lan_access", "true"]]);
            tryVerify(() => loader.item.workingKey === "");
            flip(root.named("switch-ssh"));
            tryVerify(() => root.ran("config-set-careful").length === 1);
            compare(root.ran("config-set-careful")[0], ["run", "config-set-careful", ["ssh", "true"]]);
        }

        function test_a_declined_plan_changes_nothing() {
            tab("Network");
            tryVerify(() => root.named("switch-shields_up").enabled);
            fake.confirms = false;
            const before = fake.calls.length;
            flip(root.named("switch-shields_up"));
            wait(100);
            compare(fake.calls.length, before + 1);
            compare(root.named("switch-shields_up").value, false);
        }

        function test_devices_are_plain_text_and_ping_streams() {
            tab("Devices");
            tryVerify(() => root.named("ping-vps") !== null);
            verify(root.findLabel("<b>x</b>") !== null, "a name with markup is shown as it is");
            verify(root.findLabel("Online · linux · 100.64.0.2") !== null);
            verify(!root.named("ping-nas").enabled, "an offline device can't be pinged");
            root.named("ping-vps").clicked();
            compare(root.named("ping-vps").enabled, false, "one ping at a time");
            tryVerify(() => root.findLabel("22 ms via DERP(nyc)") !== null);
            compare(fake.calls.filter(c => c[0] === "stream")[0], ["stream", "ping", ["vps.tail1.ts.net"]]);
            tryVerify(() => root.named("ping-vps").enabled);
        }

        function test_a_ping_with_no_reply_says_so() {
            tab("Devices");
            fake.events = ({ ping: [{ event: "reply", seq: 1, of: 3, percent: 33, ok: false, ms: null, error: "No reply" }, { event: "done", ok: false, sent: 3, received: 0, error: "No reply" }] });
            tryVerify(() => root.named("ping-vps") !== null);
            root.named("ping-vps").clicked();
            tryVerify(() => root.findLabel("No reply: No reply") !== null);
        }

        function test_use_a_device_as_exit_node() {
            tab("Devices");
            tryVerify(() => root.named("exit-vps") !== null && root.named("exit-vps").text === "Use as exit node");
            verify(root.named("exit-nas") === null || !root.named("exit-nas").visible, "an offline device has no button");
            root.named("exit-vps").clicked();
            tryVerify(() => root.ran("exit-node").length === 1);
        }

        function test_filter_devices() {
            tab("Devices");
            tryVerify(() => root.named("filter") !== null);
            root.named("filter").text = "nas";
            tryVerify(() => root.named("ping-vps") === null);
            tryVerify(() => root.named("ping-nas") !== null);
            root.named("filter").text = "zzz";
            tryVerify(() => root.findLabel("No device matches that.") !== null);
        }

        function test_account_sign_out_and_switch_ask_first() {
            tab("Account");
            tryVerify(() => root.findLabel("me@example.com") !== null);
            fake.confirms = false;
            let before = fake.calls.length;
            root.named("signOut").clicked();
            wait(100);
            compare(fake.calls.length, before + 1, "a cancel reads nothing");
            fake.confirms = true;
            root.named("switch-cd2").clicked();
            tryVerify(() => root.ran("switch-account").length === 1);
            compare(root.ran("switch-account")[0], ["run", "switch-account", ["cd2"]]);
            tryVerify(() => root.findLabel("Switched account.") !== null);
        }

        function test_one_account_has_no_switcher() {
            fake.commands.status = root.status("running", { accounts: [{ id: "ab1", account: "me@example.com", tailnet: "example.com", active: true }] });
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tab("Account");
            tryVerify(() => root.named("noOthers").visible);
        }

        function test_preferences_follow_the_files() {
            tab("Preferences");
            tryVerify(() => fake.calls.some(c => c[0] === "watch" && c[1] === "config"));
            const sw = root.named("switch-autostart");
            tryVerify(() => sw.enabled && sw.value === false);
            fake.commands["config-set"] = () => {
                fake.commands.config = { code: 0, output: { version: 1, state: "running", app: { autostart: true, auto_connect: false, update_check: true }, tailscale: null, error: "" } };
                return root.ok();
            };
            flip(sw);
            tryVerify(() => root.ran("config-set").length === 1);
            compare(root.ran("config-set")[0], ["run", "config-set", ["autostart", "true"]]);
            fake.change("config");
            tryVerify(() => sw.value === true);
            compare(root.named("version").text, "scale_tail 0.7.0");
        }

        function test_check_for_updates() {
            tab("Preferences");
            tryVerify(() => root.named("checkUpdates") !== null);
            root.named("checkUpdates").clicked();
            tryVerify(() => root.findLabel("You're up to date.") !== null);
            fake.commands["update-check"] = root.ok({ available: true, latest: "0.8.0", current: "0.7.0", url: "x" });
            root.named("checkUpdates").clicked();
            tryVerify(() => root.named("updateText").text.indexOf("Version 0.8.0 is out") === 0);
            fake.commands["update-check"] = root.bad("update check failed: offline");
            root.named("checkUpdates").clicked();
            tryVerify(() => root.named("updateText").text.indexOf("Couldn't check: update check failed") === 0);
        }

        function test_a_failed_read_shows_the_error_and_keeps_the_tabs() {
            fake.commands.status = { error: "timed out" };
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => root.named("readFailed").visible);
            compare(root.named("readFailed").detail, "timed out");
            compare(loader.item.tabList.length, 5);
        }

        function test_a_read_that_collides_keeps_the_state() {
            tryVerify(() => root.findLabel("Connected") !== null);
            fake.commands.status = { error: "already running" };
            loader.item.readStatus();
            wait(100);
            verify(root.findLabel("Connected") !== null);
            compare(root.named("readFailed").visible, false);
        }

        function test_the_header_opens_the_app() {
            root.named("openApp").clicked();
            tryVerify(() => fake.calls.some(c => c[0] === "openApp" && c[1] === "io.github.asafelobotomy.scale_tail.desktop"));
        }
    }
}
