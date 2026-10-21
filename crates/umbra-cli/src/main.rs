//! `umbra-cli`: the command-line front end of Umbra.
//!
//! Responsibility: `run <scenario.toml>` (headless run printing entity state and the state hash,
//! REQ-S-1, REQ-X-2), `validate <path>...` (REQ-T-6), `schema` (D-14) and `serve` (feature
//! `server`). Exit code 0 on success,
//! 1 on invalid input or a runtime error, 2 on a usage error (clap).
//!
//! Dependency rule (REQ-A-2): depends on `umbra-engine`, and on `umbra-server` behind the default
//! feature `server` (M5). It contains no `unsafe` code.

#![forbid(unsafe_code)]

mod args;
mod config;
mod output;
#[cfg(test)]
mod output_warnings_tests;
#[cfg(feature = "server")]
mod serve;

use std::io::{self, Write};
use std::ops::ControlFlow;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use tracing_subscriber::filter::LevelFilter;

use args::{Cli, Command, ListPluginsArgs, RunArgs, SchemaArgs, SchemaKind, ValidateArgs};
use config::Outcome;
use output::Format;
use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::{Diagnostic, Diagnostics, RunOptions, ScenarioError, TickRateSource};
use umbra_engine::{
    BEHAVIOR_FILE_SCHEMA, CONFIG_FILE_SCHEMA, ComponentId, ComponentKind, ControlAction,
    HostOptions, PluginInfo, SCENARIO_FILE_SCHEMA, StdWallClock, TEMPLATE_FILE_SCHEMA, TickRate,
    instantiate_with_plugins, load_plugins, load_scenario_with_plugins_and_settings,
    resolve_plugin_paths, validate_path_with_plugins_and_settings,
};

/// Stack of the thread that runs the command. Behavior graphs are ticked by recursion and need up
/// to about 1.5 MiB in a debug build (ASM-098); the main thread has only 1 MiB on Windows.
const STACK_BYTES: usize = 8 * 1024 * 1024;

/// Runs the command on a thread with an explicit stack (ASM-098, ASM-109). Arguments are parsed on
/// the main thread, so usage errors and `--help` behave as before (exit code 2 and 0). A panic
/// of the command thread is re-raised here with its original payload, so the message and the exit
/// status (101) are the same as when the command ran on the main thread.
fn main() -> ExitCode {
    let cli = Cli::parse();
    // Named `main` so that a panic message reads `thread 'main' panicked`, as it did before.
    let spawned = std::thread::Builder::new()
        .name("main".into())
        .stack_size(STACK_BYTES)
        .spawn(move || command(cli));
    match spawned {
        Ok(handle) => handle
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload)),
        Err(error) => {
            // A failed write to a closed stderr leaves nothing more to report.
            let _ = writeln!(
                io::stderr(),
                "error: cannot start the command thread: {error}"
            );
            ExitCode::FAILURE
        }
    }
}

