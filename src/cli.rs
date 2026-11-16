//! The command line behind uQ's Tailscale page (and usable on its own).
//!
//! Every command prints one JSON document on stdout (`--json` is required), always, whether it
//! worked or not: `{"ok": false, "error": "..."}` with exit code 1 when it didn't. Commands that
//! change something take `--dry-run`, which prints a plan instead and changes nothing:
//! `{"summary", "changes": [{"group", "label", "detail", "reversible"}], "would_refuse", "reason"}`.
//! `ping --stream` prints one JSON object per line (progress), ending with `{"event": "done"}`.

use super::*;
use serde_json::{Value, json};

// ---------- parsing ----------

/// A setting `config show|set` knows. `app` keys are scale_tail's own; the rest are Tailscale's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Autostart,
    AutoConnect,
    UpdateCheck,
    AcceptRoutes,
    LanAccess,
    AcceptDns,
    ShieldsUp,
    Ssh,
    AdvertiseExitNode,
}

impl Key {
    pub const ALL: [Key; 9] = [
        Key::Autostart,
        Key::AutoConnect,
        Key::UpdateCheck,
        Key::AcceptRoutes,
        Key::LanAccess,
        Key::AcceptDns,
        Key::ShieldsUp,
        Key::Ssh,
        Key::AdvertiseExitNode,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Key::Autostart => "autostart",
            Key::AutoConnect => "auto_connect",
            Key::UpdateCheck => "update_check",
            Key::AcceptRoutes => "accept_routes",
            Key::LanAccess => "lan_access",
            Key::AcceptDns => "accept_dns",
            Key::ShieldsUp => "shields_up",
            Key::Ssh => "ssh",
            Key::AdvertiseExitNode => "advertise_exit_node",
        }
    }

    fn parse(s: &str) -> Option<Key> {
        Key::ALL.into_iter().find(|k| k.name() == s)
    }

    fn is_app(self) -> bool {
        matches!(self, Key::Autostart | Key::AutoConnect | Key::UpdateCheck)
    }

    /// The `tailscale set` flag of a Tailscale setting.
    fn flag(self) -> Option<&'static str> {
        match self {
            Key::AcceptRoutes => Some("--accept-routes"),
            Key::LanAccess => Some("--exit-node-allow-lan-access"),
            Key::AcceptDns => Some("--accept-dns"),
            Key::ShieldsUp => Some("--shields-up"),
            Key::Ssh => Some("--ssh"),
            Key::AdvertiseExitNode => Some("--advertise-exit-node"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExitChoice {
    Node(String),
    None,
    Suggested,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Status { full: bool },
    ConfigShow,
    ConfigSet { key: Key, value: bool, dry_run: bool },
    ExitNode { choice: ExitChoice, dry_run: bool },
    Accounts,
    Switch { id: String, dry_run: bool },
    Logout { dry_run: bool },
    Connect,
    Disconnect,
    Ping { target: String, count: u32, stream: bool },
    UpdateCheck,
}

pub const USAGE: &str = "usage: scale_tail [--tray]       start the tray (--tray: even under uQ)
       scale_tail open            start the tray even under uQ
       scale_tail status --json [--full]
       scale_tail config show --json
       scale_tail config set <key> <true|false> --json [--dry-run]
       scale_tail exit-node <device|none|suggested> --json [--dry-run]
       scale_tail accounts --json
       scale_tail switch <account-id> --json [--dry-run]
       scale_tail logout --json [--dry-run]
       scale_tail connect --json | disconnect --json
       scale_tail ping <device> --json [--count N] [--stream]
       scale_tail update check --json
       scale_tail --version

config keys: autostart auto_connect update_check accept_routes lan_access accept_dns
             shields_up ssh advertise_exit_node";

/// A device name or Tailscale IP, as `tailscale set --exit-node` and `ping` take it: the same
/// pattern as the uQ page's `target` argument (`^[A-Za-z0-9][A-Za-z0-9.:-]{0,252}$`).
pub fn valid_target(t: &str) -> bool {
    let b = t.as_bytes();
    !b.is_empty()
        && b.len() <= 253
        && b[0].is_ascii_alphanumeric()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b':' | b'-'))
}

