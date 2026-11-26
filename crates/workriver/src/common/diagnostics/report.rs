// `workriver perf`: the timing log summed up, and the bundle the user can send back.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{PageApiTiming, dir, files};

/// The log's lines since `since` (a duration like `1h`, `30m`, `2d`; none: all).
fn lines(home: &Path, since: Option<std::time::Duration>) -> Vec<Value> {
    let from = since.map(|d| std::time::SystemTime::now() - d);
    files(home)
        .into_iter()
        .flat_map(|f| std::fs::read_to_string(f).unwrap_or_default().lines().map(str::to_string).collect::<Vec<_>>())
        .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
        .filter(|v| match (from, v["timestamp"].as_str().and_then(|t| humantime::parse_rfc3339_weak(t.trim_end_matches('Z').split('.').next().unwrap_or(t)).ok())) {
            (Some(from), Some(at)) => at >= from,
            _ => true,
        })
        .collect()
}

fn pct(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// `workriver perf`: where the time went.
pub fn summary(home: &Path, since: Option<std::time::Duration>, top: usize) -> String {
    let all = lines(home, since);
    let mut out = String::new();
    let span = match (all.first(), all.last()) {
        (Some(a), Some(b)) => format!("{} to {}", a["timestamp"].as_str().unwrap_or("?"), b["timestamp"].as_str().unwrap_or("?")),
        _ => "nothing logged".into(),
    };
    let _ = writeln!(out, "workriver timing log: {} lines, {span}", all.len());
    let of = |kind: &'static str| all.iter().filter(move |v| v["kind"] == kind);
    let ms = |v: &Value| v["ms"].as_u64().unwrap_or(0);
    // Requests by route.
    let mut routes: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for v in of("http") {
        routes.entry(format!("{} {}", v["method"].as_str().unwrap_or("?"), v["route"].as_str().unwrap_or("?"))).or_default().push(ms(v));
    }
    let mut rows: Vec<(String, Vec<u64>)> = routes.into_iter().map(|(r, mut t)| {
        t.sort();
        (r, t)
    }).collect();
    rows.sort_by_key(|(_, t)| std::cmp::Reverse(*t.last().unwrap_or(&0)));
    let _ = writeln!(out, "\nRequests (slowest first): count, p50, p95, max ms");
    for (r, t) in rows.iter().take(top) {
        let _ = writeln!(out, "  {r:<40} {:>6} {:>6} {:>6} {:>6}", t.len(), pct(t, 0.5), pct(t, 0.95), t.last().unwrap_or(&0));
    }
    let mut page_loads: Vec<_> = of("page_load").map(|v| {
        let api: Vec<PageApiTiming> = v["api"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        (v, api)
    }).collect();
    if !page_loads.is_empty() {
        let _ = writeln!(out, "\nPage loads: count, p50, p95, max");
        let sections: &[(&str, &[(&str, &str)])] = &[
            ("Time from navigation start (ms)", &[
                ("ms", "Board visible"),
                ("js_ms", "Entry JS downloaded"),
                ("css_ms", "Entry CSS downloaded"),
                ("request_ms", "Workspace request started"),
                ("response_ms", "Workspace response received"),
                ("ready_ms", "Workspace data ready"),
            ]),
            ("Durations (ms)", &[
                ("parse_ms", "Parse and prepare workspace"),
                ("app_ms", "Build and commit board"),
                ("render_ms", "Paint after commit"),
            ]),
            ("Response size (bytes)", &[("bytes", "Workspace response")]),
        ];
        for (section, fields) in sections {
            let _ = writeln!(out, "  {section}");
            for &(field, label) in *fields {
                let mut t: Vec<_> = page_loads.iter().filter_map(|(v, _)| v[field].as_u64()).collect();
                t.sort_unstable();
                let _ = writeln!(out, "    {label:<38} {:>6} {:>6} {:>6} {:>6}", t.len(), pct(&t, 0.5), pct(&t, 0.95), t.last().unwrap_or(&0));
            }
        }
        page_loads.sort_by_key(|(v, _)| std::cmp::Reverse(ms(v)));
        let _ = writeln!(out, "\nSlowest page loads:");
        for (v, api) in page_loads.iter().take(top) {
            let at = v["timestamp"].as_str().unwrap_or("?");
            let build = v["build"].as_str().map(str::to_owned)
                .or_else(|| v["build"].as_u64().map(|n| format!("{n:012x}"))).unwrap_or_else(|| "?".into());
            let pending = if v["git_pending"] == 1 { "  records-only" } else { "" };
            let count = api.iter().map(|a| a.count).sum::<u64>();
            let total = api.iter().map(|a| a.ms).sum::<u64>();
            let longest = api.iter().map(|a| a.max_ms).max().unwrap_or(0);
            let _ = writeln!(out, "  {at}  {} ms  build {build}{pending}  {count} other API requests, {total} ms total, {longest} ms max", ms(v));
            let time = |n: Option<u64>| n.map_or_else(|| "?".into(), |ms| format!("{ms} ms"));
            let response_ms = v["response_ms"].as_u64().zip(v["request_ms"].as_u64()).and_then(|(end, start)| end.checked_sub(start));
            let (ready, parse, app, paint) = (time(v["ready_ms"].as_u64()), time(v["parse_ms"].as_u64()), time(v["app_ms"].as_u64()), time(v["render_ms"].as_u64()));
            let _ = writeln!(out, "    workspace response {}, data ready at {ready}, parse {parse}, app {app}, paint {paint}", time(response_ms));
        }
        let mut paths = BTreeMap::<&str, (u64, u64, u64)>::new();
        for a in page_loads.iter().flat_map(|(_, api)| api) {
            let (count, total, longest) = paths.entry(&a.path).or_default();
            *count += a.count;
            *total += a.ms;
            *longest = (*longest).max(a.max_ms);
        }
        if !paths.is_empty() {
            let mut rows: Vec<_> = paths.into_iter().collect();
            rows.sort_by_key(|(_, (_, total, _))| std::cmp::Reverse(*total));
            let _ = writeln!(out, "\nOther API requests before board paint: count, total ms, max ms");
            for (path, (count, total, longest)) in rows.into_iter().take(top) {
                let _ = writeln!(out, "  {path:<40} {count:>6} {total:>6} {longest:>6}");
            }
        }
    }
    // Snapshots.
    for (kind, title) in [("snapshot", "Snapshots (what a page or board reads)"), ("remote_read", "Background reads of other machines")] {
        let mut t: Vec<u64> = of(kind).map(ms).collect();
        t.sort();
        let sum = |f: &str| of(kind).map(|v| v[f].as_u64().unwrap_or(0)).sum::<u64>();
        let _ = writeln!(out, "\n{title}: {} times, p50 {} ms, p95 {} ms, max {} ms; units worked out again {}, reused {}; git calls {}", t.len(), pct(&t, 0.5), pct(&t, 0.95), t.last().unwrap_or(&0), sum("recomputed"), sum("reused"), sum("git"));
    }
    // Units.
    let mut units: BTreeMap<String, Vec<(u64, u64)>> = BTreeMap::new();
    for v in of("unit") {
        units.entry(format!("{} ({})", v["unit"].as_str().unwrap_or("?"), v["machine"].as_str().unwrap_or("?"))).or_default().push((ms(v), v["git"].as_u64().unwrap_or(0)));
    }
    let mut rows: Vec<(String, u64, u64, usize)> = units.into_iter().map(|(u, t)| (u, t.iter().map(|x| x.0).max().unwrap_or(0), t.iter().map(|x| x.1).sum(), t.len())).collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    let _ = writeln!(out, "\nUnits read (slowest first): reads, max ms, git calls in all");
    for (u, max, git, n) in rows.iter().take(top) {
        let _ = writeln!(out, "  {u:<30} {n:>6} {max:>6} {git:>6}");
    }
    // Machines.
    let mut polls: BTreeMap<String, (usize, usize, u64)> = BTreeMap::new();
    for v in of("poll") {
        let e = polls.entry(v["machine"].as_str().unwrap_or("?").to_string()).or_default();
        e.0 += 1;
        e.1 += (v["ok"] != true) as usize;
        e.2 = e.2.max(ms(v));
    }
    if !polls.is_empty() {
        let _ = writeln!(out, "\nAgent reads per machine: reads, failed, max ms");
        for (m, (n, failed, max)) in &polls {
            let _ = writeln!(out, "  {m:<20} {n:>6} {failed:>6} {max:>6}");
        }
    }
    let mut pauses = of("service_pause").peekable();
    if pauses.peek().is_some() {
        let _ = writeln!(out, "\nService pauses (gaps between checks):");
        for v in pauses {
            let (from, to) = (v["from"].as_str().unwrap_or("?"), v["to"].as_str().unwrap_or("?"));
            let _ = writeln!(out, "  {from} to {to}  {:.1} s", ms(v) as f64 / 1000.0);
        }
    }
    let mut down: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for v in of("tunnel").filter(|v| v["event"] == "down") {
        let e = down.entry(v["machine"].as_str().unwrap_or("?").to_string()).or_default();
        e.0 += 1;
    }
    for v in of("tunnel").filter(|v| v["event"] == "up") {
        let e = down.entry(v["machine"].as_str().unwrap_or("?").to_string()).or_default();
        e.1 += v["down_ms"].as_u64().unwrap_or(0);
    }
    if !down.is_empty() {
        let _ = writeln!(out, "\nConnections to other machines: times down, time down in all");
        for (m, (n, ms)) in &down {
            let _ = writeln!(out, "  {m:<20} {n:>6} {:>9.1} s", *ms as f64 / 1000.0);
        }
    }
    // Calls (detail level only).
    let mut calls: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for v in of("git").chain(of("ssh")) {
        let what = match v["kind"].as_str() {
            Some("git") => format!("git {} ({})", v["args"].as_str().unwrap_or("").split_whitespace().find(|a| !a.starts_with('-') && *a != "…").unwrap_or("?"), v["via"].as_str().unwrap_or("?")),
            _ => format!("ssh {}", v["purpose"].as_str().unwrap_or("?")),
        };
        let e = calls.entry(what).or_default();
        e.0 += 1;
        e.1 += ms(v);
    }
    if calls.is_empty() {
        let _ = writeln!(out, "\n(each git and ssh call is logged with `workriver serve --perf-detail`)");
    } else {
        let mut rows: Vec<_> = calls.into_iter().collect();
        rows.sort_by_key(|(_, (_, t))| std::cmp::Reverse(*t));
        let _ = writeln!(out, "\ngit and ssh calls: count, ms in all");
        for (w, (n, t)) in rows.iter().take(top) {
            let _ = writeln!(out, "  {w:<40} {n:>6} {t:>8}");
        }
    }
    let slow = all.iter().filter(|v| v["slow"] == true).count();
    let _ = writeln!(out, "\nSlow (over 500 ms): {slow}");
    out
}

/// `workriver perf --bundle`: the summary and the newest raw lines (at most `bytes`), in one
/// file that can be sent; its path.
pub fn bundle(home: &Path, bytes: usize) -> std::io::Result<PathBuf> {
    let mut raw: Vec<String> = files(home).into_iter().flat_map(|f| std::fs::read_to_string(f).unwrap_or_default().lines().map(str::to_string).collect::<Vec<_>>()).collect();
    let mut kept = 0;
    let mut start = raw.len();
    while start > 0 && kept + raw[start - 1].len() < bytes {
        start -= 1;
        kept += raw[start].len() + 1;
    }
    raw.drain(..start);
    let path = dir(home).join(crate::common::home::perf_bundle(&humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string().replace(':', "")));
    let mut text = format!(
        "workriver {} timing bundle\nTimes, sizes, counts, service builds, route templates, unit ids and machine names only: no commit messages, file texts, task texts, findings, notes, prompts, tokens, cookies or paths.\n\n",
        crate::common::BUILD
    );
    text += &summary(home, None, 30);
    text += "\n---- raw lines, newest last ----\n";
    text += &raw.join("\n");
    text.push('\n');
    std::fs::create_dir_all(dir(home))?;
    std::fs::write(&path, text)?;
    Ok(path)
}

/// `1h`, `30m`, `2d`, `45s`.
pub fn parse_since(s: &str) -> anyhow::Result<std::time::Duration> {
    humantime::parse_duration(s).map_err(|e| anyhow::anyhow!("--since {s}: {e} (for example 30m, 1h, 2d)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_loads_report_each_milestone_and_the_slowest_loads() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir(home.path())).unwrap();
        std::fs::write(dir(home.path()).join(crate::common::home::PERF_LOG), concat!(
            r#"{"kind":"page_load","timestamp":"2026-01-01T01:00:00Z","ms":100,"js_ms":12,"css_ms":11,"request_ms":14,"response_ms":34,"bytes":1000,"ready_ms":40,"parse_ms":6,"app_ms":40,"render_ms":20,"git_pending":0,"build":1,"api":"[{\"path\":\"/api/task\",\"count\":1,\"ms\":30,\"max_ms\":30}]"}"#, "\n",
            r#"{"kind":"page_load","timestamp":"2026-01-01T01:01:00Z","ms":900,"js_ms":120,"css_ms":70,"request_ms":160,"response_ms":430,"bytes":3000,"ready_ms":500,"parse_ms":70,"app_ms":290,"render_ms":110,"git_pending":1,"build":"0.1.0 (abc123456)","api":"[{\"path\":\"/api/task\",\"count\":2,\"ms\":225,\"max_ms\":150},{\"path\":\"/api/session\",\"count\":1,\"ms\":15,\"max_ms\":15}]"}"#, "\n",
            r#"{"kind":"page_load","timestamp":"2026-01-01T01:02:00Z","ms":200,"ready_ms":150,"app_ms":45,"render_ms":5,"git_pending":0,"build":3,"api":"[]"}"#, "\n",
            r#"{"kind":"http","method":"GET","route":"/api/workspace","ms":999}"#, "\n",
        )).unwrap();
        let report = summary(home.path(), None, 2);
        let points = report.split_once("Time from navigation start (ms)").unwrap().1;
        let (points, durations) = points.split_once("Durations (ms)").unwrap();
        let (durations, size) = durations.split_once("Response size (bytes)").unwrap();
        for (section, label, counts) in [
            (points, "Board visible", ["3", "200", "900", "900"]),
            (points, "Entry JS downloaded", ["2", "120", "120", "120"]),
            (points, "Entry CSS downloaded", ["2", "70", "70", "70"]),
            (points, "Workspace request started", ["2", "160", "160", "160"]),
            (points, "Workspace response received", ["2", "430", "430", "430"]),
            (points, "Workspace data ready", ["3", "150", "500", "500"]),
            (durations, "Parse and prepare workspace", ["2", "70", "70", "70"]),
            (durations, "Build and commit board", ["3", "45", "290", "290"]),
            (durations, "Paint after commit", ["3", "20", "110", "110"]),
            (size, "Workspace response", ["2", "3000", "3000", "3000"]),
        ] {
            let row = section.lines().find_map(|l| l.trim_start().strip_prefix(label));
            assert_eq!(row.map(|l| l.split_whitespace().collect::<Vec<_>>()), Some(counts.to_vec()), "{label}: {report}");
        }
        let slow = report.split_once("Slowest page loads:").unwrap().1.split("\n\n").next().unwrap();
        assert!(slow.contains("2026-01-01T01:01:00Z  900 ms  build 0.1.0 (abc123456)  records-only"), "{slow}");
        assert!(slow.contains("workspace response 270 ms, data ready at 500 ms, parse 70 ms, app 290 ms, paint 110 ms"), "{slow}");
        assert!(slow.contains("3 other API requests, 240 ms total, 150 ms max"), "{slow}");
        assert!(slow.contains("2026-01-01T01:02:00Z  200 ms  build 000000000003"), "{slow}");
        assert!(slow.contains("workspace response ?, data ready at 150 ms, parse ?, app 45 ms, paint 5 ms"), "{slow}");
        assert!(!slow.contains("2026-01-01T01:00:00Z"), "{slow}");
        assert!(report.contains("GET /api/workspace"), "{report}");
        let api = report.split_once("Other API requests before board paint: count, total ms, max ms").unwrap().1;
        let task = api.lines().find_map(|l| l.trim_start().strip_prefix("/api/task")).unwrap();
        assert_eq!(task.split_whitespace().collect::<Vec<_>>(), ["3", "255", "150"], "{api}");
    }
}
