# ADR 0003: Scope of determinism across platforms

- **Status**: Accepted (approved by the human project owner on 2026-10-17)
- **Requirement IDs**: A-2, REQ-K-4, REQ-K-7, A-3, NFR-3, section 15 (determinism tests)
- **Affected**: `umbra-core` (math helpers), `umbra-kernel`, `umbra-motion`, test harness, CI

## Context
A-2 says that the same scenario, seed, plugin versions and input stream give identical results. It does not
say whether this must hold **across platforms** (Windows x64 against Linux x64, A-3) or only within one build
on one platform. Basic IEEE-754 operations (+, -, *, /, sqrt) are bit-reproducible on x86_64. Transcendental
functions (`sin`, `cos`, `atan2`, `exp`) come from the platform libm and can differ in the last ulp between
Windows (MSVC CRT) and Linux (glibc). Coordinate conversions (REQ-C-3, M1) and motion models depend on these
functions. Recording and replay (REQ-K-7) make this matter in practice: can a recording made on Windows be
replayed on Linux and give the same hashes?

## Options
1. **Same platform and build only (v1).** The determinism test runs on each OS and compares two runs on that
   OS. Pros: no cost, std math. Cons: hashes from a recording made on another OS may diverge.
2. **Cross-platform bit-identical.** Use a pure-Rust `libm` crate for all transcendental functions in
   simulation code, forbid `mul_add` and FMA contraction, and add a CI job that compares Windows and Linux
   state hashes. Pros: portable replays. Cons: a small speed cost, a lint/wrapper discipline (route math
   through a `umbra-core` math module), and plugins must follow it too (they cannot be forced to).
3. **Tolerance-based comparison across platforms.** Pros: cheap. Cons: weakens the hash-based gate. Chaotic
   behavior (FSM thresholds) can still branch differently.

## Decision (proposed)
Option 1 for v1. The determinism gate runs on every OS in the CI matrix but compares within one OS only.
Cross-OS replay is documented as not guaranteed. To keep option 2 cheap later, from M1 on all
transcendental math in simulation crates goes through one `umbra-core` math module. That is the
single point where `libm` would be swapped in.

## Consequences
- M0 has no effect beyond this record (the core types in M0 use only `+ - * /`).
- M1 must create the math module and a clippy `disallowed-methods` entry for direct `f64::sin` and similar
  calls in simulation crates, if the human prefers to keep option 2 open cheaply.
- Revisit if cross-OS replay or mixed-OS federations that compare hashes become a requirement.