fn valid_account_id(t: &str) -> bool {
    !t.is_empty() && t.len() <= 64 && t.bytes().all(|c| c.is_ascii_alphanumeric())
}

/// Reads the arguments of a command that is not the tray. `Ok(None)` means "not one of ours":
/// the arguments start the tray, as they always did.
pub fn parse(args: &[String]) -> Result<Option<Cmd>, String> {
    let Some(first) = args.first().map(String::as_str) else { return Ok(None) };
    if !matches!(first, "status" | "config" | "exit-node" | "accounts" | "switch" | "logout" | "connect" | "disconnect" | "ping" | "update") {
        return Ok(None);
    }
    let mut json = false;
    let mut dry_run = false;
    let mut full = false;
    let mut stream = false;
    let mut count: Option<u32> = None;
    let mut pos: Vec<&str> = vec![];
    let mut it = args[1..].iter().map(String::as_str);
    while let Some(a) = it.next() {
        match a {
            "--json" => json = true,
            "--dry-run" => dry_run = true,
            "--full" => full = true,
            "--stream" => stream = true,
            "--count" => {
                let n = it.next().ok_or("--count needs a number")?;
                count = Some(n.parse().map_err(|_| "--count needs a number")?);
            }
            a if a.starts_with('-') => return Err(format!("unknown option {a}")),
            a => pos.push(a),
        }
    }
    if !json {
        return Err(format!("{first} needs --json"));
    }
    let only_with = |ok: bool, what: &str| if ok { Ok(()) } else { Err(format!("{what} only applies to {}", what_it_applies(what))) };
    // Options that don't belong to the command are refused, not ignored.
    let cmd = match (first, pos.as_slice()) {
        ("status", []) => Cmd::Status { full },
        ("config", ["show"]) => Cmd::ConfigShow,
        ("config", ["set", key, value]) => {
            let key = Key::parse(key).ok_or_else(|| format!("unknown setting {key}"))?;
            let value = match *value {
                "true" | "on" => true,
                "false" | "off" => false,
                v => return Err(format!("{v} is not true or false")),
            };
            Cmd::ConfigSet { key, value, dry_run }
        }
        ("config", _) => return Err("config needs show or set <key> <true|false>".into()),
        ("exit-node", [t]) => {
            let choice = match *t {
                "none" => ExitChoice::None,
                "suggested" => ExitChoice::Suggested,
                t if valid_target(t) => ExitChoice::Node(t.to_string()),
                t => return Err(format!("{t} is not a device name or address")),
            };
            Cmd::ExitNode { choice, dry_run }
        }
        ("exit-node", _) => return Err("exit-node needs a device, none or suggested".into()),
        ("accounts", []) => Cmd::Accounts,
        ("switch", [id]) if valid_account_id(id) => Cmd::Switch { id: id.to_string(), dry_run },
        ("switch", _) => return Err("switch needs an account id (from accounts)".into()),
        ("logout", []) => Cmd::Logout { dry_run },
        ("connect", []) => Cmd::Connect,
        ("disconnect", []) => Cmd::Disconnect,
        ("ping", [t]) if valid_target(t) => {
            let count = count.unwrap_or(3);
            if !(1..=10).contains(&count) {
                return Err("--count is 1 to 10".into());
            }
            Cmd::Ping { target: t.to_string(), count, stream }
        }
        ("ping", _) => return Err("ping needs a device name or address".into()),
        ("update", ["check"]) => Cmd::UpdateCheck,
        ("update", _) => return Err("update needs check".into()),
        _ => return Err(format!("{first}: too many arguments")),
    };
    only_with(!full || matches!(cmd, Cmd::Status { .. }), "--full")?;
    only_with(
        !dry_run || matches!(cmd, Cmd::ConfigSet { .. } | Cmd::ExitNode { .. } | Cmd::Switch { .. } | Cmd::Logout { .. }),
        "--dry-run",
    )?;
    only_with(!stream || matches!(cmd, Cmd::Ping { .. }), "--stream")?;
    only_with(count.is_none() || matches!(cmd, Cmd::Ping { .. }), "--count")?;
    Ok(Some(cmd))
}

