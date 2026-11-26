//! The app's own settings (autostart, autoconnect, the update check) and the release check.

use super::*;

pub(crate) const REPO: &str = "asafelobotomy/scale_tail";

#[derive(Clone, Debug)]
pub(crate) struct UpdateInfo {
    pub(crate) version: String,
    pub(crate) url: String,
}

#[derive(Deserialize)]
pub(crate) struct GhRelease {
    pub(crate) tag_name: String,
    pub(crate) html_url: String,
}

pub(crate) fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().trim_start_matches('v').split(['.', '-', '+']).map(|x| x.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// Queries GitHub for the latest tagged release; Some if it is newer than this build.
pub(crate) async fn check_update() -> Result<Option<UpdateInfo>, String> {
    tokio::task::spawn_blocking(|| {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .into();
        let mut resp = agent
            .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
            .header("User-Agent", concat!("scale_tail/", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| format!("update check failed: {e}"))?;
        let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
        let rel: GhRelease = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let latest = parse_version(&rel.tag_name).ok_or("unrecognised release tag")?;
        let current = parse_version(env!("CARGO_PKG_VERSION")).ok_or("bad current version")?;
        Ok((latest > current).then(|| UpdateInfo { version: rel.tag_name.trim_start_matches('v').to_string(), url: rel.html_url }))
    })
    .await
    .map_err(|e| e.to_string())?
}

pub(crate) fn update_check_file() -> std::path::PathBuf {
    config_dir().join("scale_tail/no-update-check")
}

pub(crate) fn set_update_check(on: bool) -> Result<(), String> {
    let f = update_check_file();
    if on {
        std::fs::remove_file(&f).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).map_err(|e| e.to_string())
    } else {
        std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&f, "1").map_err(|e| e.to_string())
    }
}

pub(crate) fn config_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| ".".into())
}

pub(crate) fn autostart_file() -> std::path::PathBuf {
    config_dir().join("autostart/scale_tail.desktop")
}

pub(crate) fn autoconnect_file() -> std::path::PathBuf {
    config_dir().join("scale_tail/autoconnect")
}

pub(crate) fn load_settings() -> AppSettings {
    AppSettings { autoconnect: autoconnect_file().exists(), autostart: autostart_file().exists(), update_check: !update_check_file().exists() }
}

pub(crate) fn set_autoconnect(on: bool) -> Result<(), String> {
    let f = autoconnect_file();
    if on {
        std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&f, "1").map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(&f).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).map_err(|e| e.to_string())
    }
}

pub(crate) fn set_autostart(on: bool) -> Result<(), String> {
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
    std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&f, autostart_entry(&exe.to_string_lossy())).map_err(|e| e.to_string())
}

/// The autostart entry. `NotShowIn=uQ;`: under the uQ desktop, Tailscale lives in
/// quick settings and the Settings app instead (uq/), so the tray is not started.
pub(crate) fn autostart_entry(exe: &str) -> String {
    let exe = exe.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "[Desktop Entry]\nType=Application\nName=scale_tail\nComment=System tray control centre for Tailscale\nExec=\"{exe}\"\nIcon=scale_tail\nTerminal=false\nNotShowIn=uQ;\nX-GNOME-Autostart-enabled=true\n"
    )
}

/// An autostart entry written before `NotShowIn=uQ;` existed, with that line added;
/// `None` when the entry already says where it shows.
pub(crate) fn autostart_with_not_show_in(entry: &str) -> Option<String> {
    if entry.lines().any(|l| l.starts_with("NotShowIn=") || l.starts_with("OnlyShowIn=")) {
        return None;
    }
    let mut out = String::with_capacity(entry.len() + 16);
    for line in entry.lines() {
        out.push_str(line);
        out.push('\n');
        if line.trim() == "[Desktop Entry]" {
            out.push_str("NotShowIn=uQ;\n");
        }
    }
    Some(out)
}

/// Adds `NotShowIn=uQ;` to an existing autostart entry from an older version.
pub(crate) fn migrate_autostart() {
    let f = autostart_file();
    if let Some(new) = std::fs::read_to_string(&f).ok().and_then(|t| autostart_with_not_show_in(&t)) {
        let _ = std::fs::write(&f, new);
    }
}
