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

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use xtask::deps::{RULES, ResolvedGraph, Violation, Workspace, check_workspace};

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
    let mut args = std::env::args_os().skip(1);
    let Some(sub) = args.next() else {
        bail!("usage: cargo xtask check-deps [--manifest-path <path>]");
    };
    match sub.to_str() {
        Some("check-deps") => check_deps(args),
        _ => bail!(
            "unknown subcommand `{}`; available: check-deps",
            sub.to_string_lossy()
        ),
    }
}

fn check_deps(mut args: impl Iterator<Item = OsString>) -> Result<bool> {
    // Without --manifest-path cargo searches upward from the working directory.
    let mut manifest: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        let value: PathBuf = match arg.to_str() {
            Some("--manifest-path") => args.next().context("--manifest-path needs a value")?.into(),
            Some(a) => match a.strip_prefix("--manifest-path=") {
                Some(v) => v.into(),
                None => bail!("unknown argument `{a}`"),
            },
            None if arg.to_string_lossy().starts_with("--manifest-path=") => {
                bail!("--manifest-path value is not UTF-8; use `--manifest-path <path>`")
            }
            None => bail!("unknown argument `{}`", arg.to_string_lossy()),
        };
        if manifest.replace(value).is_some() {
            bail!("--manifest-path given more than once");
        }
    }
    // Declared pass first, so its diagnostics are printed even if resolution fails (exit 2).
    let mut ws = Workspace::load_declared(manifest.as_deref())?;
    match ResolvedGraph::load(manifest.as_deref()) {
        Ok(resolved) => ws.resolved = Some(resolved),
        Err(e) => {
            print_violations(&check_workspace(&ws, &RULES));
            return Err(e).context("resolved graph could not be loaded, check incomplete");
        }
    }
    let violations = check_workspace(&ws, &RULES);
    print_violations(&violations);
    if violations.is_empty() {
        println!("check-deps: ok ({} crates)", ws.graph.members.len());
    } else {
        println!("check-deps: {} violation(s)", violations.len());
    }
    Ok(violations.is_empty())
}

fn print_violations(violations: &[Violation]) {
    for v in violations {
        println!("{}", v.render(&RULES));
    }
}