fn what_it_applies(opt: &str) -> &'static str {
    match opt {
        "--full" => "status",
        "--dry-run" => "config set, exit-node, switch and logout",
        "--stream" | "--count" => "ping",
        _ => "other commands",
    }
}

// ---------- documents ----------

fn failure(msg: impl Into<String>) -> (Value, i32) {
    (json!({ "ok": false, "error": msg.into() }), 1)
}

fn ok(extra: Value) -> (Value, i32) {
    let mut doc = json!({ "ok": true });
    if let (Some(d), Some(e)) = (doc.as_object_mut(), extra.as_object()) {
        d.extend(e.clone());
    }
    (doc, 0)
}

fn change(group: &str, label: impl Into<String>, detail: impl Into<String>, reversible: bool) -> Value {
    json!({ "group": group, "label": label.into(), "detail": detail.into(), "reversible": reversible })
}

fn plan(summary: impl Into<String>, changes: Vec<Value>) -> (Value, i32) {
    (json!({ "summary": summary.into(), "changes": changes, "would_refuse": false, "reason": "" }), 0)
}

fn refuse(reason: impl Into<String>) -> (Value, i32) {
    (json!({ "summary": "Nothing would change", "changes": [], "would_refuse": true, "reason": reason.into() }), 0)
}

/// Words for what turning a setting on or off does, for plans and the page.
fn describe(key: Key, on: bool) -> (&'static str, &'static str) {
    match (key, on) {
        (Key::Autostart, true) => ("Start scale_tail's tray when you log in", "Has no effect under the uQ desktop, which shows Tailscale in quick settings."),
        (Key::Autostart, false) => ("Stop starting scale_tail's tray at login", ""),
        (Key::AutoConnect, true) => ("Connect to Tailscale when the tray starts", ""),
        (Key::AutoConnect, false) => ("Stop connecting when the tray starts", ""),
        (Key::UpdateCheck, true) => ("Look for new scale_tail versions on GitHub", "The tray asks GitHub every few hours."),
        (Key::UpdateCheck, false) => ("Stop looking for new scale_tail versions", ""),
        (Key::AcceptRoutes, true) => ("Use routes from other devices", "Reach the networks your other devices share."),
        (Key::AcceptRoutes, false) => ("Stop using routes from other devices", "Networks other devices share stop being reachable from here."),
        (Key::LanAccess, true) => ("Reach the local network while using an exit node", "Printers and other devices on your home network stay reachable."),
        (Key::LanAccess, false) => ("Send local network traffic through the exit node too", "With an exit node on, local devices may become unreachable."),
        (Key::AcceptDns, true) => ("Use your tailnet's DNS settings", "Device names on your tailnet resolve, and the DNS servers your tailnet sets are used."),
        (Key::AcceptDns, false) => ("Stop using your tailnet's DNS settings", "Device names on your tailnet may stop resolving on this computer."),
        (Key::ShieldsUp, true) => ("Block all incoming connections over Tailscale", "Other devices can no longer reach anything on this computer. Connections you start still work."),
        (Key::ShieldsUp, false) => ("Allow incoming connections over Tailscale", "Devices your access rules allow can reach services on this computer."),
        (Key::Ssh, true) => ("Allow Tailscale SSH to this computer", "Devices your access rules allow can open a shell here without a password."),
        (Key::Ssh, false) => ("Stop allowing Tailscale SSH to this computer", ""),
        (Key::AdvertiseExitNode, true) => ("Offer this computer as an exit node", "Other devices can send all their internet traffic through it once you approve it in the Tailscale admin console."),
        (Key::AdvertiseExitNode, false) => ("Stop offering this computer as an exit node", "Devices using it as an exit node lose their route."),
    }
}

