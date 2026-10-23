<p align="center"><img src="umbra_icon.svg" alt="Umbra" width="160" height="160"></p>

# Umbra agent pack

Claude Code configuration for building the Umbra engine from the specification.

## Install
1. Copy `.claude/`, `CLAUDE.md` and `docs/` into the root of your (empty) repository.
2. Export the spec document to Markdown and save it as `docs/SPEC.md`.
3. Open the repo in Claude Code and run: `Use the implement-milestone skill for M0.`

## Contents
- `CLAUDE.md`: global rules and team overview, loaded in every session
- `.claude/agents/`: architect, rust-engineer, plugin-abi-engineer, network-engineer, api-engineer, frontend-architect, frontend-engineer, frontend-test-engineer, test-engineer, code-reviewer, build-engineer
- `.claude/skills/`: implement-milestone, add-crate, add-plugin-kind, add-network-adapter, write-tests, update-openapi, record-decision, verify-and-build, review-change; frontend: add-feature, add-feature-shell, generate-api-client, write-frontend-tests, verify-frontend, review-frontend-change
- `docs/milestones.md`: progress tracker; `docs/assumptions.md` and `docs/adr/`: decisions the agents record

## Frontend
The GUI lives in `gui/` (Bun workspaces): `features/` and `feature-shells/` are libraries, `src/` is the thin app. Rules in `gui/CLAUDE.md` and `docs/frontend-architecture.md`. Start with: `Use frontend-architect to plan the GUI scaffold, then add-feature for ui-kit.`

## Running scenarios
- `cargo run -p umbra-cli -- run examples/scenarios/walking_skeleton/scenario.toml [--ticks N] [--seed S] [--clock afap|realtime|stepped|scaled=F] [--print-every N] [--hash-every N] [--json] [--log-level LEVEL] [--record FILE [--record-hash-every N]]` runs a scenario file headless and prints entity state and a final `state_hash=<hex>` line. `--record` also writes a recording of the run (inputs and state hashes, ADR 0053) and leaves the output unchanged; if the file cannot be written the run still completes and the exit code is 1.
- `cargo run -p umbra-cli -- validate <path>... [--json]` checks scenario and template files (or directories) without running them; exit 0 valid, 1 invalid, 2 usage.
- `cargo run -p umbra-cli -- schema template|scenario|config|component <ID>` prints a JSON Schema.
- `cargo run -p umbra-cli -- serve [--bind ADDR] [--scenario PATH] [--scenario-root DIR] [--record-dir DIR]` (recordings go to `DIR/serve-<k>/session-<n>.umbrarec.jsonl`, a new `serve-<k>` per start) serves the REST API (`/api/v1`) and the WebSocket stream (`/api/v1/stream`) on `127.0.0.1:7878` by default, with no authentication (localhost only unless you bind otherwise). Server settings and the default tick rate come from an optional `umbra.toml`. Reference, examples and the configuration keys: [`docs/api.md`](docs/api.md); the contract files are `api/openapi.json`, `api/stream.schema.json` and `api/asyncapi.json`.

## Tips
- Run one milestone per session and review the plan before implementation starts.
- Several skills call `cargo xtask ...` commands; M0 should create the `xtask` crate (`check-deps`, later `gen-api`, `gen-header`, `build-plugins`).

## License
Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option (ADR 0001).
