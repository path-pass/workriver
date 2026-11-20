// A unit's code from git: its commits and files, read in full or lightly, and what is
// read again only when a branch moved.
use super::*;

/// The texts of unit `id` as `state` lets its reader see them (none of a blind view's),
/// but for the commits in `have` (short hashes), which the reader has.
pub(super) fn code(state: &core::State, cache: &mut Cache, live: Option<&[RuntimeAgent]>, id: &str, have: &[&str]) -> Option<Code> {
    let u = state.units.iter().find(|u| u.id == id && u.has_code())?;
    cache.refs.clear();
    cache.worktrees.clear();
    (cache.recomputed, cache.reused) = (0, 0);
    let mut shown = unit(state, live.unwrap_or_default(), u, cache, &Names(HashMap::new()), None);
    if state.reduced {
        hide_unit(&mut shown);
    }
    let commits = shown.commits.into_iter().filter(|c| c.live || !have.contains(&c.id.as_str())).map(|c| CommitCode { id: c.id, files: c.files }).collect();
    Some(Code { code: shown.code, commits })
}

/// A unit's commits and its uncommitted files, read from git where its repository is.
/// A unit's commits now (oldest first), by the rules of `scan::branch`, and whether git
/// shows it merged. What the branch held is kept, for when it is merged or gone.
pub(in crate::server) fn unit_commits(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) -> scan::Result<(Vec<scan::Named>, bool)> {
    let place = format!("{}|{}|{}", u.repo, u.branch, u.base);
    let seen = cache.stored.seen.get(&u.id).filter(|s| s.place == place).cloned();
    let fork = seen.as_ref().and_then(|s| s.fork.clone());
    let ours = |c: &scan::Named| c.task.as_deref().and_then(parse_task_ref).is_some_and(|(x, _)| x == u.id) || u.links.iter().any(|l| l.hash == c.hash);
    let (commits, fork, merged) = match scan::branch(at, Path::new(&u.repo), &u.base, &u.branch, fork.as_deref(), &ours)? {
        scan::Found::Ahead { commits, fork } => (commits, Some(fork), false),
        scan::Found::Merged(commits) => (commits, fork, true),
        scan::Found::Fresh => (vec![], fork, false),
        scan::Found::Gone => return Ok((seen.map(|s| s.commits).unwrap_or_default(), false)),
    };
    let now = Seen { place, fork, commits: commits.clone() };
    if seen.as_ref() != Some(&now) {
        cache.stored.seen.insert(u.id.clone(), now);
        cache.dirty = true;
    }
    Ok((commits, merged))
}

/// Which commits are the unit's: as worked out last time while its branch, its base and
/// what is recorded about it stay as they were (one `git for-each-ref` per repository tells);
/// else worked out again (`unit_commits`).
fn unit_hashes(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) -> scan::Result<(Vec<String>, bool)> {
    if u.merged.is_some() && !u.merged_commits.is_empty() {
        return Ok((u.merged_commits.clone(), false));
    }
    let fork = cache.stored.seen.get(&u.id).and_then(|s| s.fork.clone()).unwrap_or_default();
    let key = cache.refs(at, u).and_then(|refs| {
        let base = scan::tip(&refs, &u.base)?.to_string();
        let tip = scan::tip(&refs, &u.branch).unwrap_or("-").to_string();
        // The base's remote copies too: a fetch can merge the branch upstream (`scan::since`).
        let mut up: Vec<&str> = refs.iter().filter(|(r, _)| r.strip_prefix("refs/remotes/").and_then(|r| r.split_once('/')).is_some_and(|(_, b)| b == u.base)).map(|(_, h)| h.as_str()).collect();
        up.sort();
        Some(format!("{}|{}={tip}|{}={base}|{fork}|{}|{}|{}|{}", u.repo, u.branch, u.base, u.earlier.len(), u.links.len(), u.merged_commits.len(), up.join(",")))
    });
    if let Some(m) = cache.stored.memo.get(&u.id).filter(|m| key.as_ref() == Some(&m.key)) {
        cache.reused += 1;
        return Ok((m.hashes.clone(), m.merged));
    }
    cache.recomputed += 1;
    let (list, merged) = unit_commits(at, u, cache)?;
    // Under the fork as now known: what the next read finds unchanged.
    let fork = cache.stored.seen.get(&u.id).and_then(|s| s.fork.clone()).unwrap_or_default();
    let key = key.map(|k| {
        let mut parts: Vec<&str> = k.split('|').collect();
        parts[3] = &fork;
        parts.join("|")
    });
    let hashes: Vec<String> = list.iter().map(|c| c.hash.clone()).collect();
    for c in list {
        cache.stored.meta.insert(c.hash.clone(), c);
    }
    if let Some(key) = key {
        cache.stored.memo.insert(u.id.clone(), Memo { key, hashes: hashes.clone(), merged });
    }
    cache.dirty = true;
    Ok((hashes, merged))
}

