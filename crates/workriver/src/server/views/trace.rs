//! A task's trace, the light part: which harness and model did it, and where the harness
//! keeps the session on the machine the agent ran on. Where each harness keeps it is known
//! here and nowhere else. A session file is never opened: its path is worked out, or, where
//! the harness names it by more than the id, a folder's names are listed to find it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use workriver_core as core;

use crate::common::wire::{NoFile, Trace};

/// The longest folder name Claude Code makes of a working directory; a longer one is cut
/// there and gets a suffix of its own, which is not worked out here (a `*` stands for it).
const CLAUDE_FOLDER: usize = 200;

/// Where harnesses keep sessions on this machine.
pub(in crate::server) struct Sessions {
    /// The user's home here; none: nothing is looked for, paths are shown from `~`.
    home: Option<PathBuf>,
    /// Files found by their id, by session: a session's file does not move.
    found: Mutex<HashMap<String, String>>,
}

impl Sessions {
    pub(in crate::server) fn new(home: Option<PathBuf>) -> Sessions {
        Sessions { home, found: Mutex::default() }
    }

    /// The trace of work done by the agent of `session`, as `state` records that agent;
    /// `model`: what it said when it started the task.
    pub(in crate::server) fn trace(&self, state: &core::State, session: &str, model: Option<&str>) -> Trace {
        let a = state.agents.iter().find(|a| a.session == session);
        let harness = a.map(|a| a.harness.clone()).filter(|h| !h.is_empty());
        let machine = a.and_then(|a| a.machine.clone());
        let cwd = a.map(|a| a.cwd.as_str()).unwrap_or_default();
        let (file, unknown) = match self.file(harness.as_deref(), session, cwd, machine.is_some()) {
            Ok(f) => (Some(f), None),
            Err(why) => (None, Some(why)),
        };
        Trace { session: session.to_string(), harness, model: model.map(str::to_string), machine, file, unknown }
    }

    /// Where `harness` keeps `session`; `remote`: on another machine, where nothing is looked for.
    fn file(&self, harness: Option<&str>, session: &str, cwd: &str, remote: bool) -> Result<String, NoFile> {
        let home = self.home.as_deref().filter(|_| !remote);
        let under = |rel: &str| match home {
            Some(h) => h.join(rel).display().to_string(),
            None => format!("~/{rel}"),
        };
        match harness {
            Some("claude") | Some("codex") if !own_id(session) => Err(NoFile::Id),
            // ~/.claude/projects/<the working directory, each character but a-z, A-Z and 0-9 a ->/<id>.jsonl
            Some("claude") if cwd.is_empty() => Err(NoFile::Cwd),
            Some("claude") => Ok(under(&format!(".claude/projects/{}/{session}.jsonl", claude_folder(cwd)))),
            // ~/.codex/sessions/YYYY/MM/DD/rollout-<when it began>-<id>.jsonl, maybe compressed
            // (.jsonl.zst): the day and the time are not in the records, so it is found by its id.
            Some("codex") => match home {
                None => Ok(under(&format!(".codex/sessions/*/*/*/rollout-*-{session}.jsonl*"))),
                Some(h) => self.codex(h, session).ok_or(NoFile::Missing),
            },
            _ => Err(NoFile::Harness),
        }
    }

    /// Codex's file of `session` under `home`, by the names of its folders.
    fn codex(&self, home: &Path, session: &str) -> Option<String> {
        if let Some(f) = self.found.lock().unwrap().get(session) {
            return Some(f.clone());
        }
        let names = |p: &Path| -> Vec<PathBuf> { std::fs::read_dir(p).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default() };
        let tail = format!("-{session}.jsonl");
        let root = home.join(".codex/sessions");
        let f = names(&root).iter().flat_map(|y| names(y)).flat_map(|m| names(&m)).flat_map(|d| names(&d)).find(|f| {
            f.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && (n.ends_with(&tail) || n.contains(&format!("{tail}."))))
        })?;
        let f = f.display().to_string();
        self.found.lock().unwrap().insert(session.to_string(), f.clone());
        Some(f)
    }
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
        let t = Sessions::new(Some("/Users/me".into())).trace(&s, "5d2e41a0-7c3b", Some("claude-opus-5-5"));
        assert_eq!(t.file.as_deref(), Some("/Users/me/.claude/projects/-Users-me-99-git-app-worktrees-u7/5d2e41a0-7c3b.jsonl"));
        assert_eq!((t.harness.as_deref(), t.model.as_deref(), t.machine, t.unknown), (Some("claude"), Some("claude-opus-5-5"), None, None));
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
        assert_eq!((t.file.as_deref(), t.machine.as_deref()), (Some("~/.claude/projects/-home-u-repo/s-1.jsonl"), Some("box")));
        assert_eq!(here.trace(&s, "019c8360-2e86", None).file.as_deref(), Some("~/.codex/sessions/*/*/*/rollout-*-019c8360-2e86.jsonl*"));
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
        // Found once, it is kept: the file does not move.
        std::fs::remove_dir_all(home.path().join(".codex")).unwrap();
        assert!(here.trace(&s, "019c8360-aaaa", None).file.is_some());
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
