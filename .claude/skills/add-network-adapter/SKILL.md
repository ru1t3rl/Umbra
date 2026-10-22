---
name: add-network-adapter
description: Add a network adapter (protocol implementation) such as DIS, HLA or a custom transport, built on the public plugin SDK. Use when implementing or scaffolding any adapter.
---

# Add a network adapter

Requirements: REQ-N-1..8, REQ-K-4, REQ-K-5, REQ-T-4, REQ-SDK-3, NFR-2, NFR-4, D-9, D-16, D-49..D-53. Owner: `network-engineer`.

**Read first:** `docs/net/README.md` (the network adapter guide: canonical model, tick boundary, publish policy, ghosts, identity and key spaces, echo handling, several adapters at once, determinism, faults, how to write an adapter). The reference implementation is `crates/umbra-net-dis` with its mapping `docs/net/dis.md`; the SDK side is `docs/plugin-guide.md`, "Writing a network adapter in Rust (ABI 0.1)".

1. **Mapping doc** first (`docs/net/<protocol>.md`, modelled on `dis.md`): canonical fields and messages to protocol messages and back, units and frames (ECEF, `WorldEuler`), identity packing into `NetKey` and the `KeySpace` the adapter reports, type token format, configuration table (`params`, template `[network.<name>]` keys), counters, several-instance rules, unsupported features.
2. **Crate** `umbra-net-<name>` as an SDK plugin (`cdylib` + `rlib`), via skill `add-crate`: depends on `umbra-sdk` only (dev edges included; add the rule-table entry in `xtask/src/deps/rules.rs`), `#![forbid(unsafe_code)]`, never linked by the engine side; protocol vocabulary never leaks into `umbra-core`, `umbra-kernel`, `umbra-net`, `umbra-engine` (`check-deps` protocol rule, `crates/umbra-net/tests/protocol_neutral.rs`). Any new external dependency needs a human decision and `cargo deny check`.
3. **Module split** (as built in `umbra-net-dis`):
   - `codec`: pure, no I/O, typed `DecodeError`/`EncodeError`, never panics (`#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects, clippy::unwrap_used, clippy::expect_used)]`, one bounds-checked reader); unknown message types skipped and counted;
   - `version` (if the protocol has versions): one strategy per version, the only place differences live;
   - `translate`: SDK mirror `NetMessage<'_>` <-> protocol messages (key packing, type tokens, force and weapon tables, dead-reckoning codes, timestamps from the tick and rate, never a wall clock);
   - `config`: params schema and validation in `configure` (no I/O); template tables may hold only `umbra_sdk::MANAGER_TEMPLATE_KEYS` plus the adapter's own documented keys; errors name the key or template; the exposure warning is logged from `start`, never from `configure`;
   - `transport`: adapter-owned I/O threads, bounded queues, drop-and-count, a size limit per datagram; `stop` joins every thread within a bound (and on drop);
   - `adapter`: the SDK traits (`NetworkAdapterFactory`, `NetworkAdapterConfig`, `NetworkAdapter`) over the layers; logic in inherent methods on plain values so it is testable without a host; `lib.rs` only `export_plugin! { .., network_adapters: [..] }`; `plugin.toml` with `abi = "0.1"`; package it in `xtask/src/plugins.rs` (`RUST_PLUGINS`, gate 12).
4. **What the adapter must not do** (the manager owns it, DRY): no identity map (report the `KeySpace`, pack and unpack keys only), no publish policy or thresholds, no ghost creation or timeouts, no echo filtering beyond dropping its own run-control echoes. Deliver what arrives; the manager drops echoes and never re-publishes ghosts (REQ-K-5).
5. **Failure behavior**: `publish` and `poll` never block; bad network input is dropped and counted (`decode_errors`, `rx_dropped`), never a panic or a fault; return `Err` only when the adapter can do nothing more (stopped, thread gone). Values that are not finite never become messages.
6. **Several instances** (ASM-410): no statics or global state; every setting from the instance's own `params`; document what must differ between two instances (identity range, ports, groups).
7. **Security** (NFR-4): loopback-only, receive-only defaults; anything exposed to a network is explicit config plus one warning; bound inbound input at every layer.
8. **Tests**:
   - golden packets in `crates/umbra-net-<name>/tests/fixtures/<protocol>/`, generated **independently of the codec** (a script with two independent encodings, as `gen_es.py`/`gen_warfare.py`), every field a distinct value, provenance in the fixture README; decode == expected, encode byte for byte, round trip; every fixture used (`fixture_coverage.rs`);
   - cross-check the fixtures and real captures with an independent decoder where one exists (DIS: Open-DIS Python via `xcheck_opendis.py`; optional dev tool, not a gate);
   - property tests: arbitrary and mutated bytes (truncate, extend, wrong length, version, family) give typed errors;
   - translate and config tables both ways; transport: overflow counted, oversized dropped, `stop` joins within a bound, a failed send counted while the adapter stays up;
   - the contract suite `umbra_core::network_adapter_contract_tests!(make, feed = ..)` through the real plugin host (the plugin crate cannot dev-depend on `umbra-core`; put it in `crates/umbra-engine/tests/<name>_plugin.rs` like `dis_plugin.rs`);
   - two instances exchanging on localhost, the echo case, two instances side by side and next to the loopback in one scenario (no crosstalk, no forwarding), whole engines and two `umbra run` processes; ports chosen at run time (ASM-246), bounded wait-until loops, never fixed sleeps; 10 green runs in a row.
9. **Docs**: config and counters in `docs/net/<protocol>.md`; a scenario example under `examples/scenarios/`; anything that changes the shared model or the manager goes through an ADR first (`record-decision`).
10. Run `review-change` and `verify-and-build`.
