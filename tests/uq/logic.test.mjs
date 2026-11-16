// Pure-logic and manifest tests for the Tailscale page: `node --test 'tests/uq/*.test.mjs'`
// (Node 22). logic.js runs without Qt; page.toml is read with a small TOML reader.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { loadLogic, readPage } from "./load-logic.mjs";
import { parseToml } from "./toml-lite.mjs";

const L = loadLogic();
const plain = (value) => JSON.parse(JSON.stringify(value));
const manifest = parseToml(readPage("page.toml"));
const command = (id) => manifest.command.find((c) => c.id === id);
const pageDir = new URL("../../uq/pages/scale-tail/", import.meta.url);
const qmlFiles = readdirSync(pageDir).filter((f) => f.endsWith(".qml"));
const qml = (f) => readFileSync(new URL(f, pageDir), "utf8");

// ---- page.toml -------------------------------------------------------------------------------

test("the manifest asks for uq_api 3 and every command is safe to declare", () => {
  assert.equal(manifest.uq_api, 3);
  for (const c of manifest.command) {
    assert.match(c.argv[0], /^\/usr\/bin\/(scale_tail|tailscale)$/, c.id);
    assert.ok(!c.argv.some((a) => /^--(force|accept-risk)/.test(a) || a === "-i"), `${c.id} forces something`);
    assert.ok(c.timeout_ms >= 1000 && c.timeout_ms <= 120000, `${c.id} timeout`);
  }
  assert.equal(new Set(manifest.command.map((c) => c.id)).size, manifest.command.length, "duplicate command id");
});

test("every change that asks has a plan, and the plan reads the same arguments", () => {
  const asking = manifest.command.filter((c) => c.confirm);
  assert.deepEqual(asking.map((c) => c.id).sort(), ["config-set-careful", "exit-node", "exit-node-suggested", "logout", "switch-account"]);
  for (const c of asking) {
    assert.ok(c.preview, `${c.id} asks without showing a plan`);
    const p = command(c.preview);
    assert.ok(p, `${c.id}: no command ${c.preview}`);
    assert.equal(p.output, "json");
    assert.ok(!p.confirm && !p.preview, "a plan can't ask or have a plan");
    assert.ok(p.argv.includes("--dry-run") && p.argv.includes("--json"), `${p.id} is not a dry run`);
    assert.deepEqual(plain(p.args ?? []), plain(c.args ?? []), `${p.id} takes other arguments than ${c.id}`);
    // The same words as the change itself.
    assert.deepEqual(p.argv.filter((a) => !a.startsWith("--")), c.argv.filter((a) => !a.startsWith("--")), `${p.id} plans another command`);
  }
  for (const c of manifest.command.filter((c) => c.id.startsWith("preview-"))) {
    assert.ok(manifest.command.some((x) => x.preview === c.id), `${c.id} belongs to no change`);
  }
});

test("page.toml's argument patterns equal logic.js's", () => {
  const patterns = new Map();
  for (const c of manifest.command) for (const a of c.args ?? []) {
    const key = { target: "target", account: "account", value: "bool" }[a.name];
    if (key) assert.equal(a.pattern, L.PATTERNS[key], `${c.id}.${a.name}`);
    patterns.set(`${c.id}.${a.name}`, a.pattern);
  }
  assert.ok(patterns.size >= 10);
});

test("each setting's command accepts exactly the keys that use it", () => {
  for (const id of ["config-set", "config-set-careful", "preview-config"]) {
    const re = new RegExp(command(id).args.find((a) => a.name === "key").pattern);
    for (const [key, s] of Object.entries(L.SETTINGS)) {
      const should = id === "config-set" ? s.command === "config-set" : s.command === "config-set-careful";
      assert.equal(re.test(key), should, `${id} and ${key}`);
    }
  }
});

test("scale_tail's key list is the page's", () => {
  const rust = readFileSync(new URL("../../src/cli.rs", import.meta.url), "utf8");
  const names = [...rust.matchAll(/Key::\w+ => "([a-z_]+)",/g)].map((m) => m[1]);
  assert.deepEqual([...new Set(names)].sort(), Object.keys(L.SETTINGS).sort());
});