/// The background read of repositories on other machines: each unit's commits and work,
/// and each repository's tree, into `shared`. Whether anything changed, and whether a
/// machine could not be read (then it is tried again soon).
pub(in crate::server) fn refresh_remote(state: &core::State, cache: &mut Cache) -> (bool, bool) {
    cache.refs.clear();
    cache.worktrees.clear();
    (cache.recomputed, cache.reused) = (0, 0);
    let (mut changed, mut skipped) = (false, false);
    for u in state.units.iter().filter(|u| u.machine.is_some() && u.has_code()) {
        let Some(at) = cache.runner(u.machine.as_deref()) else {
            skipped = true;
            // Not reachable (yet): what was read before the last restart, without texts.
            let known = cache.stored.memo.get(&u.id).map(|m| m.hashes.clone()).unwrap_or_else(|| u.merged_commits.clone());
            let mut shared = cache.shared.lock().unwrap();
            if !shared.units.contains_key(&u.id) && !known.is_empty() {
                let commits = known.iter().filter_map(|h| cache.stored.meta.get(h).map(|n| scan::Commit { id: h.chars().take(9).collect(), hash: h.clone(), subject: n.subject.clone(), date: n.date.clone(), author: n.author.clone(), task: n.task.clone(), merge: false, from_base: false, files: vec![] })).collect();
                let patch = cache.stored.patch.iter().filter(|(h, _)| known.contains(h)).map(|(h, p)| (h.clone(), p.clone())).collect();
                shared.units.insert(u.id.clone(), (commits, vec![], patch));
                changed = true;
            }
            continue;
        };
        let (c, f) = read_unit(&at, u, cache, state.progress(u) == core::Progress::Ready);
        let p: HashMap<String, String> = c.iter().map(|x| &x.hash).chain(u.earlier.iter().flat_map(|e| &e.commits).map(|x| &x.hash)).filter_map(|h| cache.stored.patch.get(h).map(|p| (h.clone(), p.clone()))).collect();
        let mut shared = cache.shared.lock().unwrap();
        let same = shared.units.get(&u.id).is_some_and(|(oc, of, op)| oc.iter().map(|x| &x.hash).eq(c.iter().map(|x| &x.hash)) && *of == f && *op == p);
        if !same {
            shared.units.insert(u.id.clone(), (c, f, p));
            changed = true;
        }
    }
    let repos: BTreeSet<(&str, &str)> = state.units.iter().filter(|u| u.has_code()).filter_map(|u| Some((u.machine.as_deref()?, u.repo.as_str()))).collect();
    for (m, r) in repos {
        let key = format!("{m}\0{r}");
        let before = cache.trees.get(&key).map(|(h, _)| h.clone());
        repo(Some(m), r, cache);
        changed |= cache.trees.get(&key).map(|(h, _)| h.clone()) != before;
    }
    (changed, skipped)
}

