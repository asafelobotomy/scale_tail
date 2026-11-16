.pragma library
// The Tailscale page's pure logic: reading what `scale_tail` printed, checking arguments before a
// command runs, and the page's fixed words. No Qt calls, so Node can test it too
// (tests/uq/logic.test.mjs). Everything read from a command is untrusted: wrong types give a
// "couldn't read" result, never an exception, lists are capped and text is cut before it is shown.

var MAX_PEERS = 200;
var MAX_ACCOUNTS = 20;
var MAX_TEXT = 200;
var MAX_URL = 2048;

/// Same strings as the `args` patterns in page.toml (a test compares them).
var PATTERNS = {
    target: "^[A-Za-z0-9][A-Za-z0-9.:-]{0,252}$",
    account: "^[A-Za-z0-9]{1,64}$",
    bool: "^(true|false)$"
};
var TARGET_RE = new RegExp(PATTERNS.target);
var ACCOUNT_RE = new RegExp(PATTERNS.account);
/// Only a plain https link is shown as a sign-in link.
var URL_RE = /^https:\/\/[A-Za-z0-9.-]{1,253}(\/[A-Za-z0-9._~:\/?#@!$&'()*+,;=%-]*)?$/;
var IP_RE = /^[0-9A-Fa-f.:]{2,45}$/;

/// Settings, by key. `command` is the page.toml command that changes it: the careful ones show a
/// plan and ask first. `source` is who keeps it: Tailscale, or scale_tail itself ("app").
var SETTINGS = {
    accept_routes: { id: "routes", label: "Use routes from other devices", help: "Reach the networks your other devices share, such as an office network.", command: "config-set", source: "tailscale" },
    lan_access: { id: "lan-access", label: "Reach the local network while using an exit node", help: "Keeps printers and other devices on your home network reachable.", command: "config-set", source: "tailscale" },
    accept_dns: { id: "dns", label: "Use your tailnet's DNS settings", help: "Lets you reach devices by name. Switching it off can stop those names from resolving.", command: "config-set-careful", source: "tailscale" },
    shields_up: { id: "shields-up", label: "Block incoming connections", help: "Other devices can't reach anything on this computer. Connections you start still work.", command: "config-set-careful", source: "tailscale" },
    ssh: { id: "ssh", label: "Allow Tailscale SSH to this computer", help: "Devices your access rules allow can open a shell here.", command: "config-set-careful", source: "tailscale" },
    advertise_exit_node: { id: "advertise-exit-node", label: "Offer this computer as an exit node", help: "Other devices can send their internet traffic through this one, once you approve it in the Tailscale admin console.", command: "config-set-careful", source: "tailscale" },
    autostart: { id: "preferences", label: "Start scale_tail's tray when you log in", help: "The tray doesn't start under uQ, which shows Tailscale in quick settings.", command: "config-set", source: "app" },
    auto_connect: { id: "preferences", label: "Connect to Tailscale when the tray starts", help: "", command: "config-set", source: "app" },
    update_check: { id: "preferences", label: "Look for new versions of scale_tail", help: "Asks GitHub every few hours. Nothing is installed for you.", command: "config-set", source: "app" }
};

/// Every other fixed sentence the page shows.
var STRINGS = {
    working: "Working…",
    checking: "Checking Tailscale…",
    connected: "Connected",
    off: "Off",
    signIn: "Sign in to Tailscale",
    signInLink: "Open this link in a browser to sign in, then come back here:",
    signInTerminal: "In a terminal, run: tailscale up. Then open the link it prints.",
    notRunning: "Tailscale isn't running",
    notRunningDetail: "Start it with your service manager, then use Refresh.",
    busyState: "Tailscale is starting or busy",
    readFailed: "Couldn't read Tailscale's state",
    readFailedDetail: "scale_tail didn't give an answer this page can read. Details are under Advanced details.",
    changeFailed: "Couldn't change that: ",
    noExitNode: "None",
    offlineSuffix: " (offline)",
    notSignedIn: "Not signed in",
    noAccount: "Unknown",
    online: "Online",
    offline: "Offline",
    noDevices: "No other devices on this tailnet yet.",
    morePeers: "Only the first 200 devices are shown.",
    noOtherAccounts: "This is the only account signed in on this computer.",
    unreadSettings: "Tailscale's settings couldn't be read. Use Refresh to try again.",
    needsTailscale: "Available while Tailscale is running and signed in."
};

/// Shown when Tailscale refuses a change because this account isn't its operator. The operator
/// is the local account, not the Tailscale login, so `$USER` stays literal: it is typed in a
/// shell. Nothing here runs as root.
var OPERATOR_HINT = "Tailscale needs you to be its operator once. In a terminal, run: sudo tailscale set --operator=$USER";

// ---- small helpers ---------------------------------------------------------------------------

/// A short plain string for display, or "" for anything that isn't text.
function cut(value, max) {
    if (typeof value !== "string")
        return "";
    var limit = max || MAX_TEXT;
    return value.length > limit ? value.slice(0, limit) : value;
}

function isObject(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
}

function checkTarget(target) {
    return typeof target === "string" && target.length <= 253 && target.charAt(0) !== "-" && TARGET_RE.test(target);
}

function same(a, b) {
    return JSON.stringify(a) === JSON.stringify(b);
}

/// Whether uQ refused a read because the same command was still running.
function isBusyAnswer(result) {
    return !!result && typeof result.error === "string" && /already running/i.test(result.error);
}

function isCancelled(result) {
    return !!result && result.cancelled === true;
}

// ---- arguments -------------------------------------------------------------------------------

/// Why the arguments of a command are not fine, or "". The page checks before it asks uQ to run
/// anything, so a bad value gets a plain sentence instead of uQ's refusal.
function argsProblem(command, args) {
    var a = args || [];
    switch (command) {
    case "ping":
    case "exit-node":
    case "preview-exit-node":
        return a.length === 1 && checkTarget(a[0]) ? "" : "That isn't a device name or address.";
    case "switch-account":
    case "preview-switch":
        return a.length === 1 && typeof a[0] === "string" && ACCOUNT_RE.test(a[0]) ? "" : "That isn't an account Tailscale listed.";
    case "config-set":
    case "config-set-careful":
        if (a.length !== 2 || !SETTINGS[a[0]] || SETTINGS[a[0]].command !== command)
            return "That isn't a setting this page changes.";
        return a[1] === "true" || a[1] === "false" ? "" : "A setting is on or off.";
    default:
        return a.length === 0 ? "" : "That command takes no arguments.";
    }
}

/// The command and arguments that set `key` to `value`.
function configStep(key, value) {
    var s = SETTINGS[key];
    return { command: s ? s.command : "", args: [key, value ? "true" : "false"] };
}

// ---- reading answers -------------------------------------------------------------------------

/// What a command that prints `{"ok": ...}` said, however it exited: {ok, doc, error}.
function envelope(result) {
    if (!result)
        return { ok: false, doc: null, error: "No answer" };
    var out = result.output;
    if (isObject(out) && typeof out.ok === "boolean")
        return { ok: out.ok && !result.error, doc: out, error: out.ok ? cut(result.error) : cut(out.error) };
    if (result.error !== undefined && result.error !== null)
        return { ok: false, doc: isObject(out) ? out : null, error: cut(String(result.error)) };
    return { ok: isObject(out), doc: isObject(out) ? out : null, error: isObject(out) ? "" : "No answer" };
}

/// Whether a refused write means this account isn't Tailscale's operator yet.
function operatorNeeded(text) {
    return /access denied|operator/i.test(text || "");
}

/// What a finished change says: {message, failed, hint}. A cancelled question is not a failure
/// and says nothing (null).
function outcome(result, done) {
    if (isCancelled(result))
        return null;
    var env = envelope(result);
    if (env.ok)
        return { message: done || "", failed: false, hint: "" };
    if (operatorNeeded(env.error))
        return { message: OPERATOR_HINT, failed: true, hint: "operator" };
    return { message: STRINGS.changeFailed + env.error, failed: true, hint: "" };
}

function peerRow(p) {
    if (!isObject(p))
        return null;
    var ip = cut(p.ip, 45);
    var dns = cut(p.dns);
    var name = cut(p.name) !== "" ? cut(p.name) : dns;
    var target = cut(p.target, 253);
    return {
        name: name,
        os: cut(p.os, 40),
        ip: IP_RE.test(ip) ? ip : "",
        online: p.online === true,
        canExit: p.exit_node_option === true,
        active: p.exit_node === true,
        target: checkTarget(target) ? target : ""
    };
}

function boolMap(src, keys) {
    var out = {};
    if (!isObject(src))
        return null;
    for (var i = 0; i < keys.length; i++)
        out[keys[i]] = src[keys[i]] === true;
    return out;
}

function keysOf(source) {
    var out = [];
    for (var k in SETTINGS)
        if (SETTINGS[k].source === source)
            out.push(k);
    return out;
}

/// The result of the `status` command (`scale_tail status --json --full`). A non-zero exit that
/// still printed the document is read as usual. `ok: false` when it didn't.
function parseStatus(result) {
    var out = result ? result.output : null;
    if (!isObject(out) || out.version !== 1 || typeof out.state !== "string") {
        var why = result && result.error ? cut(String(result.error)) : "";
        return { ok: false, error: why !== "" ? why : "scale_tail gave no status" };
    }
    var peers = [];
    var list = Array.isArray(out.peers) ? out.peers : [];
    for (var i = 0; i < list.length && peers.length < MAX_PEERS; i++) {
        var row = peerRow(list[i]);
        if (row !== null && row.name !== "")
            peers.push(row);
    }
    var accounts = [];
    var acc = Array.isArray(out.accounts) ? out.accounts : [];
    for (var j = 0; j < acc.length && accounts.length < MAX_ACCOUNTS; j++) {
        var a = acc[j];
        if (isObject(a) && typeof a.id === "string" && ACCOUNT_RE.test(a.id))
            accounts.push({ id: a.id, account: cut(a.account), tailnet: cut(a.tailnet), active: a.active === true });
    }
    var self = isObject(out.self) ? { name: cut(out.self.name), ip: cut(out.self.ip, 45) } : null;
    var app = isObject(out.app) ? out.app : {};
    return {
        ok: true,
        state: out.state,
        connected: out.connected === true,
        message: cut(out.message, MAX_URL),
        tailnet: cut(out.tailnet),
        account: cut(out.account),
        self: self,
        peers: peers,
        total: Array.isArray(out.peers) ? out.peers.length : 0,
        prefs: boolMap(out.prefs, keysOf("tailscale")),
        accounts: accounts,
        app: { version: cut(app.version, 40), appimage: app.appimage === true, settings: boolMap(app.settings, keysOf("app")) }
    };
}

/// The result of the `config` command: the settings of both kinds, or `problem`.
function parseConfig(result) {
    var out = result ? result.output : null;
    if (!isObject(out) || out.version !== 1)
        return { ok: false, problem: result && result.error ? cut(String(result.error)) : "scale_tail gave no settings" };
    return { ok: true, app: boolMap(out.app, keysOf("app")), tailscale: boolMap(out.tailscale, keysOf("tailscale")), problem: cut(out.error) };
}

// ---- what the page says about the state ------------------------------------------------------

/// The heading of the Connection card: {title, detail, tone} where tone is "good", "off" or "bad".
function stateInfo(status, failure) {
    if (!status || !status.ok)
        return failure ? { title: STRINGS.readFailed, detail: "", tone: "bad" } : { title: STRINGS.checking, detail: "", tone: "off" };
    var where = status.tailnet !== "" ? status.tailnet : "";
    if (status.self && status.self.name !== "")
        where = where !== "" ? status.self.name + " on " + where : status.self.name;
    switch (status.state) {
    case "running":
        return { title: STRINGS.connected, detail: where, tone: "good" };
    case "stopped":
        return { title: STRINGS.off, detail: where, tone: "off" };
    case "needs_login":
        return { title: STRINGS.signIn, detail: "", tone: "bad" };
    case "unavailable":
        return { title: STRINGS.notRunning, detail: STRINGS.notRunningDetail, tone: "bad" };
    default:
        return { title: STRINGS.busyState, detail: "", tone: "off" };
    }
}

/// The switch may be used only when Tailscale can be turned on or off. `connect` is never run
/// while a login is needed: it would wait for a browser until it timed out.
function canToggle(status) {
    return !!status && status.ok === true && (status.state === "running" || status.state === "stopped");
}

/// The raw text behind an unusual state, for Advanced details. "" when there is none.
function rawOf(status) {
    if (!status || !status.ok || status.state === "running" || status.state === "stopped" || status.state === "needs_login")
        return "";
    return status.message !== "" ? cut(status.message) : cut(status.state);
}

/// The sign-in link when Tailscale is waiting for a login and the link is a plain https one.
function signInUrl(status) {
    if (!status || !status.ok || status.state !== "needs_login")
        return "";
    var url = status.message;
    return url.length <= MAX_URL && URL_RE.test(url) ? url : "";
}

/// Whether the state changed between two reads.
function stateChanged(before, after) {
    return !before || !before.ok || !after.ok || before.state !== after.state;
}

/// The exit-node picker: "None" first, then each device that offers it; `index` is the one in
/// use. Offline ones are shown but marked.
function exitChoices(status) {
    var labels = [STRINGS.noExitNode];
    var targets = [""];
    var index = 0;
    var peers = status && status.ok ? status.peers : [];
    for (var i = 0; i < peers.length; i++) {
        if (!peers[i].canExit || peers[i].target === "")
            continue;
        labels.push(peers[i].name + (peers[i].online ? "" : STRINGS.offlineSuffix));
        targets.push(peers[i].target);
        if (peers[i].active)
            index = labels.length - 1;
    }
    return { labels: labels, targets: targets, index: index };
}

/// The device in use as the exit node, or null.
function activeExit(status) {
    var peers = status && status.ok ? status.peers : [];
    for (var i = 0; i < peers.length; i++)
        if (peers[i].active)
            return peers[i];
    return null;
}

/// A setting's value from what was read, or `undefined` while it isn't known.
function settingValue(key, status, config) {
    var s = SETTINGS[key];
    if (!s)
        return undefined;
    var sources = s.source === "app" ? [config && config.ok ? config.app : null, status && status.ok ? status.app.settings : null] : [status && status.ok ? status.prefs : null, config && config.ok ? config.tailscale : null];
    for (var i = 0; i < sources.length; i++)
        if (sources[i] && typeof sources[i][key] === "boolean")
            return sources[i][key];
    return undefined;
}

/// Whether a setting's switch may be used now.
function canChange(key, status) {
    var s = SETTINGS[key];
    if (!s)
        return false;
    return s.source === "app" || (canToggle(status) && !!status.prefs);
}

// ---- devices ---------------------------------------------------------------------------------

/// The icon name for a device's operating system; the page shows a letter tile when the theme
/// has none of them.
function osIcon(os) {
    switch ((os || "").toLowerCase()) {
    case "android":
    case "ios":
        return "smartphone";
    case "linux":
    case "windows":
    case "macos":
    case "freebsd":
        return "computer";
    default:
        return "network-workgroup";
    }
}

/// "Online" or "Offline" with the system and address, for a device row's second line.
function peerDetail(peer) {
    var parts = [peer.online ? STRINGS.online : STRINGS.offline];
    if (peer.os !== "")
        parts.push(peer.os);
    if (peer.ip !== "")
        parts.push(peer.ip);
    return parts.join(" · ");
}

/// The devices whose name, system or address contains `text` (case-insensitive).
function filterPeers(peers, text) {
    var q = (text || "").trim().toLowerCase();
    if (q === "")
        return peers;
    return peers.filter(function (p) {
        return (p.name + " " + p.os + " " + p.ip).toLowerCase().indexOf(q) >= 0;
    });
}

/// How the exit-node button of a device row reads, or "" when it has none.
function exitButton(peer, status) {
    if (!canToggle(status) || !peer.canExit || peer.target === "" || !peer.online)
        return "";
    return peer.active ? "Stop using" : "Use as exit node";
}

// ---- ping ------------------------------------------------------------------------------------

var PING_START = { running: true, percent: 0, text: "Pinging…", failed: false };

/// A device's ping state after one more line of the stream: {running, percent, text, failed}.
function pingStep(state, ev) {
    if (!isObject(ev))
        return state;
    if (ev.event === "reply") {
        var pct = typeof ev.percent === "number" ? Math.max(0, Math.min(100, Math.round(ev.percent))) : state.percent;
        return { running: true, percent: pct, text: PING_START.text, failed: false };
    }
    if (ev.event === "done")
        return pingDone(ev);
    return state;
}

/// The summary a finished ping shows.
function pingDone(ev) {
    var received = typeof ev.received === "number" ? ev.received : 0;
    var sent = typeof ev.sent === "number" ? ev.sent : 0;
    if (ev.ok !== true || received === 0)
        return { running: false, percent: 100, text: cut(ev.error) !== "" ? "No reply: " + cut(ev.error) : "No reply", failed: true };
    var via = cut(ev.via, 60);
    var text = Math.round(typeof ev.avg_ms === "number" ? ev.avg_ms : 0) + " ms";
    if (received < sent)
        text += ", " + (sent - received) + " of " + sent + " lost";
    return { running: false, percent: 100, text: via !== "" ? text + " via " + via : text, failed: false };
}

// ---- accounts --------------------------------------------------------------------------------

function otherAccounts(status) {
    var list = status && status.ok ? status.accounts : [];
    return list.filter(function (a) {
        return !a.active;
    });
}

function accountLine(status) {
    if (!status || !status.ok)
        return "";
    if (status.state === "needs_login")
        return STRINGS.notSignedIn;
    return status.account !== "" ? status.account : STRINGS.noAccount;
}

function accountLabel(a) {
    return a.tailnet !== "" ? a.account + " (" + a.tailnet + ")" : a.account;
}

/// Notes by row key: a copy with `key` set to `note` (or removed when null).
function withNote(notes, key, note) {
    var next = {};
    for (var k in notes)
        next[k] = notes[k];
    if (note === null || note === undefined)
        delete next[key];
    else
        next[key] = note;
    return next;
}
