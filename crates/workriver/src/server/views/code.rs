// A unit's code from git: its commits and files, read in full or lightly, and what is
// read again only when a branch moved.
use super::*;

/// The texts of unit `id` as `state` lets its reader see them (none of a blind view's),
/// but for the commits in `have` (short hashes), which the reader has. The unit counts as
/// open on the page from then on (`Cache::open`).
pub(super) fn code(state: &core::State, cache: &mut Cache, live: Option<&[RuntimeAgent]>, id: &str, have: &[&str]) -> Option<Code> {
    let u = state.units.iter().find(|u| u.id == id && u.has_code())?;
    cache.refs.clear();
    cache.worktrees.clear();
    cache.read_live.clear();
    (cache.recomputed, cache.reused) = (0, 0);
    // Open on the page: its uncommitted work is read with its texts from now on, in snapshots too.
    cache.open(id);
    cache.take_marks();
    let mut shown = unit(state, live.unwrap_or_default(), u, cache, &Names(HashMap::new()), None);
    if state.reduced {
        hide_unit(&mut shown);
    }
    let commits = shown.commits.into_iter().filter(|c| c.live || !have.contains(&c.id.as_str())).map(|c| CommitCode { id: c.id, files: c.files }).collect();
    Some(Code { code: shown.code, commits, net: shown.net })
}

/// A unit's commits now (oldest first), by the rules of `scan::branch`, and whether git
/// shows it merged. What the branch held is kept, for when it is merged or gone, and where
/// it met its base (`Seen::base_at`): while it is not merged, where they meet now; merged,
/// where they met while it was not, else as the base stood before the merge that took it.
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
    let repo = Path::new(&u.repo);
    let base_at = match commits.last() {
        Some(tip) if !merged => scan::merge_base(at, repo, &u.base, &tip.hash)?,
        // Merged: as the base stood before the merge that took it, from the history; only
        // when that cannot tell (a fast-forward), as last seen at this same tip.
        Some(tip) => match scan::base_before(at, repo, &u.base, &tip.hash)? {
            Some(b) => Some(b),
            None => seen.as_ref().and_then(|s| recorded_at(s, &tip.hash)),
        },
        None => seen.as_ref().and_then(|s| s.base_at.clone()),
    };
    let now = Seen { place, fork, commits: commits.clone(), base_at };
    if seen.as_ref() != Some(&now) {
        cache.stored.seen.insert(u.id.clone(), now);
        cache.dirty = true;
    }
    Ok((commits, merged))
}

/// Which commits are the unit's, whether git shows it merged, and where it meets its base:
/// as worked out last time while its branch, its base and what is recorded about it stay as
/// they were (one `git for-each-ref` per repository tells); else worked out again
/// (`unit_commits`).
fn unit_hashes(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) -> scan::Result<(Vec<String>, bool, Option<String>)> {
    if u.merged.is_some() && !u.merged_commits.is_empty() {
        return Ok((u.merged_commits.clone(), false, met_before(at, u, cache)));
    }
    let fork = cache.stored.seen.get(&u.id).and_then(|s| s.fork.clone()).unwrap_or_default();
    let key = cache.refs(at, u).and_then(|refs| {
        let base = scan::tip(&refs, &u.base)?.to_string();
        let tip = scan::tip(&refs, &u.branch).unwrap_or("-").to_string();
        // The base's remote copies too: a fetch can merge the branch upstream (`scan::since`).
        let mut up: Vec<&str> = refs.iter().filter(|(r, _)| r.strip_prefix("refs/remotes/").and_then(|r| r.split_once('/')).is_some_and(|(_, b)| b == u.base)).map(|(_, h)| h.as_str()).collect();
        // And the upstream the base has now, whatever its name, and where it points: unset or
        // set to another, what the branch is measured against changes, no hash moved.
        let tracked = scan::upstream(&refs, &u.base).map(|r| format!("{r}={}", refs.get(r).map_or("-", String::as_str)));
        up.extend(tracked.as_deref());
        up.sort();
        Some(format!("{}|{}={tip}|{}={base}|{fork}|{}|{}|{}|{}", u.repo, u.branch, u.base, u.earlier.len(), u.links.len(), u.merged_commits.len(), up.join(",")))
    });
    if let Some(m) = cache.stored.memo.get(&u.id).filter(|m| m.based && key.as_ref() == Some(&m.key)) {
        cache.reused += 1;
        return Ok((m.hashes.clone(), m.merged, m.base_at.clone()));
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
    let base_at = cache.stored.seen.get(&u.id).and_then(|s| s.base_at.clone());
    if let Some(key) = key {
        cache.stored.memo.insert(u.id.clone(), Memo { key, hashes: hashes.clone(), merged, base_at: base_at.clone(), based: true });
    }
    cache.dirty = true;
    Ok((hashes, merged, base_at))
}

/// Where a unit recorded merged met its base before it merged: as the base (or its upstream)
/// stood before the merge that took its last commit, from the history (worked out once per
/// run); only when that cannot tell (a fast-forward), as last seen at that same tip. None
/// when neither tells.
fn met_before(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) -> Option<String> {
    let tip = u.merged_commits.last()?.clone();
    if let Some((_, b)) = cache.before.get(&u.id).filter(|(t, _)| *t == tip) {
        return b.clone();
    }
    let found = match scan::base_before(at, Path::new(&u.repo), &u.base, &tip) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("unit {}: {e}", u.id);
            return None;
        }
    };
    let b = found.or_else(|| cache.stored.seen.get(&u.id).and_then(|s| recorded_at(s, &tip)));
    cache.before.insert(u.id.clone(), (tip, b.clone()));
    b
}

