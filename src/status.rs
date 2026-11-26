//! The model of `tailscale status --json` and `debug prefs`, and reading it into a [`Snapshot`].

use super::*;

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct RawStatus {
    #[serde(default)]
    pub(crate) backend_state: String,
    #[serde(default, rename = "AuthURL")]
    pub(crate) auth_url: String,
    #[serde(default, rename = "Self")]
    pub(crate) me: Option<RawNode>,
    #[serde(default)]
    pub(crate) peer: HashMap<String, RawNode>,
    #[serde(default)]
    pub(crate) current_tailnet: Option<Tailnet>,
    #[serde(default)]
    pub(crate) user: HashMap<String, RawUser>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct RawUser {
    #[serde(default)]
    pub(crate) login_name: String,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Tailnet {
    #[serde(default)]
    pub(crate) name: String,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct RawNode {
    #[serde(default)]
    pub(crate) host_name: String,
    #[serde(default, rename = "DNSName")]
    pub(crate) dns_name: String,
    #[serde(default, rename = "OS")]
    pub(crate) os: String,
    #[serde(default, rename = "TailscaleIPs")]
    pub(crate) ips: Vec<String>,
    #[serde(default)]
    pub(crate) online: bool,
    #[serde(default)]
    pub(crate) exit_node: bool,
    #[serde(default)]
    pub(crate) exit_node_option: bool,
    #[serde(default, rename = "UserID")]
    pub(crate) user_id: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node {
    pub(crate) name: String,
    pub(crate) dns: String,
    pub(crate) os: String,
    pub(crate) ip: String,
    pub(crate) online: bool,
    pub(crate) is_exit: bool,
    pub(crate) can_exit: bool,
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
pub(crate) enum State {
    Unavailable(String), // daemon not reachable / CLI missing
    Stopped,
    NeedsLogin(String), // auth url
    Running,
    Other(String),
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Prefs {
    #[serde(default)]
    pub(crate) route_all: bool,
    #[serde(default, rename = "CorpDNS")]
    pub(crate) corp_dns: bool,
    #[serde(default, rename = "ExitNodeAllowLANAccess")]
    pub(crate) exit_node_allow_lan_access: bool,
    #[serde(default)]
    pub(crate) shields_up: bool,
    #[serde(default, rename = "RunSSH")]
    pub(crate) run_ssh: bool,
    #[serde(default)]
    pub(crate) advertise_routes: Option<Vec<String>>,
}

impl Prefs {
    pub(crate) fn advertises_exit_node(&self) -> bool {
        self.advertise_routes.as_ref().is_some_and(|r| r.iter().any(|x| x == "0.0.0.0/0"))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Account {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) tailnet: String,
    pub(crate) login_name: String,
    pub(crate) active: bool,
}

/// scale_tail's own settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AppSettings {
    pub(crate) autoconnect: bool,
    pub(crate) autostart: bool,
    pub(crate) update_check: bool, // on by default; disabled by a marker file
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    pub(crate) state: State,
    pub(crate) me: Option<Node>,
    pub(crate) tailnet: String,
    pub(crate) peers: Vec<Node>,
    pub(crate) prefs: Prefs,
    pub(crate) accounts: Vec<Account>,
    pub(crate) app: AppSettings,
}

pub(crate) async fn ts(args: &[&str]) -> Result<String, String> {
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

/// What `tailscale status --json` says, without the prefs and accounts.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Status {
    pub(crate) state: State,
    pub(crate) me: Option<Node>,
    pub(crate) tailnet: String,
    /// The Tailscale login of this computer's account ("" when unknown).
    pub(crate) login: String,
    pub(crate) peers: Vec<Node>,
}

/// Reads one `tailscale status --json` run. It exits non-zero in some stopped
/// states but still prints JSON, so stdout is parsed regardless of the exit code.
pub(crate) fn parse_status(stdout: &[u8], stderr: &[u8]) -> Status {
    let raw: RawStatus = match serde_json::from_slice(stdout) {
        Ok(r) => r,
        Err(_) => {
            return Status {
                state: State::Unavailable(String::from_utf8_lossy(stderr).trim().to_string()),
                me: None,
                tailnet: String::new(),
                login: String::new(),
                peers: vec![],
            };
        }
    };
    let state = match raw.backend_state.as_str() {
        "Running" => State::Running,
        "Stopped" => State::Stopped,
        "NeedsLogin" | "NeedsMachineAuth" => State::NeedsLogin(raw.auth_url.clone()),
        other => State::Other(other.to_string()),
    };
    let mut peers: Vec<Node> = raw.peer.values().map(Node::from).collect();
    peers.sort_by(|a, b| b.online.cmp(&a.online).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let login = raw.me.as_ref().and_then(|m| raw.user.get(&m.user_id.to_string())).map(|u| u.login_name.clone()).unwrap_or_default();
    Status { state, me: raw.me.as_ref().map(Node::from), tailnet: raw.current_tailnet.map(|t| t.name).unwrap_or_default(), login, peers }
}

pub(crate) async fn fetch_status() -> Status {
    match Command::new("tailscale").args(["status", "--json"]).stdin(Stdio::null()).output().await {
        Ok(o) => parse_status(&o.stdout, &o.stderr),
        Err(e) => Status { state: State::Unavailable(format!("tailscale CLI not found: {e}")), me: None, tailnet: String::new(), login: String::new(), peers: vec![] },
    }
}

/// Tailscale's state. `prev` (when given) supplies the prefs and accounts
/// instead of asking `tailscale debug prefs` and `switch --list` again: those
/// change only through our own actions or rarely, so they are re-read less often.
pub(crate) async fn fetch(prev: Option<&Snapshot>) -> Snapshot {
    let Status { state, me, tailnet, peers, .. } = fetch_status().await;
    if let State::Unavailable(_) = state {
        return Snapshot { state, me, tailnet, peers, prefs: Prefs::default(), accounts: vec![], app: load_settings() };
    }
    let (prefs, accounts) = match prev {
        Some(p) => (p.prefs.clone(), p.accounts.clone()),
        None => (
            ts(&["debug", "prefs"]).await.ok().and_then(|o| serde_json::from_str(&o).ok()).unwrap_or_default(),
            ts(&["switch", "--list"]).await.map(|o| parse_accounts(&o)).unwrap_or_default(),
        ),
    };
    Snapshot { state, me, tailnet, peers, prefs, accounts, app: load_settings() }
}

/// The address `tailscale set --exit-node` takes for a peer (as the tray menu uses).
pub(crate) fn exit_target(n: &Node) -> String {
    if n.dns.is_empty() { n.ip.clone() } else { n.dns.clone() }
}

/// `scale_tail status --json`: a small, stable summary for scripts and the uQ
/// widget (uq/widgets/scale-tail), so they need not read `tailscale status --json`.
pub(crate) fn status_json(s: &Status) -> serde_json::Value {
    use serde_json::json;
    let (state, message) = match &s.state {
        State::Running => ("running", String::new()),
        State::Stopped => ("stopped", String::new()),
        State::NeedsLogin(url) => ("needs_login", url.clone()),
        State::Unavailable(e) => ("unavailable", e.clone()),
        State::Other(o) => ("other", o.clone()),
    };
    let exits: Vec<&Node> = s.peers.iter().filter(|p| p.can_exit).collect();
    let node = |p: &Node| json!({ "name": p.name, "target": exit_target(p), "online": p.online, "active": p.is_exit });
    json!({
        "version": 1,
        "state": state,
        "connected": s.state == State::Running,
        "message": message,
        "tailnet": s.tailnet,
        "self": s.me.as_ref().map(|m| json!({ "name": m.name, "dns": m.dns, "ip": m.ip })),
        "exit_node": exits.iter().find(|p| p.is_exit).map(|p| node(p)),
        "exit_nodes": exits.iter().map(|p| node(p)).collect::<Vec<_>>(),
    })
}

/// Parses `tailscale switch --list` (header row, then `ID  Tailnet  Account[*]`).
pub(crate) fn parse_accounts(out: &str) -> Vec<Account> {
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 3 {
                return None;
            }
            let active = f[2].ends_with('*');
            let login_name = f[2].trim_end_matches('*').to_string();
            Some(Account { id: f[0].to_string(), label: format!("{login_name} ({})", f[1]), tailnet: f[1].to_string(), login_name, active })
        })
        .collect()
}