fn group_of(key: Key) -> &'static str {
    if key.is_app() { "scale_tail" } else { "Tailscale" }
}

fn state_name(s: &State) -> &'static str {
    match s {
        State::Running => "running",
        State::Stopped => "stopped",
        State::NeedsLogin(_) => "needs_login",
        State::Unavailable(_) => "unavailable",
        State::Other(_) => "other",
    }
}

fn app_value(app: &AppSettings, key: Key) -> bool {
    match key {
        Key::Autostart => app.autostart,
        Key::AutoConnect => app.autoconnect,
        _ => app.update_check,
    }
}

fn prefs_value(p: &Prefs, key: Key) -> bool {
    match key {
        Key::AcceptRoutes => p.route_all,
        Key::LanAccess => p.exit_node_allow_lan_access,
        Key::AcceptDns => p.corp_dns,
        Key::ShieldsUp => p.shields_up,
        Key::Ssh => p.run_ssh,
        _ => p.advertises_exit_node(),
    }
}

async fn read_prefs() -> Result<Prefs, String> {
    let out = ts(&["debug", "prefs"]).await?;
    serde_json::from_str(&out).map_err(|e| format!("Tailscale's settings were not readable: {e}"))
}

async fn read_profiles() -> Vec<Account> {
    ts(&["switch", "--list"]).await.map(|o| parse_accounts(&o)).unwrap_or_default()
}

fn prefs_json(p: &Prefs) -> Value {
    let mut m = serde_json::Map::new();
    for k in Key::ALL.into_iter().filter(|k| !k.is_app()) {
        m.insert(k.name().into(), json!(prefs_value(p, k)));
    }
    Value::Object(m)
}

fn app_json(a: &AppSettings) -> Value {
    let mut m = serde_json::Map::new();
    for k in Key::ALL.into_iter().filter(|k| k.is_app()) {
        m.insert(k.name().into(), json!(app_value(a, k)));
    }
    Value::Object(m)
}

fn node_json(n: &Node) -> Value {
    json!({
        "name": n.name, "dns": n.dns, "ip": n.ip, "os": n.os, "online": n.online,
        "exit_node_option": n.can_exit, "exit_node": n.is_exit, "target": exit_target(n),
    })
}

fn account_json(a: &Account) -> Value {
    json!({ "id": a.id, "label": a.label, "tailnet": a.tailnet, "account": a.login_name, "active": a.active })
}

// ---------- commands ----------

pub async fn run(cmd: Cmd) -> i32 {
    if let Cmd::Ping { target, count, stream: true } = &cmd {
        return ping_stream(target, *count).await;
    }
    let (doc, code) = match cmd {
        Cmd::Status { full } => status(full).await,
        Cmd::ConfigShow => config_show().await,
        Cmd::ConfigSet { key, value, dry_run } => config_set(key, value, dry_run).await,
        Cmd::ExitNode { choice, dry_run } => exit_node(choice, dry_run).await,
        Cmd::Accounts => accounts().await,
        Cmd::Switch { id, dry_run } => switch(&id, dry_run).await,
        Cmd::Logout { dry_run } => logout(dry_run).await,
        Cmd::Connect => connect(true).await,
        Cmd::Disconnect => connect(false).await,
        Cmd::Ping { target, count, .. } => ping_once(&target, count).await,
        Cmd::UpdateCheck => update_check().await,
    };
    println!("{doc}");
    code
}

/// `status --json`: the small summary the widget reads; with `--full` also the settings, the
/// accounts, every device and scale_tail's own state, for the uQ page.
async fn status(full: bool) -> (Value, i32) {
    let s = fetch_status().await;
    let mut doc = status_json(&s);
    if full {
        let d = doc.as_object_mut().expect("status is an object");
        let reachable = !matches!(s.state, State::Unavailable(_));
        d.insert("account".into(), json!(s.login));
        d.insert("peers".into(), json!(s.peers.iter().map(node_json).collect::<Vec<_>>()));
        d.insert("prefs".into(), if reachable { read_prefs().await.ok().map(|p| prefs_json(&p)).into() } else { Value::Null });
        d.insert("accounts".into(), json!(if reachable { read_profiles().await } else { vec![] }.iter().map(account_json).collect::<Vec<_>>()));
        d.insert("app".into(), app_status());
    }
    (doc, 0)
}

