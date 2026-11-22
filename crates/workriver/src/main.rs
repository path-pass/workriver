// workriver: the daemon (`serve`) and the short-lived CLI that records work through it.
mod cli;
mod common;
mod runtime;
mod server;

use std::path::Path;

use anyhow::Context;
use clap::Parser;

/// Parses the command line; `serve` is started here (the one place that puts the service
/// together), every other command is the CLI's.
fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    if let cli::Cmd::Serve { terminal_app, perf_detail } = &cli.cmd {
        common::diagnostics::init(&common::home::data_dir(), *perf_detail || std::env::var("WORKRIVER_PERF").is_ok_and(|v| v == "detail"));
        // A directory or file that cannot be made is the store's error, as when it made them.
        let path = common::home::database().map_err(workriver_data::Error::from).context("cannot open the workriver database")?;
        let store = workriver_data::Store::open(&path).context("cannot open the workriver database")?;
        // Without its runtime the daemon still records and shows; it just sees no agents.
        let (runtime, warning) = runtime::connect(terminal_app.clone());
        let home = store.path().parent().map(Path::to_path_buf).unwrap_or_default();
        let all = runtime::follow::Machines::new(runtime, home, terminal_app.clone());
        if let Some(w) = &warning {
            tracing::warn!("{w}");
        }
        let allow_as = std::env::var("WORKRIVER_ALLOW_AS").is_ok_and(|v| v == "1");
        let options = server::Options { warning, waits: server::Waits::default(), machines: Some(all.clone()), allow_as, sample: true, procs: std::sync::Arc::new(common::process::System), callers: std::sync::Arc::default(), user_home: std::env::home_dir() };
        let rt = tokio::runtime::Runtime::new()?;
        return rt.block_on(async {
            let listener = server::bind(cli.port.unwrap_or(cli::DEFAULT_PORT)).await?;
            server::serve_with(listener, store, all, options).await
        });
    }
    cli::run(cli)
}
