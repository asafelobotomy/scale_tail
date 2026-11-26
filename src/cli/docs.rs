//! The JSON documents the command line prints, and reading the settings into them.

use super::*;

pub(crate) fn failure(msg: impl Into<String>) -> (Value, i32) {
    (json!({ "ok": false, "error": msg.into() }), 1)
}

pub(crate) fn ok(extra: Value) -> (Value, i32) {
    let mut doc = json!({ "ok": true });
    if let (Some(d), Some(e)) = (doc.as_object_mut(), extra.as_object()) {
        d.extend(e.clone());
    }
    (doc, 0)
}

pub(crate) fn change(group: &str, label: impl Into<String>, detail: impl Into<String>, reversible: bool) -> Value {
    json!({ "group": group, "label": label.into(), "detail": detail.into(), "reversible": reversible })
}

pub(crate) fn plan(summary: impl Into<String>, changes: Vec<Value>) -> (Value, i32) {
    (json!({ "summary": summary.into(), "changes": changes, "would_refuse": false, "reason": "" }), 0)
}

pub(crate) fn refuse(reason: impl Into<String>) -> (Value, i32) {
    (json!({ "summary": "Nothing would change", "changes": [], "would_refuse": true, "reason": reason.into() }), 0)
}

/// Words for what turning a setting on or off does, for plans and the page.
pub(crate) fn describe(key: Key, on: bool) -> (&'static str, &'static str) {
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

pub(crate) fn group_of(key: Key) -> &'static str {
    if key.is_app() { "scale_tail" } else { "Tailscale" }
}

pub(crate) fn state_name(s: &State) -> &'static str {
    match s {
        State::Running => "running",
        State::Stopped => "stopped",
        State::NeedsLogin(_) => "needs_login",
        State::Unavailable(_) => "unavailable",
        State::Other(_) => "other",
    }
}

pub(crate) fn app_value(app: &AppSettings, key: Key) -> bool {
    match key {
        Key::Autostart => app.autostart,
        Key::AutoConnect => app.autoconnect,
        _ => app.update_check,
    }
}

pub(crate) fn prefs_value(p: &Prefs, key: Key) -> bool {
    match key {
        Key::AcceptRoutes => p.route_all,
        Key::LanAccess => p.exit_node_allow_lan_access,
        Key::AcceptDns => p.corp_dns,
        Key::ShieldsUp => p.shields_up,
        Key::Ssh => p.run_ssh,
        _ => p.advertises_exit_node(),
    }
}

pub(crate) async fn read_prefs() -> Result<Prefs, String> {
    let out = ts(&["debug", "prefs"]).await?;
    serde_json::from_str(&out).map_err(|e| format!("Tailscale's settings were not readable: {e}"))
}

pub(crate) async fn read_profiles() -> Vec<Account> {
    ts(&["switch", "--list"]).await.map(|o| parse_accounts(&o)).unwrap_or_default()
}

pub(crate) fn prefs_json(p: &Prefs) -> Value {
    let mut m = serde_json::Map::new();
    for k in Key::ALL.into_iter().filter(|k| !k.is_app()) {
        m.insert(k.name().into(), json!(prefs_value(p, k)));
    }
    Value::Object(m)
}

pub(crate) fn app_json(a: &AppSettings) -> Value {
    let mut m = serde_json::Map::new();
    for k in Key::ALL.into_iter().filter(|k| k.is_app()) {
        m.insert(k.name().into(), json!(app_value(a, k)));
    }
    Value::Object(m)
}

pub(crate) fn node_json(n: &Node) -> Value {
    json!({
        "name": n.name, "dns": n.dns, "ip": n.ip, "os": n.os, "online": n.online,
        "exit_node_option": n.can_exit, "exit_node": n.is_exit, "target": exit_target(n),
    })
}

pub(crate) fn account_json(a: &Account) -> Value {
    json!({ "id": a.id, "label": a.label, "tailnet": a.tailnet, "account": a.login_name, "active": a.active })
}