fn app_status() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "settings": app_json(&load_settings()),
        "appimage": std::env::var_os("APPIMAGE").is_some(),
    })
}

async fn config_show() -> (Value, i32) {
    let state = fetch_status().await.state;
    let (tailscale, error) = match &state {
        State::Unavailable(e) => (Value::Null, if e.is_empty() { "Tailscale isn't running".to_string() } else { e.clone() }),
        _ => match read_prefs().await {
            Ok(p) => (prefs_json(&p), String::new()),
            Err(e) => (Value::Null, e),
        },
    };
    (json!({ "version": 1, "state": state_name(&state), "app": app_json(&load_settings()), "tailscale": tailscale, "error": error }), 0)
}

async fn config_set(key: Key, value: bool, dry_run: bool) -> (Value, i32) {
    let state = fetch_status().await.state;
    let current = if key.is_app() {
        Some(app_value(&load_settings(), key))
    } else if matches!(state, State::Unavailable(_)) {
        None
    } else {
        read_prefs().await.ok().map(|p| prefs_value(&p, key))
    };
    if dry_run {
        let Some(current) = current else { return refuse("Tailscale isn't running, so its settings can't be read or changed.") };
        if current == value {
            return plan(if value { "Already on" } else { "Already off" }, vec![]);
        }
        let (label, detail) = describe(key, value);
        return plan(label, vec![change(group_of(key), label, detail, true)]);
    }
    let result = if let Some(flag) = key.flag() {
        if current.is_none() {
            return failure("Tailscale isn't running, so its settings can't be changed.");
        }
        ts(&["set", &format!("{flag}={value}")]).await.map(|_| ())
    } else {
        match key {
            Key::Autostart => set_autostart(value),
            Key::AutoConnect => set_autoconnect(value),
            _ => set_update_check(value),
        }
    };
    match result {
        Ok(()) => ok(json!({ "key": key.name(), "value": value, "changed": current != Some(value) })),
        Err(e) => failure(e),
    }
}

/// What `tailscale exit-node suggest` names, as a device the page can show.
async fn suggested() -> Result<String, String> {
    let out = ts(&["exit-node", "suggest"]).await?;
    let line = out.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let name = line.split_once(':').map(|(_, n)| n).unwrap_or(line).trim().trim_end_matches('.');
    if name.is_empty() { Err("Tailscale has no suggestion right now.".into()) } else { Ok(name.to_string()) }
}

