//! `cargo xtask` entry point.
//!
//! CLI contract: `cargo xtask check-deps [--manifest-path <path>]`; exit 0 clean,
//! 1 on violations, 2 on load or usage error.

#![forbid(unsafe_code)]
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI binary: output is its interface"
)]

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use xtask::deps::{RULES, check, check_forbid_unsafe, load_crate_roots, load_graph};

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

/// Returns `Ok(true)` when the workspace conforms.
fn run() -> Result<bool> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check-deps") => check_deps(args),
        Some(other) => bail!("unknown subcommand `{other}`; available: check-deps"),
        None => bail!("usage: cargo xtask check-deps [--manifest-path <path>]"),
    }
}

fn check_deps(mut args: impl Iterator<Item = String>) -> Result<bool> {
    let mut manifest = PathBuf::from("Cargo.toml");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest-path" => {
                manifest = args.next().context("--manifest-path needs a value")?.into();
            }
            other => bail!("unknown argument `{other}`"),
        }
    }
    let graph = load_graph(&manifest)?;
    let roots = load_crate_roots(&manifest)?;
    let mut violations = check(&graph, &RULES);
    violations.extend(check_forbid_unsafe(&roots));
    violations.sort();
    for v in &violations {
        println!("{v}");
    }
    if violations.is_empty() {
        println!("check-deps: ok ({} crates)", graph.members.len());
    } else {
        println!("check-deps: {} violation(s)", violations.len());
    }
    Ok(violations.is_empty())
}
