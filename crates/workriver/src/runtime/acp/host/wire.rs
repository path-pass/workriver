use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

pub(super) fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

pub(super) fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"params":params})
}

pub(super) fn reply(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

pub(super) fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

pub(super) struct Lines<T> {
    pub io: T,
    bytes: Vec<u8>,
    scanned: usize,
}

impl<T: Read> Lines<T> {
    pub fn new(io: T) -> Self {
        Self { io, bytes: Vec::new(), scanned: 0 }
    }

    pub fn next(&mut self, max_bytes: usize) -> io::Result<Option<Value>> {
        loop {
            if let Some(end) = self.bytes[self.scanned..].iter().position(|&byte| byte == b'\n') {
                let end = self.scanned + end;
                if end > max_bytes {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "frame exceeds wire budget"));
                }
                let frame = serde_json::from_slice(&self.bytes[..end]).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid JSON-RPC frame"))?;
                self.bytes.drain(..=end);
                self.scanned = 0;
                return Ok(Some(frame));
            }
            self.scanned = self.bytes.len();
            if self.bytes.len() > max_bytes {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "frame exceeds wire budget"));
            }
            let mut chunk = [0; 8192];
            match self.io.read(&mut chunk) {
                Ok(0) if self.bytes.is_empty() => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "stream closed")),
                Ok(0) => return Err(io::Error::new(io::ErrorKind::InvalidData, "incomplete JSON-RPC frame")),
                Ok(len) => self.bytes.extend_from_slice(&chunk[..len]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }
}

pub(super) fn nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: the fd belongs to the caller and neither operation takes ownership.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags == -1 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub(super) fn interested(fd: RawFd, events: libc::c_short) -> libc::pollfd {
    libc::pollfd { fd, events, revents: 0 }
}

pub(super) fn poll(fds: &mut [libc::pollfd], timeout: Option<Duration>) -> io::Result<()> {
    let ms = timeout.map_or(-1, |d| d.as_nanos().div_ceil(1_000_000).min(i32::MAX as u128) as i32);
    // SAFETY: poll borrows the whole slice for the duration of the call.
    if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, ms) } == -1 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    Ok(())
}

pub(super) fn write(io: &mut (impl AsRawFd + Write), frame: &Value, until: Instant) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(frame)?;
    bytes.push(b'\n');
    let mut tail = &bytes[..];
    while !tail.is_empty() {
        match io.write(tail) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(len) => tail = &tail[len..],
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {
                let now = Instant::now();
                if now >= until {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                poll(&mut [interested(io.as_raw_fd(), libc::POLLOUT)], Some(until - now))?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
