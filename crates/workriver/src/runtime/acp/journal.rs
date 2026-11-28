use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, ensure};
use serde_json::{Value, json};

mod reader;

pub use reader::JournalPage;
pub(super) use reader::{JournalReader, JournalReceipts, page_limit};
use reader::{Index, open_read};

pub(super) struct Journal {
    path: PathBuf,
    file: File,
    generation: u64,
    index: Index,
    deliveries: HashMap<String, u64>,
    append_failed: bool,
}

impl Journal {
    pub(super) fn open(dir: &Path, generation: u64) -> Result<Self> {
        let path = dir.join("journal.jsonl");
        let file = OpenOptions::new().read(true).append(true).create(true)
            .mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&path)?;
        // A synced delivery must survive losing power just after the file was created.
        File::open(dir)?.sync_all()?;
        let mut index = Index::default();
        let mut deliveries = HashMap::new();
        let len = file.metadata()?.len();
        index.scan(&file, len, |seq, entry| {
            if let Some(key) = delivery_key(&entry) {
                deliveries.entry(key.to_owned()).or_insert(seq);
            }
            Ok(())
        })?;
        if index.end != len {
            // Only an interrupted final append can lack its terminating newline.
            file.set_len(index.end)?;
            file.sync_all()?;
        }
        Ok(Self { path, file, generation, index, deliveries, append_failed: false })
    }

    pub(super) fn append(&mut self, mut entry: Value, durable: bool) -> Result<u64> {
        ensure!(!self.append_failed, "a journal append failed");
        let seq = self.head() + 1;
        let fields = entry.as_object_mut().ok_or_else(|| anyhow!("journal entry must be an object"))?;
        fields.insert("seq".into(), json!(seq));
        fields.insert("gen".into(), json!(self.generation));
        fields.insert("at".into(), json!(humantime::format_rfc3339(SystemTime::now()).to_string()));

        let key = delivery_key(&entry).map(str::to_owned);
        if let Some(key) = &key {
            ensure!(!self.deliveries.contains_key(key), "delivery key has already been journaled");
        }
        let sync = durable || key.is_some() || entry["kind"] == "host";
        let mut bytes = encode_entry(&mut entry)?;
        bytes.push(b'\n');
        let wrote = self.file.write_all(&bytes).and_then(|()| if sync { self.file.sync_all() } else { Ok(()) });
        if let Err(error) = wrote {
            // A partial line is repaired on reopen. This writer may never append past it.
            self.append_failed = true;
            return Err(error).context("cannot append to the journal");
        }
        self.index.added(bytes.len());
        if let Some(key) = key {
            self.deliveries.insert(key, seq);
        }
        Ok(seq)
    }

    pub(super) fn head(&self) -> u64 {
        self.index.head
    }

    pub(super) fn deliveries(&self) -> &HashMap<String, u64> {
        &self.deliveries
    }

    pub(super) fn read(&self, after: u64, limit: usize, page: &mut Vec<u8>) -> Result<()> {
        self.index.write_page(&mut open_read(&self.path)?, after, limit, page)
    }
}

fn delivery_key(entry: &Value) -> Option<&str> {
    if entry["dir"] == "out" && entry.pointer("/msg/method").and_then(Value::as_str) == Some("session/prompt") {
        entry["key"].as_str()
    } else {
        None
    }
}

/// Preserve UTF-8 text prefixes and small ACP fields when a body needs to be cut.
fn encode_entry(entry: &mut Value) -> Result<Vec<u8>> {
    let approval = entry.pointer("/msg/method").and_then(Value::as_str) == Some("session/request_permission");
    let max = if approval { 1536 * 1024 } else { 256 * 1024 };
    let encoded = serde_json::to_vec(entry)?;
    let original = encoded.len();
    if original < max {
        return Ok(encoded);
    }
    drop(encoded);

    // Leave space for the cut count and the terminating newline.
    let budget = max - 32;
    for path in ["/msg/params/prompt/0/text", "/msg/params/update/content/text"] {
        if let Some(text) = entry.pointer_mut(path).filter(|value| value.is_string()) {
            let Value::String(text) = std::mem::replace(text, json!("")) else { unreachable!() };
            let mut remaining = budget.saturating_sub(serde_json::to_vec(entry)?.len());
            let kept: usize = text.chars().take_while(|ch| {
                let width = match ch {
                    '"' | '\\' | '\n' | '\r' | '\t' | '\x08' | '\x0c' => 2,
                    '\x00'..='\x1f' => 6,
                    ch => ch.len_utf8(),
                };
                if width > remaining { return false; }
                remaining -= width;
                true
            }).map(char::len_utf8).sum();
            *entry.pointer_mut(path).unwrap() = json!(&text[..kept]);
        }
    }
    if serde_json::to_vec(entry)?.len() > budget {
        if let Some(fields) = entry.pointer_mut("/msg/params/toolCall").and_then(Value::as_object_mut) {
            fields.retain(|name, _| matches!(name.as_str(), "toolCallId" | "status" | "kind"));
        }
        if let Some(fields) = entry.pointer_mut("/msg/params/update").and_then(Value::as_object_mut) {
            fields.remove("rawInput");
            fields.remove("rawOutput");
            if matches!(fields.get("sessionUpdate").and_then(Value::as_str), Some("tool_call" | "tool_call_update")) {
                fields.remove("content");
                fields.remove("locations");
            }
        }
        entry.as_object_mut().unwrap().remove("native");
    }
    let mut kept = serde_json::to_vec(entry)?.len();
    if kept > budget {
        let mut reduced = json!({});
        for name in ["seq", "gen", "at", "kind", "event", "dir", "key", "request", "outcome", "by", "optionKind", "source", "reason", "markedError"] {
            if let Some(value) = entry.get(name).filter(|v| v.is_u64() || v.as_str().is_some_and(|s| s.len() <= 1024)) {
                reduced[name] = value.clone();
            }
        }
        if let Some(fields) = entry["msg"].as_object_mut() {
            fields.retain(|name, value| matches!(name.as_str(), "jsonrpc" | "id" | "method")
                && (value.is_number() || value.as_str().is_some_and(|s| s.len() <= 1024)));
            reduced["msg"] = std::mem::take(&mut entry["msg"]);
        }
        *entry = reduced;
        kept = serde_json::to_vec(entry)?.len();
    }
    entry["cut"] = json!(original - kept);
    Ok(serde_json::to_vec(entry)?)
}

#[cfg(test)]
mod tests;
