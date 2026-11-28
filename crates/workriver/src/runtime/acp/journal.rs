use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, ensure};
use serde_json::{Value, json};

const INDEX_STRIDE: u64 = 256;

pub(super) struct Journal {
    path: PathBuf,
    file: File,
    generation: u64,
    head: u64,
    offsets: Vec<u64>,
    end: u64,
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
        let mut reader = BufReader::new(&file);
        let mut offsets = Vec::new();
        let mut deliveries = HashMap::new();
        let mut head = 0;
        let mut end = 0;
        let mut line = Vec::new();
        while reader.read_until(b'\n', &mut line)? != 0 {
            if !line.ends_with(b"\n") {
                // Only an interrupted final append can lack its terminating newline.
                file.set_len(end)?;
                file.sync_all()?;
                break;
            }
            let seq = head + 1;
            let entry: Value = serde_json::from_slice(&line).with_context(|| format!("invalid journal line {seq}"))?;
            ensure!(entry["seq"].as_u64() == Some(seq), "unexpected sequence at journal line {seq}");
            if let Some(key) = delivery_key(&entry) {
                deliveries.entry(key.to_owned()).or_insert(seq);
            }
            if head.is_multiple_of(INDEX_STRIDE) {
                offsets.push(end);
            }
            head = seq;
            end += line.len() as u64;
            line.clear();
        }
        drop(reader);
        Ok(Self { path, file, generation, head, offsets, end, deliveries, append_failed: false })
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
        if self.head.is_multiple_of(INDEX_STRIDE) {
            self.offsets.push(self.end);
        }
        self.head = seq;
        self.end += bytes.len() as u64;
        if let Some(key) = key {
            self.deliveries.insert(key, seq);
        }
        Ok(seq)
    }

    pub(super) fn head(&self) -> u64 {
        self.head
    }

    pub(super) fn deliveries(&self) -> &HashMap<String, u64> {
        &self.deliveries
    }

    pub(super) fn read(&self, after: u64, limit: usize, page: &mut Vec<u8>) -> Result<()> {
        // Leave room for the host's JSON-RPC envelope and the page's closing delimiters.
        let end = page.len() + 2 * 1024 * 1024 - 128 - 2;
        write!(page, "{{\"head\":{},\"entries\":[", self.head())?;
        if after < self.head && limit != 0 {
            self.read_entries(after, limit, page, end)?;
        }
        page.extend_from_slice(b"]}");
        Ok(())
    }

    fn read_entries(&self, after: u64, limit: usize, page: &mut Vec<u8>, end: usize) -> Result<()> {
        let offset = self.offsets[(after / INDEX_STRIDE) as usize];
        let mut file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut reader = BufReader::new(file);
        for _ in 0..after % INDEX_STRIDE {
            reader.skip_until(b'\n')?;
        }
        // Include the page and reply closers so a full page needs no reallocation.
        page.reserve_exact(end + 4 - page.len());
        for index in 0..(self.head - after).min(limit.min(1000) as u64) {
            let before = page.len();
            let row = before + usize::from(index != 0);
            if row >= end {
                break;
            }
            if index != 0 {
                page.push(b',');
            }
            // Read into the response itself, with one extra byte for the line ending.
            (&mut reader).take((end - row + 1) as u64).read_until(b'\n', page)?;
            if !page.ends_with(b"\n") {
                ensure!(page.len() > end, "incomplete journal entry");
                ensure!(index != 0, "journal entry exceeds the page budget");
                page.truncate(before);
                break;
            }
            page.pop();
            // A row is checked and dropped before reading the next one.
            serde_json::from_slice::<Value>(&page[row..]).context("cannot read journal entry")?;
        }
        Ok(())
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
        for name in ["seq", "gen", "at", "kind", "event", "dir", "key", "request", "outcome", "by", "optionKind", "source", "reason"] {
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
