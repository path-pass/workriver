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

    let list = commit_list(&Runner::Local, repo, "main", "feature").unwrap();
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
    assert!(commit_list(&Runner::Local, repo, "main", "not-yet").unwrap().is_empty());

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
    assert!(args.windows(2).any(|w| w == ["-o", "BatchMode=yes"]), "{args:?}");
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
    let list = commit_list(&at, repo, "main", "f").unwrap();
    assert_eq!(list.len(), 1);
    let c = commit(&at, repo, &list[0]).unwrap();
    assert_eq!((c.task.as_deref(), c.subject.as_str(), c.files[0].path.as_str(), c.files[0].after.as_deref()), (Some("u1#2"), "add \"b c\"", "b c.txt", Some("new\n")));
    let now = live(&at, repo, "f").unwrap();
    assert_eq!((now.len(), now[0].after.as_deref()), (1, Some("one\ntwo\n")), "the uncommitted file is read over ssh too");
    assert_eq!(tree(&at, repo).unwrap().files, 2);
}
