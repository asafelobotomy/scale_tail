//! The command line behind the uQ page, run against tests/fake/tailscale.
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

struct Run {
    code: i32,
    out: String,
    log: String,
}

impl Run {
    fn json(&self) -> Value {
        serde_json::from_str(self.out.trim()).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.out))
    }
    fn lines(&self) -> Vec<Value> {
        self.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
}

fn scale_tail(state: &str, args: &[&str]) -> Run {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-{}-{}", std::process::id(), args.join("_").replace('/', "")));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fake = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake");
    let log = dir.join("calls.log");
    let o = Command::new(env!("CARGO_BIN_EXE_scale_tail"))
        .args(args)
        .env("PATH", format!("{}:{}", fake.display(), std::env::var("PATH").unwrap()))
        .env("FAKE_STATE", state)
        .env("FAKE_LOG", &log)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env_remove("APPIMAGE")
        .output()
        .unwrap();
    Run { code: o.status.code().unwrap(), out: String::from_utf8_lossy(&o.stdout).into_owned(), log: std::fs::read_to_string(&log).unwrap_or_default() }
}

#[test]
fn status_full_has_everything_the_page_reads() {
    let r = scale_tail("running", &["status", "--json", "--full"]);
    assert_eq!(r.code, 0);
    let v = r.json();
    assert_eq!(v["version"], 1);
    assert_eq!(v["account"], "me@example.com");
    assert_eq!(v["peers"].as_array().unwrap().len(), 3);
    assert_eq!(v["peers"][0]["name"], "phone"); // online first, then by name
    assert_eq!(v["peers"][1]["ip"], "100.64.0.2");
    assert_eq!(v["prefs"]["accept_routes"], true);
    assert_eq!(v["prefs"]["advertise_exit_node"], true);
    assert_eq!(v["prefs"]["ssh"], false);
    assert_eq!(v["accounts"][0]["active"], true);
    assert_eq!(v["accounts"][1]["tailnet"], "work.example");
    assert_eq!(v["app"]["settings"]["update_check"], true);
    assert!(v["app"]["version"].is_string());
}

#[test]
fn plain_status_stays_small_and_asks_tailscale_once() {
    let r = scale_tail("running", &["status", "--json"]);
    assert!(r.json().get("prefs").is_none());
    assert_eq!(r.log.lines().count(), 1);
}

#[test]
fn status_full_without_a_daemon() {
    let v = scale_tail("down", &["status", "--json", "--full"]).json();
    assert_eq!(v["state"], "unavailable");
    assert!(v["prefs"].is_null());
    assert_eq!(v["accounts"].as_array().unwrap().len(), 0);
}

#[test]
fn config_show_lists_both_kinds() {
    let v = scale_tail("running", &["config", "show", "--json"]).json();
    assert_eq!(v["app"]["autostart"], false);
    assert_eq!(v["tailscale"]["accept_dns"], true);
    assert_eq!(v["error"], "");
    let v = scale_tail("down", &["config", "show", "--json"]).json();
    assert!(v["tailscale"].is_null());
    assert!(!v["error"].as_str().unwrap().is_empty());
}

#[test]
fn config_set_writes_tailscale_and_app_settings() {
    let r = scale_tail("running", &["config", "set", "ssh", "true", "--json"]);
    assert_eq!((r.code, r.json()["ok"].clone()), (0, Value::Bool(true)));
    assert!(r.log.lines().any(|l| l == "set --ssh=true"), "{}", r.log);
    let r = scale_tail("running", &["config", "set", "auto_connect", "true", "--json"]);
    assert_eq!(r.json()["changed"], true);
    assert!(r.log.lines().all(|l| !l.starts_with("set")), "an app setting must not call tailscale set");
    let r = scale_tail("down", &["config", "set", "ssh", "true", "--json"]);
    assert_eq!((r.code, r.json()["ok"].clone()), (1, Value::Bool(false)));
}

#[test]
fn dry_runs_change_nothing_and_print_one_plan() {
    for args in [
        &["config", "set", "ssh", "true", "--dry-run", "--json"][..],
        &["exit-node", "vps.tail1.ts.net", "--dry-run", "--json"],
        &["exit-node", "suggested", "--dry-run", "--json"],
        &["switch", "cd2", "--dry-run", "--json"],
        &["logout", "--dry-run", "--json"],
    ] {
        let r = scale_tail("running", args);
        assert_eq!(r.code, 0, "{args:?}");
        let v = r.json();
        assert_eq!(v["would_refuse"], false, "{args:?}: {v}");
        assert!(!v["changes"].as_array().unwrap().is_empty(), "{args:?}");
        for c in v["changes"].as_array().unwrap() {
            assert!(c["label"].as_str().is_some_and(|l| !l.is_empty()));
            assert!(c["reversible"].is_boolean());
        }
        assert!(r.log.lines().all(|l| !matches!(l.split(' ').next(), Some("set" | "up" | "down" | "logout") | None) && l != "switch cd2"), "{args:?} changed something: {}", r.log);
    }
}

