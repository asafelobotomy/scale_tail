// Pure-logic tests for the Tailscale page: `node --test 'tests/uq/*.test.mjs'` (Node 22).
import test from "node:test";
import assert from "node:assert/strict";
import { loadLogic, readPage } from "./load-logic.mjs";

const L = loadLogic();
const plain = value => JSON.parse(JSON.stringify(value));

// ---- a small reader for the subset of TOML that page.toml uses -------------------------------

function parseValue(src, i) {
    while (src[i] === " ") i++;
    if (src[i] === '"') {
        let out = "";
        i++;
        while (src[i] !== '"') {
            if (src[i] === "\\") {
                i++;
                out += { n: "\n", t: "\t", '"': '"', "\\": "\\" }[src[i]] ?? src[i];
            } else out += src[i];
            i++;
        }
        return [out, i + 1];
    }
    if (src[i] === "[") {
        const items = [];
        i++;
        for (;;) {
            while (src[i] === " ") i++;
            if (src[i] === "]") return [items, i + 1];
            let v;
            [v, i] = parseValue(src, i);
            items.push(v);
            while (src[i] === " ") i++;
            if (src[i] === ",") i++;
        }
    }
    if (src[i] === "{") {
        const table = {};
        i++;
        for (;;) {
            while (src[i] === " ") i++;
            if (src[i] === "}") return [table, i + 1];
            const eq = src.indexOf("=", i);
            const key = src.slice(i, eq).trim();
            let v;
            [v, i] = parseValue(src, eq + 1);
            table[key] = v;
            while (src[i] === " ") i++;
            if (src[i] === ",") i++;
        }
    }
    const m = /^(true|false|\d+)/.exec(src.slice(i));
    assert.ok(m, `unsupported TOML value at: ${src.slice(i, i + 20)}`);
    return [m[1] === "true" ? true : m[1] === "false" ? false : Number(m[1]), i + m[1].length];
}

function parseToml(text) {
    const doc = { setting: [], command: [] };
    let table = doc;
    for (const raw of text.split("\n")) {
        const line = raw.trim();
        if (line === "" || line.startsWith("#")) continue;
        let m;
        if ((m = /^\[\[(\w+)\]\]$/.exec(line))) {
            table = {};
            doc[m[1]].push(table);
        } else if ((m = /^\[(\w+)\]$/.exec(line))) {
            table = doc[m[1]] = {};
        } else if ((m = /^([\w-]+)\s*=\s*(.*)$/.exec(line))) {
            table[m[1]] = parseValue(m[2], 0)[0];
        } else assert.fail(`cannot read TOML line: ${line}`);
    }
    return doc;
}

const manifest = parseToml(readPage("page.toml"));
const command = id => manifest.command.find(c => c.id === id);

// ---- fixtures -------------------------------------------------------------------------------

const statusOut = {
    version: 1, state: "running", connected: true, message: "", tailnet: "example.com",
    self: { name: "laptop", dns: "laptop.tail1234.ts.net", ip: "100.64.0.1" },
    exit_node: { name: "vps", target: "vps.tail1234.ts.net", online: true, active: true },
    exit_nodes: [
        { name: "vps", target: "vps.tail1234.ts.net", online: true, active: true },
        { name: "nas", target: "100.64.0.9", online: false, active: false },
    ],
};

const peersOut = {
    BackendState: "Running",
    Self: { UserID: 42, HostName: "laptop" },
    User: { 42: { LoginName: "me@example.com", DisplayName: "Me" } },
    Peer: {
        k1: { HostName: "zeta", DNSName: "zeta.tail.ts.net.", Online: false, OS: "linux" },
        k2: { HostName: "alpha", DNSName: "alpha.tail.ts.net.", Online: true, OS: "windows" },
        k3: { HostName: "", DNSName: "beta.tail.ts.net.", Online: true, OS: "iOS" },
    },
};

// ---- tests ----------------------------------------------------------------------------------

