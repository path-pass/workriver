// The hosted agent process and its journals. The service reaches a host through its socket.
mod codex;
mod host;
mod journal;

pub(super) use host::run;
