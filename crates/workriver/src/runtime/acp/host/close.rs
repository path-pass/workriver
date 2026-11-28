use std::io;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Step {
    Unsubscribe,
    Eof,
    Terminate,
    Kill,
    Finish,
}

pub(super) struct Closing {
    pub deadline: Instant,
    pub until: Instant,
    next: Step,
    unsubscribe: bool,
    pub requested: bool,
    pub cancelled: bool,
}

impl Closing {
    pub fn new(now: Instant, working: bool, requested: bool) -> Self {
        Self {
            deadline: now + Duration::from_secs(30),
            until: now + Duration::from_secs(if working { 10 } else { 0 }),
            next: Step::Unsubscribe,
            unsubscribe: requested,
            requested,
            cancelled: false,
        }
    }

    pub fn advance(&mut self, now: Instant, turn_done: bool, leader_exited: bool, group_empty: bool) -> Option<Step> {
        let early = match self.next {
            Step::Unsubscribe => turn_done || leader_exited,
            Step::Eof => !self.unsubscribe || leader_exited,
            Step::Terminate => leader_exited,
            Step::Kill | Step::Finish => leader_exited && group_empty,
        };
        if !early && now < self.until {
            return None;
        }
        let step = self.next;
        let (next, seconds) = match step {
            Step::Unsubscribe => (Step::Eof, if self.unsubscribe { 5 } else { 0 }),
            Step::Eof => (Step::Terminate, if self.requested { 5 } else { 0 }),
            Step::Terminate => (Step::Kill, 5),
            Step::Kill => (Step::Finish, 2),
            Step::Finish => return Some(step),
        };
        self.next = next;
        self.until = (now + Duration::from_secs(seconds)).min(self.deadline);
        Some(step)
    }

    pub fn unsubscribed(&mut self, now: Instant) {
        if self.next == Step::Eof {
            self.until = now;
        }
    }

    pub fn checks_group(&self) -> bool {
        matches!(self.next, Step::Terminate | Step::Kill | Step::Finish)
    }
}

pub(super) fn outcome(checked: bool, members: &[u32]) -> Value {
    if checked && members.is_empty() {
        json!({"outcome":"ended"})
    } else {
        json!({"outcome":"stuck","count":members.len(),"pids":members})
    }
}

pub(super) fn remaining(leader: u32, leader_exited: bool, listed: io::Result<Vec<u32>>) -> (Vec<u32>, Option<io::Error>) {
    let (mut pids, error) = match listed {
        Ok(pids) => (pids, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    if leader_exited {
        pids.retain(|&pid| pid != leader);
    } else if !pids.contains(&leader) {
        pids.push(leader);
    }
    (pids, error)
}

/// Observe exit without freeing the group leader's PID for reuse.
pub(super) fn exited(pid: u32) -> io::Result<bool> {
    // SAFETY: waitid writes a siginfo_t to an initialized, valid pointer.
    unsafe {
        let mut status: libc::siginfo_t = std::mem::zeroed();
        if libc::waitid(libc::P_PID, pid as libc::id_t, &mut status, libc::WEXITED | libc::WNOWAIT | libc::WNOHANG) == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(status.si_pid() != 0)
    }
}

/// The caller still owns the unreaped group leader during every signal.
pub(super) fn signal(pid: u32, sig: libc::c_int) -> io::Result<()> {
    // SAFETY: a negative PID addresses only the group with that ID.
    if unsafe { libc::kill(-(pid as libc::pid_t), sig) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) { Ok(()) } else { Err(error) }
}

#[cfg(target_os = "macos")]
pub(super) fn members(group: u32) -> io::Result<Vec<u32>> {
    use libproc::processes::{ProcFilter, pids_by_type};
    pids_by_type(ProcFilter::ByProgramGroup { pgrpid: group })
}

#[cfg(target_os = "linux")]
pub(super) fn members(group: u32) -> io::Result<Vec<u32>> {
    use procfs::process::all_processes;
    let processes = all_processes().map_err(io::Error::other)?;
    Ok(processes.filter_map(Result::ok).filter_map(|process| {
        let stat = process.stat().ok()?;
        (stat.pgrp == group as i32).then_some(stat.pid as u32)
    }).collect())
}
