# Handoff: M7 and M8 in parallel (paused 2026-10-23)

Read this first when resuming. M7 (engine) and M8 (GUI) run in parallel under ADR 0047 (D-63). Plans,
running logs and assumptions stay the detailed source: `docs/plans/M7.md`, `docs/plans/M8.md`,
`docs/assumptions/M7.md`, `docs/assumptions/M8.md`. This file only says where things stand. The same
file is committed on `m7` and `m8`.

## Branches and setup
| Branch | Track | State at pause |
|---|---|---|
| `develop` | base | `bae882e`: A-004 (M10 documentation milestone, after M9; spec v0.4) on top of ADR 0047 |
| `m7` | M7 engine | WI-1..WI-5 done; WI-6 implemented, in review, committed as **WIP** (see M7) |
| `m8` | M8 GUI | WI-1..WI-10 done; WI-11 failing tests written, committed as **WIP** (see M8) |

All three branches were pushed at the pause. Neither `m7` nor `m8` has merged the newest `develop`
(A-004 commit `bae882e`); merge `develop` into each when convenient (docs only, no conflicts expected).

Working layout on the second PC: `develop` in `/root/Umbra`, worktrees `/root/Umbra-m7` (`m7`) and
`/root/Umbra-m8` (`m8`). Any layout works; keep the two tracks in separate trees so agents never edit
the same files.

Tags on origin: `m0-done`..`m6-done`, `m5b-done`, `pre-umbra`, `umbra-rename`.

Prerequisites on a new PC (`docs/build.md`): Rust toolchain, `cbindgen` 0.29.4, a C compiler,
`cargo-audit`, Bun 1.4.4 (`gui/package.json` pins `bun@1.4.4`; an older bun rewrites `bun.lock`, never
commit that), Node.js 22 LTS, `cd gui && bunx playwright install chromium` once, `cargo fetch --locked`
once online. The Playwright `engine` project needs `cargo build -p umbra-cli --release --locked` first.
Also needed:
- **diffutils** (`diff`, `cmp`): `scripts/rename-check.sh` fails without them (it was missing on the
  second PC; install it before the next milestone close).
- **git identity**: set `user.name`/`user.email` (the second PC used repo-local `ru1t3rl
  <lucaruiters@hotmail.nl>`).
- Separate cargo target dirs per track: on the second PC `target/` was a symlink to
  `/root/umbra-target/<track>`; run cargo and gates with `CARGO_TARGET_DIR=/root/umbra-target/m7` (else
  `tests/confine.rs` fails). If a target dir was built under another path, touch the `.rs` files to force
  a rebuild (stale test binaries reported "not found").
- Playwright screenshot baselines are machine-independent on Linux via `gui/e2e/support/fonts.conf`
  (ASM-932). On another OS or distro, if `showcase select (ops)` drifts, see ASM-932's revisit trigger;
  do not re-baseline without the human.

## M7 (branch `m7`)
Done, reviewed, committed: WI-1 (engine acceptance tests), WI-2 (plugin acceptance tests), WI-3 (API
1.2.0, `RunStatus.recording`), WI-4 (apply Spawn/Despawn, ADR 0052), WI-5 (`builtin.sea.kinematic`,
`8635118`, four review rounds; ASM-694..698). Accepted ADRs 0050..0059.

**WI-6 (`builtin.air.kinematic` and the `air_sea` example, ADR 0051): implemented, WIP commit, first
review = REQUEST CHANGES.** The open findings are in `docs/plans/M7-WI6-review-1.md` (delete that file when
WI-6 is committed as reviewed).
- Implementer's state (rust-engineer): full `scripts/gates.sh` green (gate 3: 3164 passed, 158 ignored;
  gate 4: 38 incl. the new `motion_determinism` test; gate 13: 45), goldens unchanged, `m2-compat` ok,
  sweeps 0 failures for `min_speed > 0`. New example golden `air_sea` `state_hash=3bae6dcb44fdb22b`
  (tick 9000, not frozen; it will change with the B1 fix). ASM-699..706.
- **Blocker B1**: an aircraft with `min_speed = 0` (the schema default) cannot change height at rest, and
  its horizontal-only landing snap ends any climb, so it never reaches a `MoveTo` height different from
  its own (and cannot climb back above the terrain floor when stopped). Needs a design choice recorded as
  an ASM (3D arrival and a climb rate decoupled from horizontal speed for hover-capable aircraft, or a
  minimum speed while a height error remains); ask `architect` if unclear.
- Also M1 (strengthen the aircraft waypoint check in `air_sea_example.rs`), M2 (loiter docs), m1..m4,
  nits; all in the review file.
- Next: tests first for B1 and every finding (`rust-engineer`, or `test-engineer` then `rust-engineer`),
  fix, full gates, re-review with the same depth (non-level destinations in every sweep), commit
  `feat(M7/WI-6)` on top of the WIP commit.

Then, in the execution order the plan states (ASM-679): WI-7, WI-10 (with ADR 0059 math), WI-10b,
WI-8, WI-9, WI-11, WI-12, WI-13, WI-14, WI-15, WI-16, WI-16a, WI-16b, WI-17..WI-24.

## M8 (branch `m8`)
Done, reviewed, gate 15 green (`bun run verify`): WI-1..WI-10 (last: WI-8 `e32602c`, WI-9 `6be46cd`,
WI-10 `90cec1d`; bun test 1468 pass, Playwright 38 passed). Showcase baselines approved 2026-10-22.

