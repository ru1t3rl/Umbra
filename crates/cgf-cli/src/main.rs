//! `cgf-cli`: the command-line front end of the CGF Engine.
//!
//! Responsibility: the `demo` command that runs the walking skeleton and prints entity state and
//! the state hash (REQ-X-2 minimal, ASM-027). Exit code 0 on success, 2 on a usage error (clap),
//! 1 on a runtime error.
//!
//! Dependency rule (REQ-A-2): depends on `cgf-engine` only in M1; it may also depend on
//! `cgf-server` from M5. It contains no `unsafe` code.

#![forbid(unsafe_code)]

mod args;
mod output;

use std::io::{self, Write};
use std::ops::ControlFlow;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::filter::LevelFilter;

use args::{Cli, Command, DemoArgs};
use cgf_engine::{Engine, EngineConfig, FlatTerrain, Meters, demo};
use output::Format;

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Demo(args) => run_demo(&args),
    }
}

fn init_logging(level: LevelFilter) -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(io::stderr)
        .with_ansi(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!("cannot install the log subscriber: {e}"))
}

fn run_demo(args: &DemoArgs) -> Result<()> {
    init_logging(args.log_level)?;
    let format = if args.json {
        Format::Json
    } else {
        Format::Text
    };
    let terrain = Arc::new(FlatTerrain {
        height: Meters(0.0),
    });
    let mut engine = Engine::new(
        EngineConfig {
            seed: args.seed,
            tick_rate: args.rate,
            clock: args.clock,
        },
        terrain,
    );
    demo::spawn_walking_skeleton(&mut engine).context("spawning the demo scenario")?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut write_error: Option<io::Error> = None;
    let (print_every, hash_every) = (args.print_every, args.hash_every);
    let due = |tick: u64, every: u64| every != 0 && tick.is_multiple_of(every);
    // A write error (for example a closed pipe) does not stop the simulation; it is reported after
    // the run (M2 may stop early).
    engine
        .step(args.ticks, |view| {
            let tick = view.report.time.tick();
            if write_error.is_some() {
                return ControlFlow::Continue(());
            }
            let mut write = || -> io::Result<()> {
                if due(tick, print_every) {
                    output::write_entities(&mut out, format, &view.snapshot())?;
                }
                if due(tick, hash_every) {
                    output::write_tick_hash(&mut out, format, tick, view.state_hash())?;
                }
                Ok(())
            };
            if let Err(e) = write() {
                write_error = Some(e);
            }
            ControlFlow::Continue(())
        })
        .context("running the demo")?;
    if let Some(e) = write_error {
        return Err(e).context("writing output");
    }
    output::write_final(&mut out, format, engine.now().tick(), engine.state_hash())
        .context("writing output")?;
    out.flush().context("flushing output")
}