fn command(cli: Cli) -> ExitCode {
    let config = cli.config.as_deref();
    let result = match cli.command {
        Command::Run(args) => run(&args, config),
        Command::Validate(args) => validate(&args, config),
        Command::Schema(args) => schema(&args, config),
        Command::ListPlugins(args) => list_plugins(&args, config),
        #[cfg(feature = "server")]
        Command::Serve(args) => serve::serve(&args, config),
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            // A failed write to a closed stderr leaves nothing more to report.
            let _ = writeln!(io::stderr(), "error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn init_logging(level: LevelFilter) -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(io::stderr)
        .with_ansi(false)
        .try_init()
        .map_err(|e| anyhow!("cannot install the log subscriber: {e}"))
}

/// Prints every diagnostic to stderr; exit code 1.
pub(crate) fn report_invalid(diagnostics: &[Diagnostic]) -> ExitCode {
    let mut err = io::stderr().lock();
    for d in diagnostics {
        // Nothing sensible remains to do if stderr is gone.
        let _ = writeln!(err, "{d}");
    }
    ExitCode::FAILURE
}

/// Writes the stderr `note:` for a scenario whose tick rate came from the default (ASM-160, ASM-168).
/// Written whatever `--log-level` says; stdout is untouched.
fn note_default_tick_rate(path: &std::path::Path, rate: TickRate) {
    // Nothing sensible remains to do if stderr is gone.
    let _ = writeln!(
        io::stderr(),
        "note: {}: `tick_rate_hz` is not set; using the default of {} Hz",
        path.display(),
        rate.hz()
    );
}

fn run(args: &RunArgs, config_flag: Option<&Path>) -> Result<ExitCode> {
    let configuration = match config::load(config_flag)? {
        Outcome::Loaded(loaded) => loaded,
        Outcome::Invalid(diagnostics) => return Ok(report_invalid(&diagnostics)),
    };
    let log_level = config::log_level(args.log_level, &configuration.config);
    init_logging(log_level)?;
    if !configuration.implicit {
        log_config(log_level, configuration.path.as_deref());
    }
    let registry = standard_registry().context("building the component registry")?;
    let plugin_paths = config::plugin_paths(&args.plugin_path, &configuration.config);
    let settings = config::load_settings(&configuration.config);
    let loaded = match load_scenario_with_plugins_and_settings(
        &registry,
        &args.scenario,
        &plugin_paths,
        &settings,
    ) {
        Ok(loaded) => loaded,
        Err(ScenarioError::Invalid(diagnostics)) => {
            return Ok(report_invalid(&diagnostics.into_vec()));
        }
        Err(other) => return Err(other).context("loading the scenario"),
    };
    let options = RunOptions {
        wall: Box::new(StdWallClock::new()),
        seed: args.seed,
        clock: args.clock,
    };
    if loaded.scenario.tick_rate_source == TickRateSource::Default {
        note_default_tick_rate(&loaded.scenario.source, loaded.scenario.tick_rate);
    }
    for warning in &loaded.scenario.warnings {
        // Warnings never fail the run and stay off stdout, like the default-rate note (ASM-290).
        let _ = writeln!(io::stderr(), "{warning}");
    }
    log_plugins(log_level, loaded.plugins.plugins());
    let mut run = instantiate_with_plugins(&loaded, options).context("instantiating")?;
    let ticks = args.ticks.unwrap_or(run.duration_ticks);
    let format = if args.json {
        Format::Json
    } else {
        Format::Text
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut write_error: Option<io::Error> = None;
    let (print_every, hash_every) = (args.print_every, args.hash_every);
    let due = |tick: u64, every: u64| every != 0 && tick.is_multiple_of(every);
    let names = output::name_map(&run.entities);
    // A write error (for example a closed pipe) stops the run (ControlFlow::Break).
    // Run control reaches the network around the run (ASM-295); a fault under `abort` ends it.
    if let Some(fault) = run.engine.publish_control(ControlAction::Start) {
        return Err(anyhow!(fault.to_string())).context("starting the run");
    }
    let stepped = run.engine.step(ticks, |view| {
        let tick = view.report.time.tick();
        let mut write = || -> io::Result<()> {
            if due(tick, print_every) {
                output::write_entities(&mut out, format, &view.snapshot(), &names)?;
            }
            if due(tick, hash_every) {
                output::write_tick_hash(&mut out, format, tick, view.state_hash())?;
            }
            Ok(())
        };
        match write() {
            Ok(()) => ControlFlow::Continue(()),
            Err(e) => {
                write_error = Some(e);
                ControlFlow::Break(())
            }
        }
    });
    let stop_fault = run.engine.publish_control(ControlAction::Stop);
    log_net(log_level, &run.engine);
    stepped.context("running the scenario")?;
    if let Some(fault) = stop_fault {
        return Err(anyhow!(fault.to_string())).context("stopping the run");
    }
    if let Some(e) = write_error {
        return Err(e).context("writing output");
    }
    output::write_final(
        &mut out,
        format,
        run.engine.now().tick(),
        run.engine.state_hash(),
    )
    .context("writing output")?;
    out.flush().context("flushing output")?;
    Ok(ExitCode::SUCCESS)
}

/// One stderr line per loaded plugin when the log level admits info (ASM-113). Nothing is printed
/// without plugins, and stdout is never touched.
fn log_plugins(level: LevelFilter, plugins: &[PluginInfo]) {
    if level < LevelFilter::INFO {
        return;
    }
    let mut err = io::stderr().lock();
    for p in plugins {
        // Nothing sensible remains to do if stderr is gone.
        let _ = writeln!(
            err,
            "info: loaded plugin {} {} (abi {}.{}) from {}",
            p.id, p.version, p.abi.0, p.abi.1, p.origin
        );
    }
}

/// One stderr line per network adapter when the log level admits info (ASM-250). Nothing is printed
/// without adapters, and stdout is never touched.
fn log_net(level: LevelFilter, engine: &umbra_engine::Engine) {
    if level < LevelFilter::INFO {
        return;
    }
    let mut err = io::stderr().lock();
    for status in engine
        .network()
        .map(umbra_engine::NetLink::status)
        .unwrap_or_default()
    {
        // Nothing sensible remains to do if stderr is gone.
        let _ = writeln!(err, "{}", umbra_engine::net_summary(&status));
    }
}

/// One stderr line naming the configuration file in use when the log level admits info (ADR 0029).
fn log_config(level: LevelFilter, path: Option<&Path>) {
    if level < LevelFilter::INFO {
        return;
    }
    if let Some(path) = path {
        // Nothing sensible remains to do if stderr is gone.
        let _ = writeln!(io::stderr(), "info: using configuration {}", path.display());
    }
}

fn list_plugins(args: &ListPluginsArgs, config_flag: Option<&Path>) -> Result<ExitCode> {
    let configuration = match config::load(config_flag)? {
        Outcome::Loaded(loaded) => loaded,
        Outcome::Invalid(diagnostics) => return Ok(report_invalid(&diagnostics)),
    };
    let registry = standard_registry().context("building the component registry")?;
    let flag_paths = config::plugin_paths(&args.plugin_path, &configuration.config);
    let paths = resolve_plugin_paths(&flag_paths, args.scenario.as_deref());
    let (_, host) =
        load_plugins(&registry, &paths, HostOptions::default()).context("loading the plugins")?;
    let text = if args.json {
        output::plugins_json(host.plugins())
    } else {
        output::plugins_text(host.plugins())
    };
    writeln!(io::stdout().lock(), "{text}").context("writing output")?;
    Ok(ExitCode::SUCCESS)
}

fn validate(args: &ValidateArgs, config_flag: Option<&Path>) -> Result<ExitCode> {
    let configuration = match config::load(config_flag)? {
        Outcome::Loaded(loaded) => loaded,
        Outcome::Invalid(diagnostics) => return Ok(report_invalid(&diagnostics)),
    };
    let registry = standard_registry().context("building the component registry")?;
    let plugin_paths = config::plugin_paths(&args.plugin_path, &configuration.config);
    let settings = config::load_settings(&configuration.config);
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    let mut warnings = Vec::new();
    for path in &args.paths {
        let report =
            validate_path_with_plugins_and_settings(&registry, path, &plugin_paths, &settings)
                .context("validating")?;
        for (file, rate) in &report.defaulted_tick_rates {
            note_default_tick_rate(file, *rate);
        }
        warnings.extend(report.warnings);
        files.extend(report.files_checked);
        diagnostics.extend(report.diagnostics);
    }
    // Overlapping paths (a directory and a file in it) must not double-count.
    files.sort();
    files.dedup();
    let diagnostics = Diagnostics::new(diagnostics).map_or_else(Vec::new, Diagnostics::into_vec);
    let warnings = Diagnostics::new(warnings).map_or_else(Vec::new, Diagnostics::into_vec);
    if !args.json {
        // Warnings never fail the check and stay off stdout, like the default-rate note; in
        // `--json` mode they are in the document only (ASM-290).
        for warning in &warnings {
            let _ = writeln!(io::stderr(), "{warning}");
        }
    }
    let mut out = io::stdout().lock();
    if args.json {
        writeln!(
            out,
            "{}",
            output::validation_json(&files, &diagnostics, &warnings)
        )
    } else {
        diagnostics
            .iter()
            .try_for_each(|d| writeln!(out, "{d}"))
            .and_then(|()| {
                writeln!(
                    out,
                    "{}",
                    output::validation_summary(files.len(), &diagnostics)
                )
            })
    }
    .context("writing output")?;
    Ok(if diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn schema(args: &SchemaArgs, config_flag: Option<&Path>) -> Result<ExitCode> {
    // `schema` uses no setting, but a named file must still exist and be valid (ADR 0029).
    if config_flag.is_some()
        && let Outcome::Invalid(diagnostics) = config::load(config_flag)?
    {
        return Ok(report_invalid(&diagnostics));
    }
    let text = match &args.which {
        SchemaKind::Template => TEMPLATE_FILE_SCHEMA.to_string(),
        SchemaKind::Scenario => SCENARIO_FILE_SCHEMA.to_string(),
        SchemaKind::Config => CONFIG_FILE_SCHEMA.to_string(),
        SchemaKind::Behavior => BEHAVIOR_FILE_SCHEMA.to_string(),
        SchemaKind::Component { id } => component_schema(id)?,
    };
    writeln!(io::stdout().lock(), "{}", text.trim_end()).context("writing output")?;
    Ok(ExitCode::SUCCESS)
}

/// The pretty-printed parameter schema of component `id`, searching every kind (ASM-063).
fn component_schema(id: &str) -> Result<String> {
    let registry = standard_registry().context("building the component registry")?;
    let id = ComponentId::new(id).with_context(|| format!("invalid component id {id:?}"))?;
    ComponentKind::ALL
        .iter()
        .find_map(|&kind| registry.param_schema(kind, &id))
        .ok_or_else(|| anyhow!("no component with id {id} is registered"))
        .and_then(|s| Ok(serde_json::to_string_pretty(s.as_json())?))
}