/// A unit's commits and its uncommitted work, read from git where its repository is: the
/// commits' texts only for those not read before, all in one go; none in a light build.
/// `ready`: the unit is ready to merge, so whether it looks squash-merged is looked at too.
pub(super) fn read_unit(at: &scan::Runner, u: &core::Unit, cache: &mut Cache, ready: bool) -> (Vec<scan::Commit>, Vec<scan::FileChange>) {
    let started = std::time::Instant::now();
    let calls = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed);
    let repo = Path::new(&u.repo);
    let hashes = match unit_hashes(at, u, cache) {
        Ok((hashes, merged)) => {
            // A spike is never merged: neither recorded so nor looked at for a squash.
            if merged && u.kind.merges() && u.merged.is_none() && !cache.merged_now.iter().any(|(x, _)| x == &u.id) {
                cache.merged_now.push((u.id.clone(), hashes.clone()));
            }
            if ready && u.kind.merges() && !merged && u.merged.is_none() {
                squash_check(at, u, cache);
            }
            hashes
        }
        Err(e) => {
            tracing::warn!("unit {}: {e}", u.id);
            vec![]
        }
    };
    // Light: what each commit says is enough. Else their texts, those not read yet at once.
    let named = |h: &String, cache: &Cache| cache.stored.meta.get(h).map(|n| scan::Commit { id: h.chars().take(9).collect(), hash: h.clone(), subject: n.subject.clone(), date: n.date.clone(), author: n.author.clone(), task: n.task.clone(), merge: false, from_base: false, files: vec![] });
    let missing: Vec<String> = hashes.iter().filter(|h| !cache.commits.contains_key(*h) && (!cache.light || !cache.stored.meta.contains_key(*h))).cloned().collect();
    if !missing.is_empty() {
        match scan::commits(at, repo, &missing, Some(&u.base)) {
            Ok(read) => {
                for c in read {
                    cache.stored.meta.entry(c.hash.clone()).or_insert_with(|| scan::Named { hash: c.hash.clone(), subject: c.subject.clone(), author: c.author.clone(), date: c.date.clone(), task: c.task.clone() });
                    cache.commits.insert(c.hash.clone(), c);
                }
                cache.dirty = true;
            }
            Err(e) => tracing::warn!("unit {}: {e}", u.id),
        }
    }
    // Light and bare: which files, not their texts (they are what makes a snapshot many
    // megabytes); not even copied.
    let bare = cache.light || cache.bare;
    let commits: Vec<scan::Commit> = hashes.iter().filter_map(|h| cache.commits.get(h).map(|c| if bare { bare_commit(c) } else { c.clone() }).or_else(|| named(h, cache))).collect();
    // Links whose commit is not here any more, and commits of a branch the unit left (a
    // rebase gave them other hashes): told by patch ids, worked out once each (none: "").
    let stale = u.links.iter().any(|l| !commits.iter().any(|c| c.hash == l.hash));
    if stale || !u.earlier.is_empty() {
        let old = u.earlier.iter().flat_map(|e| &e.commits).filter(|c| c.patch_id.is_empty()).map(|c| &c.hash);
        let want: Vec<String> = commits.iter().map(|c| &c.hash).chain(old).filter(|h| !cache.stored.patch.contains_key(*h)).cloned().collect::<BTreeSet<_>>().into_iter().collect();
        if !want.is_empty() {
            let found = scan::patch_ids(at, repo, &want).unwrap_or_default();
            for h in want {
                let id = found.get(&h).cloned().unwrap_or_default();
                cache.stored.patch.insert(h, id);
            }
            cache.dirty = true;
        }
    }
    let files = if u.merged.is_some() || cache.light {
        vec![]
    } else {
        let key = format!("{}\0{}", u.machine.as_deref().unwrap_or_default(), u.repo);
        if !cache.worktrees.contains_key(&key) {
            let w = scan::worktrees(at, repo).unwrap_or_default();
            cache.worktrees.insert(key.clone(), w);
        }
        match cache.worktrees.get(&key).and_then(|w| w.get(&u.branch)).cloned() {
            None => vec![],
            Some(dir) => scan::live_in(at, &dir).unwrap_or_else(|e| {
                tracing::warn!("unit {}: {e}", u.id);
                vec![]
            }),
        }
    };
    let ms = started.elapsed().as_millis() as u64;
    tracing::info!(target: "perf", kind = "unit", unit = %u.id, machine = u.machine.as_deref().unwrap_or("local"), ms, git = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed) - calls, commits = commits.len(), light = cache.light, slow = ms >= crate::common::config::defaults::SLOW_MS);
    (commits, files)
}

