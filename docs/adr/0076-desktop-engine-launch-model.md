# ADR 0076: Desktop engine launch model: managed sidecar by default, remote engine as an option

- **Status**: Proposed (frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22)
- **Requirement IDs**: D-13 ("can spawn engine sidecar"), REQ-G-1, REQ-G-6, spec section 13 ("may spawn `umbra` as a sidecar or connect to a remote engine URL"), A-002 M8 ("sidecar `umbra`"), ASM-311
- **Affected**: `gui/src-tauri`, `@umbra/desktop`, `@umbra/shell-settings`, decision row D-86

## Context
The Windows app must show live entities from a running engine. Users either want a self-contained app
(double-click, run a scenario) or want to watch an engine that runs elsewhere (a server or a lab
machine). The engine binary is `umbra serve`. On startup it prints `listening on http://<addr>`, it
accepts `--bind 127.0.0.1:0`, `--scenario-root`, `--plugin-path` and `--config`, and it shuts down
gracefully on Ctrl-C (Windows) or SIGTERM. The GUI must not contain engine source (REQ-A-4). Bundling
the release binary is allowed, because it is a separate process and not source.

## Options
1. **Two modes, managed by default (chosen).**
   - *Managed:* the app spawns the bundled `umbra serve --bind 127.0.0.1:0 --scenario-root <root>
     [--plugin-path <dir>]` as a Tauri sidecar, parses the port from the `listening on` line, points
     the gateway (ADR 0075) at it, and stops it on exit.
   - *Remote:* the gateway targets a user-given URL.
2. Remote only. Cons: not a standalone Windows app; users must run a terminal.
3. Managed only. Cons: cannot watch a remote engine, which the spec explicitly allows.
4. Link the engine into the Tauri process. Cons: violates D-1 and REQ-A-4; the GUI process would
   carry engine faults and plugins.

## Decision
Option 1.
- **Sidecar packaging.** `externalBin` is `binaries/umbra`. `bun run tauri:prepare` copies
  `target/release/umbra[.exe]` to `gui/src-tauri/binaries/umbra-<target-triple>[.exe]` (ignored by
  git). The GUI build never builds the engine (like ASM-311).
- **Scenario root.** The default is the bundled `examples/scenarios` resources (read-only). It can be
  changed in settings to a user directory. The plugin path is optional (ASM-840).
- **Lifecycle.** The engine starts when the app starts in managed mode. It is restarted when the
  engine settings change and is stopped on exit: a graceful stop first (a Ctrl-C console event on
  Windows, SIGTERM elsewhere), then a kill after the server's 5 s grace. If it crashes, the status
  becomes `exited` with the exit code and the last output lines, and the user can press "Restart
  engine". There is no automatic restart loop.
- **Bridge.** IPC commands `engine_status`, `restart_engine`, `load_settings`, `save_settings`, and an
  event `engine_output` (line, stream). These are exposed to the frontend only by `@umbra/desktop`
  (`@tauri-apps/api` is confined there by a dependency-cruiser rule).
- **Remote mode.** The URL must be `http(s)://host[:port]`. The gateway forwards to it with `Host` set
  to the target, so a non-loopback engine needs that name in its `[server] allowed_hosts` (documented).
- **Icons.** Bundle icons are generated at build time from the repository-root `umbra_icon.svg` with
  `tauri icon` into `gui/src-tauri/icons/` (ignored). They are never committed and never redrawn
  (M5b decision, ASM-842).

## Consequences
- Easier: a self-contained Windows app, and the remote case with the same UI.
- Harder: process supervision on Windows. A crash of the GUI process can orphan the engine; the
  sidecar is started in a Windows job object with kill-on-close where Tauri's shell API allows it,
  otherwise this is documented (ASM-840).
- Revisit when users need several engines at once, or the engine gains a "parent died" watchdog.