test("parses_status_and_peers", () => {
    const s = plain(L.parseStatus({ code: 0, output: statusOut }));
    assert.equal(s.ok, true);
    assert.equal(s.state, "running");
    assert.equal(s.connected, true);
    assert.equal(s.exitNodes.length, 2);

    const p = plain(L.parsePeers({ code: 0, output: peersOut }));
    assert.equal(p.ok, true);
    assert.equal(p.account, "me@example.com");
    // Online first, then by name.
    assert.deepEqual(p.peers.map(x => [x.name, x.online, x.os]), [
        ["alpha", true, "windows"], ["beta.tail.ts.net", true, "iOS"], ["zeta", false, "linux"],
    ]);

    const prefs = plain(L.parsePrefs({ code: 0, output: { RouteAll: true, ExitNodeAllowLANAccess: false } }));
    assert.deepEqual(prefs, { ok: true, routes: true, lan: false });

    // A non-zero exit that still printed the document is read as usual.
    assert.equal(L.parseStatus({ code: 1, error: "x", output: statusOut }).ok, true);
});

test("reads_are_defensive", () => {
    for (const bad of [undefined, null, {}, { error: "boom" }, { output: "text" }, { output: [] },
        { output: { version: 2, state: "running" } }, { output: { version: 1 } }]) {
        assert.equal(L.parseStatus(bad).ok, false);
    }
    assert.equal(L.parseStatus({ error: "boom" }).error, "boom");
    for (const bad of [undefined, {}, { output: "x" }, { output: null }, { output: 5 }]) {
        assert.equal(L.parsePeers(bad).ok, false);
        assert.equal(L.parsePrefs(bad).ok, false);
    }
    // Wrong types inside a good document never throw.
    const weird = L.parsePeers({ output: { Self: 3, User: "x", Peer: { a: 1, b: null, c: { HostName: 7, Online: "yes", OS: {} } } } });
    assert.equal(weird.ok, true);
    assert.equal(weird.account, "");
    assert.equal(weird.peers.length, 1);
    assert.equal(weird.peers[0].online, false);
    assert.equal(L.parsePrefs({ output: { RouteAll: "yes" } }).routes, false);
    // The account is only the login name of Self's own user.
    const other = L.parsePeers({ output: { Self: { UserID: 1 }, User: { 2: { LoginName: "x@y" } } } });
    assert.equal(other.account, "");
});

test("same_and_state_changes", () => {
    const a = L.parseStatus({ output: statusOut });
    assert.equal(L.same(a, L.parseStatus({ output: statusOut })), true);
    assert.equal(L.same(a, L.parseStatus({ output: { ...statusOut, exit_node: null } })), true, "exit_node alone is not kept");
    assert.equal(L.same(a, L.parseStatus({ output: { ...statusOut, state: "stopped", connected: false } })), false);
    assert.equal(L.same(null, null), true);
    assert.equal(L.same(null, a), false);
    // Details are read again when the state changes, and the first time.
    assert.equal(L.stateChanged(null, a), true);
    assert.equal(L.stateChanged(a, a), false);
    assert.equal(L.stateChanged(a, L.parseStatus({ output: { version: 1, state: "stopped" } })), true);
    assert.equal(L.stateChanged(a, L.parseStatus({ error: "x" })), true);
});

test("busy_answers_and_raw_text", () => {
    assert.equal(L.isBusyAnswer({ error: "already running" }), true);
    assert.equal(L.isBusyAnswer({ error: "timed out" }), false);
    assert.equal(L.isBusyAnswer(undefined), false);
    const other = L.parseStatus({ output: { version: 1, state: "other", message: "Starting" } });
    assert.equal(L.stateLine(other), L.STRINGS.busyState);
    assert.equal(L.rawOf(other), "Starting");
    assert.equal(L.rawOf(L.parseStatus({ output: statusOut })), "");
    assert.equal(L.rawOf(null), "");
});

test("peers_are_capped_and_cut", () => {
    const Peer = {};
    for (let i = 0; i < 500; i++) Peer["k" + i] = { HostName: "h" + String(i).padStart(3, "0"), Online: true, OS: "linux" };
    Peer.long = { HostName: "x".repeat(5000), Online: true, OS: "y".repeat(5000) };
    const p = L.parsePeers({ output: { Peer } });
    assert.equal(p.peers.length, 200);
    assert.equal(p.more, true);
    assert.equal(p.total, 501);
    assert.ok(p.peers.every(x => x.name.length <= 200 && x.os.length <= 200));
});

