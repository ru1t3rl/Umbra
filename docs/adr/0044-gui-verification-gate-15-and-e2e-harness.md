# ADR 0044: GUI verification: gate 15 from M5b, `bun run verify`, the e2e harness

- **Status**: Accepted by the human 2026-10-20 (proposed by frontend-architect, M5b plan `docs/plans/M5b.md`, 2026-10-20)
- **Requirement IDs**: ADR 0004 / D-19 (no CI, local gate list), REQ-A-4, REQ-X-4, A-002 (M5b acceptance 1-2), A-003 (acceptance), NFR-3 (Linux only, human decision 2026-10-19)
- **Affected**: `docs/build.md` (row 15, prerequisites), `scripts/gates.sh`, skill `verify-frontend`, `gui/package.json` (`verify`), `gui/playwright.config.ts`, `gui/e2e/`, decision row D-60

## Context
`docs/build.md` row 15 is "from M8" and runs only with `scripts/gates.sh --gui`; its command lacks
`gen:tokens --check`, `check:tokens`, `check:contrast` (A-003), licences (ADR 0043) and `test:e2e`
(M5b acceptance 2). The `verify-frontend` skill and gate 15 list the steps twice. M5b's e2e needs a real
headless `umbra serve` in stepped mode. The M6 track runs the engine gate in parallel and must not need
Bun or Chromium.

## Options
1. **One ordered script `bun run verify` in `gui/package.json`; gate 15 = `cd gui && bun install
   --frozen-lockfile && bun run verify`; active from M5b; run by `scripts/gates.sh --gui` (chosen).**
2. Make gate 15 unconditional in `scripts/gates.sh`. Cons: every engine run (M6) then needs Bun, Node and
   Chromium; merge friction with the parallel track.
3. Keep "from M8". Cons: M5b acceptance 2 would have no gate.

## Decision
Option 1.
- **`verify` order:** `gen:api --check`, `gen:tokens --check`, `check:tokens`, `check:contrast`,
  `check:licenses`, `typecheck`, `lint`, `check:deps`, `test`, `build`, `test:e2e`. Stop at the first
  failure. The skill and `docs/build.md` reference `bun run verify` and list the steps once (the skill).
- **When:** `scripts/gates.sh --gui` is the definition of done for any change under `gui/` or `api/`
  (an `api/*.json` change must keep `gen:api --check` green). Plain `scripts/gates.sh` prints
  `skip 15-gui (pass --gui)`. `bun run tauri build` on Windows stays M8.
- **Order in `gates.sh`:** gate 15 runs after gate 9, so `target/release/umbra` exists for `test:e2e`.
- **e2e harness** (`gui/e2e/`, Playwright, Chromium only, Linux baselines):
  - `globalSetup` starts `$UMBRA_BIN` (default `../target/release/umbra`; it never builds it) as
    `umbra serve --bind 127.0.0.1:0 --scenario-root ../examples` from the repository root, reads the port
    from the `listening on http://...` line, then starts `vite preview` of the e2e build with the proxy
    pointed at it; `globalTeardown` stops both (SIGTERM, then SIGKILL after 5 s).
  - Engine tests load `scenarios/walking_skeleton/scenario.toml` with `{"mode":"stepped"}` and seed `42`
    through the generated client, start the run, and step through the generated client. `workers: 1`.
  - The e2e build (`vite build --mode e2e`, `VITE_UMBRA_E2E=1`) exposes a read-only
    `window.__UMBRA_E2E__.map` inspection (source positions, rendered features, projected pixels, idle);
    a test asserts the production build contains no `__UMBRA_E2E__`.
  - Headless WebGL through SwiftShader (`--use-angle=swiftshader`, `--enable-unsafe-swiftshader`).
  - Screenshot tests (showcase, both themes) use the self-hosted fonts, reduced motion, a fixed
    viewport, and mask every map canvas; baselines are committed and Linux-only.
  - An axe check per shell if `@axe-core/playwright` is accepted (ADR 0043).

## Consequences
- Easier: one command for humans and agents; the engine track is unaffected; M8 inherits the harness.
- Harder: gate 15 needs Bun, Node LTS and the Playwright Chromium; e2e depends on the release binary.
- Revisit in M8 (Tauri build on Windows) or if Windows/macOS screenshots become required.
