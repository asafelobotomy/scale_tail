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

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct Prefs {
    #[serde(default)]
    route_all: bool,
    #[serde(default, rename = "CorpDNS")]
    corp_dns: bool,
    #[serde(default, rename = "ExitNodeAllowLANAccess")]
    exit_node_allow_lan_access: bool,
    #[serde(default)]
    shields_up: bool,
    #[serde(default, rename = "RunSSH")]
    run_ssh: bool,
    #[serde(default)]
    advertise_routes: Option<Vec<String>>,
}

impl Prefs {
    fn advertises_exit_node(&self) -> bool {
        self.advertise_routes.as_ref().is_some_and(|r| r.iter().any(|x| x == "0.0.0.0/0"))
    }
}

#[derive(Clone, Debug)]
struct Account {
    id: String,
    label: String,
    active: bool,
}

/// scale_tail's own settings.
#[derive(Clone, Debug, Default)]
struct AppSettings {
    autoconnect: bool,
    autostart: bool,
}

fn config_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| ".".into())
}

fn autostart_file() -> std::path::PathBuf {
    config_dir().join("autostart/scale_tail.desktop")
}

fn autoconnect_file() -> std::path::PathBuf {
    config_dir().join("scale_tail/autoconnect")
}

fn load_settings() -> AppSettings {
    AppSettings { autoconnect: autoconnect_file().exists(), autostart: autostart_file().exists() }
}

fn set_autoconnect(on: bool) -> Result<(), String> {
    let f = autoconnect_file();
    if on {
        std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&f, "1").map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(&f).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).map_err(|e| e.to_string())
    }
}

fn set_autostart(on: bool) -> Result<(), String> {
    let f = autostart_file();
    if !on {
        return std::fs::remove_file(&f)
            .or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) })
            .map_err(|e| e.to_string());
    }
    // Inside an AppImage, $APPIMAGE is the file to relaunch; current_exe would be a temp mount.
    let exe = std::env::var("APPIMAGE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or("cannot determine executable path")?;
    let exe = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(
        &f,
        format!(
            "[Desktop Entry]\nType=Application\nName=scale_tail\nComment=System tray control centre for Tailscale\nExec=\"{exe}\"\nIcon=scale_tail\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
        ),
    )
    .map_err(|e| e.to_string())
}

#[derive(Clone, Debug)]
struct Snapshot {
    state: State,
    me: Option<Node>,
    tailnet: String,
    peers: Vec<Node>,
    prefs: Prefs,
    accounts: Vec<Account>,
    app: AppSettings,
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
    let fail = |msg: String| Snapshot {
        state: State::Unavailable(msg),
        me: None,
        tailnet: String::new(),
        peers: vec![],
        prefs: Prefs::default(),
        accounts: vec![],
        app: load_settings(),
    };
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
    let prefs = ts(&["debug", "prefs"]).await.ok().and_then(|o| serde_json::from_str(&o).ok()).unwrap_or_default();
    let accounts = ts(&["switch", "--list"]).await.map(|o| parse_accounts(&o)).unwrap_or_default();
    Snapshot {
        state,
        me: raw.me.as_ref().map(Node::from),
        tailnet: raw.current_tailnet.map(|t| t.name).unwrap_or_default(),
        peers,
        prefs,
        accounts,
        app: load_settings(),
    }
}

/// Parses `tailscale switch --list` (header row, then `ID  Tailnet  Account[*]`).
fn parse_accounts(out: &str) -> Vec<Account> {
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 3 {
                return None;
            }
            let active = f[2].ends_with('*');
            Some(Account { id: f[0].to_string(), label: format!("{} ({})", f[2].trim_end_matches('*'), f[1]), active })
        })
        .collect()
}

// ---------- actions ----------