#[test]
fn dry_runs_say_when_they_would_refuse() {
    let why = |state: &str, args: &[&str]| {
        let v = scale_tail(state, args).json();
        assert_eq!(v["would_refuse"], true, "{args:?}: {v}");
        v["reason"].as_str().unwrap().to_string()
    };
    assert!(why("running", &["exit-node", "nas.tail1.ts.net", "--dry-run", "--json"]).contains("offline"));
    assert!(why("running", &["exit-node", "phone.tail1.ts.net", "--dry-run", "--json"]).contains("exit node"));
    assert!(why("running", &["switch", "ab1", "--dry-run", "--json"]).contains("already"));
    assert!(why("running", &["switch", "zz9", "--dry-run", "--json"]).contains("isn't signed in"));
    assert!(why("needs_login", &["logout", "--dry-run", "--json"]).contains("isn't signed in"));
    assert!(why("down", &["config", "set", "shields_up", "true", "--dry-run", "--json"]).contains("isn't running"));
}

#[test]
fn a_setting_that_is_already_right_plans_no_change() {
    let v = scale_tail("running", &["config", "set", "ssh", "false", "--dry-run", "--json"]).json();
    assert_eq!(v["changes"].as_array().unwrap().len(), 0);
    assert_eq!(v["would_refuse"], false);
}

#[test]
fn logout_plan_names_the_exit_node_it_stops() {
    let v = scale_tail("running", &["logout", "--dry-run", "--json"]).json();
    assert_eq!(v["changes"].as_array().unwrap().len(), 1, "no exit node is active in the fixture");
    assert_eq!(v["changes"][0]["group"], "Account");
}

#[test]
fn exit_node_commands_run_tailscale_set() {
    assert!(scale_tail("running", &["exit-node", "vps.tail1.ts.net", "--json"]).log.contains("set --exit-node=vps.tail1.ts.net"));
    assert!(scale_tail("running", &["exit-node", "none", "--json"]).log.contains("set --exit-node=\n") || scale_tail("running", &["exit-node", "none", "--json"]).log.lines().any(|l| l == "set --exit-node="));
    assert!(scale_tail("running", &["exit-node", "suggested", "--json"]).log.contains("set --exit-node=auto:any"));
}

#[test]
fn connect_is_never_run_while_a_login_is_needed() {
    let r = scale_tail("needs_login", &["connect", "--json"]);
    assert_eq!(r.code, 1);
    assert!(r.log.lines().all(|l| l != "up"), "tailscale up would wait for a browser");
    let r = scale_tail("stopped", &["connect", "--json"]);
    assert!(r.log.lines().any(|l| l == "up"));
    assert_eq!(r.json()["connected"], true);
    assert_eq!(scale_tail("running", &["connect", "--json"]).json()["changed"], false);
}

#[test]
fn ping_streams_progress_then_a_summary() {
    let r = scale_tail("running", &["ping", "vps.tail1.ts.net", "--json", "--stream", "--count", "3"]);
    let lines = r.lines();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0]["event"], "reply");
    assert_eq!(lines[0]["percent"], 33);
    assert_eq!(lines[1]["percent"], 66);
    assert_eq!(lines[2]["percent"], 100);
    assert_eq!(lines[3]["event"], "done");
    assert_eq!(lines[3]["received"], 3);
    assert_eq!(lines[3]["via"], "DERP(nyc)");
    // Without --stream: only the summary.
    let v = scale_tail("running", &["ping", "vps.tail1.ts.net", "--json", "--count", "2"]).json();
    assert_eq!(v["event"], "done");
    assert_eq!(v["sent"], 2);
}

#[test]
fn bad_arguments_exit_2_before_touching_tailscale() {
    let r = scale_tail("running", &["config", "set", "ssh", "maybe", "--json"]);
    assert_eq!(r.code, 2);
    assert_eq!(r.log, "");
}

#[test]
fn accounts_lists_with_the_active_one_marked() {
    let v = scale_tail("running", &["accounts", "--json"]).json();
    assert_eq!(v["accounts"][0]["id"], "ab1");
    assert_eq!(v["accounts"][0]["account"], "me@example.com");
    assert_eq!(v["accounts"][1]["active"], false);
}
