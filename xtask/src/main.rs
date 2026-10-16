//! `cargo xtask` entry point. STUB (M0 step 4): implemented in WI-3.
//!
//! CLI contract: `cargo xtask check-deps [--manifest-path <path>]`; exit 0 clean,
//! 1 on violations, 2 on load error.

#![forbid(unsafe_code)]

#[expect(clippy::todo, reason = "stub until WI-3")]
fn main() {
    todo!("WI-3: parse args, load_graph, check, print violations, exit code")
}