/// Where the branch met its base as last seen, if it was seen at `tip`: a record of an
/// earlier tip misses what came after (a last sync of the base, say).
fn recorded_at(seen: &Seen, tip: &str) -> Option<String> {
    seen.commits.last().filter(|c| c.hash == tip).and(seen.base_at.clone())
}

/// A unit's net change against its base, as `git diff from..tip` names it: from where it
/// meets its base (`base_at`), else from where its commits start; its texts read once per
/// pair (`Cache::nets`), none in a bare build. None without commits, or when git cannot
/// tell (the page then counts its commits together).
fn net(at: &scan::Runner, u: &core::Unit, cache: &mut Cache, hashes: &[String], base_at: Option<&str>) -> Option<NetRead> {
    let tip = hashes.last()?;
    let from = base_at.map(str::to_string).unwrap_or_else(|| format!("{}^", hashes[0]));
    let id = format!("{from}..{tip}");
    if !cache.nets.contains_key(&id) {
        match scan::diff(at, Path::new(&u.repo), &from, tip) {
            Ok(files) => {
                cache.nets.insert(id.clone(), files);
            }
            Err(e) => {
                tracing::warn!("unit {}: {e}", u.id);
                return None;
            }
        }
    }
    let files = &cache.nets[&id];
    Some((id, if cache.bare { files.iter().map(scan::FileChange::bare).collect() } else { files.clone() }))
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
                let commits = known.iter().filter_map(|h| cache.stored.meta.get(h).map(|n| scan::Commit { id: h.chars().take(9).collect(), hash: h.clone(), subject: n.subject.clone(), date: n.date.clone(), author: n.author.clone(), task: n.task.clone(), merge: false, parents: vec![], base_merge: false, files: vec![] })).collect();
                let patch = cache.stored.patch.iter().filter(|(h, _)| known.contains(h)).map(|(h, p)| (h.clone(), p.clone())).collect();
                shared.units.insert(u.id.clone(), (commits, vec![], patch, None));
                changed = true;
            }
            continue;
        };
        let (c, f, n) = read_unit(&at, u, cache, state.progress(u) == core::Progress::Ready);
        let p: HashMap<String, String> = c.iter().map(|x| &x.hash).chain(u.earlier.iter().flat_map(|e| &e.commits).map(|x| &x.hash)).filter_map(|h| cache.stored.patch.get(h).map(|p| (h.clone(), p.clone()))).collect();
        let mut shared = cache.shared.lock().unwrap();
        // The same commits, read against the same base: which merges brought it in and the net change's ends too.
        let same = shared.units.get(&u.id).is_some_and(|(oc, of, op, on)| oc.iter().map(|x| (&x.hash, x.base_merge)).eq(c.iter().map(|x| (&x.hash, x.base_merge))) && *of == f && *op == p && on.as_ref().map(|x| &x.0) == n.as_ref().map(|x| &x.0));
        if !same {
            shared.units.insert(u.id.clone(), (c, f, p, n));
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
pub(super) fn read_unit(at: &scan::Runner, u: &core::Unit, cache: &mut Cache, ready: bool) -> (Vec<scan::Commit>, Vec<scan::FileChange>, Option<NetRead>) {
    let started = std::time::Instant::now();
    let calls = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed);
    let repo = Path::new(&u.repo);
    let (hashes, base_at) = match unit_hashes(at, u, cache) {
        Ok((hashes, merged, base_at)) => {
            // A spike is never merged: neither recorded so nor looked at for a squash.
            if merged && u.kind.merges() && u.merged.is_none() && !cache.merged_now.iter().any(|(x, _)| x == &u.id) {
                cache.merged_now.push((u.id.clone(), hashes.clone()));
            }
            if ready && u.kind.merges() && !merged && u.merged.is_none() {
                squash_check(at, u, cache);
            }
            (hashes, base_at)
        }
        Err(e) => {
            tracing::warn!("unit {}: {e}", u.id);
            (vec![], None)
        }
    };
    // Light: what each commit says is enough. Else their texts, those not read yet at once.
    let named = |h: &String, cache: &Cache| cache.stored.meta.get(h).map(|n| scan::Commit { id: h.chars().take(9).collect(), hash: h.clone(), subject: n.subject.clone(), date: n.date.clone(), author: n.author.clone(), task: n.task.clone(), merge: false, parents: vec![], base_merge: false, files: vec![] });
    let missing: Vec<String> = hashes.iter().filter(|h| !cache.commits.contains_key(*h) && (!cache.light || !cache.stored.meta.contains_key(*h))).cloned().collect();
    if !missing.is_empty() {
        match scan::commits(at, repo, &missing) {
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
    let mut commits: Vec<scan::Commit> = hashes.iter().filter_map(|h| cache.commits.get(h).map(|c| if bare { bare_commit(c) } else { c.clone() }).or_else(|| named(h, cache))).collect();
    // Which merges brought the base in: against where the branch meets it now, so on the
    // copies shown, never on the commits kept.
    if let Some(b) = base_at.as_deref() {
        for c in commits.iter_mut().filter(|c| c.merge) {
            let key = format!("{}|{b}", c.hash);
            c.base_merge = match cache.brings.get(&key) {
                Some(x) => *x,
                None => match scan::brings_base(at, repo, c, b) {
                    Ok(x) => *cache.brings.entry(key).or_insert(x),
                    Err(e) => {
                        tracing::warn!("unit {}: {e}", u.id);
                        false
                    }
                },
            };
        }
    }
    let net = if cache.light { None } else { net(at, u, cache, &hashes, base_at.as_deref()) };
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
            None => {
                cache.live.remove(&u.id);
                vec![]
            }
            // Another machine's work tree cannot be watched: read every time, as it is.
            Some(dir) if u.machine.is_some() => scan::live_in(at, &dir).unwrap_or_else(|e| {
                tracing::warn!("unit {}: {e}", u.id);
                vec![]
            }),
            Some(dir) => {
                // The files its commits changed: whether the work tree put one back is told by
                // its id where its text is not read (`changed_paths`).
                let ends: BTreeSet<String> = match &net {
                    Some((_, files)) => files.iter().map(|f| f.path.clone()).collect(),
                    None => commits.iter().flat_map(|c| c.files.iter().map(|f| f.path.clone())).collect(),
                };
                live_work(at, u, &dir, cache, ends)
            }
        }
    };
    let ms = started.elapsed().as_millis() as u64;
    tracing::info!(target: "perf", kind = "unit", unit = %u.id, machine = u.machine.as_deref().unwrap_or("local"), ms, git = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed) - calls, commits = commits.len(), light = cache.light, slow = ms >= crate::common::config::defaults::SLOW_MS);
    (commits, files, net)
}

