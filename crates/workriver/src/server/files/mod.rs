// Attachments: the bytes on disk (store.rs), and what an agent on another machine is told
// about them: it reaches them through its machine's connection. (The handlers come with
// serve.rs.)
use crate::common::home::PORT_FILE;

pub mod store;

/// Where a file in the daemon's store is for an agent on a machine: through its tunnel, on
/// the port of the connection there is when it is fetched (a shell fills it in).
pub fn attachment_url(id: &str) -> String {
    format!("http://127.0.0.1:$(cat ~/{PORT_FILE})/api/attachments/{id}")
}

/// How an agent on another machine gets a file of the daemon's store: a command, since the
/// service answers no reader that is nobody and only the CLI shows which process asks.
pub fn fetch_command(name: &str) -> String {
    format!("`workriver attach fetch {name}`")
}

/// A message for an agent on another machine: every path into the attachment store
/// (`dir` + `<id>.<ext>`) becomes the command that fetches it through its tunnel.
pub fn links(text: &str, dir: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(k) = rest.find(dir) {
        out += &rest[..k];
        let after = &rest[k + dir.len()..];
        // The file's name: its id, a dot, its extension.
        let id = after.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(after.len());
        let ext = after[id..].strip_prefix('.').map(|e| 1 + e.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(e.len())).unwrap_or(0);
        out += &format!("{} (run it: the file is saved in the folder you are in)", fetch_command(&after[..id + ext]));
        rest = &after[id + ext..];
    }
    out + rest
}

#[cfg(test)]
mod tests;
