//! `cargo xtask` entry point.
//!
//! CLI contract (exit 0 clean, 1 on violations or drift, 2 on load, tool or usage error):
//! - `cargo xtask check-deps [--manifest-path <path>]` (REQ-A-1..3, NFR-2);
//! - `cargo xtask gen-header [--check] [--crate-dir <dir>]` (REQ-SDK-2, gate 11); the crate directory
//!   defaults to `crates/umbra-plugin-api`.

#![forbid(unsafe_code)]
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI binary: output is its interface"
)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use xtask::deps::{RULES, ResolvedGraph, Violation, Workspace, check_workspace};
use xtask::header::{self, Outcome};

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
        bail!(
            "usage: cargo xtask check-deps [--manifest-path <path>] | gen-header [--check] [--crate-dir <dir>]"
        );
    };
    match sub.to_str() {
        Some("check-deps") => check_deps(args),
        Some("gen-header") => gen_header(args),
        _ => bail!(
            "unknown subcommand `{}`; available: check-deps, gen-header",
            sub.to_string_lossy()
        ),
    }
}

fn gen_header(mut args: impl Iterator<Item = OsString>) -> Result<bool> {
    let mut check = false;
    let mut crate_dir: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--check") => check = true,
            Some("--crate-dir") => {
                let dir = args.next().context("--crate-dir needs a value")?;
                if crate_dir.replace(dir.into()).is_some() {
                    bail!("--crate-dir given more than once");
                }
            }
            _ => bail!("unknown argument `{}`", arg.to_string_lossy()),
        }
    }
    let crate_dir = crate_dir.unwrap_or_else(|| {
        let xtask_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        xtask_dir
            .parent()
            .unwrap_or(xtask_dir)
            .join("crates/umbra-plugin-api")
    });
    match header::run(&crate_dir, check)? {
        Outcome::Current => {
            println!(
                "gen-header: ok ({} is current)",
                crate_dir.join(header::HEADER_PATH).display()
            );
            Ok(true)
        }
        Outcome::Drift { path, line } => {
            println!(
                "gen-header: {} differs from the generated header from line {line}; run `cargo xtask gen-header`",
                path.display()
            );
            Ok(false)
        }
        Outcome::Written(path) => {
            println!("gen-header: wrote {}", path.display());
            Ok(true)
        }
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