async fn exit_node(choice: ExitChoice, dry_run: bool) -> (Value, i32) {
    let s = fetch_status().await;
    if let State::Unavailable(e) = &s.state {
        let why = if e.is_empty() { "Tailscale isn't running.".to_string() } else { e.clone() };
        return if dry_run { refuse(why) } else { failure(why) };
    }
    let using = s.peers.iter().find(|p| p.is_exit).cloned();
    if dry_run {
        return match &choice {
            ExitChoice::None => match using {
                Some(p) => plan(
                    format!("Stop using {}", p.name),
                    vec![change("Exit node", format!("Stop sending your internet traffic through {}", p.name), "Websites see this computer's own address again.", true)],
                ),
                None => plan("No exit node is in use", vec![]),
            },
            ExitChoice::Suggested => match suggested().await {
                Ok(name) => plan(
                    format!("Use {name}"),
                    vec![change("Exit node", format!("Send all your internet traffic through {name}"), "Tailscale picks the best one for you, and may pick another later.", true)],
                ),
                Err(e) => refuse(e),
            },
            ExitChoice::Node(t) => match s.peers.iter().find(|p| p.can_exit && (&exit_target(p) == t || &p.dns == t || &p.ip == t)) {
                None => refuse("That device isn't offering to be an exit node."),
                Some(p) if !p.online => refuse(format!("{} is offline, so your internet would stop working.", p.name)),
                Some(p) if p.is_exit => plan(format!("Already using {}", p.name), vec![]),
                Some(p) => plan(
                    format!("Use {}", p.name),
                    vec![change("Exit node", format!("Send all your internet traffic through {}", p.name), "Websites see that device's address. Local network access follows the setting under Network.", true)],
                ),
            },
        };
    }
    let arg = match &choice {
        ExitChoice::None => "--exit-node=".to_string(),
        ExitChoice::Suggested => "--exit-node=auto:any".to_string(),
        ExitChoice::Node(t) => format!("--exit-node={t}"),
    };
    match ts(&["set", &arg]).await {
        Ok(_) => ok(json!({ "exit_node": match choice { ExitChoice::Node(t) => json!(t), ExitChoice::Suggested => json!("suggested"), ExitChoice::None => Value::Null } })),
        Err(e) => failure(e),
    }
}

async fn accounts() -> (Value, i32) {
    (json!({ "version": 1, "accounts": read_profiles().await.iter().map(account_json).collect::<Vec<_>>() }), 0)
}

async fn switch(id: &str, dry_run: bool) -> (Value, i32) {
    let list = read_profiles().await;
    let Some(a) = list.iter().find(|a| a.id == id) else {
        let why = "That account isn't signed in on this computer.";
        return if dry_run { refuse(why) } else { failure(why) };
    };
    if dry_run {
        if a.active {
            return refuse("This account is already in use.");
        }
        return plan(
            format!("Switch to {}", a.label),
            vec![change("Account", format!("Switch to {}", a.label), "Tailscale reconnects under that account. Its devices and settings replace the current ones.", true)],
        );
    }
    match ts(&["switch", id]).await {
        Ok(_) => ok(json!({ "account": a.login_name, "tailnet": a.tailnet })),
        Err(e) => failure(e),
    }
}

async fn logout(dry_run: bool) -> (Value, i32) {
    let s = fetch_status().await;
    let signed_in = matches!(s.state, State::Running | State::Stopped);
    if !signed_in {
        let why = match &s.state {
            State::NeedsLogin(_) => "Tailscale isn't signed in.".to_string(),
            State::Unavailable(_) => "Tailscale isn't running.".to_string(),
            _ => "Tailscale is busy; try again in a moment.".to_string(),
        };
        return if dry_run { refuse(why) } else { failure(why) };
    }
    if !dry_run {
        return match ts(&["logout"]).await {
            Ok(_) => ok(json!({})),
            Err(e) => failure(e),
        };
    }
    let place = if s.tailnet.is_empty() { "your tailnet".to_string() } else { s.tailnet.clone() };
    let mut changes = vec![change("Account", "Sign out of Tailscale on this computer", format!("It leaves {place}, and other devices stop seeing it. Signing in again in a browser brings it back."), true)];
    if let Some(p) = s.peers.iter().find(|p| p.is_exit) {
        changes.push(change("Exit node", format!("Stop using {} as the exit node", p.name), "Your internet traffic goes out directly again.", true));
    }
    plan("Sign out of Tailscale", changes)
}

/// `connect` / `disconnect`: `tailscale up` is refused while a login is needed, because it
/// would wait for a browser.
async fn connect(on: bool) -> (Value, i32) {
    let s = fetch_status().await;
    match (&s.state, on) {
        (State::Running, true) | (State::Stopped, false) => return ok(json!({ "connected": on, "changed": false })),
        (State::Running | State::Stopped, _) => {}
        (State::NeedsLogin(_), _) => return failure("Sign in to Tailscale first."),
        (State::Unavailable(e), _) => return failure(if e.is_empty() { "Tailscale isn't running." } else { e }),
        (State::Other(_), _) => return failure("Tailscale is busy; try again in a moment."),
    }
    match ts(&[if on { "up" } else { "down" }]).await {
        Ok(_) => ok(json!({ "connected": on, "changed": true })),
        Err(e) => failure(e),
    }
}

