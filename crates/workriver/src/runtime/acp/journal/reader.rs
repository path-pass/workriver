use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use workriver_core::hosted::{Answer, Outcome, ProcessResult};

use super::super::wire::HOST_FRAME_BYTES;

const INDEX_STRIDE: u64 = 256;
// Leave space for the JSON-RPC envelope, including a short string request id.
const PAGE_BYTES: usize = HOST_FRAME_BYTES - 128;

pub(in crate::runtime::acp) fn page_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(200).clamp(1, 1000)
}

#[derive(Deserialize, Serialize)]
pub struct JournalPage {
    pub head: u64,
    pub entries: Vec<Value>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostClosed {
    pub outcome: Outcome,
    #[serde(default)]
    pub pids: Vec<u32>,
    #[serde(default)]
    pub processes: Vec<ProcessResult>,
    pub marked_error: Option<String>,
}

#[derive(Clone, Default)]
pub struct JournalReceipts {
    pub decided: Vec<(u64, Answer)>,
    pub closed: Option<HostClosed>,
}

impl JournalReceipts {
    fn note(&mut self, entry: Value) -> Result<()> {
        if entry["kind"] != "host" {
            return Ok(());
        }
        match entry["event"].as_str() {
            Some("decided") => {
                let request = entry["request"].as_u64().context("decision without a request key")?;
                let answer = match entry["optionKind"].as_str() {
                    Some("allow_once") => Answer::Allow,
                    Some("reject_once") => Answer::Reject,
                    _ => anyhow::bail!("invalid journal decision for request {request}"),
                };
                self.decided.push((request, answer));
            }
            Some("closed") => self.closed = Some(serde_json::from_value(entry).context("invalid journal close receipt")?),
            _ => {}
        }
        Ok(())
    }
}

/// The host maintains this while appending. A read-only client builds it once, then indexes
/// only the bytes appended since its last complete line.
#[derive(Default)]
pub(super) struct Index {
    pub(super) head: u64,
    pub(super) end: u64,
    pub(super) offsets: Vec<u64>,
}

impl Index {
    pub(super) fn added(&mut self, bytes: usize) {
        if self.head.is_multiple_of(INDEX_STRIDE) {
            self.offsets.push(self.end);
        }
        self.head += 1;
        self.end += bytes as u64;
    }

    pub(super) fn scan(&mut self, file: &File, len: u64, mut visit: impl FnMut(u64, Value) -> Result<()>) -> Result<()> {
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(self.end))?;
        let mut reader = reader.take(len - self.end);
        let mut line = Vec::new();
        while (&mut reader).take((PAGE_BYTES + 1) as u64).read_until(b'\n', &mut line)? != 0 {
            let seq = self.head + 1;
            ensure!(line.len() <= PAGE_BYTES, "journal line {seq} exceeds the byte budget");
            if !line.ends_with(b"\n") {
                break;
            }
            visit(seq, entry(&line, seq)?)?;
            self.added(line.len());
            line.clear();
        }
        Ok(())
    }

    pub(super) fn write_page(&self, file: &mut File, after: u64, limit: usize, page: &mut Vec<u8>) -> Result<()> {
        let end = page.len() + PAGE_BYTES - 2;
        write!(page, "{{\"head\":{},\"entries\":[", self.head)?;
        if after < self.head && limit != 0 {
            file.seek(SeekFrom::Start(self.offsets[(after / INDEX_STRIDE) as usize]))?;
            let mut reader = BufReader::new(file);
            for _ in 0..after % INDEX_STRIDE {
                reader.skip_until(b'\n')?;
            }
            // Include the page and reply closers so a full page needs no reallocation.
            page.reserve_exact(end + 4 - page.len());
            for index in 0..(self.head - after).min(page_limit(Some(limit)) as u64) {
                let before = page.len();
                let row = before + usize::from(index != 0);
                if row >= end {
                    break;
                }
                if index != 0 {
                    page.push(b',');
                }
                (&mut reader).take((end - row + 1) as u64).read_until(b'\n', page)?;
                if !page.ends_with(b"\n") {
                    ensure!(page.len() == end + 1, "incomplete journal entry");
                    ensure!(index != 0, "journal entry exceeds the page budget");
                    page.truncate(before);
                    break;
                }
                page.pop();
                entry(&page[row..], after + index + 1)?;
            }
        }
        page.extend_from_slice(b"]}");
        Ok(())
    }
}

fn entry(bytes: &[u8], seq: u64) -> Result<Value> {
    let entry: Value = serde_json::from_slice(bytes).with_context(|| format!("invalid journal line {seq}"))?;
    ensure!(entry["seq"].as_u64() == Some(seq), "unexpected sequence at journal line {seq}");
    Ok(entry)
}

pub(super) fn open_read(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)
}

#[derive(PartialEq, Eq)]
struct Snapshot {
    device: u64,
    inode: u64,
    len: u64,
    modified: std::time::SystemTime,
}

/// This reader never opens a writer or repairs an interrupted append.
#[derive(Default)]
pub(in crate::runtime::acp) struct JournalReader {
    index: Index,
    receipts: JournalReceipts,
    snapshot: Option<Snapshot>,
}

impl JournalReader {
    pub fn page(&mut self, path: &Path, after: u64, limit: Option<usize>) -> Result<JournalPage> {
        let Some(mut file) = self.refresh(path)? else {
            return Ok(JournalPage { head: 0, entries: Vec::new() });
        };
        let mut bytes = Vec::new();
        self.index.write_page(&mut file, after, page_limit(limit), &mut bytes)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn receipts(&mut self, path: &Path) -> Result<JournalReceipts> {
        self.refresh(path)?;
        Ok(self.receipts.clone())
    }

    fn refresh(&mut self, path: &Path) -> Result<Option<File>> {
        let file = match open_read(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                *self = Self::default();
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };
        let meta = file.metadata()?;
        ensure!(meta.is_file(), "journal is not a regular file");
        let now = Snapshot { device: meta.dev(), inode: meta.ino(), len: meta.len(), modified: meta.modified()? };
        if self.snapshot.as_ref() != Some(&now) {
            if self.snapshot.as_ref().is_none_or(|old| {
                old.device != now.device || old.inode != now.inode || old.len >= now.len
            }) {
                *self = Self::default();
            }
            self.index.scan(&file, now.len, |_, entry| self.receipts.note(entry))?;
            self.snapshot = Some(now);
        }
        Ok(Some(file))
    }
}
