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
