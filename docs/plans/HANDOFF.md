# Handoff: M7 and M8 in parallel (paused 2026-10-22)

Read this first when resuming. M7 (engine) and M8 (GUI) run in parallel under ADR 0047 (D-63). Plans,
running logs and assumptions stay the detailed source: `docs/plans/M7.md`, `docs/plans/M8.md`,
`docs/assumptions/M7.md`, `docs/assumptions/M8.md`. This file only says where things stand.

## Branches and setup
| Branch | Track | Last commit | Notes |
|---|---|---|---|
| `develop` | base | `bae882e` | ADR 0047 (M8 alongside M7); A-004 (M10 documentation, after M9) |
| `m7` | M7 engine | see `git log origin/m7` | WI-1..WI-5 done |
| `m8` | M8 GUI | see `git log origin/m8` | WI-1..WI-7 done |

Working layout used on the first PC: `m7` in the main clone, `m8` in a second worktree
(`git worktree add ../Umbra-m8 m8`). Any layout works; keep the two tracks in separate trees so agents
never edit the same files.

Tags: `m0-done`..`m6-done`, `m5b-done`, `pre-umbra`, `umbra-rename` are on origin. `m0-done`..`m5-done`,
`pre-umbra` and `umbra-rename` were recreated on 2026-10-22 on their close-out commits (they were
missing); `scripts/rename-check.sh pre-umbra umbra-rename` and `scripts/m2-compat.sh` pass with them.

Prerequisites on the new PC (`docs/build.md`): Rust toolchain, `cbindgen` 0.29.4, a C compiler,
Bun 1.4.4 (`gui/package.json` pins `bun@1.4.4`; an older bun rewrites `bun.lock`, never commit that),
Node.js 22 LTS, `cd gui && bunx playwright install chromium` once, `cargo fetch --locked` once online.
The Playwright `engine` project needs `cargo build -p umbra-cli --release --locked` first.

## M7 (branch `m7`)
Done, reviewed, pushed: WI-1 (engine acceptance tests), WI-2 (plugin acceptance tests), WI-3 (API 1.2.0,
`RunStatus.recording`). Accepted ADRs 0050..0059 (0059: `umbra_sdk::math`, amends ADR 0003).

**WI-4 (apply Spawn/Despawn, ADR 0052): done.** The WIP commit `e58ae8f` was reviewed; the findings were fixed
(directory in the engine, caps as `Limits`, owned-only total cap, publish-order and build-scope tests,
ASM-688..693), the re-review approved, and the fixes are committed on top of it. Gates green (gate 3: 3068
passed), goldens `849026656b7b42d2`, `3a7bfda0a76111b9`, `88954c5dc6b52b48`, `3cc077a4c83c4a63` unchanged.

**WI-5 (`builtin.sea.kinematic`, ADR 0051): done** (2026-10-23) after four review rounds (arrival by
construction: hold-then-turn with a behind-band, ground's near snap, no pass capture; ASM-694..698).
Gates green (gate 3: 3108 passed), goldens unchanged, `m2-compat` ok with the permanent ASM-698 allowlist.
Next is WI-6.

Then, in the execution order the plan states (ASM-679): WI-6, WI-7, WI-10 (with ADR 0059 math),
WI-10b, WI-8, WI-9, WI-11, WI-12, WI-13, WI-14, WI-15, WI-16, WI-16a, WI-16b, WI-17..WI-24.

## M8 (branch `m8`)
Done, reviewed, full gate 15 green (`bun run verify`, 12 steps incl. Playwright 38 passed): WI-1..WI-7.
Showcase screenshot baselines approved by the human 2026-10-22 (`70efdfe`). ADR 0043 amended
(components import their own CSS, human decision 2026-10-22).

Next: WI-8 (scenario-control `ScenarioLoader`, `ScenarioSummary`), then WI-9..WI-23 in plan order.
M7-dependent items wait for M7 (ADR 0047): WI-24 (contract sync after `m7` merges into `develop`;
M7 already regenerated its own GUI client for 1.2.0 on `m7`), WI-25, WI-26, WI-27, WI-28 (close-out
after `m7-done`). Gap G-1/G-2 fallbacks stay until M7 WI-16a/16b land.

## Decisions already made by the human (2026-10-22), do not re-ask
M7 H1..H10 (plan), ADR 0058 accepted, ADR 0059 accepted, frozen-test edits for WI-3 (`cli_serve.rs`
version literal, `recording: None,` in `wire.rs`, append-only AT-21 rows in `contract.rs`; no standing
rule: ask again for any other existing-test edit). M8 H-1..H-10 (plan), ADR 0043 amendment (option a),
bun pin 1.4.4, showcase baselines.
Frozen motion-model assertions (2026-10-23, M7 WI-5, option "assert containment"): the four pre-M7 tests
that pin the exact motion-model list or count (`umbra-motion/tests/factory.rs:233`,
`umbra-engine/tests/{builtin_schemas.rs:73, registered_schemas.rs:94, standard_registry.rs:52}`) assert
containment instead; standing approval for these four files through WI-6 (air), nothing else.

## Working rules learned in this session
- Each work item: failing tests (test engineer) -> implementation -> `code-reviewer` -> fixes ->
  gates -> commit with requirement IDs. Agents commit only and never push; the human pushes at milestone end.
- On this machine `target/` is a symlink to `/root/umbra-target/<track>`: run cargo and the gates with
  `CARGO_TARGET_DIR=/root/umbra-target/m7` (else `tests/confine.rs` fails). `cargo-audit` is installed.
- Mutation testing only in a uniquely named scratch copy, never in the real tree, and never reuse another
  agent's scratch scripts (a shared script once mutated the real tree concurrently with a test run).
- Ask the human before editing any existing (frozen) test file; append-only additions are listed in the
  plan's Compatibility/Proof section.
- Numbering blocks: M7 ASM-600..799, ADR 0050..0069, D-64..D-79; M8 ASM-800..999, ADR 0070..0089,
  D-80..D-95 (`docs/assumptions.md`).
