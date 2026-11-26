//! `ping` (one document, or `--stream` progress lines) and `update-check`.

use super::*;

// ---------- ping ----------

/// One `tailscale ping -c 1` line: ("DERP(nyc)", 23.0) from
/// `pong from laptop (100.64.0.1) via DERP(nyc) in 23ms`.
pub(crate) fn parse_pong(out: &str) -> Option<(String, f64)> {
    let line = out.lines().find(|l| l.starts_with("pong from"))?;
    let (_, rest) = line.split_once(" via ")?;
    let (via, ms) = rest.rsplit_once(" in ")?;
    let ms = ms.trim().strip_suffix("ms")?.parse::<f64>().ok()?;
    Some((via.trim().to_string(), ms))
}

pub(crate) struct Reply {
    pub(crate) ms: Option<f64>,
    pub(crate) via: String,
    pub(crate) error: String,
}

pub(crate) async fn ping_one(target: &str) -> Reply {
    match ts(&["ping", "-c", "1", "--timeout", "3s", target]).await {
        Ok(out) => match parse_pong(&out) {
            Some((via, ms)) => Reply { ms: Some(ms), via, error: String::new() },
            None => Reply { ms: None, via: String::new(), error: "No reply".into() },
        },
        Err(e) => Reply { ms: None, via: String::new(), error: e.lines().next().unwrap_or("No reply").to_string() },
    }
}

pub(crate) fn done_doc(sent: u32, replies: &[Reply]) -> Value {
    let times: Vec<f64> = replies.iter().filter_map(|r| r.ms).collect();
    let received = times.len() as u32;
    let min = times.iter().copied().fold(f64::INFINITY, f64::min);
    let max = times.iter().copied().fold(0.0, f64::max);
    let avg = if times.is_empty() { 0.0 } else { times.iter().sum::<f64>() / times.len() as f64 };
    let round = |x: f64| (x * 10.0).round() / 10.0;
    json!({
        "event": "done", "ok": received > 0, "sent": sent, "received": received,
        "loss_percent": (sent - received) * 100 / sent.max(1), "percent": 100,
        "min_ms": if times.is_empty() { Value::Null } else { json!(round(min)) },
        "avg_ms": if times.is_empty() { Value::Null } else { json!(round(avg)) },
        "max_ms": if times.is_empty() { Value::Null } else { json!(round(max)) },
        "via": replies.iter().rev().find(|r| r.ms.is_some()).map(|r| r.via.clone()),
        "error": if received == 0 { replies.last().map(|r| r.error.clone()).unwrap_or_default() } else { String::new() },
    })
}

/// `ping --stream`: a line for each reply as it arrives, then the summary.
pub(crate) async fn ping_stream(target: &str, count: u32) -> i32 {
    use std::io::Write;
    let mut replies = vec![];
    for seq in 1..=count {
        let r = ping_one(target).await;
        let line = json!({
            "event": "reply", "seq": seq, "of": count, "percent": seq * 100 / count,
            "ok": r.ms.is_some(), "ms": r.ms, "via": r.via, "error": r.error,
        });
        replies.push(r);
        println!("{line}");
        let _ = std::io::stdout().flush();
    }
    println!("{}", done_doc(count, &replies));
    0
}

pub(crate) async fn ping_once(target: &str, count: u32) -> (Value, i32) {
    let mut replies = vec![];
    for _ in 0..count {
        replies.push(ping_one(target).await);
    }
    (done_doc(count, &replies), 0)
}

// ---------- updates ----------

pub(crate) async fn update_check() -> (Value, i32) {
    match check_update().await {
        Ok(found) => (
            json!({
                "ok": true, "current": env!("CARGO_PKG_VERSION"),
                "available": found.is_some(),
                "latest": found.as_ref().map(|u| u.version.clone()).unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
                "url": found.map(|u| u.url).unwrap_or_default(),
            }),
            0,
        ),
        Err(e) => failure(e),
    }
}