test("the page's search entries exist and are used", () => {
  const ids = manifest.setting.map((s) => s.id);
  assert.equal(new Set(ids).size, ids.length);
  const used = new Set();
  for (const f of qmlFiles) for (const m of qml(f).matchAll(/settingId: "([a-z-]+)"/g)) used.add(m[1]);
  for (const s of Object.values(L.SETTINGS)) used.add(s.id);
  for (const id of ids) assert.ok(used.has(id), `[[setting]] ${id} is on no row`);
  for (const id of used) assert.ok(ids.includes(id), `row ${id} has no [[setting]]`);
});

test("ping streams, and the followed files are few and absolute", () => {
  assert.equal(command("ping").output, "ndjson");
  assert.equal(command("config").watch.files.length, 3);
  for (const f of command("config").watch.files) assert.match(f, /^~\//);
  assert.ok(qmlFiles.some((f) => /command\.stream\("ping"/.test(qml(f))));
});

test("tst_page.qml lists the commands that confirm", () => {
  const test = readFileSync(new URL("./tst_page.qml", import.meta.url), "utf8");
  const listed = /confirmed:\s*\[([^\]]*)\]/.exec(test);
  assert.ok(listed);
  assert.deepEqual([...listed[1].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]).sort(), manifest.command.filter((c) => c.confirm).map((c) => c.id).sort());
});

// ---- the QML files ---------------------------------------------------------------------------

