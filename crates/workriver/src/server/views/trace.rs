//! A task's trace, the light part: which harness and model did it, and where the harness
//! keeps the session on the machine the agent ran on. Where each harness keeps it is known
//! here and nowhere else. Task traces only show the path. Rating a manager's words reads
//! session metadata from its log, without storing the transcript.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use workriver_core as core;

use crate::common::config::defaults::SESSION_RECHECK_SECS;
use crate::common::wire::{NoFile, Trace};

/// The longest folder name Claude Code makes of a working directory; a longer one is cut
/// there and gets a suffix of its own, which is not worked out here (a `*` stands for it).
const CLAUDE_FOLDER: usize = 200;

/// Which harness and model did work of the agent of `session`: all the page shows of a trace.
pub(in crate::server) fn ran(state: &core::State, session: &str, model: Option<&str>) -> crate::common::wire::workspace::Ran {
    let harness = state.agents.iter().find(|a| a.session == session).map(|a| a.harness.clone()).filter(|h| !h.is_empty());
    crate::common::wire::workspace::Ran { harness, model: model.map(str::to_string) }
}

/// What a look for a session's file by its id came to.
enum Looked {
    /// There, when last looked (checked again before it is shown).
    At(PathBuf),
    /// Not found then.
    Not(Instant),
}

/// Where harnesses keep sessions on this machine.
pub(in crate::server) struct Sessions {
    /// The user's home here; none: nothing is looked for, paths are shown from `~`.
    home: Option<PathBuf>,
    /// Files looked for by their id, by session.
    looked: Mutex<HashMap<String, Looked>>,
    /// How long a session not found is not looked for again.
    recheck: Duration,
}

impl Sessions {
    pub(in crate::server) fn new(home: Option<PathBuf>) -> Sessions {
        Sessions { home, looked: Mutex::default(), recheck: Duration::from_secs(SESSION_RECHECK_SECS) }
    }

    pub(in crate::server) fn usage(&self, agent: Option<&core::Agent>, at: &str) -> core::SessionUsage {
        let Some(a) = agent else { return core::SessionUsage::default() };
        let mut usage = core::SessionUsage { harness: Some(a.harness.clone()).filter(|h| !h.is_empty()), ..Default::default() };
        let Ok(until) = humantime::parse_rfc3339(at) else { return usage };
        let Ok((path, true)) = self.file(usage.harness.as_deref(), &a.session, &a.cwd, a.machine.is_some()) else { return usage };
        let path = Path::new(&path);
        // The trace also locates archived, compressed logs. Usage can only be read from JSONL.
        if path.extension().is_none_or(|ext| ext != "jsonl") {
            return usage;
        }
        let Ok(file) = std::fs::File::open(path) else { return usage };
        read_usage(BufReader::new(file), &a.harness, until, &mut usage);
        usage
    }

    /// The trace of work done by the agent of `session`, as `state` records that agent;
    /// `model`: what it said when it started the task.
    pub(in crate::server) fn trace(&self, state: &core::State, session: &str, model: Option<&str>) -> Trace {
        let a = state.agents.iter().find(|a| a.session == session);
        let harness = a.map(|a| a.harness.clone()).filter(|h| !h.is_empty());
        let machine = a.and_then(|a| a.machine.clone());
        let cwd = a.map(|a| a.cwd.as_str()).unwrap_or_default();
        let (file, unconfirmed, unknown) = match self.file(harness.as_deref(), session, cwd, machine.is_some()) {
            Ok((f, seen)) => (Some(f), !seen, None),
            Err(why) => (None, false, Some(why)),
        };
        Trace { session: session.to_string(), harness, model: model.map(str::to_string), machine, file, unconfirmed, unknown }
    }

