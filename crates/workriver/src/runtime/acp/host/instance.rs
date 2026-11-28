use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use anyhow::{Context, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub(super) struct Instance {
    pub id: String,
    pub harness: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub auth: String,
    pub generation: u64,
    pub state: String,
}

pub(super) struct Directory {
    pub path: PathBuf,
    _lock: File,
    pub proof: [u8; 64],
}

impl Directory {
    pub fn open(path: &Path) -> anyhow::Result<(Self, Instance)> {
        let meta = fs::symlink_metadata(path)?;
        // SAFETY: geteuid has no arguments or failure mode.
        ensure!(meta.is_dir() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o777 == 0o700, "instance directory must be owned by the caller with mode 700");
        let lock = OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(path.join("lock"))?;
        // SAFETY: lock owns the fd until this Directory is dropped.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(std::io::Error::last_os_error()).context("cannot lock the instance");
        }
        let key = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path.join("host.key"))?;
        let meta = key.metadata()?;
        // SAFETY: geteuid has no arguments or failure mode.
        ensure!(meta.is_file() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o777 == 0o600 && meta.len() == 32, "host.key must contain 32 bytes and have mode 600");
        let mut bytes = [0; 32];
        key.take(32).read_exact(&mut bytes)?;
        let mut proof = [0; 64];
        for (out, byte) in proof.chunks_exact_mut(2).zip(bytes) {
            let hex = b"0123456789abcdef";
            out[0] = hex[(byte >> 4) as usize];
            out[1] = hex[(byte & 15) as usize];
        }
        let instance: Instance = serde_json::from_slice(&fs::read(path.join("instance.json"))?).context("cannot read the instance")?;
        ensure!(instance.harness == "codex" && instance.state == "starting" && instance.cwd.is_absolute() && instance.generation > 0, "instance must be a reserved Codex start");
        ensure!(instance.id.strip_prefix('a').is_some_and(|n| n.parse::<u64>().is_ok_and(|id| id > 0)), "invalid agent id");
        Ok((Self { path: path.to_path_buf(), _lock: lock, proof }, instance))
    }

    pub fn bind(&self) -> anyhow::Result<Socket> {
        let path = self.path.join("host.sock");
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let listener = UnixListener::bind(&path).context("cannot bind the host socket")?;
        let socket = Socket { path, listener };
        fs::set_permissions(&socket.path, fs::Permissions::from_mode(0o600))?;
        socket.listener.set_nonblocking(true)?;
        Ok(socket)
    }

    pub fn save(&self, value: &Value) -> anyhow::Result<()> {
        let mut next = tempfile::NamedTempFile::new_in(&self.path)?;
        serde_json::to_writer(&mut next, value)?;
        next.write_all(b"\n")?;
        next.as_file().sync_all()?;
        next.persist(self.path.join("instance.json"))?;
        File::open(&self.path)?.sync_all()?;
        Ok(())
    }
}

pub(super) struct Socket {
    path: PathBuf,
    pub listener: UnixListener,
}

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl Instance {
    pub fn record(&self) -> anyhow::Result<Value> {
        let pid = std::process::id();
        let host = crate::common::process::chain(pid).into_iter().next().context("cannot read the host process")?;
        Ok(json!({
            "id":self.id, "harness":self.harness, "cwd":self.cwd, "model":self.model,
            "auth":self.auth, "generation":self.generation, "state":"starting",
            "host":{"pid":pid,"start":host.start}
        }))
    }
}