test("the QML files use only what logic.js and page.toml provide, as plain text", () => {
  const exported = new Set(Object.keys(L));
  const ids = new Set(manifest.command.map((c) => c.id));
  for (const f of qmlFiles) {
    const text = qml(f);
    assert.ok(text.split("\n").length <= 400, `${f} is over 400 lines`);
    for (const m of text.matchAll(/\bLogic\.([A-Za-z_]+)/g)) assert.ok(exported.has(m[1]), `${f}: Logic.${m[1]} does not exist`);
    for (const m of text.matchAll(/(?:\.write\([^,]+,\s*|command\.(?:run|watch|stream)\()"([a-z-]+)"/g)) assert.ok(ids.has(m[1]), `${f}: no command ${m[1]}`);
    assert.ok(!/\bText\s*\{|\bLabel\s*\{|Qt\.createQmlObject|Kirigami/.test(text.replace(/UqLabel\s*\{/g, "")), `${f}: stock text item`);
  }
});

const BANNED = /D-?Bus|polkit|JSON|systemctl|tailscaled|\borg\.[a-z0-9_]+\.[A-Za-z0-9_.]+/i;

test("the words the page shows are plain", () => {
  const found = [...Object.values(L.STRINGS), L.OPERATOR_HINT, manifest.name];
  for (const s of Object.values(L.SETTINGS)) found.push(s.label, s.help);
  for (const s of manifest.setting) found.push(s.label);
  for (const c of manifest.command) if (c.confirm) found.push(c.confirm);
  for (const f of qmlFiles) for (const line of qml(f).split("\n")) if (/\b(text|title|detail|help|placeholderText):/.test(line)) for (const m of line.matchAll(/"((?:[^"\\]|\\.)*)"/g)) found.push(m[1]);
  assert.ok(found.length > 50);
  for (const s of found) assert.ok(!BANNED.test(s), `jargon in: ${s}`);
});

// ---- reading answers -------------------------------------------------------------------------

const peer = (o = {}) => ({ name: "vps", dns: "vps.tail1.ts.net", ip: "100.64.0.2", os: "linux", online: true, exit_node_option: true, exit_node: false, target: "vps.tail1.ts.net", ...o });
const fullStatus = (o = {}) => ({
  code: 0,
  output: {
    version: 1, state: "running", connected: true, message: "", tailnet: "example.com", account: "me@example.com",
    self: { name: "laptop", dns: "laptop.tail1.ts.net", ip: "100.64.0.1" },
    peers: [peer(), peer({ name: "nas", target: "100.64.0.3", ip: "100.64.0.3", online: false }), peer({ name: "phone", os: "android", exit_node_option: false })],
    prefs: { accept_routes: true, lan_access: false, accept_dns: true, shields_up: false, ssh: false, advertise_exit_node: false },
    accounts: [{ id: "ab1", account: "me@example.com", tailnet: "example.com", active: true }, { id: "cd2", account: "me@work.example", tailnet: "work.example", active: false }],
    app: { version: "0.7.0", settings: { autostart: false, auto_connect: true, update_check: true }, appimage: false },
    ...o,
  },
});

test("parseStatus reads the full document", () => {
  const s = plain(L.parseStatus(fullStatus()));
  assert.equal(s.ok, true);
  assert.equal(s.peers.length, 3);
  assert.equal(s.prefs.accept_routes, true);
  assert.equal(s.app.settings.auto_connect, true);
  assert.equal(s.accounts.length, 2);
  assert.equal(s.account, "me@example.com");
});

test("parseStatus never throws on rubbish", () => {
  for (const bad of [null, undefined, {}, { output: null }, { output: [] }, { output: "x" }, { output: { version: 2, state: "running" } }, { output: { version: 1 } }, { error: "timed out" }]) {
    const s = L.parseStatus(bad);
    assert.equal(s.ok, false);
    assert.equal(typeof s.error, "string");
  }
  const odd = L.parseStatus({ output: { version: 1, state: "running", peers: [null, 5, "x", { name: 3 }, peer({ ip: "not an ip!", target: "-rf" })], accounts: [{ id: "../x" }, 4], prefs: "x", app: 3 } });
  assert.equal(odd.ok, true);
  assert.equal(odd.peers.length, 1);
  assert.equal(odd.peers[0].ip, "");
  assert.equal(odd.peers[0].target, "");
  assert.equal(odd.accounts.length, 0);
  assert.equal(odd.prefs, null);
});

test("lists are capped and text is cut", () => {
  const many = Array.from({ length: 300 }, (_, i) => peer({ name: "d" + i }));
  const s = L.parseStatus(fullStatus({ peers: many }));
  assert.equal(s.peers.length, 200);
  assert.equal(s.total, 300);
  assert.equal(L.parseStatus(fullStatus({ tailnet: "x".repeat(500) })).tailnet.length, 200);
});

test("stateInfo says what is going on", () => {
  const info = (o, failure) => plain(L.stateInfo(L.parseStatus(fullStatus(o)), failure));
  assert.deepEqual(info({}), { title: "Connected", detail: "laptop on example.com", tone: "good" });
  assert.equal(info({ state: "stopped", connected: false }).title, "Off");
  assert.equal(info({ state: "needs_login" }).title, "Sign in to Tailscale");
  assert.equal(info({ state: "unavailable" }).tone, "bad");
  assert.equal(info({ state: "other" }).title, "Tailscale is starting or busy");
  assert.equal(L.stateInfo(null, false).title, "Checking Tailscale…");
  assert.equal(L.stateInfo(null, true).tone, "bad");
});

test("only a running or stopped Tailscale can be switched", () => {
  const can = (state) => L.canToggle(L.parseStatus(fullStatus({ state })));
  assert.deepEqual(["running", "stopped", "needs_login", "unavailable", "other"].map(can), [true, true, false, false, false]);
  assert.equal(L.canToggle(null), false);
});

test("the sign-in link is shown only when it is a plain https link", () => {
  const url = (message) => L.signInUrl(L.parseStatus(fullStatus({ state: "needs_login", message })));
  assert.equal(url("https://login.tailscale.com/a/abc123"), "https://login.tailscale.com/a/abc123");
  for (const bad of ["http://login.tailscale.com/a", "javascript:alert(1)", "https://x y", "https://", "", "ftp://a.b/c", "https://a.b/\u0000"]) assert.equal(url(bad), "");
  assert.equal(L.signInUrl(L.parseStatus(fullStatus())), "");
});

test("the exit-node picker starts with None and marks the one in use", () => {
  let c = plain(L.exitChoices(L.parseStatus(fullStatus())));
  assert.deepEqual(c.labels, ["None", "vps", "nas (offline)"]);
  assert.deepEqual(c.targets, ["", "vps.tail1.ts.net", "100.64.0.3"]);
  assert.equal(c.index, 0);
  const s = fullStatus({ peers: [peer({ exit_node: true })] });
  assert.equal(L.exitChoices(L.parseStatus(s)).index, 1);
  assert.equal(L.activeExit(L.parseStatus(s)).name, "vps");
  assert.equal(L.activeExit(null), null);
  assert.deepEqual(plain(L.exitChoices(null)).labels, ["None"]);
});

test("a setting's value comes from the newest source that knows it", () => {
  const s = L.parseStatus(fullStatus());
  assert.equal(L.settingValue("accept_routes", s, null), true);
  assert.equal(L.settingValue("auto_connect", s, null), true);
  const config = L.parseConfig({ output: { version: 1, app: { auto_connect: false }, tailscale: null, error: "" } });
  assert.equal(L.settingValue("auto_connect", s, config), false, "the followed file wins");
  assert.equal(L.settingValue("accept_routes", null, null), undefined);
  assert.equal(L.settingValue("nope", s, null), undefined);
  assert.equal(L.canChange("ssh", s), true);
  assert.equal(L.canChange("ssh", L.parseStatus(fullStatus({ state: "unavailable", prefs: null }))), false);
  assert.equal(L.canChange("autostart", null), true, "scale_tail's own settings don't need Tailscale");
});

test("configStep picks the command that asks for the careful settings", () => {
  assert.deepEqual(plain(L.configStep("ssh", true)), { command: "config-set-careful", args: ["ssh", "true"] });
  assert.deepEqual(plain(L.configStep("lan_access", false)), { command: "config-set", args: ["lan_access", "false"] });
  for (const key of Object.keys(L.SETTINGS)) assert.equal(L.argsProblem(L.configStep(key, true).command, L.configStep(key, true).args), "");
});

test("arguments are checked before a command runs", () => {
  assert.equal(L.argsProblem("exit-node", ["vps.tail1.ts.net"]), "");
  assert.notEqual(L.argsProblem("exit-node", ["-rf"]), "");
  assert.notEqual(L.argsProblem("exit-node", []), "");
  assert.notEqual(L.argsProblem("ping", ["a b"]), "");
  assert.equal(L.argsProblem("switch-account", ["cd2"]), "");
  assert.notEqual(L.argsProblem("switch-account", ["../x"]), "");
  assert.notEqual(L.argsProblem("config-set", ["ssh", "true"]), "", "ssh asks, so it is not config-set");
  assert.notEqual(L.argsProblem("config-set", ["lan_access", "yes"]), "");
  assert.equal(L.argsProblem("logout", []), "");
  assert.notEqual(L.argsProblem("logout", ["now"]), "");
});

test("a change's answer is read however the command exited", () => {
  assert.deepEqual(plain(L.outcome({ code: 0, output: { ok: true } }, "Done.")), { message: "Done.", failed: false, hint: "" });
  const refused = plain(L.outcome({ code: 1, error: "exit code 1", output: { ok: false, error: "Access denied: x" } }));
  assert.equal(refused.failed, true);
  assert.equal(refused.message, L.OPERATOR_HINT);
  assert.equal(refused.hint, "operator");
  assert.equal(L.outcome({ error: "Cancelled", cancelled: true }), null);
  assert.match(L.outcome({ code: 1, output: { ok: false, error: "no such device" } }).message, /^Couldn't change that: no such device/);
  assert.match(L.outcome({ error: "timed out" }).message, /timed out/);
  assert.equal(L.outcome(null).failed, true);
  // An ok document with a non-zero exit is still a failure.
  assert.equal(L.outcome({ code: 1, error: "exit code 1", output: { ok: true } }).failed, true);
});

test("ping lines move the bar and the last line says how it went", () => {
  let st = plain(L.PING_START);
  st = plain(L.pingStep(st, { event: "reply", percent: 33 }));
  assert.deepEqual(st, { running: true, percent: 33, text: "Pinging…", failed: false });
  st = plain(L.pingStep(st, { event: "reply", percent: 400 }));
  assert.equal(st.percent, 100);
  assert.deepEqual(plain(L.pingStep(st, null)), st);
  assert.deepEqual(plain(L.pingStep(st, { event: "done", ok: true, sent: 3, received: 3, avg_ms: 22.4, via: "DERP(nyc)" })), { running: false, percent: 100, text: "22 ms via DERP(nyc)", failed: false });
  assert.equal(L.pingStep(st, { event: "done", ok: true, sent: 3, received: 2, avg_ms: 5, via: "" }).text, "5 ms, 1 of 3 lost");
  const none = plain(L.pingStep(st, { event: "done", ok: false, sent: 3, received: 0, error: "No reply" }));
  assert.equal(none.failed, true);
  assert.equal(none.text, "No reply: No reply");
});

test("devices: icons, filter, detail line and the exit button", () => {
  const s = L.parseStatus(fullStatus());
  assert.equal(L.osIcon("android"), "smartphone");
  assert.equal(L.osIcon("Linux"), "computer");
  assert.equal(L.osIcon("plan9"), "network-workgroup");
  assert.equal(L.peerDetail(s.peers[0]), "Online · linux · 100.64.0.2");
  assert.equal(L.peerDetail(s.peers[1]), "Offline · linux · 100.64.0.3");
  assert.deepEqual(plain(L.filterPeers(s.peers, "ANDROID")).map((p) => p.name), ["phone"]);
  assert.equal(L.filterPeers(s.peers, "  ").length, 3);
  assert.equal(L.exitButton(s.peers[0], s), "Use as exit node");
  assert.equal(L.exitButton(s.peers[1], s), "", "offline");
  assert.equal(L.exitButton(s.peers[2], s), "", "not an exit node");
  assert.equal(L.exitButton(s.peers[0], L.parseStatus(fullStatus({ state: "needs_login" }))), "");
  const active = L.parseStatus(fullStatus({ peers: [peer({ exit_node: true })] }));
  assert.equal(L.exitButton(active.peers[0], active), "Stop using");
});

test("accounts and notes", () => {
  const s = L.parseStatus(fullStatus());
  assert.deepEqual(plain(L.otherAccounts(s)).map((a) => a.id), ["cd2"]);
  assert.equal(L.accountLabel(s.accounts[1]), "me@work.example (work.example)");
  assert.equal(L.accountLine(s), "me@example.com");
  assert.equal(L.accountLine(L.parseStatus(fullStatus({ state: "needs_login" }))), "Not signed in");
  assert.equal(L.accountLine(L.parseStatus(fullStatus({ account: "" }))), "Unknown");
  const a = L.withNote({}, "x", { message: "m" });
  const b = L.withNote(a, "x", null);
  assert.deepEqual(plain(a), { x: { message: "m" } });
  assert.deepEqual(plain(b), {});
});

test("parseConfig", () => {
  assert.equal(L.parseConfig({ error: "timed out" }).ok, false);
  const c = L.parseConfig({ output: { version: 1, app: { autostart: true }, tailscale: { ssh: true }, error: "" } });
  assert.equal(c.ok, true);
  assert.equal(c.app.autostart, true);
  assert.equal(c.tailscale.ssh, true);
  assert.equal(c.app.update_check, false, "missing means off");
});

test("a read that found the same read still running is recognised", () => {
  assert.equal(L.isBusyAnswer({ error: "already running" }), true);
  assert.equal(L.isBusyAnswer({ error: "timed out" }), false);
  assert.equal(L.isBusyAnswer(null), false);
});