/// A local unit's uncommitted work in its work tree `dir`: as read last while the watcher saw
/// nothing change there or in its repository's git folder (`Cache::take_marks`) and that read
/// had what this one needs; else read again, without reading the files known by then again
/// (`scan::live_with`). Their texts (and line counts) for a unit open on the page
/// (`Cache::opened`) and in a build with texts (`code`); names only for the rest of the board,
/// which needs no more, with the ids of those among `ends` (the files its commits changed).
fn live_work(at: &scan::Runner, u: &core::Unit, dir: &Path, cache: &mut Cache, ends: BTreeSet<String>) -> Vec<scan::FileChange> {
    let texts = !cache.bare || cache.opened.contains(&u.id);
    let real = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let followed = cache.follows(&real);
    if let Some(l) = cache.live.get(&u.id).filter(|l| followed && l.dir == dir && !l.stale && (l.texts || !texts)) {
        return l.files.clone();
    }
    let mut l = cache.live.remove(&u.id).filter(|l| l.dir == dir).unwrap_or_default();
    if l.dir != dir {
        let common = scan::common_dir(at, dir).ok().map(|c| c.canonicalize().unwrap_or(c));
        l = Live { dir: dir.to_path_buf(), real, common, ..Default::default() };
    }
    cache.read_live.push(u.id.clone());
    let want = if texts { scan::Want::Texts } else { scan::Want::Names(ends.into_iter().collect()) };
    match scan::live_with(at, dir, &mut l.known, &want) {
        Ok(files) => {
            (l.texts, l.stale) = (texts, false);
            l.files = files.clone();
            cache.live.insert(u.id.clone(), l);
            files
        }
        Err(e) => {
            tracing::warn!("unit {}: {e}", u.id);
            vec![]
        }
    }
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
    scan::Commit { id: c.id.clone(), hash: c.hash.clone(), subject: c.subject.clone(), date: c.date.clone(), author: c.author.clone(), task: c.task.clone(), merge: c.merge, parents: c.parents.clone(), base_merge: c.base_merge, files: c.files.iter().map(scan::FileChange::bare).collect() }
}

