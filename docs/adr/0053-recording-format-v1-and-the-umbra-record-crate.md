# ADR 0053: Recording format v1 and the `umbra-record` crate

- **Status**: Accepted by the human 2026-10-22 as proposed (H4: the new crate `umbra-record`; H6:
  lossless recording with backpressure; proposed by the architect, M7 plan `docs/plans/M7.md`, 2026-10-22).
- **Requirement IDs**: REQ-K-7, REQ-K-4, REQ-K-6, REQ-K-8, REQ-N-8, REQ-X-1, REQ-A-1, NFR-2, A-2, D-4,
  D-23, ADR 0006, ADR 0007, ADR 0024, ADR 0029, ADR 0030
- **Affected**: new crate `umbra-record`; `umbra-engine` (recorder), `umbra-net` (inbound tap),
  `umbra-cli` (`run --record`, `serve --record-dir`, `schema recording`), `xtask` rule table

## Context
REQ-K-7: record the input command/event stream plus seed so a run can be replayed deterministically. The
inputs of a run are: the scenario files, plugins and their versions (A-2), the seed and the effective tick
rate (ADR 0029 note), state-changing commands applied between ticks (ADR 0024: every reply carries its
tick), and network inbound messages, whose arrival tick depends on wall timing (ADR 0030). Everything
else (clock mode, wall time, subscribers) does not change state (gate 4). Core types have no serde
derives (D-23). A recording must be lossless to be replayable; REQ-K-6 forbids observers from blocking
the tick, but the recorder is part of the engine, not an observer.

## Options
1. **JSON Lines in a new `umbra-record` crate (core + serde + serde_json), hand-mapped records (chosen).**
   Pros: no new dependency (serde_json is in the tree; ryu prints shortest round-trip f64, so values
   replay bit-exactly); human-readable; diff-able; a JSON Schema documents it; the crate is testable
   alone and keeps the engine facade small (SRP). Cons: larger than binary for big network captures.
2. **Binary (custom or CBOR/MessagePack).** Pros: compact, fast. Cons: a new dependency or a hand codec;
   opaque to users; premature before measured need. The header's `encoding` field leaves room for it.
3. **A module inside `umbra-engine`.** Pros: no new crate. Cons: mixes a file format with the facade;
   harder to reuse (e.g. a recording viewer) without the engine.

## Decision
Option 1. `umbra-record` may depend on `umbra-core` only among workspace crates (rule table entry);
`#![forbid(unsafe_code)]`.
- **File:** UTF-8 JSON Lines, one record per line, keys in a fixed order, no wall-clock data (two
  recordings of the same run are byte-identical, ASM-621). First line is the header:
  `{"type":"header","format":"umbra.recording","version":1,"encoding":"json","umbra":<engine version>,
  "scenario":{"path","relative_path","files":[{"path","fnv1a64"}]},"seed":"<decimal>","tick_rate_hz",
  "tick_rate_source","state_hash_version","hash_every","on_plugin_fault",
  "plugins":[{"id","version","abi","library_fnv1a64"}],
  "network_adapters":[{"name","adapter","key_space":{"base","count"}}]}`.
- **Records** (`before_tick` = ticks completed when applied; `tick` = the tick about to run or just run):
  `command` (`before_tick`, `seq`, `command`: `spawn` with the full `TemplateSpawn` and the resulting id,
  `remove`, `set_adapter_enabled`, and informational `start`, `pause`, `resume`, `step`, `set_clock`,
  `stop`), `inbound` (`tick`, `adapter`, `messages`: the canonical `NetMessage`s exactly as polled, before
  the manager processes them), `fault` (`tick`, `adapter`, `call`: a network adapter fault seen live),
  `hash` (`tick`, `hash`), `end` (`tick`, `reason`, `hash`). Unknown record types are an error in v1.
- **Versioning:** `version` is bumped for any incompatible change; a reader rejects a newer version
  (REQ-T-5 analogue); additive optional keys keep `version` 1.
- **Hash cadence:** `hash_every` (default the tick rate, i.e. once per simulated second; tests use 1), plus
  the tick after every command or inbound batch and the final tick (ASM-624). Hashes use the header's
  `state_hash_version` (ADR 0055).
- **Writer:** records are encoded on the engine thread and handed to a writer thread through a bounded
  queue (4096 records). **Lossless with backpressure** (ASM-625): when the queue is full the engine thread
  waits; in real time this shows as an overrun warning (REQ-K-8). A write error marks the recording failed,
  is logged once, never stops the simulation, and makes `umbra run --record` exit 1 after the run.
- **Digests:** FNV-1a 64 over file bytes (integrity against accidental change, not security), reusing
  `umbra_core::hash::fnv1a64`.
- **Sources:** `umbra run --record <file>` (header, hashes, `start`/`stop`, `end`);
  `umbra serve --record-dir <dir>` (one file per session load, `session-<n>.umbrarec.jsonl`, all handle
  commands); the network manager reports polled messages and adapter faults through an `InboundTap`.
- `umbra schema recording` emits `recording.v1.schema.json` (committed under the CLI's schemas, drift
  tested like the other file schemas).

## Consequences
- Easier: UDP runs become reproducible from their recording; recordings are inspectable with `jq`.
- Harder: disk speed can bound the tick rate while recording (documented); large captures are verbose.
- Revisit for a binary encoding, compression, or seek/snapshot support (REQ-B-5 instance state).
