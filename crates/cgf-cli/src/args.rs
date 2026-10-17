//! Command-line arguments (REQ-X-2 minimal, ASM-027).

use clap::{Parser, Subcommand};

use cgf_engine::{ClockMode, TickRate, TimeScale};

/// The CGF Engine command line.
#[derive(Debug, Parser)]
#[command(name = "cgf-cli", version, about = "CGF Engine command-line interface")]
pub struct Cli {
    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Commands. Only `demo` exists in M1.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the built-in walking-skeleton scenario and print entity state and the state hash.
    Demo(DemoArgs),
}

/// Options of `demo`.
#[derive(Debug, clap::Args)]
pub struct DemoArgs {
    /// Number of ticks to run.
    #[arg(long, default_value_t = 900)]
    pub ticks: u64,
    /// Seed of the simulation's random streams.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    /// Tick rate in whole hertz.
    #[arg(long, default_value = "30", value_parser = parse_rate)]
    pub rate: TickRate,
    /// Clock mode: afap, realtime, stepped or `scaled=FACTOR`.
    #[arg(long, default_value = "afap", value_parser = parse_clock)]
    pub clock: ClockMode,
    /// Print entity state every N ticks (0 prints none).
    #[arg(long, default_value_t = 30)]
    pub print_every: u64,
    /// Print JSON Lines instead of text.
    #[arg(long)]
    pub json: bool,
    /// Log level: error, warn, info, debug or trace.
    #[arg(long, default_value = "warn")]
    pub log_level: String,
}

fn parse_rate(s: &str) -> Result<TickRate, String> {
    let hz: u32 = s.parse().map_err(|e| format!("invalid rate {s:?}: {e}"))?;
    TickRate::new(hz).ok_or_else(|| "rate must be at least 1 Hz".to_string())
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