/// Which texts a unit's commits, net change and uncommitted files have: its commits' hashes
/// (a commit never changes), its net change's ends (they change with the base, the commits
/// the same) and its uncommitted files' git entries (a mode changed alone changes it) and
/// what stands for their bytes.
pub(super) fn code_of(commits: &[scan::Commit], files: &[scan::FileChange], net: Option<&NetRead>) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for c in commits {
        c.hash.hash(&mut h);
    }
    net.map(|n| &n.0).hash(&mut h);
    // A work-tree file seen here stands for its bytes by its size and time, read or not (names
    // only, or texts), so the token is the same however much of it was read; one over ssh by
    // its text and the id `git add` would give it.
    for f in files {
        (&f.path, &f.status, &f.before_mode, &f.before_id, &f.after_mode).hash(&mut h);
        match f.stamp {
            Some(s) => s.hash(&mut h),
            None => (&f.before, &f.after, &f.after_id).hash(&mut h),
        }
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

/// The files a unit changes, as its code tree shows them with nothing picked (the page's
/// `wholeDelta`): its net change against its base (`net`) with its uncommitted work on top,
/// a file whose entry where the first of them starts differs from its entry where the last
/// ends; without a net change (git could not tell), its commits and uncommitted work taken
/// together (the page's `fileDelta`), a file there at neither end left out. An empty net
/// change is the unit's: its commits do not stand in for it. Uncommitted work's side has no
/// object id unless it was hashed (a file of the commits' read for its name, `live_work`), so
/// its text is compared, as the page compares it; one with neither counts as changed.
pub(super) fn changed_paths(commits: &[&scan::Commit], live: &[scan::FileChange], net: Option<&[scan::FileChange]>) -> BTreeSet<String> {
    let mut ends: BTreeMap<&str, (&scan::FileChange, &scan::FileChange)> = BTreeMap::new();
    let earlier: Vec<&scan::FileChange> = match net {
        Some(net) => net.iter().collect(),
        None => commits.iter().flat_map(|c| &c.files).collect(),
    };
    for f in earlier.into_iter().chain(live) {
        ends.entry(&f.path).and_modify(|e| e.1 = f).or_insert((f, f));
    }
    let gone = |first: &scan::FileChange, last: &scan::FileChange| net.is_none() && first.before_mode == GONE && last.after_mode == GONE;
    ends.into_iter().filter(|(_, (first, last))| !gone(first, last) && !same(first, last)).map(|(p, _)| p.to_string()).collect()
}

/// Whether a file is the same at two ends (the page's `same`): the same mode and object id;
/// for a side git has not hashed (uncommitted work), the same text, when it was read.
fn same(first: &scan::FileChange, last: &scan::FileChange) -> bool {
    let unhashed = last.after_mode != GONE && last.after_id.bytes().all(|b| b == b'0');
    // Without its text (read for its names only, or not readable) it is not known to be the same.
    first.before_mode == last.after_mode && if unhashed { last.after.is_some() && first.before == last.after } else { first.before_id == last.after_id }
}

/// git's mode for a side that does not exist.
const GONE: &str = "000000";
