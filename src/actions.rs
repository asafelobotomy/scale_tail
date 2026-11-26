//! What the tray's menu asks for, and doing it.

use super::*;

#[derive(Debug)]
pub(crate) enum Action {
    Connect,
    Disconnect,
    SetExitNode(Option<String>),
    UseSuggestedExitNode,
    SetPref(&'static str, bool), // `tailscale set --<flag>=<bool>`
    SetAutostart(bool),
    SetAutoconnect(bool),
    SetUpdateCheck(bool),
    CheckUpdates(bool), // true = user-initiated (always report the result)
    OpenUrl(String),
    SwitchAccount(String),
    Ping(String),
    SendFile(String),
    Login(String),
    Copy(String),
    Refresh,
    Quit,
}

pub(crate) async fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "Tailscale", "-i", "network-vpn", title, body])
        .status()
        .await;
}

pub(crate) async fn copy_to_clipboard(text: &str) {
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

pub(crate) async fn perform(action: Action) {
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
        Action::SetUpdateCheck(on) => set_update_check(on),
        Action::OpenUrl(u) => {
            let _ = Command::new("xdg-open").arg(u).spawn();
            Ok(())
        }
        Action::CheckUpdates(_) => Ok(()), // handled in main loop (needs the tray handle)
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