**WI-11 (`@umbra/log-view`): failing tests written (WIP commit), implementation not started.**
- Tests in `gui/features/log-view/` (package, tsconfig, lock entry, root tsconfig reference exist).
  `bun test`: 1467 pass, 7 fail, all from missing WI-11 modules (formatRecord, mergeSources,
  filterRecords, toJsonLines, LogView, index.ts) plus the dependency-cruiser real-tree test, which
  passes once the modules exist. `tsc -b` and `check:deps` stay red until the `@umbra/desktop` stub exists.
- Decisions pinned by the tests (ASM-944): merge by `received_at`, then list order, then each list's own
  order (not `seq`, which is per source); level is a minimum severity (All / Warnings / Errors), an empty
  source list means all, `LogFilter = { level, sources, text }` plus `NO_FILTER`; Download is visible in
  compact mode; polite "N of M records" status (throttling left to the implementer); at most 2 commits
  per journal notification; rows `row-sm`.
- Next step: `frontend-engineer` implements. Create a stub foundation package `@umbra/desktop` exporting
  `useEngineOutputSource(): JournalSource | undefined` (returns `undefined`; WI-22 fills it), add it as a
  dependency of log-view (package.json, tsconfig reference, `bun install`). LogView reads only
  `useJournalSource()`. Do not edit tests or doubles (`src/logTesting.tsx`). Then `code-reviewer`
  (skill `review-frontend-change`), fixes, `bun run verify`, commit.

Then WI-12..WI-23 in plan order. M7-dependent items wait for M7 (ADR 0047): WI-24 (contract sync after
`m7` merges into `develop`), WI-25, WI-26, WI-27, WI-28 (close-out after `m7-done`; also owns the
deferred 32 px selected-symbol growth, ASM-935(j)). Gap G-1/G-2 fallbacks stay until M7 WI-16a/16b land.

## Decisions already made by the human, do not re-ask
2026-10-22: M7 H1..H10 (plan), ADR 0058 and ADR 0059 accepted, WI-3 frozen-test edits (`cli_serve.rs`
version literal, `recording: None,` in `wire.rs`, append-only AT-21 rows in `contract.rs`). M8 H-1..H-10
(plan), ADR 0043 amendment (option a), bun pin 1.4.4, showcase baselines.

2026-10-23:
- **A-004 / M10 documentation milestone** (`develop` `bae882e`): M10 after M9, mandatory and last; may
  overlap M8/M8b, accepted only after `m8b-done`; generated Playwright screenshots in the web-app
  manual; Mermaid parsed in proposed gate 17 (dependency ADR during M10 planning, D-59); plain Markdown,
  mdBook-ready; SPEC.md updated to v0.4 (D-96; M10 block D-97..D-111, ASM-1000..1099, ADR 0090..0099).
- **M7 WI-5 frozen tests**: the four pre-M7 motion-model assertions (`umbra-motion/tests/factory.rs`,
  `umbra-engine/tests/{builtin_schemas,registered_schemas,standard_registry}.rs`) assert containment;
  approval stood through WI-6. `scripts/m2-compat.sh` carries a **permanent** exact-line allowlist for the
  two M0-M2 files (ASM-698, like H4/ASM-266); never remove it.
- **M7 WI-6**: two corrections to M7's own WI-1 tests approved (ASM-705): `air.rs` level-off destination
  built from geodetic coordinates; `air_sea_example.rs` checks `next + 1 == len` (the route follower holds
  at the last index).
- **M8**: Playwright pins fontconfig instead of re-baselining (ASM-932); standing approval for
  append-only export names in M8 features' `index.test.ts` lists; WI-9 review fixture edits (`--azure`
  in fixtures, required `MapInspection` members, url-style layer and source pins include `selection`,
  FakeMap focusable canvas and inline-source `FakeSource`); global `afterEach(cleanup)` in
  `gui/test/setup.ts` (ASM-941).
- Any other edit of an existing (frozen) test still needs asking.

## Working rules learned
- Each work item: failing tests (test engineer) -> implementation -> `code-reviewer` -> fixes (tests
  first again) -> re-review -> gates -> commit with requirement IDs. The human pushes (branches were
  pushed at this pause so work can continue elsewhere).
- Run gate wrappers so the real exit code is visible (`...; echo EXIT=$?`); a `; echo` after a pipeline
  once hid a failing gate.
- Mutation testing only in a uniquely named scratch copy, never in the real tree, never reusing another
  agent's scratch scripts. Reviewers' sweep and mutation evidence decides; "the sweep covers it" is not a
  test.
- Motion models: arrival by construction, no landing rule that breaks per-step limits, derived test
  bounds (state any headroom factor), a one-step test per snap guard, no exact-list registry assertions.
- GUI: never import a feature's `index.ts` in a test that loads real maplibre (use source-text export
  checks); sync `act()` for frame pushes that must not flush; check new tests against existing pinned
  ones before handing off; never change public types in a way that breaks existing typed usage.
- Numbering blocks: M7 ASM-600..799, ADR 0050..0069, D-64..D-79; M8 ASM-800..999, ADR 0070..0089,
  D-80..D-95; M10 ASM-1000..1099, ADR 0090..0099, D-97..D-111 (`docs/assumptions.md`).
