//! Tests for the command line.

use super::*;

fn a(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parses_commands() {
    assert_eq!(parse(&a(&[])), Ok(None));
    assert_eq!(parse(&a(&["--tray"])), Ok(None));
    assert_eq!(parse(&a(&["status", "--json"])), Ok(Some(Cmd::Status { full: false })));
    assert_eq!(parse(&a(&["status", "--json", "--full"])), Ok(Some(Cmd::Status { full: true })));
    assert_eq!(parse(&a(&["config", "show", "--json"])), Ok(Some(Cmd::ConfigShow)));
    assert_eq!(
        parse(&a(&["config", "set", "ssh", "true", "--dry-run", "--json"])),
        Ok(Some(Cmd::ConfigSet { key: Key::Ssh, value: true, dry_run: true }))
    );
    assert_eq!(parse(&a(&["exit-node", "none", "--json"])), Ok(Some(Cmd::ExitNode { choice: ExitChoice::None, dry_run: false })));
    assert_eq!(
        parse(&a(&["exit-node", "vps.tail1.ts.net", "--json"])),
        Ok(Some(Cmd::ExitNode { choice: ExitChoice::Node("vps.tail1.ts.net".into()), dry_run: false }))
    );
    assert_eq!(
        parse(&a(&["ping", "100.64.0.2", "--json", "--stream", "--count", "5"])),
        Ok(Some(Cmd::Ping { target: "100.64.0.2".into(), count: 5, stream: true }))
    );
    assert_eq!(parse(&a(&["update", "check", "--json"])), Ok(Some(Cmd::UpdateCheck)));
}

#[test]
fn refuses_what_it_does_not_know() {
    for bad in [
        &["status"][..],
        &["config", "set", "nope", "true", "--json"],
        &["config", "set", "ssh", "maybe", "--json"],
        &["config", "show", "--json", "--dry-run"],
        &["exit-node", "-x", "--json"],
        &["exit-node", "a b", "--json"],
        &["exit-node", "--json"],
        &["switch", "a/b", "--json"],
        &["ping", "x", "--json", "--count", "11"],
        &["ping", "x", "--json", "--count", "0"],
        &["accounts", "--json", "--stream"],
        &["status", "--json", "--bogus"],
        &["logout", "now", "--json"],
    ] {
        assert!(parse(&a(bad)).is_err(), "{bad:?} should be refused");
    }
}

#[test]
fn targets_match_the_page_pattern() {
    assert!(valid_target("vps.tail1234.ts.net"));
    assert!(valid_target("fd7a::1"));
    assert!(!valid_target("-rf"));
    assert!(!valid_target(""));
    assert!(!valid_target("a b"));
    assert!(!valid_target(&"a".repeat(254)));
}

#[test]
fn every_setting_has_words_both_ways() {
    for k in Key::ALL {
        assert_eq!(Key::parse(k.name()), Some(k));
        assert!(!describe(k, true).0.is_empty() && !describe(k, false).0.is_empty());
        assert_eq!(k.flag().is_some(), !k.is_app());
    }
}

#[test]
fn reads_a_pong() {
    assert_eq!(parse_pong("pong from vps (100.64.0.2) via DERP(nyc) in 23ms\n"), Some(("DERP(nyc)".into(), 23.0)));
    assert_eq!(parse_pong("pong from vps (100.64.0.2) via 1.2.3.4:41641 in 4ms"), Some(("1.2.3.4:41641".into(), 4.0)));
    assert_eq!(parse_pong("no reply"), None);
}

#[test]
fn summarises_replies() {
    let r = |ms: Option<f64>| Reply { ms, via: "DERP(nyc)".into(), error: "No reply".into() };
    let d = done_doc(3, &[r(Some(10.0)), r(None), r(Some(20.0))]);
    assert_eq!(d["received"], 2);
    assert_eq!(d["loss_percent"], 33);
    assert_eq!(d["avg_ms"], 15.0);
    assert_eq!(d["ok"], true);
    let d = done_doc(2, &[r(None), r(None)]);
    assert_eq!(d["ok"], false);
    assert_eq!(d["error"], "No reply");
    assert!(d["avg_ms"].is_null());
}
