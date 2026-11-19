use std::sync::Arc;
use std::time::Duration;

use workriver_core::runtime::Root;

use super::caller::*;
use super::pins::*;
use crate::cli::fetch;
use crate::common::home::SOCKET;
use crate::common::process::{Proc, System};

fn p(pid: u32, parent: u32, tty: Option<&str>) -> Proc {
    Proc { pid, parent, tty: tty.map(str::to_string), start: 7, name: format!("n{pid}") }
}

fn root(place: &str, shell: Option<u32>, front: &[u32]) -> Root {
    Root { place: place.into(), shell, front: front.to_vec(), at: Default::default() }
}

#[test]
fn a_process_is_at_the_place_whose_root_it_descends_from() {
    let roots = [root("p1", Some(10), &[]), root("p2", Some(20), &[21]), root("p3", None, &[30, 31])];
    let none = |_: u32| None;
    let at = |c: &[Proc]| place_of(c, &roots, &none).map(|r| r.place.as_str());
    assert_eq!(at(&[p(99, 21, None), p(21, 20, None), p(20, 1, None), p(1, 0, None)]), Some("p2"), "a grandchild of its shell");
    assert_eq!(at(&[p(10, 1, None)]), Some("p1"), "the shell itself");
    assert_eq!(at(&[p(99, 31, None), p(31, 1, None)]), Some("p3"), "no shell known: what is in front is the root");
    assert_eq!(at(&[p(99, 21, None), p(21, 1, None)]), None, "what is in front of a known shell is not a root");
    assert_eq!(at(&[p(99, 5, None), p(5, 1, None)]), None);
    assert_eq!(at(&[]), None);
    // A place inside a place: the nearest root.
    let nested = [root("outer", Some(20), &[]), root("inner", Some(21), &[])];
    assert_eq!(place_of(&[p(99, 21, None), p(21, 20, None), p(20, 1, None)], &nested, &none).map(|r| r.place.as_str()), Some("inner"));
    // A job left behind (`( cmd & )`) keeps its terminal; one that left its session has none.
    let tty = |pid: u32| (pid == 10).then(|| "ttys022".to_string());
    assert_eq!(place_of(&[p(99, 1, Some("ttys022")), p(1, 0, None)], &roots, &tty).map(|r| r.place.as_str()), Some("p1"));
    assert_eq!(place_of(&[p(99, 1, Some("ttys000")), p(1, 0, None)], &roots, &tty).map(|r| r.place.as_str()), None);
    assert_eq!(place_of(&[p(99, 1, None), p(1, 0, None)], &roots, &tty).map(|r| r.place.as_str()), None);
    // Descent comes before the terminal.
    assert_eq!(place_of(&[p(99, 20, Some("ttys022")), p(20, 1, None)], &roots, &tty).map(|r| r.place.as_str()), Some("p2"));
}

#[test]
fn a_proof_stands_for_its_chain_while_it_lasts() {
    let callers = Callers::default();
    let chain = vec![p(3, 2, None)];
    let proof = callers.admit(chain.clone()).unwrap();
    assert_eq!(proof.len(), 32);
    assert_ne!(callers.admit(chain.clone()).unwrap(), proof);
    assert_eq!(callers.chain(&proof), Some(chain.clone()));
    assert_eq!(callers.chain("no-such-proof"), None);
    callers.set_ttl(Duration::from_millis(30));
    let short = callers.admit(chain.clone()).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(callers.chain(&short), None, "expired");
    assert!(callers.chain(&proof).is_some(), "the earlier one still stands");
}

/// A real socket: the proof a process is given stands for its own chain, as the kernel
/// names it. (A real child that connects by itself: tests/caller.rs, with the binary.)
#[test]
fn the_kernel_says_which_process_connected() {
    let home = tempfile::tempdir().unwrap();
    let callers = Arc::new(Callers::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    {
        let _in = rt.enter();
        listen(home.path(), callers.clone(), Arc::new(System)).unwrap();
    }
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(home.path().join(SOCKET)).unwrap().permissions().mode() & 0o777, 0o600);
    // This process.
    let mine = callers.chain(&fetch(home.path()).expect("a proof")).unwrap();
    assert_eq!(mine[0].pid, std::process::id());
    // No service there: no proof.
    assert_eq!(fetch(&home.path().join("nowhere")), None);
    // A path a socket cannot have is said at start.
    let long = home.path().join("x".repeat(120));
    std::fs::create_dir(&long).unwrap();
    let _in = rt.enter();
    let err = listen(&long, callers, Arc::new(System)).unwrap_err().to_string();
    assert!(err.contains("a Unix socket's path takes") && err.contains("WORKRIVER_HOME"), "{err}");
}

#[test]
fn pins_outlive_a_restart() {
    let home = tempfile::tempdir().unwrap();
    let pin = Pin { machine: None, place: "w1:p2".into(), pid: 28023, start: 1795075812053486 };
    let far = Pin { machine: Some("box".into()), place: "w1:p1".into(), pid: 7, start: 9 };
    let pins = Pins::open(home.path());
    assert_eq!(pins.get("SESS-A"), None);
    pins.change(vec![("SESS-A".into(), Some(pin.clone())), ("SESS-B".into(), Some(far.clone())), ("SESS-C".into(), Some(far.clone()))]);
    pins.change(vec![("SESS-C".into(), None)]);
    let again = Pins::open(home.path());
    assert_eq!((again.get("SESS-A"), again.get("SESS-B"), again.get("SESS-C")), (Some(pin), Some(far), None));
    assert_eq!(again.all().len(), 2);
}