/// Whether a ready unit git does not show merged looks squash-merged (`scan::squashed`):
/// worked out once per state of its branch and base (the key of its commits), and kept in
/// `Shared::squash`, where the page and the service's reminder read it.
fn squash_check(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) {
    let Some(key) = cache.stored.memo.get(&u.id).map(|m| m.key.clone()) else { return };
    let found = match cache.stored.squash.get(&u.id) {
        Some((k, f)) if *k == key => f.clone(),
        _ => {
            // Its branch gone, or where its commits start not known: nothing to compare.
            let tip = cache.refs(at, u).and_then(|r| scan::tip(&r, &u.branch).map(str::to_string));
            let fork = cache.stored.seen.get(&u.id).and_then(|s| s.fork.clone());
            let (Some(tip), Some(fork)) = (tip, fork) else { return };
            let found = match scan::squashed(at, Path::new(&u.repo), &u.base, &u.branch, &fork) {
                Ok(f) => f.map(|(into, commit)| Squash { tip, into, commit }),
                Err(e) => {
                    tracing::warn!("unit {}: {e}", u.id);
                    return;
                }
            };
            cache.stored.squash.insert(u.id.clone(), (key, found.clone()));
            cache.dirty = true;
            found
        }
    };
    let mut shared = cache.shared.lock().unwrap();
    match found {
        Some(s) => shared.squash.insert(u.id.clone(), s),
        None => shared.squash.remove(&u.id),
    };
}

pub(super) fn bare_commit(c: &scan::Commit) -> scan::Commit {
    scan::Commit { id: c.id.clone(), hash: c.hash.clone(), subject: c.subject.clone(), date: c.date.clone(), author: c.author.clone(), task: c.task.clone(), merge: c.merge, from_base: c.from_base, files: c.files.iter().map(scan::FileChange::bare).collect() }
}

/// Which texts a unit's commits and uncommitted files have: its commits' hashes (a commit
/// never changes) and its uncommitted texts and git entries (a mode changed alone changes it).
pub(super) fn code_of(commits: &[scan::Commit], files: &[scan::FileChange]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for c in commits {
        c.hash.hash(&mut h);
    }
    for f in files {
        (&f.path, &f.status, &f.before, &f.after, &f.before_mode, &f.before_id, &f.after_mode, &f.after_id).hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

/// The commits of branches `u` left, each once, against `now`: those found again here, as
/// new hash → old short hash (a rebase), and those that are not here.
pub(super) fn earlier_commits(u: &core::Unit, now: &[scan::Commit], patch: &HashMap<String, String>) -> (HashMap<String, String>, Vec<core::SeenCommit>) {
    let mut again = HashMap::new();
    let mut gone: Vec<core::SeenCommit> = vec![];
    let mut seen = BTreeSet::new();
    for old in u.earlier.iter().flat_map(|e| &e.commits) {
        if !seen.insert(old.hash.clone()) {
            continue;
        }
        if now.iter().any(|c| c.hash == old.hash) {
            continue;
        }
        let old_patch = if old.patch_id.is_empty() { patch.get(&old.hash).cloned().unwrap_or_default() } else { old.patch_id.clone() };
        let same = |c: &&scan::Commit| {
            (!old_patch.is_empty() && patch.get(&c.hash) == Some(&old_patch))
                || (!old.subject.is_empty() && c.subject == old.subject && (old.date.is_empty() || c.date == old.date))
        };
        match now.iter().find(same) {
            Some(c) => {
                again.entry(c.hash.clone()).or_insert_with(|| old.hash.chars().take(9).collect());
            }
            None => gone.push(old.clone()),
        }
    }
    (again, gone)
}
