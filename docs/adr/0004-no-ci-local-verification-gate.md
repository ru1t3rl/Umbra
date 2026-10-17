# ADR 0004: No CI; the local verification run is the quality gate

- **Status**: Accepted (decided by the human project owner on 2026-10-17)
- **Requirement IDs**: NFR-3, NFR-4, REQ-X-4, spec section 4 ("CI-enforced"), section 15 (CI jobs), section 17 (M0 acceptance)
- **Affected**: `.github/workflows/ci.yml`, `docs/build.md`, `docs/plans/M0.md`, `docs/milestones.md`, the `verify-and-build` skill

## Context
The spec assumes a hosted CI: section 4 rules are "CI-enforced", NFR-3 asks for Windows and Linux CI
builds, NFR-4 runs `cargo audit`/`cargo deny` in CI, section 15 lists CI jobs, REQ-X-4 has a CI drift
check, and M0 acceptance 1 is "CI green on Windows + Linux". The project has no remote, and the human
has decided the project will not use CI.

## Options
1. **Keep CI as specified.** Matches the spec; needs a remote and runners the project will not have.
2. **No CI; every gate runs locally via `verify-and-build` (chosen).** Same commands (fmt, clippy
   `-D warnings`, tests, doc, `cargo xtask check-deps`, `cargo deny`, `cargo audit`, later the
   determinism gate and OpenAPI drift check), run by whoever declares a task done (CLAUDE.md rule 8).
   No automatic enforcement on every change, and no Windows run unless someone runs it on Windows.

## Decision
Option 2. Wherever the spec says "CI", read "the local verification run (`verify-and-build`)". The
checks themselves are unchanged: every gate stays a scripted command, so CI can be added back later
without code changes. M0 acceptance 1 ("CI green on Windows + Linux") is waived; it is replaced by
"the full local verification run passes on Linux".

## Consequences
- Easier: no remote, runner or secret management.
- Harder: gates are only as good as the discipline of running them; nothing blocks a commit that skips
  them. Windows portability (NFR-3) is unverified until someone runs `verify-and-build` on Windows;
  record that run as evidence when it happens.
- `.github/workflows/ci.yml` (WI-2) is kept as a dormant reference unless the human asks to delete it.
- Revisit if the project gets a shared remote, more contributors, or a Windows deliverable (M8).
