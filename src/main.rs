//! scale_tail: system-tray control centre for the Tailscale CLI.
//!
//! Shells out to `tailscale` (status --json / up / down / set) and exposes
//! the result as a StatusNotifierItem menu (KDE, GNOME w/ AppIndicator, etc).

mod actions;
mod cli;
mod settings;
mod status;
mod tray;
#[rustfmt::skip]
mod uq_consts; // uQ's native/uq-consts, vendored unchanged

use ksni::menu::*;
use ksni::TrayMethods;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use actions::*;
use settings::*;
use status::*;
use tray::*;

/// Whether the uQ desktop runs in this session (its daemon owns `org.uq.Desktop1`).
/// uQ shows Tailscale in quick settings and Settings, so the tray stays off there.
async fn uq_running() -> bool {
    let check = async {
        let conn = zbus::Connection::session().await.ok()?;
        let dbus = zbus::fdo::DBusProxy::new(&conn).await.ok()?;
        let name = zbus::names::BusName::try_from(uq_consts::dbus::DESKTOP_NAME).ok()?;
        dbus.name_has_owner(name).await.ok()
    };
    tokio::time::timeout(Duration::from_secs(2), check).await.ok().flatten().unwrap_or(false)
}

/// The session-bus name the tray owns, so only one runs per session.
const INSTANCE_NAME: &str = "io.github.asafelobotomy.scale_tail";

/// One tray per session: own [`INSTANCE_NAME`] on the session bus. `Err` when another scale_tail
/// tray already owns it (a second one would show a second icon). The connection must be kept for
/// the tray's lifetime. Without a session bus the tray starts anyway.
async fn claim_instance() -> Result<Option<zbus::Connection>, ()> {
    let Ok(conn) = zbus::Connection::session().await else { return Ok(None) };
    match conn.request_name_with_flags(INSTANCE_NAME, zbus::fdo::RequestNameFlags::DO_NOT_QUEUE).await {
        Ok(_) => Ok(Some(conn)),
        Err(zbus::Error::NameTaken) => Err(()),
        Err(_) => Ok(None),
    }
}

#[derive(Debug, PartialEq)]
enum Cli {
    Tray { force: bool },
    Run(cli::Cmd),
}

