// The service's tests, in process: each suite starts its own daemon on a free port over a
// stand-in runtime (support.rs). Compiled with the binary under cfg(test).
mod access;
mod commands;
mod conversations;
mod files;
// Guards that hold behaviour while code moves (docs/design/guards.md).
mod guards;
mod machines;
mod ownership;
mod planning;
// Conversations read by their workspace only; a manager's seat (docs/security.md).
mod scope;
mod seats;
mod starts;
pub(super) mod support;
mod terminals;
mod views;
mod work;
