use std::mem::{MaybeUninit, size_of};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

use crate::common::process;

pub(super) fn allowed(stream: &UnixStream) -> bool {
    outside_hosts(stream) == Some(true)
}

fn outside_hosts(stream: &UnixStream) -> Option<bool> {
    let accepted = process::now();
    let pid = peer_pid(stream)?;
    let sid = session(pid)?;
    let own = process::one(std::process::id())?;
    let chain = process::chain(pid);
    let peer = chain.first()?;
    if accepted == u64::MAX || peer.start > accepted || chain.last()?.parent != 0 {
        return None;
    }
    // An orphan can still belong to a host's session after it loses its original parents.
    let session_leader = if sid != 0 && !chain.iter().any(|p| p.pid == sid) {
        Some(process::one(sid)?)
    } else {
        None
    };
    for p in chain.iter().chain(session_leader.iter()) {
        if p.pid == own.pid || ((p.name == "workriver" || p.name == own.name) && is_host(p.pid)?) {
            return Some(false);
        }
    }
    let current = process::one(pid)?;
    Some(current.start == peer.start && current.parent == peer.parent && session(pid)? == sid)
}

fn session(pid: u32) -> Option<u32> {
    // SAFETY: getsid takes a PID and has no pointer arguments.
    unsafe { libc::getsid(pid as libc::pid_t) }.try_into().ok()
}

#[cfg(target_os = "macos")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    // SAFETY: LOCAL_PEERPID writes a pid_t.
    let pid: libc::pid_t = unsafe { socket_option(stream, libc::SOL_LOCAL, libc::LOCAL_PEERPID)? };
    (pid > 0).then_some(pid as u32)
}

#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    // SAFETY: SO_PEERCRED writes a ucred.
    let cred: libc::ucred = unsafe { socket_option(stream, libc::SOL_SOCKET, libc::SO_PEERCRED)? };
    (cred.pid > 0).then_some(cred.pid as u32)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn peer_pid(_: &UnixStream) -> Option<u32> {
    None
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
unsafe fn socket_option<T>(stream: &UnixStream, level: libc::c_int, opt: libc::c_int) -> Option<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut len = size_of::<T>() as libc::socklen_t;
    // SAFETY: the fd is borrowed and the caller chooses the exact C type for the option.
    unsafe {
        if libc::getsockopt(
            stream.as_raw_fd(),
            level,
            opt,
            value.as_mut_ptr().cast(),
            &mut len,
        ) == 0
            && len as usize == size_of::<T>()
        {
            Some(value.assume_init())
        } else {
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn is_host(pid: u32) -> Option<bool> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    let mut len = 0;
    let mut bytes;
    // SAFETY: the first call sizes the buffer and the second borrows its initialized bytes.
    unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        bytes = vec![0u8; len];
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
    }
    // Darwin puts argc first, then the NUL-terminated executable path, padding, and argv.
    let (argc, strings) = bytes.get(..len)?.split_first_chunk::<4>()?;
    let argc = i32::from_ne_bytes(*argc).try_into().ok()?;
    let args = strings
        .split(|&b| b == 0)
        .skip(1)
        .skip_while(|arg| arg.is_empty());
    host_command(args.take(argc))
}

#[cfg(target_os = "linux")]
fn is_host(pid: u32) -> Option<bool> {
    let args = procfs::process::Process::new(pid as i32)
        .ok()?
        .cmdline()
        .ok()?;
    host_command(args.iter())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn is_host(_: u32) -> Option<bool> {
    None
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn host_command(mut args: impl Iterator<Item = impl AsRef<[u8]>>) -> Option<bool> {
    args.next()?;
    while let Some(arg) = args.next() {
        match arg.as_ref() {
            b"--port" | b"--as" => {
                args.next()?;
            }
            flag if flag.starts_with(b"--port=") || flag.starts_with(b"--as=") => {}
            command => return Some(command == b"host"),
        }
    }
    Some(false)
}
