# ADR 0018: Rename the project to Umbra

- **Status**: Accepted (decided by the human project owner on 2026-10-18; choices C1-C10 accepted as drafted). Spec decision D-35 (amendment A-002).
- **Requirement IDs**: REQ-A-1..4 (dependency rules are keyed by crate name), REQ-S-1, REQ-S-2, REQ-X-2, REQ-P-1, REQ-P-2, REQ-SDK-1, REQ-SDK-2, REQ-SDK-4, REQ-K-4, REQ-T-5, NFR-5; D-7, D-10, D-12, D-19; A-001 compatibility constraints 1-5
- **Affected**: every workspace crate, `xtask` (rules and fixtures), `Cargo.toml`, `Cargo.lock`, `clippy.toml`, `scripts/m2-compat.sh`, `docs/`, `.claude/`, `CLAUDE.md`, `gui/CLAUDE.md`, `README.md`, `LICENSE-MIT`, `.github/workflows/ci.yml`

This file is kept verbatim by the rename (it is on the exclusion list, E1), so the old names below stay readable.

## Context
The human decided on 2026-10-18 that the project is called **Umbra**, and that the rename is full:
crates, binary, GUI package scope, schema IDs and the future C header prefix, not only the docs.
"CGF" stays the domain term: Umbra is a Computer Generated Forces (CGF) engine.