#[derive(Debug)]
enum Action {
    Connect,
    Disconnect,
    SetExitNode(Option<String>),
    UseSuggestedExitNode,
    SetPref(&'static str, bool), // `tailscale set --<flag>=<bool>`
    SetAutostart(bool),
    SetAutoconnect(bool),
    SwitchAccount(String),
    Ping(String),
    SendFile(String),
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
        Action::UseSuggestedExitNode => ts(&["set", "--exit-node=auto:any"]).await.map(|_| ()),
        Action::SetPref(flag, on) => ts(&["set", &format!("--{flag}={on}")]).await.map(|_| ()),
        Action::SetAutostart(on) => set_autostart(on),
        Action::SetAutoconnect(on) => set_autoconnect(on),
        Action::SwitchAccount(id) => ts(&["switch", &id]).await.map(|_| ()),
        Action::Ping(target) => match ts(&["ping", "-c", "3", "--timeout", "3s", &target]).await {
            Ok(out) => {
                notify(&format!("Ping {target}"), out.lines().last().unwrap_or("done")).await;
                Ok(())
            }
            Err(e) => Err(e),
        },
        Action::SendFile(target) => {
            let pick = Command::new("zenity")
                .args(["--file-selection", "--multiple", "--separator=\n", "--title=Send files via Tailscale"])
                .stdin(Stdio::null())
                .output()
                .await;
            match pick {
                Ok(o) if o.status.success() => {
                    let files: Vec<String> = String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect();
                    if files.is_empty() {
                        Ok(())
                    } else {
                        let dest = format!("{target}:");
                        let mut args: Vec<&str> = vec!["file", "cp"];
                        args.extend(files.iter().map(String::as_str));
                        args.push(&dest);
                        let r = ts(&args).await.map(|_| ());
                        if r.is_ok() {
                            notify("File sent", &format!("{} file(s) to {target}", files.len())).await;
                        }
                        r
                    }
                }
                Ok(_) => Ok(()), // cancelled
                Err(_) => Err("install zenity to pick files".into()),
            }
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

/// Decode an RGBA PNG into the ARGB32 (network order) layout StatusNotifierItem expects.
fn decode_icon(bytes: &[u8]) -> ksni::Icon {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().expect("valid png");
    let mut buf = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    let data = buf[..info.buffer_size()]
        .chunks_exact(4)
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    ksni::Icon { width: info.width as i32, height: info.height as i32, data }
}

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
        if !exits.is_empty() {
            submenu.push(
                StandardItem {
                    label: "Use suggested exit node".into(),
                    icon_name: "starred-symbolic".into(),
                    activate: self.act(|| Action::UseSuggestedExitNode),
                    ..Default::default()
                }
                .into(),
            );
            submenu.push(MenuItem::Separator);
        }
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

    fn check(&self, label: &str, checked: bool, next: impl Fn(bool) -> Action + Send + 'static) -> MenuItem<Self> {
        CheckmarkItem { label: label.into(), checked, activate: self.act(move || next(!checked)), ..Default::default() }.into()
    }

    fn settings_menu(&self) -> MenuItem<Self> {
        let p = &self.snap.prefs;
        let items = vec![
            self.check("Start on login", self.snap.app.autostart, Action::SetAutostart),
            self.check("Auto-connect on launch", self.snap.app.autoconnect, Action::SetAutoconnect),
            MenuItem::Separator,
            self.check("Use Tailscale DNS", p.corp_dns, |v| Action::SetPref("accept-dns", v)),
            self.check("Accept subnet routes", p.route_all, |v| Action::SetPref("accept-routes", v)),
            self.check("Allow LAN access with exit node", p.exit_node_allow_lan_access, |v| {
                Action::SetPref("exit-node-allow-lan-access", v)
            }),
            self.check("Shields up (block incoming)", p.shields_up, |v| Action::SetPref("shields-up", v)),
            self.check("Tailscale SSH server", p.run_ssh, |v| Action::SetPref("ssh", v)),
            self.check("Advertise as exit node", p.advertises_exit_node(), |v| Action::SetPref("advertise-exit-node", v)),
        ];
        SubMenu { label: "Settings".into(), icon_name: "configure".into(), submenu: items, ..Default::default() }.into()
    }

    fn accounts_menu(&self) -> Option<MenuItem<Self>> {
        if self.snap.accounts.len() < 2 {
            return None;
        }
        let selected = self.snap.accounts.iter().position(|a| a.active).unwrap_or(0);
        let ids: Vec<String> = self.snap.accounts.iter().map(|a| a.id.clone()).collect();
        let tx = self.tx.clone();
        let options = self.snap.accounts.iter().map(|a| RadioItem { label: a.label.clone(), ..Default::default() }).collect();
        Some(
            SubMenu {
                label: "Account".into(),
                submenu: vec![RadioGroup {
                    selected,
                    select: Box::new(move |_, i| {
                        let _ = tx.send(Action::SwitchAccount(ids[i].clone()));
                    }),
                    options,
                }
                .into()],
                ..Default::default()
            }
            .into(),
        )
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
                    if p.online {
                        let target = if p.dns.is_empty() { p.ip.clone() } else { p.dns.clone() };
                        let t2 = target.clone();
                        sub.push(MenuItem::Separator);
                        sub.push(
                            StandardItem {
                                label: "Ping".into(),
                                icon_name: "network-wireless".into(),
                                activate: self.act(move || Action::Ping(target.clone())),
                                ..Default::default()
                            }
                            .into(),
                        );
                        sub.push(
                            StandardItem {
                                label: "Send file…".into(),
                                icon_name: "document-send".into(),
                                activate: self.act(move || Action::SendFile(t2.clone())),
                                ..Default::default()
                            }
                            .into(),
                        );
                    }
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
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        static ICONS: std::sync::OnceLock<(ksni::Icon, ksni::Icon)> = std::sync::OnceLock::new();
        let (on, off) = ICONS.get_or_init(|| {
            (
                decode_icon(include_bytes!("../packaging/tray_on.png")),
                decode_icon(include_bytes!("../packaging/tray_off.png")),
            )
        });
        let icon = if self.snap.state == State::Running { on } else { off };
        vec![ksni::Icon { width: icon.width, height: icon.height, data: icon.data.clone() }]
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
                m.extend(self.accounts_menu());
                m.push(self.settings_menu());
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
                m.extend(self.accounts_menu());
                m.push(self.settings_menu());
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
    let first = fetch().await;
    if first.app.autoconnect && first.state == State::Stopped {
        perform(Action::Connect).await;
    }
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
