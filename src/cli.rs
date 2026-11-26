//! The command line behind uQ's Tailscale page (and usable on its own).
//!
//! Every command prints one JSON document on stdout (`--json` is required), always, whether it
//! worked or not: `{"ok": false, "error": "..."}` with exit code 1 when it didn't. Commands that
//! change something take `--dry-run`, which prints a plan instead and changes nothing:
//! `{"summary", "changes": [{"group", "label", "detail", "reversible"}], "would_refuse", "reason"}`.
//! `ping --stream` prints one JSON object per line (progress), ending with `{"event": "done"}`.

use super::*;
use serde_json::{Value, json};

mod docs;
mod ping;
use docs::*;
use ping::*;

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

#[cfg(test)]
mod tests;
