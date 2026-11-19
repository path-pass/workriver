// Another machine's Herdr: the same client behind a socket forwarded from it.
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use super::Herdr;

impl Herdr {
    /// The Herdr of another machine, behind a socket forwarded from it.
    pub fn on(socket: PathBuf, machine: &str, terminal_app: String) -> Herdr {
        Herdr { socket, terminal_app, ids: AtomicU64::new(1), panes: Mutex::default(), machine: Some(machine.into()) }
    }
}