// ---------- ping ----------

/// One `tailscale ping -c 1` line: ("DERP(nyc)", 23.0) from
/// `pong from laptop (100.64.0.1) via DERP(nyc) in 23ms`.
fn parse_pong(out: &str) -> Option<(String, f64)> {
    let line = out.lines().find(|l| l.starts_with("pong from"))?;
    let (_, rest) = line.split_once(" via ")?;
    let (via, ms) = rest.rsplit_once(" in ")?;
    let ms = ms.trim().strip_suffix("ms")?.parse::<f64>().ok()?;
    Some((via.trim().to_string(), ms))
}

struct Reply {
    ms: Option<f64>,
    via: String,
    error: String,
}

async fn ping_one(target: &str) -> Reply {
    match ts(&["ping", "-c", "1", "--timeout", "3s", target]).await {
        Ok(out) => match parse_pong(&out) {
            Some((via, ms)) => Reply { ms: Some(ms), via, error: String::new() },
            None => Reply { ms: None, via: String::new(), error: "No reply".into() },
        },
        Err(e) => Reply { ms: None, via: String::new(), error: e.lines().next().unwrap_or("No reply").to_string() },
    }
}

fn done_doc(sent: u32, replies: &[Reply]) -> Value {
    let times: Vec<f64> = replies.iter().filter_map(|r| r.ms).collect();
    let received = times.len() as u32;
    let min = times.iter().copied().fold(f64::INFINITY, f64::min);
    let max = times.iter().copied().fold(0.0, f64::max);
    let avg = if times.is_empty() { 0.0 } else { times.iter().sum::<f64>() / times.len() as f64 };
    let round = |x: f64| (x * 10.0).round() / 10.0;
    json!({
        "event": "done", "ok": received > 0, "sent": sent, "received": received,
        "loss_percent": (sent - received) * 100 / sent.max(1), "percent": 100,
        "min_ms": if times.is_empty() { Value::Null } else { json!(round(min)) },
        "avg_ms": if times.is_empty() { Value::Null } else { json!(round(avg)) },
        "max_ms": if times.is_empty() { Value::Null } else { json!(round(max)) },
        "via": replies.iter().rev().find(|r| r.ms.is_some()).map(|r| r.via.clone()),
        "error": if received == 0 { replies.last().map(|r| r.error.clone()).unwrap_or_default() } else { String::new() },
    })
}

/// `ping --stream`: a line for each reply as it arrives, then the summary.
async fn ping_stream(target: &str, count: u32) -> i32 {
    use std::io::Write;
    let mut replies = vec![];
    for seq in 1..=count {
        let r = ping_one(target).await;
        let line = json!({
            "event": "reply", "seq": seq, "of": count, "percent": seq * 100 / count,
            "ok": r.ms.is_some(), "ms": r.ms, "via": r.via, "error": r.error,
        });
        replies.push(r);
        println!("{line}");
        let _ = std::io::stdout().flush();
    }
    println!("{}", done_doc(count, &replies));
    0
}

async fn ping_once(target: &str, count: u32) -> (Value, i32) {
    let mut replies = vec![];
    for _ in 0..count {
        replies.push(ping_one(target).await);
    }
    (done_doc(count, &replies), 0)
}

// ---------- updates ----------

