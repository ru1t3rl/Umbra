# CGF agent pack

Claude Code configuration for building the CGF engine from the specification.

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

## Tips
- Run one milestone per session and review the plan before implementation starts.
- Several skills call `cargo xtask ...` commands; M0 should create the `xtask` crate (`check-deps`, later `gen-api`, `gen-header`, `build-plugins`).

## License
Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option (ADR 0001).