fn cli_command(args: &[String]) -> Result<Cli, String> {
    if let Some(cmd) = cli::parse(args)? {
        return Ok(Cli::Run(cmd));
    }
    // `open` starts the tray even under uQ (the launcher entry runs it). Other arguments are
    // ignored, as they always were, so existing launchers keep working.
    Ok(Cli::Tray { force: args.iter().any(|a| a == "--tray" || a == "open") })
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("scale_tail {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    match cli_command(&args) {
        Ok(Cli::Tray { force }) => {
            if !force && uq_running().await {
                eprintln!("scale_tail: uQ is running and shows Tailscale in quick settings; not starting the tray (use --tray to start it anyway)");
                return;
            }
        }
        Ok(Cli::Run(cmd)) => std::process::exit(cli::run(cmd).await),
        Err(e) => {
            eprintln!("scale_tail: {e}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    }
    let Ok(_instance) = claim_instance().await else {
        eprintln!("scale_tail: the tray is already running in this session");
        return;
    };
    migrate_autostart();
    let (tx, mut rx) = unbounded_channel::<Action>();
    let first = fetch(None).await;
    if first.app.autoconnect && first.state == State::Stopped {
        perform(Action::Connect).await;
    }
    let mut last = fetch(None).await;
    let tray = TsTray { snap: last.clone(), update: None, tx: tx.clone() };
    let handle = match tray.spawn().await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("failed to create tray icon (is a StatusNotifier host running?): {e}");
            std::process::exit(1);
        }
    };

    let mut tick = tokio::time::interval(Duration::from_secs(5));
    let mut update_tick = tokio::time::interval(Duration::from_secs(6 * 60 * 60)); // first tick fires at startup
    let mut notified: Option<String> = None;
    // Prefs and accounts are re-read every FULL_EVERY ticks, and after any action.
    const FULL_EVERY: u32 = 6;
    let mut ticks: u32 = 0;
    loop {
        let mut refresh = false;
        let mut full = false;
        let mut check: Option<bool> = None;
        tokio::select! {
            _ = tick.tick() => {
                refresh = true;
                ticks = ticks.wrapping_add(1);
                full = ticks.is_multiple_of(FULL_EVERY);
            }
            _ = update_tick.tick() => {
                if load_settings().update_check { check = Some(false); }
            }
            Some(a) = rx.recv() => {
                match a {
                    Action::Quit => { handle.shutdown().await; return; }
                    Action::CheckUpdates(manual) => check = Some(manual),
                    // An action finished (see below): refresh everything.
                    Action::Refresh => { refresh = true; full = true; }
                    // Run actions off the loop, so a file picker or a ping never
                    // stalls the tray; each one asks for a refresh when it is done.
                    a => {
                        let done = tx.clone();
                        tokio::spawn(async move {
                            perform(a).await;
                            let _ = done.send(Action::Refresh);
                        });
                    }
                }
            }
        }
        if let Some(manual) = check {
            match check_update().await {
                Ok(found) => {
                    if let Some(u) = &found {
                        if manual || notified.as_deref() != Some(&u.version) {
                            let how = if std::env::var_os("APPIMAGE").is_some() {
                                "Update through your AppImage launcher or AppImageUpdate, or download it from the release page."
                            } else {
                                "Click the tray menu to open the release page."
                            };
                            notify("scale_tail update available", &format!("Version {} is out. {how}", u.version)).await;
                            notified = Some(u.version.clone());
                        }
                    } else if manual {
                        notify("scale_tail", "You're up to date.").await;
                    }
                    handle.update(move |t: &mut TsTray| t.update = found).await;
                }
                Err(e) if manual => notify("Update check failed", &e).await,
                Err(_) => {}
            }
        }
        if refresh {
            let snap = fetch(if full { None } else { Some(&last) }).await;
            // Only touch the tray (a D-Bus menu rebuild) when something changed.
            if snap != last {
                last = snap.clone();
                handle.update(move |t: &mut TsTray| t.snap = snap).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from real `tailscale status --json` output.
    const STATUS: &str = r#"{
        "BackendState": "Running",
        "AuthURL": "",
        "Self": {"HostName": "laptop", "DNSName": "laptop.tail1234.ts.net.", "OS": "linux",
                 "TailscaleIPs": ["100.64.0.1", "fd7a::1"], "Online": true},
        "CurrentTailnet": {"Name": "example.com"},
        "Peer": {
            "nodekey:a": {"HostName": "vps", "DNSName": "vps.tail1234.ts.net.", "OS": "linux",
                          "TailscaleIPs": ["100.64.0.2"], "Online": true,
                          "ExitNode": true, "ExitNodeOption": true},
            "nodekey:b": {"HostName": "home", "DNSName": "", "OS": "linux",
                          "TailscaleIPs": ["100.64.0.3"], "Online": false,
                          "ExitNode": false, "ExitNodeOption": true},
            "nodekey:c": {"HostName": "phone", "DNSName": "phone.tail1234.ts.net.", "OS": "android",
                          "TailscaleIPs": ["100.64.0.4"], "Online": true}
        }
    }"#;

    #[test]
    fn status_json_running_with_exit_node() {
        let v = status_json(&parse_status(STATUS.as_bytes(), b""));
        assert_eq!(v["version"], 1);
        assert_eq!(v["state"], "running");
        assert_eq!(v["connected"], true);
        assert_eq!(v["tailnet"], "example.com");
        assert_eq!(v["self"]["name"], "laptop");
        assert_eq!(v["self"]["dns"], "laptop.tail1234.ts.net");
        assert_eq!(v["self"]["ip"], "100.64.0.1");
        assert_eq!(v["exit_node"]["name"], "vps");
        assert_eq!(v["exit_node"]["target"], "vps.tail1234.ts.net");
        // Only exit-node options, online first; a peer without a DNS name is reached by IP.
        let exits = v["exit_nodes"].as_array().unwrap();
        assert_eq!(exits.len(), 2);
        assert_eq!(exits[0]["name"], "vps");
        assert_eq!(exits[0]["active"], true);
        assert_eq!(exits[1]["name"], "home");
        assert_eq!(exits[1]["target"], "100.64.0.3");
        assert_eq!(exits[1]["online"], false);
        assert_eq!(exits[1]["active"], false);
    }

    #[test]
    fn status_json_stopped_and_needs_login() {
        let v = status_json(&parse_status(br#"{"BackendState":"Stopped"}"#, b""));
        assert_eq!(v["state"], "stopped");
        assert_eq!(v["connected"], false);
        assert!(v["exit_node"].is_null());
        assert!(v["self"].is_null());
        assert_eq!(v["exit_nodes"].as_array().unwrap().len(), 0);

        let v = status_json(&parse_status(br#"{"BackendState":"NeedsLogin","AuthURL":"https://login.tailscale.com/a/x"}"#, b""));
        assert_eq!(v["state"], "needs_login");
        assert_eq!(v["message"], "https://login.tailscale.com/a/x");
    }

    #[test]
    fn status_json_unavailable_keeps_stderr() {
        let v = status_json(&parse_status(b"", b"failed to connect to local tailscaled\n"));
        assert_eq!(v["state"], "unavailable");
        assert_eq!(v["connected"], false);
        assert_eq!(v["message"], "failed to connect to local tailscaled");
    }

    #[test]
    fn cli_parsing() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(cli_command(&a(&[])), Ok(Cli::Tray { force: false }));
        assert_eq!(cli_command(&a(&["--tray"])), Ok(Cli::Tray { force: true }));
        assert_eq!(cli_command(&a(&["open"])), Ok(Cli::Tray { force: true }));
        assert_eq!(cli_command(&a(&["status", "--json"])), Ok(Cli::Run(cli::Cmd::Status { full: false })));
        assert!(cli_command(&a(&["status"])).is_err());
        assert!(cli_command(&a(&["status", "--text"])).is_err());
        // Unknown arguments start the tray, as before.
        assert_eq!(cli_command(&a(&["--whatever"])), Ok(Cli::Tray { force: false }));
    }

    #[test]
    fn autostart_entry_is_off_under_uq() {
        let e = autostart_entry("/opt/My \"Apps\"/scale_tail");
        assert!(e.lines().any(|l| l == "NotShowIn=uQ;"));
        assert!(e.contains(r#"Exec="/opt/My \"Apps\"/scale_tail""#));
        assert_eq!(autostart_with_not_show_in(&e), None);
    }

    #[test]
    fn old_autostart_entry_gains_not_show_in() {
        let old = "[Desktop Entry]\nType=Application\nName=scale_tail\nExec=\"/usr/bin/scale_tail\"\n";
        let new = autostart_with_not_show_in(old).unwrap();
        assert_eq!(new, "[Desktop Entry]\nNotShowIn=uQ;\nType=Application\nName=scale_tail\nExec=\"/usr/bin/scale_tail\"\n");
        assert_eq!(autostart_with_not_show_in(&new), None);
        assert_eq!(autostart_with_not_show_in("[Desktop Entry]\nOnlyShowIn=KDE;\n"), None);
    }
}
