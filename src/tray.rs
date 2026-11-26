//! The StatusNotifierItem: its icon and menu.

use super::*;

/// Decode an RGBA PNG into the ARGB32 (network order) layout StatusNotifierItem expects.
pub(crate) fn decode_icon(bytes: &[u8]) -> ksni::Icon {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().expect("valid png");
    let mut buf = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    let data = buf[..info.buffer_size()]
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [a, r, g, b])
        .collect();
    ksni::Icon { width: info.width as i32, height: info.height as i32, data }
}

pub(crate) struct TsTray {
    pub(crate) snap: Snapshot,
    pub(crate) update: Option<UpdateInfo>,
    pub(crate) tx: UnboundedSender<Action>,
}

impl TsTray {
    pub(crate) fn act(&self, a: impl Fn() -> Action + Send + 'static) -> Box<dyn Fn(&mut Self) + Send> {
        let tx = self.tx.clone();
        Box::new(move |_| {
            let _ = tx.send(a());
        })
    }

    pub(crate) fn label(text: impl Into<String>) -> MenuItem<Self> {
        StandardItem { label: text.into(), enabled: false, ..Default::default() }.into()
    }

    pub(crate) fn exit_node_menu(&self) -> MenuItem<Self> {
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
            .chain(exits.iter().map(|p| Some(exit_target(p))))
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

    pub(crate) fn check(&self, label: &str, checked: bool, next: impl Fn(bool) -> Action + Send + 'static) -> MenuItem<Self> {
        CheckmarkItem { label: label.into(), checked, activate: self.act(move || next(!checked)), ..Default::default() }.into()
    }

    pub(crate) fn settings_menu(&self) -> MenuItem<Self> {
        let p = &self.snap.prefs;
        let items = vec![
            self.check("Start on login", self.snap.app.autostart, Action::SetAutostart),
            self.check("Auto-connect on launch", self.snap.app.autoconnect, Action::SetAutoconnect),
            self.check("Check for updates automatically", self.snap.app.update_check, Action::SetUpdateCheck),
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

    pub(crate) fn accounts_menu(&self) -> Option<MenuItem<Self>> {
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

    pub(crate) fn peers_menu(&self) -> MenuItem<Self> {
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
        if let Some(u) = &self.update {
            let url = u.url.clone();
            m.push(
                StandardItem {
                    label: format!("Update available: v{}", u.version),
                    icon_name: "software-update-available".into(),
                    activate: self.act(move || Action::OpenUrl(url.clone())),
                    ..Default::default()
                }
                .into(),
            );
            m.push(MenuItem::Separator);
        }
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
                label: "Check for updates".into(),
                icon_name: "system-software-update".into(),
                activate: self.act(|| Action::CheckUpdates(true)),
                ..Default::default()
            }
            .into(),
        );
        m.push(Self::label(concat!("scale_tail v", env!("CARGO_PKG_VERSION"))));
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