async fn update_check() -> (Value, i32) {
    match check_update().await {
        Ok(found) => (
            json!({
                "ok": true, "current": env!("CARGO_PKG_VERSION"),
                "available": found.is_some(),
                "latest": found.as_ref().map(|u| u.version.clone()).unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
                "url": found.map(|u| u.url).unwrap_or_default(),
            }),
            0,
        ),
        Err(e) => failure(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_commands() {
        assert_eq!(parse(&a(&[])), Ok(None));
        assert_eq!(parse(&a(&["--tray"])), Ok(None));
        assert_eq!(parse(&a(&["status", "--json"])), Ok(Some(Cmd::Status { full: false })));
        assert_eq!(parse(&a(&["status", "--json", "--full"])), Ok(Some(Cmd::Status { full: true })));
        assert_eq!(parse(&a(&["config", "show", "--json"])), Ok(Some(Cmd::ConfigShow)));
        assert_eq!(
            parse(&a(&["config", "set", "ssh", "true", "--dry-run", "--json"])),
            Ok(Some(Cmd::ConfigSet { key: Key::Ssh, value: true, dry_run: true }))
        );
        assert_eq!(parse(&a(&["exit-node", "none", "--json"])), Ok(Some(Cmd::ExitNode { choice: ExitChoice::None, dry_run: false })));
        assert_eq!(
            parse(&a(&["exit-node", "vps.tail1.ts.net", "--json"])),
            Ok(Some(Cmd::ExitNode { choice: ExitChoice::Node("vps.tail1.ts.net".into()), dry_run: false }))
        );
        assert_eq!(
            parse(&a(&["ping", "100.64.0.2", "--json", "--stream", "--count", "5"])),
            Ok(Some(Cmd::Ping { target: "100.64.0.2".into(), count: 5, stream: true }))
        );
        assert_eq!(parse(&a(&["update", "check", "--json"])), Ok(Some(Cmd::UpdateCheck)));
    }

    #[test]
    fn refuses_what_it_does_not_know() {
        for bad in [
            &["status"][..],
            &["config", "set", "nope", "true", "--json"],
            &["config", "set", "ssh", "maybe", "--json"],
            &["config", "show", "--json", "--dry-run"],
            &["exit-node", "-x", "--json"],
            &["exit-node", "a b", "--json"],
            &["exit-node", "--json"],
            &["switch", "a/b", "--json"],
            &["ping", "x", "--json", "--count", "11"],
            &["ping", "x", "--json", "--count", "0"],
            &["accounts", "--json", "--stream"],
            &["status", "--json", "--bogus"],
            &["logout", "now", "--json"],
        ] {
            assert!(parse(&a(bad)).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn targets_match_the_page_pattern() {
        assert!(valid_target("vps.tail1234.ts.net"));
        assert!(valid_target("fd7a::1"));
        assert!(!valid_target("-rf"));
        assert!(!valid_target(""));
        assert!(!valid_target("a b"));
        assert!(!valid_target(&"a".repeat(254)));
    }

    #[test]
    fn every_setting_has_words_both_ways() {
        for k in Key::ALL {
            assert_eq!(Key::parse(k.name()), Some(k));
            assert!(!describe(k, true).0.is_empty() && !describe(k, false).0.is_empty());
            assert_eq!(k.flag().is_some(), !k.is_app());
        }
    }

    #[test]
    fn reads_a_pong() {
        assert_eq!(parse_pong("pong from vps (100.64.0.2) via DERP(nyc) in 23ms\n"), Some(("DERP(nyc)".into(), 23.0)));
        assert_eq!(parse_pong("pong from vps (100.64.0.2) via 1.2.3.4:41641 in 4ms"), Some(("1.2.3.4:41641".into(), 4.0)));
        assert_eq!(parse_pong("no reply"), None);
    }

    #[test]
    fn summarises_replies() {
        let r = |ms: Option<f64>| Reply { ms, via: "DERP(nyc)".into(), error: "No reply".into() };
        let d = done_doc(3, &[r(Some(10.0)), r(None), r(Some(20.0))]);
        assert_eq!(d["received"], 2);
        assert_eq!(d["loss_percent"], 33);
        assert_eq!(d["avg_ms"], 15.0);
        assert_eq!(d["ok"], true);
        let d = done_doc(2, &[r(None), r(None)]);
        assert_eq!(d["ok"], false);
        assert_eq!(d["error"], "No reply");
        assert!(d["avg_ms"].is_null());
    }
}
