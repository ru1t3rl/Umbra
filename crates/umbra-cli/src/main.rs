//! `umbra-cli`: the command-line front end of Umbra.
//!
//! Responsibility: `run <scenario.toml>` (headless run printing entity state and the state hash,
//! REQ-S-1, REQ-X-2), `validate <path>...` (REQ-T-6) and `schema` (D-14). Exit code 0 on success,
//! 1 on invalid input or a runtime error, 2 on a usage error (clap).
//!
//! Dependency rule (REQ-A-2): depends on `umbra-engine` only in M2; it may also depend on
//! `umbra-server` from M5. It contains no `unsafe` code.

#![forbid(unsafe_code)]

mod args;
mod output;

use std::io::{self, Write};
use std::ops::ControlFlow;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use tracing_subscriber::filter::LevelFilter;

use args::{Cli, Command, ListPluginsArgs, RunArgs, SchemaArgs, SchemaKind, ValidateArgs};
use output::Format;
use umbra_engine::builtins::standard_registry;
use umbra_engine::scenario::{Diagnostic, Diagnostics, RunOptions, ScenarioError, TickRateSource};
use umbra_engine::{
    BEHAVIOR_FILE_SCHEMA, ComponentId, ComponentKind, HostOptions, PluginInfo,
    SCENARIO_FILE_SCHEMA, StdWallClock, TEMPLATE_FILE_SCHEMA, TickRate, instantiate_with_plugins,
    load_plugins, load_scenario_with_plugins, resolve_plugin_paths, validate_path_with_plugins,
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
    let result = match cli.command {
        Command::Run(args) => run(&args),
        Command::Validate(args) => validate(&args),
        Command::Schema(args) => schema(&args),
        Command::ListPlugins(args) => list_plugins(&args),
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

fn init_logging(level: LevelFilter) -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(io::stderr)
        .with_ansi(false)
        .try_init()
        .map_err(|e| anyhow!("cannot install the log subscriber: {e}"))
}

/// Prints every diagnostic to stderr; exit code 1.
fn report_invalid(diagnostics: &[Diagnostic]) -> ExitCode {
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

fn run(args: &RunArgs) -> Result<ExitCode> {
    init_logging(args.log_level)?;
    let registry = standard_registry().context("building the component registry")?;
    let loaded = match load_scenario_with_plugins(&registry, &args.scenario, &args.plugin_path) {
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
    log_plugins(args.log_level, loaded.plugins.plugins());
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
    run.engine
        .step(ticks, |view| {
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
        })
        .context("running the scenario")?;
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

fn list_plugins(args: &ListPluginsArgs) -> Result<ExitCode> {
    let registry = standard_registry().context("building the component registry")?;
    let paths = resolve_plugin_paths(&args.plugin_path, args.scenario.as_deref());
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

fn validate(args: &ValidateArgs) -> Result<ExitCode> {
    let registry = standard_registry().context("building the component registry")?;
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    for path in &args.paths {
        let report =
            validate_path_with_plugins(&registry, path, &args.plugin_path).context("validating")?;
        for (file, rate) in &report.defaulted_tick_rates {
            note_default_tick_rate(file, *rate);
        }
        files.extend(report.files_checked);
        diagnostics.extend(report.diagnostics);
    }
    // Overlapping paths (a directory and a file in it) must not double-count.
    files.sort();
    files.dedup();
    let diagnostics = Diagnostics::new(diagnostics).map_or_else(Vec::new, Diagnostics::into_vec);
    let mut out = io::stdout().lock();
    if args.json {
        writeln!(out, "{}", output::validation_json(&files, &diagnostics))
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

fn schema(args: &SchemaArgs) -> Result<ExitCode> {
    let text = match &args.which {
        SchemaKind::Template => TEMPLATE_FILE_SCHEMA.to_string(),
        SchemaKind::Scenario => SCENARIO_FILE_SCHEMA.to_string(),
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
