use std::os::unix::fs::PermissionsExt;

use super::store::*;

#[test]
fn stores_once_per_content_privately() {
    let home = tempfile::tempdir().unwrap();
    let a = id(b"png bytes");
    let pa = put(home.path(), &a, b"png bytes", "Shot.PNG").unwrap();
    let b = id(b"png bytes");
    let pb = put(home.path(), &b, b"png bytes", "other.png").unwrap();
    assert_eq!((a.len(), &a, &pa), (16, &b, &pb), "same content, same file");
    assert_eq!(pa.file_name().unwrap().to_string_lossy(), format!("{a}.png"));
    assert_eq!(std::fs::metadata(&pa).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(crate::common::home::attachments_dir(home.path())).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(usage(home.path()), (1, 9));
    assert_eq!(crate::common::files::mime("a.png", None), "image/png");
    assert_eq!(crate::common::files::mime("report", Some("text/plain")), "text/plain");
}

/// The same content written by several at once (two uploads of one file): every write
/// succeeds, one file is kept, whole, and no temporary file is left.
#[test]
fn the_same_content_at_once_is_stored_once() {
    let home = tempfile::tempdir().unwrap();
    let bytes = vec![7u8; 4 * 1024 * 1024];
    for round in 0..5 {
        let bytes: Vec<u8> = bytes.iter().map(|b| b ^ round).collect();
        let start = std::sync::Barrier::new(8);
        let got: Vec<_> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..8).map(|_| s.spawn(|| { start.wait(); put(home.path(), &id(&bytes), &bytes, "big.bin") })).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let first = got[0].as_ref().unwrap().clone();
        assert!(got.iter().all(|r| r.as_ref().ok() == Some(&first)), "{got:?}");
        assert_eq!(std::fs::read(&first).unwrap(), bytes);
        assert_eq!(std::fs::metadata(&first).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(usage(home.path()), (5, 5 * 4 * 1024 * 1024), "nothing but the stored files");
}

mod target {
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use serde_json::json;
    use sha2::{Digest, Sha256};
    use workriver_core::Target;

    use crate::server::files::target::*;

    fn folder(path: &Path) -> Target {
        serde_json::from_value(json!({ "kind": "folder", "path": path })).unwrap()
    }

    fn id_of(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().take(8).map(|b| format!("{b:02x}")).collect()
    }

    fn in_folder(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_folder_is_checked_and_given_back_canonical() {
        let home = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let canonical = out.path().canonicalize().unwrap();
        // Through a link to it: the canonical path is what is saved.
        let link = tempfile::tempdir().unwrap();
        symlink(out.path(), link.path().join("to")).unwrap();
        assert_eq!(check(&folder(&link.path().join("to")), home.path()), Ok(folder(&canonical)));
        assert_eq!(in_folder(&canonical), Vec::<String>::new(), "the probe is gone");
        let refused = |t: Target, words: &str| {
            let e = check(&t, home.path()).expect_err(words);
            assert!(e.contains(words), "{e}");
        };
        refused(serde_json::from_value(json!({ "kind": "folder", "path": "relative/x" })).unwrap(), "absolute");
        refused(folder(&out.path().join("missing")), "is not there");
        std::fs::write(out.path().join("file"), b"x").unwrap();
        refused(folder(&out.path().join("file")), "not a folder");
        std::fs::create_dir(home.path().join("inside")).unwrap();
        refused(folder(&home.path().join("inside")), "data directory");
        symlink(home.path().join("inside"), out.path().join("into")).unwrap();
        refused(folder(&out.path().join("into")), "data directory");
        refused(folder(home.path()), "data directory");
        refused(serde_json::from_value(json!({ "kind": "folder" })).unwrap(), "path");
        // Never made: a folder that is missing is a drive that is not mounted.
        assert!(!out.path().join("missing").exists());
    }

    #[test]
    fn a_kind_it_does_not_know_is_refused_by_all_three() {
        let home = tempfile::tempdir().unwrap();
        let bucket: Target = serde_json::from_value(json!({ "kind": "bucket", "name": "b" })).unwrap();
        for e in [check(&bucket, home.path()).unwrap_err(), put(&bucket, "0123456789abcdef", "a.png", b"x").unwrap_err(), get(&bucket, "0123456789abcdef", "a.png", 1).unwrap_err()] {
            assert_eq!(e, "no target of kind \"bucket\"");
        }
    }

    #[test]
    fn a_copy_is_verified_and_replaces_a_link_without_following_it() {
        let out = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let bytes = b"picture bytes";
        let id = id_of(bytes);
        std::fs::write(elsewhere.path().join("victim"), b"keep me").unwrap();
        symlink(elsewhere.path().join("victim"), out.path().join(format!("{id}.png"))).unwrap();
        put(&folder(out.path()), &id, "Shot.PNG", bytes).unwrap();
        assert_eq!(std::fs::read(elsewhere.path().join("victim")).unwrap(), b"keep me", "nothing written through the link");
        assert!(!std::fs::symlink_metadata(out.path().join(format!("{id}.png"))).unwrap().file_type().is_symlink());
        assert_eq!(in_folder(out.path()), [format!("{id}.png")], "no temporary file left");
        assert_eq!(get(&folder(out.path()), &id, "Shot.PNG", bytes.len() as u64).unwrap(), bytes);
        // Bytes that are not the id's are not written as it.
        let e = put(&folder(out.path()), "0000000000000000", "a.png", bytes).unwrap_err();
        assert!(e.contains("does not match"), "{e}");
        assert!(!out.path().join("0000000000000000.png").exists());
        // An id not of the store's form names no file.
        assert!(put(&folder(out.path()), "../x", "a.png", bytes).is_err());
    }

    #[test]
    fn a_read_takes_only_a_regular_file_of_the_recorded_size_and_content() {
        let out = tempfile::tempdir().unwrap();
        let bytes = b"report";
        let id = id_of(bytes);
        let t = folder(out.path());
        let at = out.path().join(format!("{id}.txt"));
        let refused = |size: u64, words: &str| {
            let e = get(&t, &id, "r.txt", size).expect_err(words);
            assert!(e.contains(words), "{e}");
        };
        refused(6, "cannot open");
        // A link: not opened.
        std::fs::write(out.path().join("real"), bytes).unwrap();
        symlink(out.path().join("real"), &at).unwrap();
        refused(6, "cannot open");
        std::fs::remove_file(&at).unwrap();
        // A FIFO: answered at once, nothing read.
        let c = std::ffi::CString::new(at.to_string_lossy().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let started = std::time::Instant::now();
        refused(6, "not a regular file");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        std::fs::remove_file(&at).unwrap();
        // A folder.
        std::fs::create_dir(&at).unwrap();
        refused(6, "not a regular file");
        std::fs::remove_dir(&at).unwrap();
        // Another size, or other bytes of that size.
        std::fs::write(&at, b"report, longer").unwrap();
        refused(6, "not the recorded size");
        std::fs::write(&at, b"REPORT").unwrap();
        refused(6, "does not match");
        // Over the store's limit: refused before anything is opened.
        std::fs::write(&at, bytes).unwrap();
        refused(workriver_core::MAX_ATTACHMENT as u64 + 1, "larger than");
        assert_eq!(get(&t, &id, "r.txt", 6).unwrap(), bytes);
    }
}
