use std::cell::Cell;
use std::os::unix::net::UnixListener;
use std::sync::{TryLockError, mpsc};
use std::thread;

use super::*;

thread_local! {
    static BEFORE_CANCEL_WRITE: Cell<Option<Box<dyn FnOnce()>>> = const { Cell::new(None) };
}

pub(super) fn before_cancel_write(method: &str) {
    if method == "cancel" && let Some(pause) = BEFORE_CANCEL_WRITE.take() {
        pause();
    }
}

fn request(stream: &mut BufReader<UnixStream>, method: &str) -> Value {
    let mut line = String::new();
    assert_ne!(stream.read_line(&mut line).unwrap(), 0);
    let request: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(request["method"], method);
    request["id"].clone()
}

fn reply(stream: &mut BufReader<UnixStream>, id: Value, result: Value) {
    writeln!(stream.get_mut(), "{}", json!({"jsonrpc":"2.0", "id":id, "result":result})).unwrap();
}

#[test]
fn cancel_orders_the_recheck_and_host_write_against_reseating() {
    let tmp = tempfile::tempdir().unwrap();
    let hosts = Hosts::new(tmp.path());
    let agent = "a1";
    let (stream, peer) = UnixStream::pair().unwrap();
    let wait = Duration::from_secs(5);
    peer.set_read_timeout(Some(wait)).unwrap();
    hosts.track(agent, 0, 0);
    let connection = hosts.connection(agent).unwrap();
    *connection.conn.lock().unwrap() = Some(Conn { stream: BufReader::new(stream), next: 0, epoch: 1 });

    let order = &Mutex::new(());
    let (at_write, writing) = mpsc::channel();
    let (allow_write, resume) = mpsc::channel();
    thread::scope(|scope| {
        let cancel = scope.spawn(move || {
            BEFORE_CANCEL_WRITE.set(Some(Box::new(move || {
                at_write.send(()).unwrap();
                resume.recv_timeout(wait).expect("cancel write was never released");
            })));
            let mut checked_under_order = None;
            let result = hosts.cancel(agent, order, || {
                checked_under_order = Some(matches!(order.try_lock(), Err(TryLockError::WouldBlock)));
                Ok(7)
            }, |error| error);
            (checked_under_order, result)
        });

        writing.recv_timeout(wait).expect("cancel did not reach the host write");
        // Reseating needs the same lock before it can commit a seat change.
        let reseat_blocked = matches!(order.try_lock(), Err(TryLockError::WouldBlock));
        allow_write.send(()).unwrap();

        let mut peer = BufReader::new(peer);
        let mut line = String::new();
        peer.read_line(&mut line).unwrap();
        let frame: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(frame["method"], "cancel");
        assert_eq!(frame["params"], json!({"generation": 7}));
        reply(&mut peer, frame["id"].clone(), json!({"state": "cancelled"}));

        let (checked_under_order, result) = cancel.join().unwrap();
        assert!(reseat_blocked, "reseating could acquire the ordering lock before the cancel write");
        assert_eq!(checked_under_order, Some(true), "the recheck must hold the ordering lock");
        assert_eq!(result.unwrap(), "cancelled");
    });
}

#[test]
fn a_lost_reply_drops_the_epoch_and_a_new_connection_has_its_own() {
    let tmp = tempfile::Builder::new().prefix("wr-acp-").tempdir_in("/tmp").unwrap();
    let hosts = Hosts::new(tmp.path());
    let agent = "a1";
    let dir = hosts.dir(agent);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("host.key"), [1; 32]).unwrap();

    let listener = UnixListener::bind(dir.join("host.sock")).unwrap();
    let server = std::thread::spawn(move || {
        let ready = json!({"protocolVersion":HOST_PROTOCOL, "state":"running", "generation":1, "session":"one"});
        let mut first = BufReader::new(listener.accept().unwrap().0);
        let id = request(&mut first, "hello");
        reply(&mut first, id, ready.clone());
        request(&mut first, "answer");
        drop(first);

        let mut second = BufReader::new(listener.accept().unwrap().0);
        let id = request(&mut second, "hello");
        reply(&mut second, id, ready);
        let id = request(&mut second, "status");
        reply(&mut second, id, json!({"state":"idle", "generation":1, "head":3, "pending":{}}));
    });

    let me = process::one(std::process::id()).unwrap();
    hosts.track(agent, me.pid, me.start);
    assert_eq!(hosts.connection_epoch(agent), None);
    let (first, hello) = hosts.hello(agent).unwrap();
    assert_eq!(hello.session.as_deref(), Some("one"));
    assert_eq!(hosts.connection_epoch(agent), Some(first));
    assert!(matches!(hosts.answer(agent, 1, 2, Answer::Allow), Err(HostError::Unreachable(_))));
    assert_eq!(hosts.connection_epoch(agent), None);
    assert_eq!(hosts.status(agent).unwrap().head, 3);
    assert!(hosts.connection_epoch(agent).unwrap() > first);
    server.join().unwrap();
}
