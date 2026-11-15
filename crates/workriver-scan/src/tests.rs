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

    let list = commit_list(repo, "main", "feature").unwrap();
    assert_eq!(list.len(), 2);
    let first = commit(repo, &list[0]).unwrap();
    assert_eq!(first.task.as_deref(), Some("u1#1"));
    assert_eq!(first.subject, "feat: two");
    assert_eq!(first.author, "agent-a");
    let a = first.files.iter().find(|f| f.path == "app/a.ts").unwrap();
    assert_eq!((a.status.as_str(), a.before.as_deref(), a.after.as_deref()), ("M", Some("one\n"), Some("one\ntwo\n")));
    let b = first.files.iter().find(|f| f.path == "app/b.ts").unwrap();
    assert_eq!((b.status.as_str(), b.before.as_deref()), ("A", None));

    let second = commit(repo, &list[1]).unwrap();
    assert_eq!(second.task, None, "no trailer: unregistered");
    assert_eq!(second.files[0].status, "D");
    assert_eq!(second.files[0].after, None);

    let mut now = live(repo, "feature").unwrap();
    now.sort_by(|x, y| x.path.cmp(&y.path));
    assert_eq!(now.len(), 2);
    assert_eq!(now[0].after.as_deref(), Some("one\ntwo\nthree\n"));
    assert_eq!((now[1].path.as_str(), now[1].status.as_str()), ("app/c.ts", "A"));
    assert!(live(repo, "main").unwrap().is_empty(), "main is checked out nowhere");
    assert!(commit_list(repo, "main", "not-yet").unwrap().is_empty());

    let t = tree(repo).unwrap();
    assert_eq!((t.files, t.loc), (2, 3), "app/a.ts (2 lines) and app/b.ts (1) at HEAD");
    let app = &t.children.as_ref().unwrap()[0];
    assert_eq!((app.path.as_str(), app.files), ("app", 2));
}
