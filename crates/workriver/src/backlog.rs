//! `workriver backlog list` and `export`: a project's roadmap and deferred findings, read
//! from the daemon's snapshot.

use anyhow::bail;
use serde_json::Value;

use crate::cli::Daemon;

/// One deferred (or scheduled) finding: where it is, and the finding.
struct Deferred<'a> {
    at: String,
    finding: &'a Value,
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

fn external(v: &Value) -> String {
    match v.get("external") {
        Some(e) if e.is_object() => format!(" [{}:{}]", s(&e["system"]), s(&e["key"])),
        _ => String::new(),
    }
}

/// A project as given (p1, or its folder) as its folder.
fn folder(w: &Value, given: &str) -> Option<String> {
    w["projects"].as_array().into_iter().flatten().find(|p| s(&p["id"]) == given || s(&p["key"]) == given).map(|p| s(&p["id"]).to_string())
}

fn roadmap<'a>(w: &'a Value, project: &str) -> Vec<&'a Value> {
    let mut list: Vec<&Value> = w["intents"].as_array().into_iter().flatten().filter(|i| s(&i["project"]) == project && s(&i["state"]) == "planned").collect();
    list.sort_by_key(|i| i["order"].as_u64().unwrap_or(u64::MAX));
    list
}

fn deferred<'a>(w: &'a Value, project: &str) -> Vec<Deferred<'a>> {
    let waiting = |f: &Value| matches!(s(&f["state"]), "deferred" | "scheduled");
    let mut out = vec![];
    for i in w["intents"].as_array().into_iter().flatten().filter(|i| s(&i["project"]) == project) {
        for u in i["units"].as_array().into_iter().flatten() {
            for t in u["tasks"].as_array().into_iter().flatten() {
                for f in t["output"]["findings"].as_array().into_iter().flatten().filter(|f| waiting(f)) {
                    out.push(Deferred { at: format!("{} {}#{} {}", s(&i["id"]), s(&u["id"]), s(&t["id"]), s(&f["id"])), finding: f });
                }
            }
        }
    }
    for b in w["backlog"].as_array().into_iter().flatten().filter(|b| s(&b["project"]) == project && waiting(&b["finding"])) {
        let from = if s(&b["source"]).is_empty() { String::new() } else { format!(" from {}", s(&b["source"])) };
        out.push(Deferred { at: format!("{}{from}", s(&b["finding"]["id"])), finding: &b["finding"] });
    }
    out
}

pub fn list(d: &Daemon, project: Option<&str>) -> anyhow::Result<()> {
    let w = d.get("/api/workspace?light=true")?;
    let wanted = project.map(|p| folder(&w, p).unwrap_or_default());
    let projects: Vec<&Value> = w["projects"].as_array().into_iter().flatten().filter(|p| wanted.as_deref().is_none_or(|x| s(&p["id"]) == x)).collect();
    if projects.is_empty() {
        bail!("no project {}; see `workriver project list`", project.unwrap_or("yet"));
    }
    for p in projects {
        let id = s(&p["id"]);
        println!("{}  {id}{}", s(&p["title"]), p["key"].as_str().map(|k| format!("  ({k})")).unwrap_or_default());
        for i in w["intents"].as_array().into_iter().flatten().filter(|i| s(&i["project"]) == id && s(&i["state"]) == "active") {
            let at = i["workspace"]["label"].as_str().map(|l| format!("  (in {l})")).unwrap_or_default();
            println!("  active  {} {}{at}", s(&i["id"]), s(&i["title"]));
        }
        let road = roadmap(&w, id);
        if !road.is_empty() {
            println!("  roadmap:");
        }
        for (k, i) in road.iter().enumerate() {
            println!("    {}. {} {} — {}{}", k + 1, s(&i["id"]), s(&i["title"]), s(&i["goal"]), external(i));
        }
        let open = deferred(&w, id);
        if !open.is_empty() {
            println!("  deferred findings:");
        }
        for f in open {
            let scheduled = if s(&f.finding["state"]) == "scheduled" { "  (scheduled)" } else { "" };
            println!("    {} {}: {}{}{scheduled}", s(&f.finding["severity"]), f.at, s(&f.finding["problem"]), external(f.finding));
        }
    }
    Ok(())
}

pub fn export(d: &Daemon, project: &str) -> anyhow::Result<()> {
    let w = d.get("/api/workspace?light=true")?;
    let Some(project) = folder(&w, project) else { bail!("no project {project}; see `workriver project list`") };
    let project = project.as_str();
    let p = w["projects"].as_array().into_iter().flatten().find(|p| s(&p["id"]) == project).expect("found above");
    println!("# {} backlog\n", s(&p["title"]));
    println!("Exported from workriver at {}. A read-only copy: change the backlog with `workriver backlog …` or in the web UI.\n", s(&w["builtAt"]));
    println!("## Roadmap\n");
    for (k, i) in roadmap(&w, project).iter().enumerate() {
        let link = match i.get("external") {
            Some(e) if e.is_object() => match e["url"].as_str() {
                Some(u) => format!(" ([{}:{}]({u}))", s(&e["system"]), s(&e["key"])),
                None => format!(" ({}:{})", s(&e["system"]), s(&e["key"])),
            },
            _ => String::new(),
        };
        println!("{}. **{}** ({}){link}", k + 1, s(&i["title"]), s(&i["id"]));
        println!("   {}", s(&i["goal"]));
        for line in s(&i["note"]).lines() {
            println!("   {line}");
        }
    }
    println!("\n## Deferred findings\n");
    for f in deferred(&w, project) {
        println!("- {} {}: {}{}", s(&f.finding["severity"]), f.at, s(&f.finding["problem"]), external(f.finding));
        if let Some(n) = f.finding["note"].as_str() {
            for line in n.lines() {
                println!("  {line}");
            }
        }
    }
    Ok(())
}