test("operator_needed_detected", () => {
    assert.equal(L.operatorNeeded({ code: 1, error: "exit 1: Access denied: prefs write access denied" }), true);
    assert.equal(L.operatorNeeded({ code: 1, error: "access denied" }), true);
    assert.equal(L.operatorNeeded({ code: 1, error: "x", output: "Access denied: checkprefs" }), true);
    assert.equal(L.operatorNeeded({ code: 1, error: "timed out" }), false);
    assert.equal(L.operatorNeeded({ code: 0, output: null }), false);
    assert.equal(L.operatorNeeded(undefined), false);
    assert.equal(
        L.OPERATOR_HINT,
        "Tailscale needs you to be its operator once. In a terminal, run: sudo tailscale set --operator=$USER");
    assert.equal(L.writeProblem({ code: 1, error: "Access denied" }), L.OPERATOR_HINT);
    assert.equal(L.writeProblem({ code: 1, error: "timed out" }).includes("timed out"), true);
    // Cancel is not a failure.
    assert.equal(L.writeProblem({ error: "Cancelled", cancelled: true }), "");
    assert.equal(L.writeProblem({ code: 0, output: null }), "");
});

test("exit_node_options", () => {
    const s = L.parseStatus({ output: statusOut });
    const c = plain(L.exitChoices(s));
    assert.deepEqual(c.labels, ["None", "vps", "nas (offline)"]);
    assert.deepEqual(c.targets, ["", "vps.tail1234.ts.net", "100.64.0.9"]);
    assert.equal(c.index, 1);

    const none = plain(L.exitChoices(L.parseStatus({ output: { ...statusOut, exit_node: null, exit_nodes: [{ ...statusOut.exit_nodes[0], active: false }] } })));
    assert.equal(none.index, 0);
    assert.deepEqual(plain(L.exitChoices(null)), { labels: ["None"], targets: [""], index: 0 });

    // An option whose target the command would refuse is not offered.
    const hostile = L.parseStatus({ output: { ...statusOut, exit_nodes: [
        { name: "evil", target: "-rf", online: true, active: false },
        { name: "ok", target: "ok.ts.net", online: true, active: false }] } });
    assert.deepEqual(plain(L.exitChoices(hostile).targets), ["", "ok.ts.net"]);
});

test("sign_in_and_toggle_rules", () => {
    const login = L.parseStatus({ output: { version: 1, state: "needs_login", connected: false, message: "https://login.tailscale.com/a/abc123" } });
    assert.equal(L.signInUrl(login), "https://login.tailscale.com/a/abc123");
    assert.equal(L.canToggle(login), false, "up is never run while a login is needed");
    for (const message of ["", "javascript:alert(1)", "http://login.tailscale.com/a/x", "https://x.com/a b", "https://x.com/\n", "https://" + "a".repeat(3000)]) {
        assert.equal(L.signInUrl(L.parseStatus({ output: { version: 1, state: "needs_login", message } })), "", message);
    }
    assert.equal(L.signInUrl(L.parseStatus({ output: statusOut })), "");
    assert.equal(L.canToggle(L.parseStatus({ output: statusOut })), true);
    assert.equal(L.canToggle(L.parseStatus({ output: { version: 1, state: "stopped" } })), true);
    assert.equal(L.canToggle(L.parseStatus({ output: { version: 1, state: "unavailable", message: "x" } })), false);
    assert.equal(L.canToggle(null), false);
    assert.equal(L.readsDetail(login), false);
    assert.equal(L.readsDetail(L.parseStatus({ output: statusOut })), true);
});

test("patterns_match_manifest", () => {
    const arg = command("exit-node").args;
    assert.equal(arg.length, 1);
    assert.equal(arg[0].name, "target");
    assert.equal(arg[0].pattern, L.TARGET_PATTERN);
    for (const c of manifest.command)
        for (const a of c.args ?? []) assert.match(a.pattern, /^\^.*\$$/, `${c.id} is anchored`);
});

test("rejects_hostile_args", () => {
    for (const bad of ["-rf", "--force", "-", "a\nb", "a\n", "", " ", "x".repeat(5000), "x".repeat(254),
        "../x", ".hidden", "$(id)", "a b", "a;b", "a/b", undefined, null, 5]) {
        assert.equal(L.checkTarget(bad), false, JSON.stringify(bad));
        if (typeof bad === "string") assert.equal(new RegExp(command("exit-node").args[0].pattern).test(bad), false);
    }
    for (const good of ["vps.tail1234.ts.net", "100.64.0.9", "fd7a:115c:a1e0::1", "a", "x".repeat(253)]) {
        assert.equal(L.checkTarget(good), true, good);
    }
});

