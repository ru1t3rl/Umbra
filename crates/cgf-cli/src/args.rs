//! Command-line arguments (REQ-X-2, REQ-T-6, ASM-062).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use cgf_engine::{ClockMode, TimeScale};
use tracing_subscriber::filter::LevelFilter;

/// The CGF Engine command line.
#[derive(Debug, Parser)]
#[command(name = "cgf-cli", version, about = "CGF Engine command-line interface")]
pub struct Cli {
    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a scenario file headless and print entity state and the state hash.
    Run(RunArgs),
    /// Check scenario and template files without running anything.
    Validate(ValidateArgs),
    /// Print a JSON Schema: the file formats or a registered component's parameters.
    Schema(SchemaArgs),
}

/// Options of `run`.
#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// The scenario file.
    pub scenario: PathBuf,
    /// Number of ticks to run (default: the scenario's duration).
    #[arg(long)]
    pub ticks: Option<u64>,
    /// Replaces the scenario's seed.
    #[arg(long)]
    pub seed: Option<u64>,
    /// Replaces the scenario's clock: afap, realtime, stepped or `scaled=FACTOR`.
    #[arg(long, value_parser = parse_clock)]
    pub clock: Option<ClockMode>,
    /// Print entity state every N ticks (0 prints none).
    #[arg(long, default_value_t = 30)]
    pub print_every: u64,
    /// Print JSON Lines instead of text.
    #[arg(long)]
    pub json: bool,
    /// Print the state hash after every N ticks (0 prints none; the final hash is always printed).
    #[arg(long, default_value_t = 0)]
    pub hash_every: u64,
    /// Log level: off, error, warn, info, debug or trace.
    #[arg(long, default_value = "warn", value_parser = parse_level)]
    pub log_level: LevelFilter,
}

/// Options of `validate`.
#[derive(Debug, clap::Args)]
pub struct ValidateArgs {
    /// Scenario files, template files or directories (checked recursively).
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,
    /// Print one JSON document instead of text.
    #[arg(long)]
    pub json: bool,
}

/// Options of `schema`.
#[derive(Debug, clap::Args)]
pub struct SchemaArgs {
    /// Which schema to print.
    #[command(subcommand)]
    pub which: SchemaKind,
}

/// The schemas `schema` can print.
#[derive(Debug, Subcommand)]
pub enum SchemaKind {
    /// The template file format.
    Template,
    /// The scenario file format.
    Scenario,
    /// The parameter schema of a registered component, of any kind.
    Component {
        /// The component ID, for example `builtin.ground.kinematic`.
        id: String,
    },
}

fn parse_level(s: &str) -> Result<LevelFilter, String> {
    s.parse()
        .map_err(|e| format!("invalid log level {s:?}: {e}"))
}

fn parse_clock(s: &str) -> Result<ClockMode, String> {
    match s {
        "afap" => Ok(ClockMode::AsFastAsPossible),
        "realtime" => Ok(ClockMode::RealTime),
        "stepped" => Ok(ClockMode::Stepped),
        _ => {
            let factor = s.strip_prefix("scaled=").ok_or_else(|| {
                format!("unknown clock mode {s:?}: use afap, realtime, stepped or scaled=<factor>")
            })?;
            let factor: f64 = factor
                .parse()
                .map_err(|e| format!("invalid scale {factor:?}: {e}"))?;
            let scale = TimeScale::new(factor).map_err(|e| e.to_string())?;
            Ok(ClockMode::Scaled(scale))
        }
    }
}
