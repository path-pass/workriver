use std::io::Write;

use serde_json::json;
use tempfile::tempdir;

use super::*;

fn read_page(journal: &Journal, after: u64, limit: usize) -> Result<Value> {
    let mut bytes = Vec::new();
    journal.read(after, limit, &mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn prompt(key: &str, text: &str) -> Value {
    json!({
        "kind": "acp", "dir": "out", "key": key,
        "msg": {
            "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
            "params": {"sessionId": "thread", "prompt":[{"type":"text", "text":text}]}
        }
    })
}

fn approval(command: String) -> Value {
    json!({
        "kind": "acp", "dir": "in",
        "msg": {
            "jsonrpc":"2.0", "id":0, "method":"session/request_permission",
            "params": {
                "sessionId": "thread",
                "toolCall": {"toolCallId":"item", "rawInput":{"command":command}},
                "options": [{"optionId":"reject", "name":"Reject", "kind":"reject_once"}]
            }
        }
    })
}

#[test]
fn replay_recovers_durable_keys_before_a_torn_tail() {
    let dir = tempdir().unwrap();
    let mut journal = Journal::open(dir.path(), 1).unwrap();
    assert_eq!(journal.append(prompt("delivery", "hello"), true).unwrap(), 1);
    assert_eq!(journal.deliveries().get("delivery"), Some(&1));
    drop(journal);

    let path = dir.path().join("journal.jsonl");
    let length = std::fs::metadata(&path).unwrap().len();
    OpenOptions::new().append(true).open(&path).unwrap().write_all(br#"{"seq":2"#).unwrap();

    let mut journal = Journal::open(dir.path(), 2).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), length);
    assert_eq!(journal.deliveries().get("delivery"), Some(&1));
    assert!(journal.append(prompt("delivery", "changed"), true).is_err());
    assert_eq!(journal.append(prompt("another", "next"), true).unwrap(), 2);
    let all = read_page(&journal, 0, 200).unwrap();
    assert_eq!(all["head"], 2);
    assert_eq!(all["entries"][0]["gen"], 1);
    assert_eq!(all["entries"][1]["gen"], 2);
    assert_eq!(all["entries"][0]["msg"]["params"]["prompt"][0]["text"], "hello");
    humantime::parse_rfc3339(all["entries"][0]["at"].as_str().unwrap()).unwrap();
}

#[test]
fn a_completed_corrupt_line_is_never_discarded() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("journal.jsonl");
    std::fs::write(&path, b"{broken}\n").unwrap();
    assert!(Journal::open(dir.path(), 1).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"{broken}\n");
}

#[test]
fn an_append_error_retires_the_writer_without_advancing_its_receipts() {
    let dir = tempdir().unwrap();
    let mut journal = Journal::open(dir.path(), 1).unwrap();
    let writable = std::mem::replace(&mut journal.file, File::open(&journal.path).unwrap());
    assert!(journal.append(prompt("failed", "not written"), true).is_err());
    assert_eq!(journal.head(), 0);
    assert!(journal.deliveries().is_empty());
    journal.file = writable;
    assert!(journal.append(prompt("retry", "still refused"), true).is_err());
    assert_eq!(std::fs::metadata(&journal.path).unwrap().len(), 0);

    drop(journal);
    let mut journal = Journal::open(dir.path(), 2).unwrap();
    assert_eq!(journal.append(prompt("retry", "written"), true).unwrap(), 1);
}

#[test]
fn pages_stop_at_encoded_byte_budget_and_resume_from_last_entry() {
    let dir = tempdir().unwrap();
    let mut journal = Journal::open(dir.path(), 1).unwrap();
    let command = "\\".repeat(512 * 1024);
    for seq in 1..=3 {
        assert_eq!(journal.append(approval(command.clone()), false).unwrap(), seq);
    }
    let first = read_page(&journal, 0, 1000).unwrap();
    assert!(serde_json::to_vec(&first).unwrap().len() <= 2 * 1024 * 1024);
    assert_eq!(first["head"], 3);
    assert_eq!(first["entries"].as_array().unwrap().len(), 1);
    assert_eq!(first["entries"][0]["seq"], 1);
    assert_eq!(first["entries"][0]["msg"]["params"]["toolCall"]["rawInput"]["command"], command);
    assert!(first["entries"][0].get("cut").is_none());
    let second = read_page(&journal, 1, 1000).unwrap();
    assert_eq!(second["entries"][0]["seq"], 2);
    assert_eq!(read_page(&journal, 2, 1).unwrap()["entries"][0]["seq"], 3);
    assert_eq!(read_page(&journal, 3, 1000).unwrap(), json!({"entries": [], "head": 3}));
}

#[test]
fn the_service_and_host_fit_the_same_encoded_page() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("journal.jsonl");
    let mut file = File::create(&path).unwrap();
    let payload = "x".repeat(1024 * 1024 - 300);
    for seq in 1..=2 {
        writeln!(file, "{}", json!({"seq": seq, "payload": payload})).unwrap();
    }
    drop(file);

    let journal = Journal::open(dir.path(), 1).unwrap();
    let host = read_page(&journal, 0, 1000).unwrap();
    assert_eq!(host["entries"].as_array().unwrap().len(), 2);
    let service = JournalReader::default().page(&path, 0, Some(1000)).unwrap();
    assert_eq!(serde_json::to_value(service).unwrap(), host);
}

