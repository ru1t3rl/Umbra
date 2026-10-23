//! `umbra serve` (REQ-X-2, REQ-X-8, ADR 0029, ADR 0026).
//!
//! Order of events: read the configuration, apply the flags, start the engine thread, preload the
//! scenario (a failure is exit 1 before anything is printed), bind, print the one stdout line,
//! then hand over to [`umbra_server::run`] until SIGINT or SIGTERM.

use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use tracing_subscriber::filter::LevelFilter;
use umbra_engine::handle::{EngineHandle, MIN_STACK_SIZE};
use umbra_server::{ServerSettings, ShutdownMode};

use crate::args::ServeArgs;
use crate::config::{self, Outcome};

/// Stack of the engine thread: behavior graphs recurse (ADR 0024).
const ENGINE_STACK_BYTES: usize = {
    let bytes = 8 * 1024 * 1024;
    assert!(bytes >= MIN_STACK_SIZE);
    bytes
};

/// The settings of this run: flag over file over default, per setting (ADR 0029).
fn settings(args: &ServeArgs, config: &umbra_engine::Config) -> ServerSettings {
    let mut settings = ServerSettings::from_config(config);
    if let Some(bind) = args.bind {
        settings.server.bind = bind;
    }
    if let Some(root) = &args.scenario_root {
        settings.server.scenario_root.clone_from(root);
    }
    settings.plugin_paths = config::plugin_paths(&args.plugin_path, config);
    settings
}

/// The warning for an address other than loopback; `None` for loopback.
fn exposure_warning(addr: SocketAddr) -> Option<String> {
    (!addr.ip().is_loopback()).then(|| {
        format!(
            "warning: listening on {addr}, which is not a loopback address; the API has no \
             authentication, so anyone who can reach this address can control the simulation"
        )
    })
}

/// Loads (and with `--start`, starts) the scenario named on the command line.
fn preload(handle: &EngineHandle, args: &ServeArgs) -> Result<()> {
    let Some(path) = &args.scenario else {
        return Ok(());
    };
    handle
        .load(path.clone(), args.seed, args.clock)
        .with_context(|| format!("preloading {}", path.display()))?;
    if args.start {
        handle.start().context("starting the preloaded scenario")?;
    }
    Ok(())
}

/// Writes the exposure warning (if any) to stderr and the one stdout line `listening on http://<addr>`.
fn announce(addr: SocketAddr, log_level: LevelFilter) -> io::Result<()> {
    if log_level >= LevelFilter::WARN
        && let Some(warning) = exposure_warning(addr)
    {
        // Nothing sensible remains to do if stderr is gone.
        let _ = writeln!(io::stderr(), "{warning}");
    }
    let mut out = io::stdout().lock();
    writeln!(out, "listening on http://{addr}")?;
    out.flush()
}

/// Runs `umbra serve`; returns when the server has shut down.
pub fn serve(args: &ServeArgs, config_flag: Option<&Path>) -> Result<ExitCode> {
    let configuration = match config::load(config_flag)? {
        Outcome::Loaded(loaded) => loaded,
        Outcome::Invalid(diagnostics) => return Ok(crate::report_invalid(&diagnostics)),
    };
    let log_level = config::log_level(args.log_level, &configuration.config);
    crate::init_logging(log_level)?;
    let settings = settings(args, &configuration.config);
    let mut handle_config = settings.handle_config().context("setting up the engine")?;
    handle_config.stack_size = ENGINE_STACK_BYTES;
    if let Some(dir) = &args.record_dir {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating the record directory {}", dir.display()))?;
        handle_config.record_dir = Some(dir.clone());
    }
    let (handle, thread) =
        EngineHandle::spawn(handle_config).context("starting the engine thread")?;
    preload(&handle, args)?;
    let on_listening = |addr| announce(addr, log_level);
    umbra_server::run(
        settings,
        handle,
        thread,
        ShutdownMode::Signals,
        on_listening,
    )
    .context("serving")?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::exposure_warning;

    #[test]
    fn only_a_non_loopback_address_gets_the_warning() {
        let loopback: SocketAddr = "127.0.0.1:7878".parse().expect("addr");
        let v6_loopback: SocketAddr = "[::1]:7878".parse().expect("addr");
        let open: SocketAddr = "0.0.0.0:7878".parse().expect("addr");
        assert!(exposure_warning(loopback).is_none());
        assert!(exposure_warning(v6_loopback).is_none());
        let text = exposure_warning(open).expect("warning").to_lowercase();
        assert!(text.contains("warn") && text.contains("authentication"));
    }
}
