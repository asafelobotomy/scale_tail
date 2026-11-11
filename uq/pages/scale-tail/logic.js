.pragma library
// The Tailscale page's pure logic: parsing command output, checking arguments and the page's
// fixed messages. No Qt calls, so Node can test it too (tests/uq/logic.test.mjs). Everything
// read from a command is untrusted: wrong types give a "couldn't read" result, never an
// exception, lists are capped and text is cut before it is shown.

var MAX_PEERS = 200;
var MAX_TEXT = 200;
var MAX_URL = 2048;

/// Same string as the `target` argument of the `exit-node` command in page.toml.
var TARGET_PATTERN = "^[A-Za-z0-9][A-Za-z0-9.:-]{0,252}$";
var TARGET_RE = new RegExp(TARGET_PATTERN);
/// Only a plain https link is shown as a sign-in link.
var URL_RE = /^https:\/\/[A-Za-z0-9.-]{1,253}(\/[A-Za-z0-9._~:\/?#@!$&'()*+,;=%-]*)?$/;

/// The confirm texts of page.toml, by command id. Only signing out asks.
var CONFIRMS = {
    logout: "Sign out of Tailscale on this computer? You'll need to sign in again in a browser."
};

/// Shown when Tailscale refuses a change because this account isn't its operator. The operator
/// is the local account, not the Tailscale login, so `$USER` stays literal: it is typed in a
/// shell. Nothing here runs as root.
var OPERATOR_HINT = "Tailscale needs you to be its operator once. In a terminal, run: sudo tailscale set --operator=$USER";

/// Every other fixed sentence the page shows.
var STRINGS = {
    checking: "Checking…",
    connected: "Connected",
    off: "Off",
    signIn: "Sign in to Tailscale",
    signInLink: "Open this link in a browser to sign in, then come back here:",
    signInTerminal: "In a terminal, run: tailscale up. Then open the link it prints.",
    notRunning: "Tailscale isn't running.",
    readFailed: "Couldn't read Tailscale's state",
    working: "Working…",
    changeFailed: "Couldn't change that: ",
    noExitNode: "None",
    offlineSuffix: " (offline)",
    notSignedIn: "Not signed in",
    noAccount: "Unknown",
    online: "Online",
    offline: "Offline",
    noDevices: "No other devices.",
    morePeers: "Only the first 200 devices are shown."
};

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

/// Whether `target` may be given to `tailscale set --exit-node`: the manifest's pattern, and
/// never a leading dash.
function checkTarget(target) {
    return typeof target === "string" && target.length <= 253 && target.charAt(0) !== "-" && TARGET_RE.test(target);
}

/// The result of the `status` command (`scale_tail status --json`). A non-zero exit that still
/// printed the document is read as usual.
function parseStatus(result) {
    var out = result ? result.output : null;
    if (!isObject(out) || out.version !== 1 || typeof out.state !== "string") {
        var why = result && result.error ? cut(String(result.error)) : "";
        return { ok: false, error: why !== "" ? why : "scale_tail gave no status" };
    }
    var nodes = [];
    var list = Array.isArray(out.exit_nodes) ? out.exit_nodes : [];
    for (var i = 0; i < list.length && nodes.length < MAX_PEERS; i++) {
        var n = list[i];
        if (!isObject(n) || !checkTarget(n.target))
            continue;
        nodes.push({ name: cut(n.name), target: n.target, online: n.online === true, active: n.active === true });
    }
    return {
        ok: true,
        state: out.state,
        connected: out.connected === true,
        message: cut(out.message, MAX_URL),
        exitNodes: nodes
    };
}

/// The result of the `peers` command (`tailscale status --json`): the account and the devices,
/// online ones first, at most 200.
function parsePeers(result) {
    var out = result ? result.output : null;
    if (!isObject(out))
        return { ok: false, error: result && result.error ? cut(String(result.error)) : "No answer from Tailscale" };
    var account = "";
    if (isObject(out.Self) && isObject(out.User)) {
        var user = out.User[String(out.Self.UserID)];
        if (isObject(user))
            account = cut(user.LoginName);
    }
    var peers = [];
    var raw = isObject(out.Peer) ? out.Peer : {};
    for (var key in raw) {
        var p = raw[key];
        if (!isObject(p))
            continue;
        var dns = cut(p.DNSName).replace(/\.$/, "");
        peers.push({
            name: cut(p.HostName) !== "" ? cut(p.HostName) : dns,
            online: p.Online === true,
            os: cut(p.OS)
        });
    }
    peers.sort(function (a, b) {
        if (a.online !== b.online)
            return a.online ? -1 : 1;
        var x = a.name.toLowerCase();
        var y = b.name.toLowerCase();
        return x < y ? -1 : x > y ? 1 : 0;
    });
    return { ok: true, account: account, total: peers.length, more: peers.length > MAX_PEERS, peers: peers.slice(0, MAX_PEERS) };
}

/// The result of the `prefs` command (`tailscale debug prefs`).
function parsePrefs(result) {
    var out = result ? result.output : null;
    if (!isObject(out))
        return { ok: false, error: result && result.error ? cut(String(result.error)) : "No answer from Tailscale" };
    return { ok: true, routes: out.RouteAll === true, lan: out.ExitNodeAllowLANAccess === true };
}

/// The exit-node picker: "None" first, then each option; `index` is the one in use.
function exitChoices(status) {
    var labels = [STRINGS.noExitNode];
    var targets = [""];
    var index = 0;
    var nodes = status && status.ok ? status.exitNodes : [];
    for (var i = 0; i < nodes.length; i++) {
        labels.push(nodes[i].name !== "" ? nodes[i].name + (nodes[i].online ? "" : STRINGS.offlineSuffix) : nodes[i].target);
        targets.push(nodes[i].target);
        if (nodes[i].active)
            index = labels.length - 1;
    }
    return { labels: labels, targets: targets, index: index };
}

/// The short state line under the page's heading.
function stateLine(status) {
    if (!status || !status.ok)
        return STRINGS.checking;
    switch (status.state) {
    case "running":
        return STRINGS.connected;
    case "stopped":
        return STRINGS.off;
    case "needs_login":
        return STRINGS.signIn;
    case "unavailable":
        return STRINGS.notRunning;
    default:
        return status.message !== "" ? cut(status.message) : cut(status.state);
    }
}

/// The switch may be used only when Tailscale can be turned on or off. `up` is never run
/// while a login is needed: it would wait for a browser until it timed out.
function canToggle(status) {
    return !!status && status.ok === true && (status.state === "running" || status.state === "stopped");
}

/// Peers, prefs and the account can be read in these states only.
function readsDetail(status) {
    return canToggle(status);
}

/// Whether two parsed results hold the same data.
function same(a, b) {
    return JSON.stringify(a) === JSON.stringify(b);
}

/// Whether to read the devices and settings again after a status read.
function stateChanged(before, after) {
    return !before || !before.ok || !after.ok || before.state !== after.state;
}

/// The sign-in link when Tailscale is waiting for a login and the link is a plain https one,
/// else "".
function signInUrl(status) {
    if (!status || !status.ok || status.state !== "needs_login")
        return "";
    var url = status.message;
    return url.length <= MAX_URL && URL_RE.test(url) ? url : "";
}

/// Whether a refused write means this account isn't Tailscale's operator yet.
function operatorNeeded(result) {
    if (!result)
        return false;
    var text = (typeof result.error === "string" ? result.error : "") + " " + (typeof result.output === "string" ? result.output : "");
    return /access denied/i.test(text);
}

/// The message for a failed write: the operator hint, or the error. A cancelled confirm is not
/// a failure and gives "".
function writeProblem(result) {
    if (!result || result.cancelled)
        return "";
    if (operatorNeeded(result))
        return OPERATOR_HINT;
    if (result.error !== undefined && result.error !== null)
        return STRINGS.changeFailed + cut(String(result.error));
    return "";
}

/// "Online" or "Offline" for a device row.
function presence(peer) {
    return peer.online ? STRINGS.online : STRINGS.offline;
}

/// What the account row shows.
function accountLine(status, peers) {
    if (status && status.ok && status.state === "needs_login")
        return STRINGS.notSignedIn;
    return peers && peers.ok && peers.account !== "" ? peers.account : STRINGS.noAccount;
}
