use std::os::unix::fs::PermissionsExt;

use super::store::*;

#[test]
fn stores_once_per_content_privately() {
    let home = tempfile::tempdir().unwrap();
    let (a, pa) = put(home.path(), b"png bytes", "Shot.PNG").unwrap();
    let (b, pb) = put(home.path(), b"png bytes", "other.png").unwrap();
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
            let hs: Vec<_> = (0..8).map(|_| s.spawn(|| { start.wait(); put(home.path(), &bytes, "big.bin") })).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let first = got[0].as_ref().unwrap().clone();
        assert!(got.iter().all(|r| r.as_ref().ok() == Some(&first)), "{got:?}");
        assert_eq!(std::fs::read(&first.1).unwrap(), bytes);
        assert_eq!(std::fs::metadata(&first.1).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(usage(home.path()), (5, 5 * 4 * 1024 * 1024), "nothing but the stored files");
}