    /// Where `harness` keeps `session`, and whether it was seen there; `remote`: on another
    /// machine, where nothing is looked for.
    fn file(&self, harness: Option<&str>, session: &str, cwd: &str, remote: bool) -> Result<(String, bool), NoFile> {
        let home = self.home.as_deref().filter(|_| !remote);
        let under = |rel: &str| match home {
            Some(h) => h.join(rel).display().to_string(),
            None => format!("~/{rel}"),
        };
        match harness {
            Some("claude") | Some("codex") if !own_id(session) => Err(NoFile::Id),
            // ~/.claude/projects/<the directory the session was begun in, each character but
            // a-z, A-Z and 0-9 a ->/<id>.jsonl. That is the one the agent was started in
            // unless it was resumed elsewhere: if not there, it is found by its id.
            Some("claude") => {
                let worked = (!cwd.is_empty()).then(|| format!(".claude/projects/{}/{session}.jsonl", claude_folder(cwd)));
                let Some(h) = home else { return worked.map(|w| (under(&w), false)).ok_or(NoFile::Cwd) };
                if let Some(w) = worked.as_deref().filter(|w| h.join(w).exists()) {
                    return Ok((under(w), true));
                }
                match self.find(session, || claude(h, session)) {
                    Some(f) => Ok((f, true)),
                    None => worked.map(|w| (under(&w), false)).ok_or(NoFile::Cwd),
                }
            }
            // ~/.codex/sessions/YYYY/MM/DD/rollout-<when it began>-<id>.jsonl, maybe compressed
            // (.jsonl.zst), or archived to ~/.codex/archived_sessions/: the day and the time
            // are not in the records, so it is found by its id.
            Some("codex") => match home {
                None => Ok((under(&format!(".codex/sessions/*/*/*/rollout-*-{session}.jsonl*")), false)),
                Some(h) => self.find(session, || codex(h, session)).map(|f| (f, true)).ok_or(NoFile::Missing),
            },
            _ => Err(NoFile::Harness),
        }
    }

    /// `session`'s file by `look`, which lists folders: where it was found, while it is still
    /// there; a session not found is looked for again only after `recheck`, so a snapshot
    /// lists nothing for a session it found missing a moment ago (once per session, not
    /// per task).
    fn find(&self, session: &str, look: impl FnOnce() -> Option<PathBuf>) -> Option<String> {
        match self.looked.lock().unwrap().get(session) {
            Some(Looked::At(f)) if f.exists() => return Some(f.display().to_string()),
            Some(Looked::Not(at)) if at.elapsed() < self.recheck => return None,
            _ => {}
        }
        let f = look();
        self.looked.lock().unwrap().insert(session.to_string(), f.clone().map_or_else(|| Looked::Not(Instant::now()), Looked::At));
        f.map(|f| f.display().to_string())
    }
}

fn read_usage(log: impl BufRead, harness: &str, until: SystemTime, usage: &mut core::SessionUsage) {
    let mut count = None;
    for line in log.split(b'\n') {
        let Ok(line) = line else { return };
        let Ok(entry) = serde_json::from_slice::<serde_json::Value>(&line) else { continue };
        if let Some(at) = entry["timestamp"].as_str()
            && humantime::parse_rfc3339(at).is_ok_and(|at| at > until)
        {
            break;
        }
        let kind = entry["type"].as_str();
        let version = match (harness, kind) {
            ("claude", _) => entry["version"].as_str(),
            ("codex", Some("session_meta")) => entry["payload"]["cli_version"].as_str(),
            _ => None,
        };
        if let Some(version) = version.filter(|v| !v.is_empty()) {
            usage.harness_version = Some(version.to_string());
        }
        let Some(kind) = kind else { continue };
        let seen = count.get_or_insert(0);
        let mut model = None;
        match (harness, kind) {
            ("claude", "system") if entry["subtype"] == "compact_boundary" => *seen += 1,
            ("claude", "assistant") if entry["isSidechain"] != true && entry["message"]["model"] != "<synthetic>" => {
                let m = &entry["message"];
                model = m["model"].as_str();
                let u = &m["usage"];
                if let Some(tokens) = u["input_tokens"].as_u64() {
                    usage.context_tokens = tokens
                        .checked_add(u["cache_creation_input_tokens"].as_u64().unwrap_or(0))
                        .and_then(|n| n.checked_add(u["cache_read_input_tokens"].as_u64().unwrap_or(0)));
                }
            }
            ("codex", "compacted") => *seen += 1,
            ("codex", "turn_context") => {
                let p = &entry["payload"];
                model = p["model"].as_str().or_else(|| p["collaboration_mode"]["settings"]["model"].as_str());
            }
            ("codex", "event_msg") => {
                let p = &entry["payload"];
                match p["type"].as_str() {
                    Some("token_count") => {
                        let info = &p["info"];
                        if let Some(n) = info["last_token_usage"]["input_tokens"].as_u64() {
                            usage.context_tokens = Some(n);
                        }
                        usage.context_window = info["model_context_window"].as_u64().or(usage.context_window);
                    }
                    Some("turn_started") => {
                        usage.context_window = p["model_context_window"].as_u64().or(usage.context_window);
                    }
                    _ => {}
                }
            }
            ("codex", "token_usage_record") => {
                if let Some(n) = entry["payload"]["usage"]["input_tokens"].as_u64() {
                    usage.context_tokens = Some(n);
                }
            }
            _ => {}
        }
        if let Some(model) = model.filter(|m| !m.is_empty()) {
            usage.model = Some(model.to_string());
        }
    }
    usage.compactions = count;
}