#[test]
fn an_oversized_first_row_is_a_read_error_instead_of_an_empty_page() {
    let dir = tempdir().unwrap();
    let mut journal = Journal::open(dir.path(), 1).unwrap();
    journal.append(json!({"kind":"native","msg":"original"}), false).unwrap();
    let mut invalid = vec![b' '; 3 * 1024 * 1024];
    invalid.push(b'\n');
    std::fs::write(&journal.path, invalid).unwrap();

    assert!(read_page(&journal, 0, 1000).is_err());
}

#[test]
fn oversized_text_and_approval_are_marked_and_bounded_on_disk() {
    let dir = tempdir().unwrap();
    let mut journal = Journal::open(dir.path(), 1).unwrap();
    let text = "\n山".repeat(256 * 1024);
    journal.append(prompt("durable", &text), true).unwrap();
    journal.append(approval(text.clone() + &text), false).unwrap();

    let page = read_page(&journal, 0, 2).unwrap();
    let bounded_prompt = &page["entries"][0];
    let kept = bounded_prompt["msg"]["params"]["prompt"][0]["text"].as_str().unwrap();
    assert!(!kept.is_empty() && text.starts_with(kept) && kept.len() < text.len());
    assert_eq!(bounded_prompt["key"], "durable");
    assert!(bounded_prompt["cut"].as_u64().unwrap() > 0);
    assert!(serde_json::to_vec(bounded_prompt).unwrap().len() < 256 * 1024);
    let bounded_approval = &page["entries"][1];
    assert_eq!(bounded_approval["msg"]["params"]["toolCall"]["toolCallId"], "item");
    assert!(bounded_approval["cut"].as_u64().unwrap() > 0);
    assert!(serde_json::to_vec(bounded_approval).unwrap().len() < 1536 * 1024);

    drop(journal);
    let journal = Journal::open(dir.path(), 2).unwrap();
    assert_eq!(journal.deliveries().get("durable"), Some(&1));
    assert_eq!(read_page(&journal, 0, 2).unwrap(), page);
}

#[test]
fn sparse_offsets_seek_across_blocks_after_reopening() {
    let dir = tempdir().unwrap();
    let mut journal = Journal::open(dir.path(), 1).unwrap();
    for seq in 1..=512 {
        journal.append(json!({"kind": "acp", "msg": "山".repeat(seq % 31)}), false).unwrap();
    }
    assert_eq!(journal.index.offsets.len(), 2);
    assert_eq!(read_page(&journal, 255, 2).unwrap()["entries"][1]["seq"], 257);
    drop(journal);

    let mut journal = Journal::open(dir.path(), 2).unwrap();
    assert_eq!(journal.index.offsets.len(), 2);
    assert_eq!(journal.head(), 512);
    assert_eq!(journal.append(prompt("delivery", "next block"), true).unwrap(), 513);
    for (after, want) in [(0, 1), (255, 256), (256, 257), (257, 258), (511, 512), (512, 513)] {
        let page = read_page(&journal, after, 1).unwrap();
        assert_eq!(page["head"], 513);
        assert_eq!(page["entries"][0]["seq"], want);
    }
    assert_eq!(read_page(&journal, u64::MAX, 1).unwrap(), json!({"entries": [], "head": 513}));
}

#[test]
fn a_read_only_index_seeks_old_pages_and_picks_up_only_complete_appends() {
    let dir = tempdir().unwrap();
    let mut journal = Journal::open(dir.path(), 1).unwrap();
    let mut reader = JournalReader::default();
    for count in 1..=512 {
        journal.append(json!({"kind":"native","msg":count}), false).unwrap();
    }
    for (after, want) in [(255, 256), (256, 257), (511, 512)] {
        let page = reader.page(&journal.path, after, Some(1)).unwrap();
        assert_eq!(page.head, 512);
        assert_eq!(page.entries[0]["seq"], want);
    }
    let path = journal.path.clone();
    drop(journal);
    let mut tail = OpenOptions::new().append(true).open(&path).unwrap();
    tail.write_all(br#"{"seq":513"#).unwrap();
    let length = tail.metadata().unwrap().len();
    let page = reader.page(&path, 512, None).unwrap();
    assert_eq!(page.head, 512);
    assert!(page.entries.is_empty());
    assert_eq!(tail.metadata().unwrap().len(), length);

    tail.write_all(b"}\n").unwrap();
    let page = reader.page(&path, 512, None).unwrap();
    assert_eq!(page.head, 513);
    assert_eq!(page.entries, [json!({"seq":513})]);
    let unchanged = reader.page(&path, 513, None).unwrap();
    assert_eq!(unchanged.head, 513);
    assert!(unchanged.entries.is_empty());
}
