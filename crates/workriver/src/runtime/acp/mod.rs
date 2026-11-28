// The hosted agent process and its journals (host/, codex/, journal.rs), and the service's
// side of them: spawning and asking hosts (service.rs), the hosts as a runtime (side.rs).
mod codex;
mod host;
mod journal;
mod service;
mod side;
mod wire;

pub(super) use host::run;
pub use service::{HostError, Hosts, PROTOCOL, Published, journal_lines, journal_page};