A-001 froze the M0-M2 deliverables ("additive changes only; a breaking change requires an ADR with a
migration plan"). M3 is done and tagged `m3-done`. The rename moves every crate directory, changes
every `use cgf_*` line and the command name. That is a breaking change to frozen deliverables, so it
needs this ADR and the migration plan below. `scripts/m2-compat.sh` compares by path against
`m2-done`, so it fails after the rename unless it is changed too.

The timing matters. M4 fixes the C ABI: the entry symbol (`cgf_plugin_entry`, REQ-P-1), the
`#[repr(C)]` type names, the header (`cgf_plugin_api.h`, REQ-SDK-2) and the ABI constants. After
M4, REQ-P-2 (append-only within a major version) makes those names expensive to change. Before M4
there is no plugin, no published header, no server, no GUI code and no external user
(`publish = false`).

## Options
1. **Full rename now, before M4 (chosen).** Pros: one name everywhere; the ABI is born with its final
   names; the cost is lowest now (one mechanical change, no external users). Cons: it breaks the
   path-based freeze proof; one large diff; history and blame cross a rename commit.
2. **Docs-only rename (crates, binary and symbols keep `cgf`).** Pros: no code change; the freeze is
   untouched. Cons: two names for good; the C ABI would carry `cgf_` forever; the human asked for a
   full rename.
3. **Full rename later (after M4 or at M9).** Pros: no disruption now. Cons: a renamed entry symbol
   and header mean a major ABI bump (REQ-P-9 rejects old plugins), plus a renamed server, OpenAPI and
   GUI packages; strictly more work than option 1.

## Decision
Option 1, done as **one dedicated work item with no behavior change**, committed on its own, before
M4 starts. Decided choices:

| # | Choice | Decided | Rejected alternatives |
|---|---|---|---|
| C1 | Crate prefix | `umbra-*` (`umbra-core`, `umbra-kernel`, `umbra-motion`, `umbra-behavior`, `umbra-templates`, `umbra-engine`, `umbra-cli`; planned `umbra-plugin-api`, `umbra-plugin-host`, `umbra-sdk`, `umbra-net`, `umbra-net-dis`, `umbra-net-hla`, `umbra-api-types`, `umbra-server`). Rust paths `umbra_core::` etc. `xtask` keeps its name | `umb-*` |
| C2 | Binary name | Package `umbra-cli`, binary **`umbra`**: `crates/umbra-cli/Cargo.toml` gets `[[bin]] name = "umbra"` and `path = "src/main.rs"`. Commands: `umbra run`, `umbra validate`, `umbra schema`, later `umbra new-plugin`. Cargo invocations stay `cargo run -p umbra-cli -- ...` | Binary `umbra-cli` |
| C3 | GUI package scope | `@umbra/<name>`, `@umbra/shell-<name>`; the `package.json` key `"cgf": {...}` becomes `"umbra": {...}` (`umbra.tier`); all packages `"private": true` | `@umbra-gui/` |
| C4 | Schema `$id` base | The rename adds no `$id` (none exists). When one is added (planned with the M5 stream schema): `urn:umbra:schema:<name>:v<N>`, for example `urn:umbra:schema:template:v1`. The file-format schema `title`s become "Umbra template file, format v1", "Umbra scenario file, format v1", "Umbra behavior graph file, format v1" | `https://<domain>/...` (no domain, ASM-003); keeping "CGF" titles |
| C5 | C header and symbol prefix (M4) | Header `umbra_plugin_api.h`, include guard `UMBRA_PLUGIN_API_H`; functions `umbra_*` (entry `umbra_plugin_entry`); types `Umbra*` (`UmbraPluginDescriptor`, `UmbraHostApi`, `UmbraCapabilityKind`); macros and constants `UMBRA_*` (`UMBRA_ABI_VERSION_MAJOR`) | `umb_` / `Umb` / `UMB_` |
| C6 | Environment variables | Prefix `UMBRA_`; `CGF_STACK_KIB` becomes `UMBRA_STACK_KIB` | Keep `CGF_` |
| C7 | Repository and directory names | Workspace `repository` placeholder `https://example.invalid/umbra`. The local directory `/home/ru1t3rl/Documents/cgf/cgf-agents` is not renamed by the work item; the human may rename it later (see Risks) | Rename the directory in the same step |
| C8 | Existing tags | `m0-done`..`m3-done` stay; history is not rewritten. New annotated tags: `pre-umbra` (last commit with old names, gates green) and `umbra-rename` (the rename commit) | Rewrite history; retag |
| C9 | Historical docs | Renamed everywhere (ADRs 0001-0017, A-001, plans M0-M3, milestone evidence), with one note in `docs/milestones.md` (added in WI-R4) that M0-M3 evidence is given in post-rename names and that the tags keep the originals. Only the exclusion list E1-E5 stays verbatim | Keep plans and evidence verbatim |
| C10 | Freeze scope after the rename | The ongoing A-001 check keeps its M0-M2 scope (Migration 4); M3 is covered by the one-time rename proof only | Extend the ongoing freeze to M3 |

"CGF" in the domain sense stays (rule D below). "CGF Engine" and "CGF" used as the product name become "Umbra".

## Rename specification (executed mechanically by the build-engineer in WI-R2)

### Exclusion list (verbatim: byte-identical before and after)
- **E1** `docs/adr/0018-rename-the-project-to-umbra.md` (this file)
- **E2** `docs/amendments/A-002-umbra-and-visual-timeline.md`
- **E3** `docs/amendments/spec-v0.1-to-v0.2.patch` (must still apply to v0.1 text)
- **E4** `scripts/rename-check.sh`
- **E5** `scripts/rename-map.sed` (the content rules below, in this order)

Never touched: `.git/`, `target/`, untracked files. Every other tracked file is in scope.

### Path rules (applied with `git mv`, directory by directory)
- **P1** `crates/cgf-<x>/` to `crates/umbra-<x>/` (7 directories: core, kernel, motion, behavior, templates, engine, cli).
- **P2** `xtask/tests/fixtures/<fixture>/crates/cgf-<x>/` to `xtask/tests/fixtures/<fixture>/crates/umbra-<x>/` (every fixture workspace; includes `cgf-mystery` to `umbra-mystery`).
- No other tracked path contains `cgf` (checked: no file name contains it). The check fails if one does.

### Content rules (literal strings unless marked *regex*; applied to every in-scope file, top to bottom; a later rule sees the output of earlier ones)
Rules are case-sensitive. In the tables, `\|` is Markdown escaping for a plain `|`; regexes are GNU `sed -E` syntax (in A3 the `|` are alternation).
Group A, binary and specific phrases (before the generic prefixes):

| Rule | Old | New | Hits (where) |
|---|---|---|---|
| A1 | `CARGO_BIN_EXE_cgf-cli` | `CARGO_BIN_EXE_umbra` | `crates/cgf-cli/tests/common/mod.rs`, `cli_run.rs`, `docs/plans/M1.md` |
| A2 | `#[command(name = "cgf-cli"` | `#[command(name = "umbra"` | `crates/cgf-cli/src/args.rs` |
| A3 | *regex* `cgf-cli (run\|validate\|schema\|demo\|new-plugin\|list-templates\|list-plugins)\b` | `umbra \1` | the command in prose, comments and docs (`cgf-cli run --ticks N`, `cgf-cli validate <dir>`, `cgf-cli run/validate/schema`, ...) |
| A4 | `` spawn `cgf-cli` as `` | `` spawn `umbra` as `` | `docs/SPEC.md` (section 13), `docs/frontend-architecture.md` (section 6) |
| A5 | `` sidecar packaging of `cgf-cli` `` | `` sidecar packaging of `umbra` `` | `.claude/agents/build-engineer.md` |
| A6 | `Engine process (cgf-cli)` | `Engine process (umbra)` | `docs/SPEC.md` section 3 diagram |
| A7 | `cgf-server (optional feature)          \|` (10 spaces) | `umbra-server (optional feature)        \|` (8 spaces) | `docs/SPEC.md` section 3 diagram (keeps the box border aligned) |
| A8 | `cgf-engine (facade, embeddable lib)    \|` (4 spaces) | `umbra-engine (facade, embeddable lib)  \|` (2 spaces) | same diagram |
| A9 | `example.invalid/cgf-engine` | `example.invalid/umbra` | root `Cargo.toml` (C7) |

Group B, generic prefixes:

| Rule | Old | New | Covers |
|---|---|---|---|
| B1 | `cgf-` | `umbra-` | package and directory names in manifests, `Cargo.lock`, `rules.rs`, tests, docs; remaining `cgf-cli` (package sense, `-p cgf-cli`) becomes `umbra-cli`; temp dir `cgf-behavior-doc` |
| B2 | `cgf_` | `umbra_` | Rust paths, `clippy.toml` reasons, C names in the spec and skills (`cgf_plugin_entry`, `cgf_plugin_api.h`) |
| B3 | `@cgf/` | `@umbra/` | GUI packages in docs and skills |
| B4 | `"cgf":` | `"umbra":` | `package.json` metadata key in `gui/CLAUDE.md` and skills |
| B5 | `cgf.tier` | `umbra.tier` | `docs/frontend-architecture.md`, `review-frontend-change` skill |
| B6 | `CGF_` | `UMBRA_` | `CGF_STACK_KIB` (`stack_budget.rs`, `docs/assumptions.md`) |
| B7 | *regex* `Cgf([A-Z])` | `Umbra\1` | `CgfPluginDescriptor`, `CgfHostApi` (SPEC REQ-P-1), `CgfCapabilityKind` (add-plugin-kind skill) |

Group C, the product name (upper-case `CGF` that means this project):

| Rule | Old | New | Hits |
|---|---|---|---|
| C-1 | `The CGF Engine` | `The Umbra` | `LICENSE-MIT` ("The Umbra contributors"), `docs/plans/M0.md` (quotes of it), `crates/cgf-cli/src/args.rs` doc comment ("The Umbra command line.") |
| C-2 | `the CGF Engine` | `Umbra` | crate docs in `lib.rs`/`main.rs` ("for Umbra", "of Umbra") |
| C-3 | `CGF Engine` | `Umbra` | `Cargo.toml` descriptions ("Umbra core types: ...", "Umbra command-line interface."), clap `about`, `# Umbra: project instructions ...`, the spec title quote in `CLAUDE.md`, `# Umbra: Software Specification v0.2` |
| C-4 | `CGF engine` | `Umbra engine` | README ("building the Umbra engine"), agents (architect description, code-reviewer, rust-engineer: "the Umbra engine core") |
| C-5 | *regex* `CGF (template\|scenario\|behavior graph) file` | `Umbra \1 file` | the three schema `title`s and the assertion in `crates/cgf-cli/tests/cli_schema_behavior.rs` (C4) |
| C-6 | `# CGF agent pack` | `# Umbra agent pack` | `README.md` |
| C-7 | `a CGF change` | `an Umbra change` | `review-change` skill |
| C-8 | `CGF requirements` | `Umbra requirements` | `write-tests` skill |
| C-9 | `full CGF verification` | `full Umbra verification` | `verify-and-build` skill |
| C-10 | `the CGF Cargo workspace` | `the Umbra Cargo workspace` | `add-crate` skill |
| C-11 | `one CGF milestone` | `one Umbra milestone` | `implement-milestone` skill |
| C-12 | `for the CGF (Computer Generated Forces) engine` | `for Umbra, the CGF (Computer Generated Forces) engine` | `.claude/agents/architect.md` |

Group D, the domain term (kept). Upper-case `CGF` stays where it names the kind of system, not this
project: "Computer Generated Force (CGF)", "a CGF engine", "CGF (Computer Generated Forces)",
"CGF scenario/simulation" in the generic sense. At `efcc1e0`, after groups A-C, exactly two in-scope
occurrences remain and both are domain uses: `docs/SPEC.md` section 1.1 ("A Computer Generated Force
(CGF) simulates ...") and `.claude/agents/architect.md` (the parenthesis kept by C-12). Any other
remaining `CGF` is a failure of the check: classify it by hand with this rule and add a group C rule.

### Steps after the rules (part of WI-R2, all offline)
1. `cargo fmt --all`: longer names (`umbra_` is 2 characters longer) and changed import order (rustfmt sorts `use umbra_core` after `use semver`) produce reflow-only changes.
2. Root `Cargo.lock`: `cargo metadata --offline --format-version 1 > /dev/null` (rewrites only the renamed entries and their order; never `cargo update` or `cargo generate-lockfile` at the root).
3. Fixture locks: `cargo generate-lockfile --offline` inside each `xtask/tests/fixtures/<fixture>/` that has a `Cargo.lock` (the documented procedure in `xtask/tests/fixtures/README.md`; their dependencies are local).
4. Add `[[bin]] name = "umbra"`, `path = "src/main.rs"` to `crates/umbra-cli/Cargo.toml` (C2). Manifests are not re-sorted.
5. `cargo clean` before running the gates (removes stale `cgf_*` artifacts).

### Allowed manual hunks (each one listed with its reason in the WI-R2 review record)
- M1: the `[[bin]]` entry (step 4).
- M2: an `xtask` test whose expected list is ordered by crate name, if one fails only because `umbra-*` sorts differently from `cgf-*` against non-renamed names (`shim`, `fake_external`, `impostor`, `xtask`): reorder the expected list, change nothing else.
- Nothing else. A hunk that is not produced by the rules, rustfmt, the lockfile steps, M1 or M2 is a defect.

### Occurrences that need manual review (expected to be correct after the rules; the reviewer confirms each)
1. `cgf-cli` where it names the binary without a following subcommand: `crates/cgf-cli/tests/common/mod.rs` doc line "Runs `cgf-cli` with ..." and the panic text "failed to run cgf-cli" become `umbra-cli` (B1). Accepted as is (it names the package that builds the binary); no extra rule.
2. M1/M2 history: `cgf-cli demo` becomes `umbra demo` (A3) in `docs/plans/M1.md`, `M2.md`, `docs/assumptions.md`, `docs/milestones.md`, `examples/scenarios/walking_skeleton/README.md` and `crates/cgf-cli/tests/cli_run.rs`, although that command never existed under the new name. Accepted under C9 (the milestones note explains it).
3. `docs/SPEC.md` section 3 diagram after A6-A8: check that the box borders still line up.
4. Markdown tables and code blocks in docs are not re-wrapped; only the SPEC diagram is width-sensitive.
5. The two group D occurrences above.
6. `.github/workflows/ci.yml` (dormant): `cargo test -p umbra-engine ...` in a comment.

## Inventory (read at `efcc1e0`: 319 files, about 1,800 occurrences outside `target/` and `.git`)
1. **Crate directories and package names**: `crates/cgf-{core,kernel,motion,behavior,templates,engine,cli}`; `name` and `description` ("CGF Engine ...") in each `Cargo.toml`; path dependencies `../cgf-*`. Planned names in spec section 4 and in `xtask/src/deps/rules.rs`: `cgf-plugin-api`, `cgf-plugin-host`, `cgf-sdk`, `cgf-net`, `cgf-net-dis`, `cgf-net-hla`, `cgf-api-types`, `cgf-server`.
2. **Rust paths**: `cgf_core::`, `cgf_kernel::`, `cgf_motion::`, `cgf_behavior::`, `cgf_templates::`, `cgf_engine::` in `src`, `tests`, `benches`, doc tests and intra-doc links; `clippy.toml` (54 `disallowed-methods` reasons "use cgf_core::math"); root `Cargo.toml` lint comment and `repository`; `Cargo.lock` (7 workspace entries).
3. **Binary and CLI text**: package and binary `cgf-cli`; `crates/cgf-cli/src/args.rs:12` `#[command(name = "cgf-cli", version, about = "CGF Engine command-line interface")]` and its doc comment; crate docs in `main.rs`; `CARGO_BIN_EXE_cgf-cli` in `crates/cgf-cli/tests/common/mod.rs:13` and `cli_run.rs:337`; panic text "failed to run cgf-cli".
4. **Schemas**: the `title` of `crates/cgf-templates/schemas/{template,scenario,behavior}.v1.schema.json` (asserted in `crates/cgf-cli/tests/cli_schema_behavior.rs:23`). No `$id` anywhere. Component and node-type schemas contain no "cgf".
5. **xtask**: `RULES` (16 entries) and `UNSAFE_ALLOWED` in `xtask/src/deps/rules.rs`; doc comments in `xtask/src/deps/mod.rs`; tests `check_rules.rs` (133 occurrences), `check_deps.rs` (66), `check_unsafe.rs` (29), `check_lints.rs` (1); 14 fixture workspaces in `xtask/tests/fixtures/` with `crates/cgf-*` members, their manifests, 10 fixture `Cargo.lock` files and the fixtures `README.md` ("Package names reuse the real crate names so the real rule table applies", so fixtures must be renamed with the table). The tool has no `cgf` prefix logic; a crate missed by the rename is reported as unknown.
6. **Scripts**: `scripts/m2-compat.sh` (self-test paths and the ADR 0011 allowlist path).
7. **Environment and temp names**: `CGF_STACK_KIB`; temp dir `cgf-behavior-doc`. No other env vars or prefixes.
8. **Examples and fixtures (comments only)**: `examples/scenarios/walking_skeleton/{scenario.toml,README.md}` (an M2 fixture), `examples/scenarios/patrol_and_engage/{scenario.toml,README.md}`, `crates/cgf-engine/tests/fixtures/behavior_swap/{a,b}/scenario.toml`, `crates/cgf-cli/tests/fixtures/bad/{a_cycle,b_cycle,c_unknown_parent}.toml`.
9. **Docs**: `docs/SPEC.md`, `architecture.md`, `build.md`, `templates.md`, `behavior.md`, `test-matrix.md`, `assumptions.md`, `milestones.md`, `frontend-architecture.md`, `plans/M0..M3.md`, ADRs 0001-0017, A-001; E3 stays verbatim.
10. **Agent configuration**: `CLAUDE.md`; `gui/CLAUDE.md`; `.claude/agents/` (architect, rust-engineer, plugin-abi-engineer, network-engineer, api-engineer, frontend-engineer, build-engineer, code-reviewer); `.claude/skills/` (add-crate, add-plugin-kind, add-network-adapter, update-openapi, implement-milestone, verify-and-build, write-tests, review-change, add-feature, add-feature-shell, review-frontend-change).
11. **Repository files**: `README.md`, `LICENSE-MIT` (`LICENSE-APACHE` has none), `.github/workflows/ci.yml`.
12. **`gui/`**: only `gui/CLAUDE.md` exists.
13. **Outside the repository**: the directory `/home/ru1t3rl/Documents/cgf/cgf-agents`; the Claude Code project memory `~/.claude/projects/-home-ru1t3rl-Documents-cgf-cgf-agents/`; the tags.

**Checked and free of "cgf"**: component, behavior, graph and template IDs (`builtin.*`, `example.*`; ADR 0002 has no reserved `cgf` namespace); diagnostic codes and messages; every state-hash input; RNG stream keys (numeric, ADR 0006); catalog versions (`CARGO_PKG_VERSION`, workspace-wide `0.1.0`); no code reads `CARGO_PKG_NAME`, `module_path!` or `std::any::type_name`.

## Compatibility and migration
1. **User-visible changes in M2/M3 output**: the command is `umbra` (`cargo run -p umbra-cli -- ...`); `--version` prints `umbra 0.1.0`; `--help` shows `Usage: umbra <COMMAND>` and "Umbra command-line interface"; `umbra schema template|scenario|behavior` differs only in `title`; log lines on stderr show targets `umbra_engine::...`; rustdoc and panic locations show new paths. **Unchanged**: `run` stdout (entity lines, `state_hash=` lines, `--json`), `validate` output and exit codes, every diagnostic, `schema component <id>`.
2. **File formats unchanged**: `schema_version` stays 1, no key changes, component IDs unchanged. Every user scenario, template and behavior graph file keeps working without edits.
3. **State hash cannot change**: `crates/cgf-kernel/src/state.rs` hashes tick, tick rate, entity count and, per entity, id, ownership (and remote source), motion model id (`builtin.ground.kinematic`), position, velocity, orientation, route and RNG state (ADR 0007 v2). No crate, binary or package name feeds the hash, an RNG seed or a stream key. The walking-skeleton final hash `849026656b7b42d2` and the patrol_and_engage hash `3a7bfda0a76111b9` (tick 4200) must be unchanged.
4. **A-001 freeze proof after the rename**:
   - **One-time rename proof** (`pre-umbra` to `umbra-rename`): M0-M3 are content-identical apart from the mechanical rename. After the rename, "frozen" means: content at `umbra-rename` equals the expected tree built from `pre-umbra` (below), and the A-001 rules apply from `umbra-rename` onward.
   - **Rebased ongoing check** (WI-R3): `m2-compat.sh` uses the default baseline tag `umbra-rename` for content (checks 1, 2, 2b keep their plain `git diff` form). Its *scope* (which files are M2-era) is the file list at `m2-done` with P1 applied (`sed 's#^crates/cgf-#crates/umbra-#'`), so the scope stays M0-M2 (C10). Check 3 lists the tests at `m2-done`, maps package names (B1) and binary names (`cgf_<x>` to `umbra_<x>`; the `cgf-cli` unit-test binary `cgf_cli` to `umbra`), and requires them to be a subset of the current list; the ignored-test check is mapped the same way. The ADR 0011 allowlist is removed because that edit is in the baseline. `--self-test` is updated and must pass.
5. **Mechanical-rename verification** (`scripts/rename-check.sh pre-umbra umbra-rename`, written in WI-R1):
   - Build the **expected tree**: a scratch worktree of `pre-umbra` under `target/`; apply P1-P2 (`git mv`), the content rules (`sed -E -f scripts/rename-map.sed`) to every in-scope file, and steps 1-3 above.
   - `diff -r` the expected tree against `umbra-rename` (excluding `.git` and `target`). Allowed differences: only M1 and M2 hunks. E1-E5 must be byte-identical with `pre-umbra`.
   - Leftover scan at `umbra-rename`: no `cgf-`, `cgf_`, `@cgf/`, `"cgf":`, `cgf.tier`, `CGF_` or `Cgf[A-Z]` outside E1-E5; upper-case `CGF` only at the two group D places.
   - `cargo test --workspace -- --list` and `-- --list --ignored` at both tags are equal (not just a subset) after mapping package and binary names.
   - Every gate in `docs/build.md` passes with the same test totals as at `pre-umbra`; `cargo deny` and `cargo audit` see the same crate count; root `Cargo.lock` external entries (name, version, source, checksum) are identical as a set.
   - `umbra run <scenario> --hash-every 1` stdout is byte-identical to `cgf-cli run` at `pre-umbra` for `walking_skeleton` and `patrol_and_engage` (final hashes as in item 3); `validate --json` output is identical; `schema component <id>` is identical for every built-in; `cargo xtask check-deps` on the `violating` fixture reports the same violations with mapped names.
   - `--self-test`: a planted extra edit, a missed `cgf_` and a changed E1 file each make the check fail.

## Sequencing
Before M4, as its own work items, with no behavior change and no other work mixed in:
1. (Done 2026-10-18) Human accepted this ADR and A-002.
2. **WI-R1** (build-engineer): `scripts/rename-map.sed` (groups A-C, in order) and `scripts/rename-check.sh` with `--self-test`; full gate and `m2-compat.sh` against `m2-done` pass; commit; tag `pre-umbra`.
3. **WI-R2** (build-engineer, reviewed by code-reviewer): the rename commit (P1-P2, rules, steps 1-5, at most M1/M2 hunks); verification above; one commit, so every commit builds; tag `umbra-rename`.
4. **WI-R3** (build-engineer): rebase `m2-compat.sh` (Migration 4); add `.git-blame-ignore-revs` with the rename commit hash.
5. **WI-R4** (architect, after WI-R3): apply A-002 and D-35/D-36 to `docs/SPEC.md`, `docs/milestones.md` (including the C9 note), `CLAUDE.md`, `docs/frontend-architecture.md`, `gui/CLAUDE.md`, in the new names.
Then M4 starts with `umbra-plugin-api` and the C5 names.

## Consequences and risks
- **Easier**: one name; the M4 ABI and header are born final; later GUI and server work uses final names.
- **Missed references**: caught by the expected-tree diff, the leftover scan, `check-deps` reporting unknown crates and the exact test-list comparison.
- **`Cargo.lock` churn**: only steps 2-3; no new dependency and no version change is allowed.
- **Sort order**: anything sorted by crate name (xtask output, lock files) can change order against non-renamed names; covered by M2 and the lock steps. No engine output is sorted by crate name.
- **Git history and blame**: use `git mv`; renames are detected because most files change only a few lines. `git log --follow` works; `.git-blame-ignore-revs` hides the rename commit in blame. Old commands and paths remain valid at the old tags.
- **Notes outside git**: the memory notes in `~/.claude/projects/-home-ru1t3rl-Documents-cgf-cgf-agents/memory/` mention `cgf` paths; the human updates them. If the directory is renamed, Claude Code keys project memory by path, so move that folder too.
- **Name clashes**: `umbra` may exist on crates.io or npm; harmless while `publish = false` and the GUI packages are private. Revisit before any publication.
- **Revisit** if a name must change again (the map changes; the method stays).
