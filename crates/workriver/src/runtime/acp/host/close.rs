use std::collections::BTreeMap;
use std::io;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::common::process::{self, Owned};

/// What a scan of this account's processes gives: each marked one, its start and its marker;
/// none when the processes cannot be listed.
pub(super) type Scan = Option<Vec<(u32, Owned, String)>>;

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
    pub requested: bool,
    pub cancelled: bool,
    /// The processes that carry this launch's marker outside the agent's group, too.
    pub sweep: Sweep,
}

/// The processes a close found carrying this launch's marker (`a7/2`), each by its id and
/// start: one that left the agent's group (its own session, a double fork) is still ended.
/// Before every signal each is read again (the same start, this account's, the marker still
/// there); one whose environment can no longer be read is left alone as unverifiable.
#[derive(Default)]
pub(super) struct Sweep {
    seen: BTreeMap<u32, u64>,
    /// A scan could not list the processes: kept apart from a scan that found none.
    pub failed: bool,
}

impl Sweep {
    /// Adds what a scan found with `marker`, but the agent's leader (the group's own).
    pub fn take(&mut self, scan: Scan, marker: &str, leader: u32) {
        match scan {
            None => self.failed = true,
            Some(found) => {
                for (pid, owned, _) in found.into_iter().filter(|(pid, _, m)| m == marker && *pid != leader) {
                    self.seen.entry(pid).or_insert(owned.start);
                }
            }
        }
    }

    /// One process's state now: `exited` (gone, a zombie, or another process with its id),
    /// `alive` (still this account's, its start and marker as seen), `unverifiable`.
    fn state(marker: &str, pid: u32, start: u64) -> &'static str {
        match process::owned(pid) {
            Some(o) if o.start == start && !o.zombie => match process::marker(pid) {
                Ok(Some(m)) if m == marker => "alive",
                Ok(_) => "exited",
                Err(()) => "unverifiable",
            },
            _ => "exited",
        }
    }

    /// Those not known to have exited: alive, or unreadable now (one dying shows no
    /// environment for a moment). The close waits for them within its deadline.
    pub fn alive(&self, marker: &str) -> Vec<u32> {
        self.seen.iter().filter(|&(&pid, &start)| Self::state(marker, pid, start) != "exited").map(|(&pid, _)| pid).collect()
    }

    /// Signals each one still alive, each read again just before.
    pub fn signal(&self, marker: &str, sig: libc::c_int) {
        for pid in self.seen.iter().filter(|&(&pid, &start)| Self::state(marker, pid, start) == "alive").map(|(&pid, _)| pid) {
            // SAFETY: a positive pid addresses one process, read again just above.
            if unsafe { libc::kill(pid as libc::pid_t, sig) } != 0 {
                eprintln!("host: signalling marked process {pid} ({sig}): {}", io::Error::last_os_error());
            }
        }
    }

    /// Each process found, with how it ended: `exited`, `alive` or `unverifiable`.
    pub fn report(&self, marker: &str) -> Value {
        json!(self.seen.iter().map(|(&pid, &start)| json!({"pid": pid, "outcome": Self::state(marker, pid, start)})).collect::<Vec<_>>())
    }
}

impl Closing {
    pub fn new(now: Instant, working: bool, requested: bool) -> Self {
        Self {
            deadline: now + Duration::from_secs(30),
            until: now + Duration::from_secs(if working { 10 } else { 0 }),
            next: Step::Unsubscribe,
            requested,
            cancelled: false,
            sweep: Sweep::default(),
        }
    }

    pub fn advance(&mut self, now: Instant, turn_done: bool, leader_exited: bool, group_empty: bool) -> Option<Step> {
        let early = match self.next {
            Step::Unsubscribe => turn_done || leader_exited,
            Step::Eof => !self.requested || leader_exited,
            Step::Terminate => leader_exited,
            Step::Kill | Step::Finish => leader_exited && group_empty,
        };
        if !early && now < self.until {
            return None;
        }
        let step = self.next;
        let (next, seconds) = match step {
            Step::Unsubscribe => (Step::Eof, if self.requested { 5 } else { 0 }),
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