test("confirms_match_plan", () => {
    const plan = {
        logout: "Sign out of Tailscale on this computer? You'll need to sign in again in a browser.",
    };
    const confirms = {};
    for (const c of manifest.command) if (c.confirm !== undefined) confirms[c.id] = c.confirm;
    assert.deepEqual(confirms, plan, "only logout asks, with exactly this text");
    assert.deepEqual(plain(L.CONFIRMS), plan);
    // Switches and the exit-node picker can be undone with the same control: no confirm.
    for (const id of ["up", "down", "exit-node", "exit-node-off", "routes-on", "routes-off", "lan-on", "lan-off"])
        assert.equal(command(id).confirm, undefined, id);
});

test("manifest_shape", () => {
    assert.equal(manifest.id, "scale-tail");
    assert.equal(manifest.name, "Tailscale");
    assert.equal(manifest.icon, "network-vpn");
    assert.deepEqual(manifest.requires.bins, ["tailscale", "scale_tail"]);
    assert.deepEqual(manifest.setting.map(s => s.id), ["connection", "exit-node", "routes", "lan-access", "peers", "account"]);
    for (const s of manifest.setting) assert.ok(s.keywords?.length > 0, `${s.id} has keywords`);
    const argv = {
        status: ["/usr/bin/scale_tail", "status", "--json"],
        peers: ["/usr/bin/tailscale", "status", "--json"],
        prefs: ["/usr/bin/tailscale", "debug", "prefs"],
        up: ["/usr/bin/tailscale", "up"],
        down: ["/usr/bin/tailscale", "down"],
        "exit-node": ["/usr/bin/tailscale", "set", "--exit-node"],
        "exit-node-off": ["/usr/bin/tailscale", "set", "--exit-node="],
        "routes-on": ["/usr/bin/tailscale", "set", "--accept-routes=true"],
        "routes-off": ["/usr/bin/tailscale", "set", "--accept-routes=false"],
        "lan-on": ["/usr/bin/tailscale", "set", "--exit-node-allow-lan-access=true"],
        "lan-off": ["/usr/bin/tailscale", "set", "--exit-node-allow-lan-access=false"],
        logout: ["/usr/bin/tailscale", "logout"],
    };
    assert.deepEqual(manifest.command.map(c => c.id).sort(), Object.keys(argv).sort());
    for (const c of manifest.command) {
        assert.deepEqual(c.argv, argv[c.id], c.id);
        assert.equal(c.timeout_ms, 30000, c.id);
        assert.equal(c.output, ["status", "peers", "prefs"].includes(c.id) ? "json" : "none", c.id);
        assert.equal(c.watch, undefined, "tailscaled has no signal: the page polls");
    }
    assert.equal(manifest.dbus, undefined, "no D-Bus access");
    // No page command signs in on its own or switches the account.
    for (const c of manifest.command) assert.ok(!c.argv.some(a => /^(login|switch|--operator|--force-reauth)/.test(a)), c.id);
});

test("page_strings_are_plain", () => {
    const banned = /\b(D-Bus|dbus|polkit|JSON|systemctl|scdrd|xzramd|notifd|tailscaled|sched_ext|EEVDF|zram-generator|sysctl)\b/i;
    const strings = [];
    // Every string literal on a `text:`, `label:`, `title:` or `confirm` line of the page and its components.
    for (const file of ["Page.qml"]) {
        for (const line of readPage(file).split("\n")) {
            if (!/\b(text|label|title):|confirm/.test(line)) continue;
            for (const m of line.matchAll(/"((?:[^"\\]|\\.)*)"/g)) strings.push(m[1]);
        }
    }
    // The strings logic.js hands to the page.
    strings.push(...Object.values(plain(L.STRINGS)), L.OPERATOR_HINT);
    strings.push(manifest.name, ...manifest.setting.map(s => s.label), ...manifest.command.flatMap(c => (c.confirm ? [c.confirm] : [])));
    assert.ok(strings.length > 15);
    for (const s of strings) assert.doesNotMatch(s, banned, s);
    // Sentence case, no shouting: nothing starts with a lower-case letter.
    for (const s of strings) assert.doesNotMatch(s, /^[a-z]/, s);
    // The page never builds markup or shows a stock control's untrusted text.
    const qml = readPage("Page.qml");
    assert.doesNotMatch(qml, /\b(Text|Label|TextEdit|TextInput)\s*\{|Qt\.createQmlObject|Kirigami|ToolTip/);
    assert.doesNotMatch(readPage("logic.js"), /\bQt\./);
});
