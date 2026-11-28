use std::os::unix::net::UnixListener;

use super::*;

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
