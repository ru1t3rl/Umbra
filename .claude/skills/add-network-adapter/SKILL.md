---
name: add-network-adapter
description: Add a network adapter (protocol implementation) such as DIS, HLA or a custom transport, built on the public plugin SDK. Use when implementing or scaffolding any adapter.
---

# Add a network adapter

Requirements: REQ-N-1..8, REQ-SDK-3, D-9, D-16. Owner: `network-engineer`.

1. **Mapping doc** first (`docs/net/<protocol>.md`): table mapping canonical events and fields to protocol messages and back, units and coordinate frames, identity scheme, unsupported features.
2. **Crate** `umbra-net-<name>` as an SDK-based plugin (`cdylib`), via skill `add-crate`. It must not depend on engine internals.
3. **Codec layer** (pure, no I/O): encode/decode with `Result` errors, never panics on malformed input. Golden packets in `tests/fixtures/<protocol>/` with documented provenance. Property test round-trips.
4. **Adapter layer**: implements the `NetworkAdapter` SDK trait (`configure`, `start`, `publish`, `poll`, `stop`). I/O on adapter-owned threads, bounded queues, drop-and-count on overflow, never block the tick.
5. **Identity**: maintain the `EntityId <-> protocol id` bijection via the shared network-manager helper, not a private copy (DRY).
6. **Publishing policy** stays in `umbra-net` (thresholds, heartbeats). Do not reimplement it per protocol.
7. **Template integration**: read `[network.<name>]` template sections as opaque config (REQ-T-4) and document the schema.
8. **Inbound**: emit canonical events only; ghosts are created by the kernel (REQ-N-4). Never re-publish remote entities.
9. **Tests**: golden packets, malformed-input fuzz/property tests, loopback integration, two-instance localhost exchange, echo-loop prevention, ghost timeout.
10. Run `review-change` and `verify-and-build`. Document config in `docs/net/<protocol>.md`.
