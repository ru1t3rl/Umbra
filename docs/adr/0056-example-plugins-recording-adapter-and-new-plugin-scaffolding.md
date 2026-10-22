# ADR 0056: Third-party-style example plugins, the recording network adapter and `umbra new-plugin`

- **Status**: Accepted by the human 2026-10-22 as proposed (H7; proposed by the architect, M7 plan
  `docs/plans/M7.md`, 2026-10-22).
- **Requirement IDs**: REQ-SDK-3, REQ-SDK-4, REQ-X-2, REQ-N-8, REQ-P-4, REQ-A-3, NFR-2, NFR-5, D-7, D-41,
  ADR 0022, ADR 0023, ADR 0033 (M6 decision H7 a, b)
- **Affected**: `examples/plugins/*` (new crates), `examples/scenarios/plugin_kinds`, `xtask` (`build-plugins`),
  `umbra-cli` (`new-plugin`), `docs/plugin-guide.md`

## Context
M7 acceptance: "a third-party-style example plugin of each kind in `examples/`". Kinds in M7's row:
motion model, sensor, network, condition, action (behavior has `wander` and `c-circle` since M4).
REQ-N-8 asks for a recording adapter (SHOULD), moved from M6 to M7 (M6 H7 b). REQ-SDK-4 asks for
`umbra new-plugin <kind>`, deferred from M4 to M7 (M4 decision 7). "Third-party style" means: depends on
`umbra-sdk` only, `#![forbid(unsafe_code)]`, packaged with a `plugin.toml`, documented as an author would.

## Options
1. **One crate per kind, each a small but realistic plugin, the network one doubling as the REQ-N-8
   recording adapter; one combined example scenario (chosen).** Pros: each example is readable alone;
   REQ-N-8 is met by SDK-built code exactly like a third party would ship it (REQ-SDK-3). Cons: five more
   workspace members.
2. **One crate exporting all four new kinds.** Pros: fewer crates. Cons: not what a third party would copy
   for one kind; muddles the per-kind guide.
3. **Recording adapter as a built-in in `umbra-net`.** Pros: always available. Cons: file I/O in the
   manager crate; duplicates ADR 0053's engine recorder, which already captures inbound losslessly.

## Decision
Option 1. Crates (workspace members, `cdylib` + `rlib`, `umbra-sdk` only, packaged by
`cargo xtask build-plugins` into `target/plugins/<dir>`):
- `examples/plugins/hover` - `example.motion.hover`: a multirotor-style kinematic model holding a height
  above ground through the terrain service (proves `UmbraTerrain`).
- `examples/plugins/radar` - `example.sensor.radar`: range and elevation gate, line of sight through the
  terrain service, detection probability drawn from the entity stream (proves RNG for sensors).
- `examples/plugins/bearing` - `example.cond.bearing_within`: reads a target entity from the blackboard
  and checks its relative bearing (proves `UmbraBlackboard` read).
- `examples/plugins/dispatch` - `example.action.dispatch`: waits `delay_s`, emits a `Spawn` of a template
  at an offset, writes a `Bool` key, then succeeds; halt resets (proves action instances, write access and
  the spawn path of ADR 0052).
- `examples/plugins/net-jsonl` - `example.net.jsonl`: the REQ-N-8 recording adapter. Mode `record` writes
  every published canonical message to a JSON Lines file on its own thread (bounded queue, drop-and-count,
  it is a debugging aid); mode `playback` replays such a file as inbound at the recorded tick offsets.
  Its encoding is its own (the SDK cannot see `umbra-record`, REQ-A-3); the guide notes that the engine
  recorder (ADR 0053) is the lossless replay input.
- `examples/scenarios/plugin_kinds/` uses all five plus `builtin.air.kinematic` and `builtin.sea.kinematic`.
- `umbra new-plugin <kind> <dir> [--id ..] [--sdk-path ..]`: kinds `behavior`, `motion_model`, `sensor`,
  `condition`, `action`, `network_adapter`; writes `Cargo.toml`, `src/lib.rs`, `plugin.toml`, `README.md`
  and a contract test from templates embedded in the CLI; refuses a non-empty directory; the generated
  crate builds offline against the workspace SDK (`--sdk-path`) and its test passes (an acceptance test).

## Consequences
- Easier: authors copy one directory per kind; the guide and the scaffolding share the same code shape.
- Harder: gate 12 builds five more plugins; the examples are API surface the guide must keep current.
- Revisit if the recording adapter should ship as a packaged built-in, or when C# samples arrive (M9).
