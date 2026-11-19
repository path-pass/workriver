// What the CLI and the service both use and no domain owns: where the data directory is and
// what is in it, the settings, reading processes, the performance log, and the shapes they exchange.
pub mod config;
pub mod diagnostics;
pub mod files;
pub mod home;
pub mod process;
pub mod wire;

/// The version and the commit this binary was built from.
pub const BUILD: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("WORKRIVER_COMMIT"), ")");
