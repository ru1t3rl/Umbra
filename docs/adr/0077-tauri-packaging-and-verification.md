# ADR 0077: Tauri packaging and verification: own Cargo workspace, opt-in Tauri step in gate 15, Windows checked by the human

- **Status**: Accepted by the human 2026-10-22 (H-3: NSIS per-user, WebView2 `embedBootstrapper`, unsigned, examples bundled, identifier `org.umbra.desktop`; H-4: dependencies accepted, work stops on a licence failure; H-5: opt-in `--tauri` step; H-6: the human runs the Windows checklist after WI-23 and at close-out; proposed by frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22)
- **Requirement IDs**: D-13, D-19 / ADR 0004 (no CI, local gates), ADR 0044 (gate 15), NFR-3, NFR-4, root `CLAUDE.md` rules 5 and 8, REQ-A-4
- **Affected**: `gui/src-tauri` (new), `gui/package.json` scripts, `scripts/gates.sh` (gate 15 block only), `docs/build.md` (row 15, prerequisites), `deny.toml` (read, not changed, unless H-4 says so), decision row D-87

## Context
M8 packages the app for Windows with Tauri 2. The development machine is Linux, so no Windows build can
be verified there. The root Cargo workspace (`crates/*`, `xtask`) and its gates must stay unchanged
(ADR 0047). Tauri on Linux needs system libraries (WebKitGTK 4.1, libsoup 3 and others), which today's
gate 15 prerequisites do not include.

## Options
1. **`gui/src-tauri` as its own Cargo workspace; an opt-in Tauri step `scripts/gates.sh --gui --tauri`;
   desktop e2e through `tauri-driver`; the Windows installer built and checked by the human on
   Windows (chosen).**
2. Add `gui/src-tauri` to the root workspace. Cons: root `Cargo.toml` and `Cargo.lock` change; every
   engine gate (fmt, clippy, test, deny, audit) would pull in Tauri, its system libraries and its
   dependency tree.
3. Cross-compile the Windows installer on Linux (`cargo-xwin`, NSIS under Wine). Cons: fragile, not
   supported by Tauri for MSI, and still no runtime check of WebView2.
4. Package only and do no desktop e2e. Cons: nothing would prove "the Windows build shows live
   entities" except a manual look.

## Decision
Option 1.
- **Workspace.**
  - `gui/src-tauri/Cargo.toml` has its own `[workspace]` and commits its own `Cargo.lock`.
  - `#![forbid(unsafe_code)]`, clippy `-D warnings`, `rustfmt`.
  - No dependency on any `crates/` package. A test reads `cargo metadata` and fails on any path
    dependency outside `gui/` (REQ-A-4).
- **Bundle.** Windows target: NSIS installer (`.exe`), per-user install, WebView2 install mode per
  H-3; unsigned in M8 (H-3).
  - Identifier `org.umbra.desktop` (ASM-844); product name "Umbra".
  - Resources: `examples/scenarios` (text files only).
  - Sidecar and icons as ADR 0076.
  - Linux targets (`deb`, `AppImage`) are built only to prove the bundling path; they are not shipped
    in M8.
- **Checks on Linux (Tauri step, opt-in).** `bun run check:tauri` runs:
  1. `cargo fmt --check`, `cargo clippy --all-targets --locked -D warnings`, `cargo test --locked` in
     `gui/src-tauri`;
  2. `cargo deny check` with the root `deny.toml` (`--config ../../deny.toml`);
  3. `bun run tauri:prepare` (sidecar copy and icon generation);
  4. `bun run tauri build --bundles deb`;
  5. `bun run test:e2e:tauri`: WebdriverIO plus `tauri-driver` with WebKitWebDriver. It starts the
     built app in managed mode, waits for the walking_skeleton entities, selects one and reads the
     inspector.

  Plain `scripts/gates.sh --gui` does not change, so a Bun-only machine still passes gate 15. The
  Tauri step is mandatory for changes under `gui/src-tauri/` and `@umbra/desktop`.
- **Checks on Windows (human).** On a Windows 10 or 11 x64 machine with the prerequisites:
  1. `cargo build -p umbra-cli --release --locked`;
  2. `cd gui && bun install --frozen-lockfile && bun run tauri:prepare && bun run tauri build`;
  3. install the NSIS output;
  4. run the manual checklist in `gui/README.md` ("Windows acceptance");
  5. optionally `bun run test:e2e:tauri` (tauri-driver with msedgedriver).

  Evidence (screenshots, versions, installer hash) goes into `docs/milestones.md` (ASM-845).

## Consequences
- Easier: the engine gates and the root lockfile are untouched; the Linux machine proves everything
  that is not Windows-specific.
- Harder: Windows-only defects are found late, by hand. The engine-source rule for dependency-cruiser
  on Windows paths (M5b risk) is checked only there.
- Revisit when a Windows machine or runner becomes available to the agents (then make the Windows
  steps a gate), or when Tauri supports reliable cross-builds.
