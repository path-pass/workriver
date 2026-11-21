use std::path::Path;
use std::process::Command;

use crate::*;

fn sh(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "agent-a")
        .env("GIT_AUTHOR_EMAIL", "a@example.invalid")
        .env("GIT_COMMITTER_NAME", "agent-a")
        .env("GIT_COMMITTER_EMAIL", "a@example.invalid")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn write(dir: &Path, path: &str, text: &str) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
fn reads_a_units_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    sh(repo, &["init", "-q", "-b", "main"]);
    write(repo, "app/a.ts", "one\n");
    write(repo, "README.md", "hi\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "base"]);
    sh(repo, &["switch", "-q", "-c", "feature"]);
    write(repo, "app/a.ts", "one\ntwo\n");
    write(repo, "app/b.ts", "new\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "feat: two\n\nTask: u1#1"]);
    std::fs::remove_file(repo.join("README.md")).unwrap();
    sh(repo, &["commit", "-q", "-am", "chore: drop readme"]);
    // Uncommitted: a change and an untracked file.
    write(repo, "app/a.ts", "one\ntwo\nthree\n");
    write(repo, "app/c.ts", "draft\n");

    let list: Vec<String> = names(&Runner::Local, repo, "main", "feature").unwrap().into_iter().map(|c| c.hash).collect();
    assert_eq!(list.len(), 2);
    let first = commit(&Runner::Local, repo, &list[0]).unwrap();
    assert_eq!(first.task.as_deref(), Some("u1#1"));
    assert_eq!(first.subject, "feat: two");
    assert_eq!(first.author, "agent-a");
    let a = first.files.iter().find(|f| f.path == "app/a.ts").unwrap();
    assert_eq!((a.status.as_str(), a.before.as_deref(), a.after.as_deref()), ("M", Some("one\n"), Some("one\ntwo\n")));
    let b = first.files.iter().find(|f| f.path == "app/b.ts").unwrap();
    assert_eq!((b.status.as_str(), b.before.as_deref()), ("A", None));

    let second = commit(&Runner::Local, repo, &list[1]).unwrap();
    assert_eq!(second.task, None, "no trailer: unregistered");
    assert_eq!(second.files[0].status, "D");
    assert_eq!(second.files[0].after, None);

    let mut now = live(&Runner::Local, repo, "feature").unwrap();
    now.sort_by(|x, y| x.path.cmp(&y.path));
    assert_eq!(now.len(), 2);
    assert_eq!(now[0].after.as_deref(), Some("one\ntwo\nthree\n"));
    assert_eq!((now[1].path.as_str(), now[1].status.as_str()), ("app/c.ts", "A"));
    assert!(live(&Runner::Local, repo, "main").unwrap().is_empty(), "main is checked out nowhere");
    assert!(names(&Runner::Local, repo, "main", "not-yet").unwrap().is_empty());

    let t = tree(&Runner::Local, repo).unwrap();
    assert_eq!((t.files, t.loc), (2, 3), "app/a.ts (2 lines) and app/b.ts (1) at HEAD");
    let app = &t.children.as_ref().unwrap()[0];
    assert_eq!((app.path.as_str(), app.files), ("app", 2));
}

/// What is handed to ssh: options, the target, and one shell-quoted command line.
#[test]
fn ssh_command_is_quoted_and_never_asks() {
    let at = Runner::ssh("me@box", Path::new("/data/wr"));
    let c = at.command(&["git", "-C", "/srv/it's here", "log", "--format=%h%x00%s"]);
    let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(c.get_program(), "ssh");
    for o in ["BatchMode=yes", "ForwardAgent=no", "StrictHostKeyChecking=yes", "ClearAllForwardings=yes"] {
        assert!(args.windows(2).any(|w| w == ["-o", o]), "{o} in {args:?}");
    }
    assert!(args.contains(&"ControlPath=/data/wr/ssh-%C".to_string()), "{args:?}");
    let n = args.len();
    assert_eq!(&args[n - 3..], ["--", "me@box", r#"'git' '-C' '/srv/it'\''s here' 'log' '--format=%h%x00%s'"#]);
}

/// The same reading through an ssh stand-in that runs the command line in a local shell:
/// a repository in a folder with a space and a quote, a file with a space.
#[test]
fn reads_a_repository_over_ssh() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let fake = tmp.path().join("fake-ssh");
    // Drops the options and the target, as ssh would, and runs the rest in a shell.
    std::fs::write(&fake, "#!/bin/sh\nwhile [ \"$1\" != \"--\" ]; do shift; done\nshift; shift\nexec sh -c \"$1\"\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let repo = tmp.path().join("it's a repo");
    std::fs::create_dir(&repo).unwrap();
    let repo = repo.as_path();
    sh(repo, &["init", "-q", "-b", "main"]);
    write(repo, "a.txt", "one\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "base"]);
    sh(repo, &["switch", "-q", "-c", "f"]);
    write(repo, "b c.txt", "new\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "add \"b c\"\n\nTask: u1#2"]);
    write(repo, "a.txt", "one\ntwo\n");

    let at = Runner::Ssh { program: fake.to_string_lossy().into_owned(), options: vec!["-o".into(), "BatchMode=yes".into()], target: "box".into() };
    assert_eq!(std::fs::canonicalize(toplevel(&at, repo).unwrap()).unwrap(), std::fs::canonicalize(repo).unwrap());
    let list: Vec<String> = names(&at, repo, "main", "f").unwrap().into_iter().map(|c| c.hash).collect();
    assert_eq!(list.len(), 1);
    let c = commit(&at, repo, &list[0]).unwrap();
    assert_eq!((c.task.as_deref(), c.subject.as_str(), c.files[0].path.as_str(), c.files[0].after.as_deref()), (Some("u1#2"), "add \"b c\"", "b c.txt", Some("new\n")));
    let now = live(&at, repo, "f").unwrap();
    assert_eq!((now.len(), now[0].after.as_deref()), (1, Some("one\ntwo\n")), "the uncommitted file is read over ssh too");
    assert_eq!(tree(&at, repo).unwrap().files, 2);
}

/// What `branch` makes of a unit's branch after each way of merging it.
#[test]
fn a_units_commits_survive_merges() {
    let new = |name: &str| {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().to_path_buf();
        sh(&repo, &["init", "-q", "-b", "main"]);
        sh(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
        sh(&repo, &["switch", "-q", "-c", name]);
        for (k, task) in [(1, "Task: u1#1"), (2, ""), (3, "Task: u1#2")] {
            write(&repo, &format!("f{k}.rs"), &format!("{k}\n"));
            sh(&repo, &["add", "."]);
            sh(&repo, &["commit", "-q", "-m", &format!("Step {k}\n\n{task}")]);
        }
        // The mainline moves on meanwhile.
        sh(&repo, &["switch", "-q", "main"]);
        write(&repo, "other.rs", "x\n");
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-q", "-m", "Someone else's work"]);
        (tmp, repo)
    };
    let at = Runner::Local;
    let ours = |c: &Named| c.task.as_deref().is_some_and(|t| t.starts_with("u1#"));
    let subjects = |f: &Found| match f {
        Found::Ahead { commits, .. } | Found::Merged(commits) => commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>().join(","),
        Found::Fresh => "fresh".into(),
        Found::Gone => "gone".into(),
    };
    let fork_of = |f: &Found| match f {
        Found::Ahead { fork, .. } => fork.clone(),
        _ => panic!("{f:?}"),
    };

    // Before any merge: ahead; the fork is kept.
    let (_t, repo) = new("f");
    let before = branch(&at, &repo, "main", "f", None, &ours).unwrap();
    assert!(matches!(before, Found::Ahead { .. }) && subjects(&before) == "Step 1,Step 2,Step 3");
    let fork = fork_of(&before);

    // Merge commit: the branch is all in main; with the fork, and without it (walked back).
    sh(&repo, &["merge", "-q", "--no-ff", "-m", "Merge f", "f"]);
    for fork in [Some(fork.as_str()), None] {
        let f = branch(&at, &repo, "main", "f", fork, &ours).unwrap();
        assert!(matches!(f, Found::Merged(_)) && subjects(&f) == "Step 1,Step 2,Step 3", "{fork:?}: {f:?}");
    }

    // Fast-forward (main had not moved): the same.
    let (_t, repo) = new("f");
    sh(&repo, &["reset", "-q", "--hard", "HEAD~1"]);
    let fork = fork_of(&branch(&at, &repo, "main", "f", None, &ours).unwrap());
    sh(&repo, &["merge", "-q", "--ff-only", "f"]);
    for fork in [Some(fork.as_str()), None] {
        let f = branch(&at, &repo, "main", "f", fork, &ours).unwrap();
        assert!(matches!(f, Found::Merged(_)) && subjects(&f) == "Step 1,Step 2,Step 3", "{fork:?}: {f:?}");
    }

    // Squash merge, branch kept: still ahead, still listed. Branch deleted: gone.
    let (_t, repo) = new("f");
    sh(&repo, &["merge", "-q", "--squash", "f"]);
    sh(&repo, &["commit", "-q", "-m", "Squashed f"]);
    assert_eq!(subjects(&branch(&at, &repo, "main", "f", None, &ours).unwrap()), "Step 1,Step 2,Step 3");
    sh(&repo, &["branch", "-q", "-D", "f"]);
    assert_eq!(branch(&at, &repo, "main", "f", None, &ours).unwrap(), Found::Gone);

    // Rebased onto a new branch, which is then merged: found on the new branch.
    let (_t, repo) = new("f");
    sh(&repo, &["switch", "-q", "-c", "f2", "main"]);
    sh(&repo, &["cherry-pick", "f~2", "f~1", "f"]);
    let fork = fork_of(&branch(&at, &repo, "main", "f2", None, &ours).unwrap());
    sh(&repo, &["switch", "-q", "main"]);
    sh(&repo, &["merge", "-q", "--ff-only", "f2"]);
    for fork in [Some(fork.as_str()), None] {
        let f = branch(&at, &repo, "main", "f2", fork, &ours).unwrap();
        assert!(matches!(f, Found::Merged(_)) && subjects(&f) == "Step 1,Step 2,Step 3", "{fork:?}: {f:?}");
    }

    // A branch just made from its base: nothing yet, not merged.
    let (_t, repo) = new("f");
    sh(&repo, &["branch", "fresh", "main"]);
    assert_eq!(branch(&at, &repo, "main", "fresh", None, &ours).unwrap(), Found::Fresh);
    let fork = branch(&at, &repo, "main", "main", Some("main"), &ours).unwrap();
    assert_eq!(fork, Found::Fresh, "its tip is its fork");
}

/// A local base behind its upstream: a branch made from the newer `origin/main` holds only
/// its own commits, not the teammates' ones in between; merged upstream, it is merged.
#[test]
fn a_stale_local_base_counts_its_upstream() {
    let tmp = tempfile::tempdir().unwrap();
    let (origin, work, other) = (tmp.path().join("origin.git"), tmp.path().join("work"), tmp.path().join("other"));
    for d in [&origin, &work, &other] {
        std::fs::create_dir(d).unwrap();
    }
    sh(&origin, &["init", "-q", "--bare", "-b", "main"]);
    sh(&work, &["init", "-q", "-b", "main"]);
    sh(&work, &["commit", "-q", "--allow-empty", "-m", "base"]);
    sh(&work, &["remote", "add", "origin", &origin.to_string_lossy()]);
    sh(&work, &["push", "-q", "-u", "origin", "main"]);
    // A teammate pushes to main; here only origin/main learns of it.
    sh(&other, &["clone", "-q", &origin.to_string_lossy(), "."]);
    for s in ["fix(deps): anyio", "chore(security): pin"] {
        sh(&other, &["commit", "-q", "--allow-empty", "-m", s]);
    }
    sh(&other, &["push", "-q", "origin", "main"]);
    sh(&work, &["fetch", "-q", "origin"]);
    sh(&work, &["switch", "-q", "-c", "feat/x", "origin/main"]);
    sh(&work, &["commit", "-q", "--allow-empty", "-m", "Mine\n\nTask: u1#1"]);

    let at = Runner::Local;
    let ours = |c: &Named| c.task.as_deref() == Some("u1#1");
    let subjects = |l: &[Named]| l.iter().map(|c| c.subject.clone()).collect::<Vec<_>>();
    assert_eq!(subjects(&names(&at, &work, "main", "feat/x").unwrap()), ["Mine"]);
    let Found::Ahead { commits, fork } = branch(&at, &work, "main", "feat/x", None, &ours).unwrap() else { panic!("ahead") };
    assert_eq!(subjects(&commits), ["Mine"]);
    // A base without an upstream (a remote branch itself) reads as before.
    assert_eq!(subjects(&names(&at, &work, "origin/main", "feat/x").unwrap()), ["Mine"]);

    // Merged upstream, the local main still stale: merged, not ahead.
    sh(&work, &["push", "-q", "origin", "feat/x:main"]);
    sh(&work, &["fetch", "-q", "origin"]);
    for fork in [Some(fork.as_str()), None] {
        let f = branch(&at, &work, "main", "feat/x", fork, &ours).unwrap();
        assert!(matches!(&f, Found::Merged(c) if subjects(c) == ["Mine"]), "{fork:?}: {f:?}");
    }
}

/// A squash merge git cannot show: every file the branch changed is the same in the base (or
/// its upstream), named by the commit that brought it; a part of it there is not enough; the
/// base moving on elsewhere still shows it, changing one of the same files again hides it.
#[test]
fn a_squash_merge_looks_merged() {
    let new = || {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().to_path_buf();
        sh(&repo, &["init", "-q", "-b", "main"]);
        write(&repo, "shared.rs", "a\n");
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-q", "-m", "base"]);
        sh(&repo, &["switch", "-q", "-c", "f"]);
        write(&repo, "f1.rs", "1\n");
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-q", "-m", "One\n\nTask: u1#1"]);
        write(&repo, "shared.rs", "a\nb\n");
        sh(&repo, &["commit", "-q", "-am", "Two\n\nTask: u1#1"]);
        sh(&repo, &["switch", "-q", "main"]);
        write(&repo, "other.rs", "x\n");
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-q", "-m", "Someone else's work"]);
        (tmp, repo)
    };
    let at = Runner::Local;
    let fork = |repo: &Path| match branch(&at, repo, "main", "f", None, &|_| false).unwrap() {
        Found::Ahead { fork, .. } => fork,
        f => panic!("{f:?}"),
    };
    let head = |repo: &Path, rev: &str| resolve(&at, repo, rev).unwrap();

    let (_t, repo) = new();
    let k = fork(&repo);
    assert_eq!(squashed(&at, &repo, "main", "f", &k).unwrap(), None, "not merged");
    // Part of it: not merged.
    sh(&repo, &["checkout", "f", "--", "f1.rs"]);
    sh(&repo, &["commit", "-q", "-m", "Only f1"]);
    assert_eq!(squashed(&at, &repo, "main", "f", &k).unwrap(), None, "partly merged");
    // The rest by squash: merged, by that commit; the branch is still ahead by git's account.
    sh(&repo, &["merge", "-q", "--squash", "f"]);
    sh(&repo, &["commit", "-q", "-m", "Squashed f"]);
    let squash = head(&repo, "main");
    assert!(matches!(branch(&at, &repo, "main", "f", None, &|_| false).unwrap(), Found::Ahead { .. }));
    assert_eq!(squashed(&at, &repo, "main", "f", &k).unwrap(), Some(("main".into(), squash.clone())));
    // The base moves on in other files: still.
    write(&repo, "later.rs", "y\n");
    sh(&repo, &["add", "."]);
    sh(&repo, &["commit", "-q", "-m", "Later"]);
    assert_eq!(squashed(&at, &repo, "main", "f", &k).unwrap(), Some(("main".into(), squash)));
    // The base changes one of its files again: no longer told by content.
    write(&repo, "shared.rs", "a\nb\nc\n");
    sh(&repo, &["commit", "-q", "-am", "More"]);
    assert_eq!(squashed(&at, &repo, "main", "f", &k).unwrap(), None);

    // Squashed into the base's upstream only (merged on the remote, local main behind).
    let (_t, repo) = new();
    let k = fork(&repo);
    sh(&repo, &["switch", "-q", "-c", "up"]);
    sh(&repo, &["merge", "-q", "--squash", "f"]);
    sh(&repo, &["commit", "-q", "-m", "Squashed f upstream"]);
    sh(&repo, &["branch", "-q", "--set-upstream-to=up", "main"]);
    assert_eq!(squashed(&at, &repo, "main", "f", &k).unwrap(), Some(("up".into(), head(&repo, "up"))));
}

/// Many commits read at once are what reading them one by one gives: additions, deletions,
/// a merge (against its first parent), the root commit, a binary file.
#[test]
fn commits_read_at_once_as_one_by_one() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    sh(repo, &["init", "-q", "-b", "main"]);
    write(repo, "a.txt", "one\n");
    write(repo, "b.txt", "bee\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "root"]);
    sh(repo, &["switch", "-q", "-c", "f"]);
    write(repo, "a.txt", "one\ntwo\n");
    std::fs::remove_file(repo.join("b.txt")).unwrap();
    std::fs::write(repo.join("bin.dat"), [0u8, 1, 2, 0]).unwrap();
    write(repo, "dir/c.txt", "new\n");
    sh(repo, &["add", "-A"]);
    sh(repo, &["commit", "-q", "-m", "Change things\n\nTask: u1#2"]);
    sh(repo, &["switch", "-q", "main"]);
    write(repo, "m.txt", "main\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "On main"]);
    sh(repo, &["switch", "-q", "f"]);
    sh(repo, &["merge", "-q", "--no-ff", "-m", "Merge main", "main"]);
    let at = Runner::Local;
    let all: Vec<String> = String::from_utf8(Command::new("git").arg("-C").arg(repo).args(["rev-list", "--reverse", "f"]).output().unwrap().stdout).unwrap().lines().map(str::to_string).collect();
    assert_eq!(all.len(), 4);
    let one_by_one: Vec<Commit> = all.iter().map(|h| commit(&at, repo, h).unwrap()).collect();
    let at_once = commits(&at, repo, &all).unwrap();
    for (a, b) in one_by_one.iter().zip(&at_once) {
        assert_eq!((&a.hash, &a.subject, &a.author, &a.date, &a.task, a.merge), (&b.hash, &b.subject, &b.author, &b.date, &b.task, b.merge));
        assert_eq!(b.merge, b.subject == "Merge main", "{}", b.subject);
        assert!(b.id.len() >= 9 && a.hash.starts_with(&b.id));
        let paths = |c: &Commit| c.files.iter().map(|f| (f.path.clone(), f.status.clone(), f.before.clone(), f.after.clone())).collect::<Vec<_>>();
        assert_eq!(paths(a), paths(b), "{}", a.subject);
    }
    let mut sizes: Vec<usize> = at_once.iter().map(|c| c.files.len()).collect();
    sizes.sort();
    assert_eq!(sizes, [1, 1, 2, 3], "root: two; change: a, b, c (the binary left out); main: m; merge: m against its first parent");
    // Patch ids at once are the ones one by one.
    let ids = patch_ids(&at, repo, &all).unwrap();
    for h in &all {
        assert_eq!(ids.get(h).cloned().unwrap_or_default(), patch_id(&at, repo, h).unwrap(), "{h}");
    }
    let r = refs(&at, repo).unwrap();
    assert_eq!(tip(&r, "f"), Some(all[3].as_str()));
    assert_eq!(tip(&r, &all[0]), Some(all[0].as_str()));
    assert_eq!(tip(&r, "nope"), None);
}

/// Lines added and removed as the web UI's diff counts them (a change, an add, a last line
/// that gains its newline).
#[test]
fn line_counts() {
    assert_eq!(counts("a\nb\nc\n", "a\nB\nc\nd\n"), (2, 1));
    assert_eq!(counts("", "x\ny\n"), (2, 0));
    assert_eq!(counts("a", "a\n"), (1, 1));
    assert_eq!(counts("same\n", "same\n"), (0, 0));
}

/// Each file change carries both sides' git entry, as `--raw` names it (mode and object id;
/// "000000" and zeros for a side that does not exist), read at once or one by one alike: a
/// mode change, a file made a symlink, a submodule pointer (its commit not in the repository),
/// an add and a delete; uncommitted work has the work tree's mode and an id git has not
/// worked out (zeros).
#[test]
fn file_changes_carry_their_git_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let id = |rev: &str| String::from_utf8(Command::new("git").arg("-C").arg(repo).args(["rev-parse", rev]).output().unwrap().stdout).unwrap().trim().to_string();
    let (gone, zero) = ("000000", "0".repeat(40));
    let (pointer_a, pointer_b) = ("1111111111111111111111111111111111111111", "2222222222222222222222222222222222222222");
    sh(repo, &["init", "-q", "-b", "main"]);
    write(repo, "run.sh", "echo hi\n");
    write(repo, "link", "text\n");
    write(repo, "old.txt", "old\n");
    sh(repo, &["add", "."]);
    sh(repo, &["update-index", "--add", "--cacheinfo", &format!("160000,{pointer_a},sub")]);
    sh(repo, &["commit", "-q", "-m", "base"]);
    let (run, link, old) = (id("HEAD:run.sh"), id("HEAD:link"), id("HEAD:old.txt"));
    sh(repo, &["update-index", "--chmod=+x", "run.sh"]);
    std::fs::remove_file(repo.join("link")).unwrap();
    std::os::unix::fs::symlink("run.sh", repo.join("link")).unwrap();
    sh(repo, &["rm", "-q", "old.txt"]);
    write(repo, "new.txt", "new\n");
    sh(repo, &["add", "link", "new.txt"]);
    sh(repo, &["update-index", "--cacheinfo", &format!("160000,{pointer_b},sub")]);
    sh(repo, &["commit", "-q", "-m", "change"]);
    let (to, new) = (id("HEAD:link"), id("HEAD:new.txt"));
    let head = id("HEAD");
    let entries = |c: &Commit| {
        let mut e: Vec<_> = c.files.iter().map(|f| (f.path.clone(), f.before_mode.clone(), f.before_id.clone(), f.after_mode.clone(), f.after_id.clone())).collect();
        e.sort();
        e
    };
    let s = |x: &str| x.to_string();
    let want = vec![
        (s("link"), s("100644"), link, s("120000"), to),
        (s("new.txt"), s(gone), zero.clone(), s("100644"), new),
        (s("old.txt"), s("100644"), old, s(gone), zero.clone()),
        (s("run.sh"), s("100644"), run.clone(), s("100755"), run.clone()),
        (s("sub"), s("160000"), s(pointer_a), s("160000"), s(pointer_b)),
    ];
    assert_eq!(entries(&commits(&Runner::Local, repo, std::slice::from_ref(&head)).unwrap()[0]), want, "at once");
    assert_eq!(entries(&commit(&Runner::Local, repo, &head).unwrap()), want, "one by one");

    // The work tree as committed (executable, the submodule unpopulated), then changed.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(repo.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir(repo.join("sub")).unwrap();
    write(repo, "run.sh", "echo bye\n");
    write(repo, "draft.txt", "draft\n");
    let mut now: Vec<_> = live_in(&Runner::Local, repo).unwrap().into_iter().map(|f| (f.path, f.before_mode, f.before_id, f.after_mode, f.after_id)).collect();
    now.sort();
    assert_eq!(now, vec![(s("draft.txt"), s(gone), zero.clone(), s("100644"), zero.clone()), (s("run.sh"), s("100755"), run, s("100755"), zero)]);
}

/// An uncommitted symlink's text is where it points, not what the file there holds: a link
/// moved from old-target to new-target, whose file holds the text "old-target", still changed.
#[test]
fn an_uncommitted_symlink_is_its_target() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    sh(repo, &["init", "-q", "-b", "main"]);
    write(repo, "old-target", "original payload\n");
    write(repo, "new-target", "old-target");
    std::os::unix::fs::symlink("old-target", repo.join("link.txt")).unwrap();
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "base"]);
    std::fs::remove_file(repo.join("link.txt")).unwrap();
    std::os::unix::fs::symlink("new-target", repo.join("link.txt")).unwrap();
    let now: Vec<_> = live_in(&Runner::Local, repo).unwrap().into_iter().map(|f| (f.path, f.before, f.after, f.after_mode)).collect();
    let s = |x: &str| x.to_string();
    assert_eq!(now, vec![(s("link.txt"), Some(s("old-target")), Some(s("new-target")), s("120000"))]);
}

/// What `git diff --no-renames --name-only` gives, sorted.
fn git_names(repo: &Path, args: &[&str]) -> Vec<String> {
    let out = Command::new("git").arg("-C").arg(repo).args(["diff", "--no-renames", "--name-only"]).args(args).output().unwrap().stdout;
    let mut names: Vec<String> = String::from_utf8(out).unwrap().lines().map(str::to_string).collect();
    names.sort();
    names
}

fn rev(repo: &Path, r: &str) -> String {
    String::from_utf8(Command::new("git").arg("-C").arg(repo).args(["rev-parse", r]).output().unwrap().stdout).unwrap().trim().to_string()
}

/// The branch's change against its base is from where they meet, as `git diff base...tip`:
/// a base that reached the branch through another branch (made from a newer base) is not
/// the branch's change, while that branch's own work is; a merge of the base itself brought
/// the base in, a merge of the other branch did not.
#[test]
fn the_change_is_from_where_the_branch_meets_its_base() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let at = Runner::Local;
    sh(repo, &["init", "-q", "-b", "main"]);
    write(repo, "root.txt", "root\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "root"]);
    sh(repo, &["switch", "-q", "-c", "feature"]);
    write(repo, "feature.txt", "own\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "Own\n\nTask: u1#1"]);
    sh(repo, &["switch", "-q", "main"]);
    write(repo, "base-only.txt", "new base file\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "Advance base"]);
    sh(repo, &["switch", "-q", "-c", "side"]);
    write(repo, "side.txt", "side work\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "Side work"]);
    sh(repo, &["switch", "-q", "feature"]);
    sh(repo, &["merge", "-q", "--no-ff", "-m", "Bring side with a newer base\n\nTask: u1#2", "side"]);
    let side_merge = rev(repo, "HEAD");
    let base_at = merge_base(&at, repo, "main", "feature").unwrap().unwrap();
    assert_eq!(base_at, rev(repo, "main"), "they meet where side left main");
    let names = |from: &str| {
        let mut n: Vec<String> = diff(&at, repo, from, "feature").unwrap().into_iter().map(|f| f.path).collect();
        n.sort();
        n
    };
    assert_eq!(names(&base_at), git_names(repo, &["main...feature"]));
    assert_eq!(names(&base_at), ["feature.txt", "side.txt"]);
    let read = commits(&at, repo, std::slice::from_ref(&side_merge)).unwrap();
    assert!(read[0].merge);
    assert!(!brings_base(&at, repo, &read[0], &base_at).unwrap(), "side is not the base");
    // The base itself merged in.
    sh(repo, &["switch", "-q", "main"]);
    write(repo, "base-two.txt", "two\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "Base two"]);
    sh(repo, &["switch", "-q", "feature"]);
    sh(repo, &["merge", "-q", "--no-ff", "-m", "Sync\n\nTask: u1#3", "main"]);
    let sync = commits(&at, repo, &[rev(repo, "HEAD")]).unwrap().remove(0);
    let base_at = merge_base(&at, repo, "main", "feature").unwrap().unwrap();
    assert!(brings_base(&at, repo, &sync, &base_at).unwrap());
    assert!(!brings_base(&at, repo, &read[0], &base_at).unwrap());
    assert_eq!(names(&base_at), git_names(repo, &["main...feature"]));
    // The base's upstream ahead of it and merged into the branch: they meet there.
    sh(repo, &["switch", "-q", "-c", "up", "main"]);
    write(repo, "up.txt", "up\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "Upstream"]);
    sh(repo, &["branch", "-q", "--set-upstream-to=up", "main"]);
    sh(repo, &["switch", "-q", "feature"]);
    sh(repo, &["merge", "-q", "--no-ff", "-m", "Upstream in", "up"]);
    assert_eq!(merge_base(&at, repo, "main", "feature").unwrap().unwrap(), rev(repo, "up"));
}

/// A branch merged into its base meets it, as before that merge, where it did then: a merge
/// of another branch it holds stays that branch's work once the base has it too. Merged by a
/// fast-forward, where the base stood before is not known.
#[test]
fn a_merged_branch_meets_its_base_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let at = Runner::Local;
    sh(repo, &["init", "-q", "-b", "main"]);
    write(repo, "root.txt", "root\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "root"]);
    sh(repo, &["switch", "-q", "-c", "feature"]);
    write(repo, "feature.txt", "own\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "Own\n\nTask: u1#1"]);
    sh(repo, &["switch", "-q", "-c", "side", "main"]);
    write(repo, "side.txt", "side work\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "Side"]);
    sh(repo, &["switch", "-q", "feature"]);
    sh(repo, &["merge", "-q", "--no-ff", "-m", "Bring side in\n\nTask: u1#2", "side"]);
    let tip = rev(repo, "feature");
    let side_merge = commits(&at, repo, std::slice::from_ref(&tip)).unwrap().remove(0);
    let before = merge_base(&at, repo, "main", "feature").unwrap().unwrap();
    assert!(!brings_base(&at, repo, &side_merge, &before).unwrap());
    assert_eq!(base_before(&at, repo, "main", &tip).unwrap(), None, "not merged yet");
    sh(repo, &["switch", "-q", "main"]);
    sh(repo, &["merge", "-q", "--no-ff", "-m", "Merge the unit", "feature"]);
    // Now the base has side too; as it stood before, it did not.
    assert!(brings_base(&at, repo, &side_merge, &merge_base(&at, repo, "main", &tip).unwrap().unwrap()).unwrap());
    let then = base_before(&at, repo, "main", &tip).unwrap().unwrap();
    assert_eq!(then, before);
    assert!(!brings_base(&at, repo, &side_merge, &then).unwrap());
    // A fast-forward leaves no trace of where the base stood.
    sh(repo, &["switch", "-q", "-c", "ff", "main"]);
    write(repo, "more.txt", "more\n");
    sh(repo, &["add", "."]);
    sh(repo, &["commit", "-q", "-m", "More"]);
    sh(repo, &["switch", "-q", "main"]);
    sh(repo, &["merge", "-q", "--ff-only", "ff"]);
    assert_eq!(base_before(&at, repo, "main", &rev(repo, "ff")).unwrap(), None);
}
