# ADR 0054: Replay semantics and verification

- **Status**: Accepted by the human 2026-10-22 as proposed (proposed by the architect, M7 plan `docs/plans/M7.md`, 2026-10-22)
- **Requirement IDs**: REQ-K-7, REQ-K-4, REQ-K-5, REQ-N-1, REQ-N-3, REQ-X-1, REQ-X-2, REQ-S-1, A-2, D-5,
  D-49, ADR 0024, ADR 0030, ADR 0033, ADR 0053
- **Affected**: `umbra-engine` (`replay` module, shared command applier), `umbra-net` (`ReplayAdapter`),
  `umbra-cli` (`umbra replay`)

## Context
Acceptance (spec 17, M7): "replay reproduces state hashes". A replay must reach the recorded per-tick
hashes from the recording alone, with the same scenario files and plugins, and without network I/O (the
peer is gone). Commands must apply at the same boundaries through the same code as live (DRY), or replay
proves nothing.

## Options
1. **Re-execution from tick 0 (chosen):** rebuild the session from the header, run as fast as possible,
   apply recorded commands before their tick, feed recorded inbound through replay adapters, compare
   hashes at the recorded ticks. Pros: needs no state snapshots; exercises the real code. Cons: replaying a
   long run takes as long as computing it.
2. **Snapshot-based seek.** Pros: random access. Cons: needs serialisable behavior and plugin state
   (REQ-B-5 opaque blobs, a new ABI entry); much larger scope; possible later on top of option 1.

## Decision
Option 1.
- **Build:** load the scenario at `relative_path` (resolved against the recording's directory), else
  `path`, else `--scenario`; check every file digest and every plugin (id, version, ABI, library digest);
  any mismatch refuses with exit 2 unless `--allow-mismatch` (then warn, still verify). Seed and tick rate
  come from the header (CLI overrides refused). Clock: as fast as possible.
- **Network:** each `[[network_adapters]]` entry is replaced by a `ReplayAdapter` with the recorded key
  space (must equal the header) that never opens sockets, discards `publish`, and returns from `poll` the
  messages recorded for that tick and adapter; a recorded `fault` is reproduced at its tick. Manager
  settings (publish, ghosts, type map) come from the scenario file as live.
- **Commands:** applied before their `before_tick` in `seq` order through one `CommandApplier` shared with
  the engine handle (spawn, remove, set adapter enabled); a spawn whose resulting id differs is a
  divergence. Informational records are not applied (they change no state); `stop` and `end` end the run.
- **Verification:** at each recorded `hash` tick the replay computes the hash with the header's version and
  compares; the first mismatch ends the replay with exit 1 and `error: replay diverged at tick N: expected
  <h>, got <h>`. Success prints the same final lines as `umbra run` (`state_hash=..`) and exits 0.
  `--no-verify` runs without comparing; `--partial` accepts a recording without `end` (crash) and stops at
  its last record.
- **CLI:** `umbra replay <recording> [--plugin-path ..] [--scenario ..] [--allow-mismatch] [--no-verify]
  [--partial] [--hashes] [--json]`; exit 0 ok, 1 divergence or runtime error, 2 input errors.
- Replay through the control API (load a recording as a session) is not in M7 (ASM-639).

## Consequences
- Easier: network runs and API-driven runs are reproducible and debuggable after the fact; a replay is an
  end-to-end determinism test in itself.
- Harder: a recording is only replayable with the same files and plugin builds on the same OS (A-2,
  ADR 0003); documented.
- Revisit for seek (snapshots), API replay sessions (M8 on request), or cross-OS replay.
