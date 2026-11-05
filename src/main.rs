//! scale_tail: system-tray control centre for the Tailscale CLI.
//!
//! Shells out to `tailscale` (status --json / up / down / set) and exposes
//! the result as a StatusNotifierItem menu (KDE, GNOME w/ AppIndicator, etc).

use ksni::menu::*;
use ksni::TrayMethods;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

// ---------- tailscale status model ----------

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct RawStatus {
    #[serde(default)]
    backend_state: String,
    #[serde(default, rename = "AuthURL")]
    auth_url: String,
    #[serde(default, rename = "Self")]
    me: Option<RawNode>,
    #[serde(default)]
    peer: HashMap<String, RawNode>,
    #[serde(default)]
    current_tailnet: Option<Tailnet>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct Tailnet {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct RawNode {
    #[serde(default)]
    host_name: String,
    #[serde(default, rename = "DNSName")]
    dns_name: String,
    #[serde(default, rename = "OS")]
    os: String,
    #[serde(default, rename = "TailscaleIPs")]
    ips: Vec<String>,
    #[serde(default)]
    online: bool,
    #[serde(default)]
    exit_node: bool,
    #[serde(default)]
    exit_node_option: bool,
}

#[derive(Clone, Debug)]
struct Node {
    name: String,
    dns: String,
    os: String,
    ip: String,
    online: bool,
    is_exit: bool,
    can_exit: bool,
}

impl From<&RawNode> for Node {
    fn from(n: &RawNode) -> Self {
        let dns = n.dns_name.trim_end_matches('.').to_string();
        Node {
            name: if n.host_name.is_empty() { dns.clone() } else { n.host_name.clone() },
            dns,
            os: n.os.clone(),
            ip: n.ips.first().cloned().unwrap_or_default(),
            online: n.online,
            is_exit: n.exit_node,
            can_exit: n.exit_node_option,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    Unavailable(String), // daemon not reachable / CLI missing
    Stopped,
    NeedsLogin(String), // auth url
    Running,
    Other(String),
}

#[derive(Clone, Debug)]
struct Snapshot {
    state: State,
    me: Option<Node>,
    tailnet: String,
    peers: Vec<Node>,
}

async fn ts(args: &[&str]) -> Result<String, String> {
    let out = Command::new("tailscale")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("cannot run tailscale: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { format!("tailscale {} failed", args.join(" ")) } else { err })
    }
}

async fn fetch() -> Snapshot {
    // `status --json` exits non-zero in some stopped states but still prints JSON,
    // so parse stdout regardless of exit code.
    let out = Command::new("tailscale")
        .args(["status", "--json"])
        .stdin(Stdio::null())
        .output()
        .await;
    let fail = |msg: String| Snapshot { state: State::Unavailable(msg), me: None, tailnet: String::new(), peers: vec![] };
    let out = match out {
        Ok(o) => o,
        Err(e) => return fail(format!("tailscale CLI not found: {e}")),
    };
    let raw: RawStatus = match serde_json::from_slice(&out.stdout) {
        Ok(r) => r,
        Err(_) => return fail(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    };
    let state = match raw.backend_state.as_str() {
        "Running" => State::Running,
        "Stopped" => State::Stopped,
        "NeedsLogin" | "NeedsMachineAuth" => State::NeedsLogin(raw.auth_url.clone()),
        other => State::Other(other.to_string()),
    };
    let mut peers: Vec<Node> = raw.peer.values().map(Node::from).collect();
    peers.sort_by(|a, b| b.online.cmp(&a.online).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Snapshot {
        state,
        me: raw.me.as_ref().map(Node::from),
        tailnet: raw.current_tailnet.map(|t| t.name).unwrap_or_default(),
        peers,
    }
}

// ---------- actions ----------

#[derive(Debug)]
enum Action {
    Connect,
    Disconnect,
    SetExitNode(Option<String>),
    Login(String),
    Copy(String),
    Refresh,
    Quit,
}

async fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "Tailscale", "-i", "network-vpn", title, body])
        .status()
        .await;
}

async fn copy_to_clipboard(text: &str) {
    use tokio::io::AsyncWriteExt;
    for (cmd, args) in [("wl-copy", vec![]), ("xclip", vec!["-selection", "clipboard"]), ("xsel", vec!["-ib"])] {
        if let Ok(mut child) = Command::new(cmd).args(&args).stdin(Stdio::piped()).spawn() {
            if let Some(mut si) = child.stdin.take() {
                let _ = si.write_all(text.as_bytes()).await;
            }
            let _ = child.wait().await;
            notify("Copied", text).await;
            return;
        }
    }
    notify("Copy failed", "install wl-clipboard or xclip").await;
}

async fn perform(action: Action) {
    let res = match action {
        Action::Connect => ts(&["up"]).await.map(|_| ()),
        Action::Disconnect => ts(&["down"]).await.map(|_| ()),
        Action::SetExitNode(n) => {
            let arg = format!("--exit-node={}", n.unwrap_or_default());
            ts(&["set", &arg]).await.map(|_| ())
        }
        Action::Login(url) => {
            if url.is_empty() {
                // ask the daemon to start an interactive login and open the URL it reports
                let _ = Command::new("tailscale").args(["login"]).stdin(Stdio::null()).spawn();
            } else {
                let _ = Command::new("xdg-open").arg(url).spawn();
            }
            Ok(())
        }
        Action::Copy(t) => {
            copy_to_clipboard(&t).await;
            Ok(())
        }
        Action::Refresh | Action::Quit => Ok(()),
    };
    if let Err(e) = res {
        let hint = if e.contains("access denied") || e.contains("Access denied") {
            format!("{e}\n\nRun once: run0 tailscale set --operator=$USER")
        } else {
            e
        };
        notify("Tailscale error", &hint).await;
    }
}

// ---------- tray ----------

struct TsTray {
    snap: Snapshot,
    tx: UnboundedSender<Action>,
}

impl TsTray {
    fn act(&self, a: impl Fn() -> Action + Send + 'static) -> Box<dyn Fn(&mut Self) + Send> {
        let tx = self.tx.clone();
        Box::new(move |_| {
            let _ = tx.send(a());
        })
    }

    fn label(text: impl Into<String>) -> MenuItem<Self> {
        StandardItem { label: text.into(), enabled: false, ..Default::default() }.into()
    }

    fn exit_node_menu(&self) -> MenuItem<Self> {
        let exits: Vec<&Node> = self.snap.peers.iter().filter(|p| p.can_exit).collect();
        let selected = exits.iter().position(|p| p.is_exit).map(|i| i + 1).unwrap_or(0);
        let title = match exits.iter().find(|p| p.is_exit) {
            Some(p) => format!("Exit node: {}", p.name),
            None => "Exit node: none".to_string(),
        };

        let mut options = vec![RadioItem { label: "None".into(), ..Default::default() }];
        for p in &exits {
            let dot = if p.online { "●" } else { "○" };
            options.push(RadioItem {
                label: format!("{dot} {}", p.name),
                enabled: p.online,
                ..Default::default()
            });
        }
        let targets: Vec<Option<String>> = std::iter::once(None)
            .chain(exits.iter().map(|p| Some(if p.dns.is_empty() { p.ip.clone() } else { p.dns.clone() })))
            .collect();
        let tx = self.tx.clone();

        let mut submenu: Vec<MenuItem<Self>> = vec![];
        if exits.is_empty() {
            submenu.push(Self::label("No exit nodes available"));
        } else {
            submenu.push(
                RadioGroup {
                    selected,
                    select: Box::new(move |_, i| {
                        let _ = tx.send(Action::SetExitNode(targets[i].clone()));
                    }),
                    options,
                }
                .into(),
            );
        }
        SubMenu { label: title, submenu, ..Default::default() }.into()
    }

    fn peers_menu(&self) -> MenuItem<Self> {
        let online = self.snap.peers.iter().filter(|p| p.online).count();
        let items = if self.snap.peers.is_empty() {
            vec![Self::label("No peers")]
        } else {
            self.snap
                .peers
                .iter()
                .map(|p| {
                    let dot = if p.online { "●" } else { "○" };
                    let mut sub = vec![Self::label(format!("{}  ·  {}", p.ip, p.os))];
                    let ip = p.ip.clone();
                    sub.push(
                        StandardItem {
                            label: "Copy IP".into(),
                            icon_name: "edit-copy".into(),
                            activate: self.act(move || Action::Copy(ip.clone())),
                            ..Default::default()
                        }
                        .into(),
                    );
                    let dns = p.dns.clone();
                    sub.push(
                        StandardItem {
                            label: "Copy DNS name".into(),
                            icon_name: "edit-copy".into(),
                            activate: self.act(move || Action::Copy(dns.clone())),
                            ..Default::default()
                        }
                        .into(),
                    );
                    SubMenu { label: format!("{dot} {}", p.name), submenu: sub, ..Default::default() }.into()
                })
                .collect()
        };
        SubMenu {
            label: format!("Peers ({online}/{} online)", self.snap.peers.len()),
            submenu: items,
            ..Default::default()
        }
        .into()
    }
}

impl ksni::Tray for TsTray {
    fn id(&self) -> String {
        "scale_tail".into()
    }
    fn title(&self) -> String {
        "Tailscale".into()
    }
    fn icon_name(&self) -> String {
        match self.snap.state {
            State::Running => "network-vpn",
            _ => "network-vpn-disconnected",
        }
        .into()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        let desc = match &self.snap.state {
            State::Running => "Connected".to_string(),
            State::Stopped => "Disconnected".into(),
            State::NeedsLogin(_) => "Login required".into(),
            State::Unavailable(e) => e.clone(),
            State::Other(s) => s.clone(),
        };
        ksni::ToolTip { title: "Tailscale".into(), description: desc, ..Default::default() }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut m: Vec<MenuItem<Self>> = vec![];
        match &self.snap.state {
            State::Unavailable(e) => {
                m.push(Self::label("Tailscale unavailable"));
                if !e.is_empty() {
                    m.push(Self::label(e.lines().next().unwrap_or("").to_string()));
                }
            }
            State::Stopped => {
                m.push(Self::label("● Disconnected"));
                m.push(
                    StandardItem {
                        label: "Connect".into(),
                        icon_name: "network-connect".into(),
                        activate: self.act(|| Action::Connect),
                        ..Default::default()
                    }
                    .into(),
                );
            }
            State::NeedsLogin(url) => {
                m.push(Self::label("Login required"));
                let url = url.clone();
                m.push(
                    StandardItem {
                        label: "Log in…".into(),
                        icon_name: "dialog-password".into(),
                        activate: self.act(move || Action::Login(url.clone())),
                        ..Default::default()
                    }
                    .into(),
                );
            }
            State::Other(s) => m.push(Self::label(format!("State: {s}"))),
            State::Running => {
                if let Some(me) = &self.snap.me {
                    m.push(Self::label(format!("● Connected — {}", me.name)));
                    if !self.snap.tailnet.is_empty() {
                        m.push(Self::label(format!("Tailnet: {}", self.snap.tailnet)));
                    }
                    let ip = me.ip.clone();
                    m.push(
                        StandardItem {
                            label: format!("{}  (click to copy)", me.ip),
                            icon_name: "edit-copy".into(),
                            activate: self.act(move || Action::Copy(ip.clone())),
                            ..Default::default()
                        }
                        .into(),
                    );
                }
                m.push(MenuItem::Separator);
                m.push(self.exit_node_menu());
                m.push(self.peers_menu());
                m.push(MenuItem::Separator);
                m.push(
                    StandardItem {
                        label: "Disconnect".into(),
                        icon_name: "network-disconnect".into(),
                        activate: self.act(|| Action::Disconnect),
                        ..Default::default()
                    }
                    .into(),
                );
            }
        }
        m.push(MenuItem::Separator);
        m.push(
            StandardItem {
                label: "Refresh".into(),
                icon_name: "view-refresh".into(),
                activate: self.act(|| Action::Refresh),
                ..Default::default()
            }
            .into(),
        );
        m.push(
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: self.act(|| Action::Quit),
                ..Default::default()
            }
            .into(),
        );
        m
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (tx, mut rx) = unbounded_channel::<Action>();
    let tray = TsTray { snap: fetch().await, tx };
    let handle = match tray.spawn().await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("failed to create tray icon (is a StatusNotifier host running?): {e}");
            std::process::exit(1);
        }
    };

    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        let refresh;
        tokio::select! {
            _ = tick.tick() => { refresh = true; }
            Some(a) = rx.recv() => {
                if matches!(a, Action::Quit) { handle.shutdown().await; return; }
                perform(a).await;
                refresh = true;
            }
        }
        if refresh {
            let snap = fetch().await;
            handle.update(move |t: &mut TsTray| t.snap = snap).await;
        }
    }
}