/// The entries of folder `p`, by name; none if it cannot be listed.
fn names(p: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(p).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default()
}

/// Claude Code's file of `session` under `home`: in whichever of its folders has that name.
fn claude(home: &Path, session: &str) -> Option<PathBuf> {
    let name = format!("{session}.jsonl");
    names(&home.join(".claude/projects")).into_iter().map(|d| d.join(&name)).find(|f| f.exists())
}

/// Codex's file of `session` under `home`, by the names of its folders: dated, or archived.
fn codex(home: &Path, session: &str) -> Option<PathBuf> {
    let tail = format!("-{session}.jsonl");
    let dated = names(&home.join(".codex/sessions")).into_iter().flat_map(|y| names(&y)).flat_map(|m| names(&m)).flat_map(|d| names(&d));
    let archived = names(&home.join(".codex/archived_sessions"));
    dated.chain(archived).find(|f| f.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && (n.ends_with(&tail) || n.contains(&format!("{tail}.")))))
}

/// A harness's own session id, safe as a part of a path: a runtime that has none names the
/// session by its place, in a form of its own (with `:`), which is no file's name.
fn own_id(session: &str) -> bool {
    !session.is_empty() && session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The folder Claude Code keeps a working directory's sessions in: each character but an
/// ASCII letter or digit becomes `-` (one for each UTF-16 unit: it counts in JavaScript).
fn claude_folder(cwd: &str) -> String {
    let folder: String = cwd.chars().flat_map(|c| if c.is_ascii_alphanumeric() { vec![c] } else { vec!['-'; c.len_utf16()] }).collect();
    if folder.len() > CLAUDE_FOLDER { format!("{}-*", &folder[..CLAUDE_FOLDER]) } else { folder }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_usage_reads_the_latest_input_and_counts_each_compaction_once() {
        let home = tempfile::tempdir().unwrap();
        let records = state(json!([{ "session": "codex-session", "name": "boss", "harness": "codex" }]));
        let here = Sessions::new(Some(home.path().to_path_buf()));
        let a = records.agents.first();
        let logs = home.path().join(".codex/sessions/2026/11/25");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("rollout-2026-11-25T10-00-00-codex-session.jsonl"), include_str!("__fixtures__/codex-usage.jsonl")).unwrap();
        let at = "2026-11-25T10:00:00Z";
        let usage = here.usage(a, at);
        assert_eq!(serde_json::to_value(usage).unwrap(), json!({
            "harness": "codex", "harness_version": "0.150.0", "model": "gpt-6.2-sol", "compactions": 1, "context_tokens": 32000, "context_window": 230000
        }));
        let remote = core::Agent { machine: Some("box".into()), ..a.unwrap().clone() };
        assert_eq!(serde_json::to_value(here.usage(Some(&remote), at)).unwrap(), json!({"harness":"codex"}));
    }

    fn state(agents: serde_json::Value) -> core::State {
        let mut s = core::State::default();
        for (k, a) in agents.as_array().unwrap().iter().enumerate() {
            s.apply(&serde_json::from_value(json!({"seq": k + 1, "at": "2026-11-22T00:00:00Z", "actor": "workriver", "kind": "agent.seen", "body": a})).unwrap());
        }
        s
    }

    #[test]
    fn claude_keeps_a_session_by_its_working_directory() {
        let s = state(json!([{"session": "5d2e41a0-7c3b", "name": "impl", "harness": "claude", "cwd": "/Users/me/99_git/app.worktrees/u7"}]));
        let home = tempfile::tempdir().unwrap();
        let folder = home.path().join(".claude/projects/-Users-me-99-git-app-worktrees-u7");
        let here = Sessions::new(Some(home.path().to_path_buf()));
        // Not there (yet): the path it would have, said to be worked out only.
        let t = here.trace(&s, "5d2e41a0-7c3b", Some("claude-opus-5-5"));
        assert_eq!((t.file, t.unconfirmed), (Some(folder.join("5d2e41a0-7c3b.jsonl").display().to_string()), true));
        assert_eq!((t.harness.as_deref(), t.model.as_deref(), t.machine, t.unknown), (Some("claude"), Some("claude-opus-5-5"), None, None));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("5d2e41a0-7c3b.jsonl"), "").unwrap();
        let t = here.trace(&s, "5d2e41a0-7c3b", None);
        assert_eq!((t.file, t.unconfirmed), (Some(folder.join("5d2e41a0-7c3b.jsonl").display().to_string()), false));
        // With no home to show it under, from `~`.
        assert_eq!(Sessions::new(None).trace(&s, "5d2e41a0-7c3b", None).file.as_deref(), Some("~/.claude/projects/-Users-me-99-git-app-worktrees-u7/5d2e41a0-7c3b.jsonl"));
    }

    #[test]
    fn a_folder_name_counts_as_javascript_does_and_is_cut_where_claude_cuts_it() {
        assert_eq!(claude_folder("/a/中文 x/😀"), "-a----x---");
        let long = format!("/{}", "a".repeat(250));
        assert_eq!(claude_folder(&long), format!("-{}-*", "a".repeat(199)));
    }

    #[test]
    fn another_machine_is_shown_from_its_home_and_nothing_is_looked_for() {
        let s = state(json!([
            {"session": "s-1", "name": "far", "harness": "claude", "cwd": "/home/u/repo", "machine": "box"},
            {"session": "019c8360-2e86", "name": "far-x", "harness": "codex", "machine": "box"}
        ]));
        let here = Sessions::new(Some("/Users/me".into()));
        let t = here.trace(&s, "s-1", None);
        assert_eq!((t.file.as_deref(), t.machine.as_deref(), t.unconfirmed), (Some("~/.claude/projects/-home-u-repo/s-1.jsonl"), Some("box"), true));
        let t = here.trace(&s, "019c8360-2e86", None);
        assert_eq!((t.file.as_deref(), t.unconfirmed), (Some("~/.codex/sessions/*/*/*/rollout-*-019c8360-2e86.jsonl*"), true));
    }

    #[test]
    fn codex_is_found_by_its_id_compressed_or_not() {
        let home = tempfile::tempdir().unwrap();
        let day = home.path().join(".codex/sessions/2026/02/22");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(day.join("rollout-2026-02-22T12-24-04-019c8360-aaaa.jsonl.zst"), "").unwrap();
        std::fs::write(day.join("rollout-2026-02-22T13-00-00-019c8360-bbbb.jsonl"), "").unwrap();
        // Another id that ends the same is not it.
        std::fs::write(day.join("rollout-2026-02-22T13-00-00-x019c8360-cccc.jsonl"), "").unwrap();
        let s = state(json!([
            {"session": "019c8360-aaaa", "name": "a", "harness": "codex"},
            {"session": "019c8360-bbbb", "name": "b", "harness": "codex"},
            {"session": "8360-cccc", "name": "c", "harness": "codex"},
        ]));
        let here = Sessions::new(Some(home.path().to_path_buf()));
        assert_eq!(here.trace(&s, "019c8360-aaaa", None).file, Some(day.join("rollout-2026-02-22T12-24-04-019c8360-aaaa.jsonl.zst").display().to_string()));
        assert_eq!(here.trace(&s, "019c8360-bbbb", None).file, Some(day.join("rollout-2026-02-22T13-00-00-019c8360-bbbb.jsonl").display().to_string()));
        assert_eq!(here.trace(&s, "8360-cccc", None).unknown, Some(NoFile::Missing));
        assert!(!here.trace(&s, "019c8360-bbbb", None).unconfirmed);
    }

    /// Codex moves a session it archives out of the dated folders into one of its own: it is
    /// found there, at first and after it moved; a path kept from before is checked first.
    #[test]
    fn an_archived_codex_session_is_found_where_it_went() {
        let home = tempfile::tempdir().unwrap();
        let day = home.path().join(".codex/sessions/2026/02/22");
        let archive = home.path().join(".codex/archived_sessions");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::create_dir_all(&archive).unwrap();
        let name = "rollout-2026-02-22T12-24-04-019c8360-aaaa.jsonl";
        std::fs::write(archive.join("rollout-2026-02-01T00-00-00-019c8360-old.jsonl.zst"), "").unwrap();
        std::fs::write(day.join(name), "").unwrap();
        let s = state(json!([{"session": "019c8360-aaaa", "name": "a", "harness": "codex"}, {"session": "019c8360-old", "name": "o", "harness": "codex"}]));
        let here = Sessions::new(Some(home.path().to_path_buf()));
        assert_eq!(here.trace(&s, "019c8360-old", None).file, Some(archive.join("rollout-2026-02-01T00-00-00-019c8360-old.jsonl.zst").display().to_string()));
        assert_eq!(here.trace(&s, "019c8360-aaaa", None).file, Some(day.join(name).display().to_string()));
        std::fs::rename(day.join(name), archive.join(name)).unwrap();
        let t = here.trace(&s, "019c8360-aaaa", None);
        assert_eq!((t.file, t.unknown), (Some(archive.join(name).display().to_string()), None));
        // Gone altogether: not found, never the old path.
        std::fs::remove_file(archive.join(name)).unwrap();
        let t = here.trace(&s, "019c8360-aaaa", None);
        assert_eq!((t.file, t.unknown), (None, Some(NoFile::Missing)));
    }

    /// Claude Code keeps a session by the directory it was begun in, which need not be the
    /// one the agent was seen in: the session is then found by its id among its folders.
    #[test]
    fn a_claude_session_begun_elsewhere_is_found_by_its_id() {
        let home = tempfile::tempdir().unwrap();
        let projects = home.path().join(".claude/projects");
        for (folder, id) in [("-r", "s-1"), ("-other", "s-1x"), ("-other", "x-s-1")] {
            std::fs::create_dir_all(projects.join(folder)).unwrap();
            std::fs::write(projects.join(folder).join(format!("{id}.jsonl")), "").unwrap();
        }
        let s = state(json!([
            {"session": "s-1", "name": "sub", "harness": "claude", "cwd": "/r/sub"},
            {"session": "s-2", "name": "gone", "harness": "claude", "cwd": "/r/sub"},
        ]));
        let here = Sessions::new(Some(home.path().to_path_buf()));
        let t = here.trace(&s, "s-1", None);
        assert_eq!((t.file, t.unconfirmed), (Some(projects.join("-r/s-1.jsonl").display().to_string()), false));
        // Nowhere: the path it would have, as worked out only.
        let t = here.trace(&s, "s-2", None);
        assert_eq!((t.file, t.unconfirmed), (Some(projects.join("-r-sub/s-2.jsonl").display().to_string()), true));
        // Seen before its directory was kept: found by its id all the same.
        let old = state(json!([{"session": "s-1", "name": "sub", "harness": "claude"}, {"session": "s-2", "name": "gone", "harness": "claude"}]));
        assert_eq!(here.trace(&old, "s-1", None).file, Some(projects.join("-r/s-1.jsonl").display().to_string()));
        assert_eq!(here.trace(&old, "s-2", None).unknown, Some(NoFile::Cwd));
    }

    /// A session not found is not looked for again for a while (every snapshot would list
    /// the folders again, once per task): once per session, then after `recheck`.
    #[test]
    fn a_session_not_found_is_looked_for_again_only_after_a_while() {
        let home = tempfile::tempdir().unwrap();
        let day = home.path().join(".codex/sessions/2026/02/22");
        std::fs::create_dir_all(&day).unwrap();
        let s = state(json!([{"session": "019c8360-late", "name": "l", "harness": "codex"}]));
        let mut here = Sessions::new(Some(home.path().to_path_buf()));
        assert_eq!(here.trace(&s, "019c8360-late", None).unknown, Some(NoFile::Missing));
        std::fs::write(day.join("rollout-2026-02-22T12-24-04-019c8360-late.jsonl"), "").unwrap();
        assert_eq!(here.trace(&s, "019c8360-late", None).unknown, Some(NoFile::Missing), "remembered, not looked for again");
        here.recheck = std::time::Duration::ZERO;
        assert!(here.trace(&s, "019c8360-late", None).file.is_some());
    }

    #[test]
    fn what_cannot_be_told_says_why() {
        let s = state(json!([
            {"session": "s-old", "name": "old", "harness": "claude"},
            {"session": "rt:w1:p2", "name": "no-id", "harness": "claude", "cwd": "/r"},
            {"session": "s-other", "name": "other", "harness": "aider", "cwd": "/r"},
            {"session": "../../x", "name": "odd", "harness": "codex"}
        ]));
        let here = Sessions::new(Some("/Users/me".into()));
        let why = |session: &str| here.trace(&s, session, None).unknown;
        assert_eq!(why("s-old"), Some(NoFile::Cwd));
        assert_eq!(why("rt:w1:p2"), Some(NoFile::Id));
        assert_eq!(why("s-other"), Some(NoFile::Harness));
        assert_eq!(why("../../x"), Some(NoFile::Id));
        // Never seen: no harness to tell.
        let t = here.trace(&s, "s-never", Some("m"));
        assert_eq!((t.harness, t.unknown, t.model.as_deref()), (None, Some(NoFile::Harness), Some("m")));
    }
}
