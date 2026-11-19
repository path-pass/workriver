// The service's tests, in process: each suite starts its own daemon on a free port over a
// stand-in runtime (support.rs). Compiled with the binary under cfg(test).
mod access;
mod commands;
mod conversations;
mod files;
// Guards that hold today's behaviour while the files move (docs/design/restructure.md).
mod guards;
mod machines;
mod planning;
pub(crate) mod support;
mod views;
mod work;
