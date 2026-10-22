# ADR 0031: Identity map, key spaces, echo suppression and ghost lifecycle

- **Status**: Accepted by the human 2026-10-20 (proposed by the architect, M6 plan `docs/plans/M6.md`, 2026-10-20)
- **Requirement IDs**: REQ-N-4, REQ-N-5, REQ-K-5, REQ-K-4, REQ-C-2, D-16; human decision 2026-10-20
  (protocol neutrality, ADR 0030)
- **Affected**: `umbra-net` (`identity`, `ghosts`), `umbra-core::net` (`NetKey`, `KeySpace`),
  `umbra-engine` (`GhostWorld` implementation), `umbra-kernel` (remote spawn and update, ADR 0032)

## Context
REQ-N-5 asks for a bijective map `EntityId <-> adapter-specific id`, with the adapter config supplying
site and application ids (DIS vocabulary; neutral reading: "the adapter config supplies the local
identity range"). REQ-K-5 forbids re-publishing `Remote` entities (echo loops). REQ-N-4 asks for stale
ghost removal after a timeout. The manager must not know protocol identities (human decision
2026-10-20), yet it must allocate ids for owned entities and recognise its own traffic coming back
(multicast and broadcast loop packets back to the sender).

## Options
1. **Opaque `u64` keys with an adapter-declared local key space; the manager allocates and maps
   (chosen).** DIS encodes `site << 32 | application << 16 | entity` and declares
   `KeySpace { base: site << 32 | application << 16, count: 65534 }` (entity 1..65534, inside the adapter);
   HLA/RPR encodes its `EntityIdentifier` the same way.
2. **Opaque byte-string keys.** General, but an allocation per message and no cheap range check; no
   known protocol needs more than 64 bits (RPR reuses the 48-bit identifier).
3. **Adapter allocates and maps.** A private identity map per adapter (rejected by the
   `add-network-adapter` skill and REQ-N-5).

## Decision
Option 1.
- **Identity map** (`umbra_net::identity::IdentityMap`, one per adapter): two `BTreeMap`s,
  `EntityId -> NetKey` and `NetKey -> EntityId`, plus a role per entry (`Local` or `Remote`). Insertions
  that would break bijectivity are refused (`IdentityError`), never overwritten. Entries are dropped when
  the entity is removed; a key is never reused within a run (as `EntityId`s, REQ-C-2).
- **Local keys.** An owned entity gets its key at its first publish on that adapter:
  `base + n`, `n` counting from 1 in first-publish order (entities are visited in `EntityId` order, so
  allocation is deterministic). When `count` keys are used, further owned entities are not published on
  that adapter (counter `keys_exhausted`, one warning).
- **Echo suppression, three layers.** (1) The manager publishes only `Ownership::Owned` entities and only
  events whose source is owned (REQ-K-5); a `Remote` entity never reaches `publish`. (2) Inbound messages
  whose key (or shooter key) lies in the adapter's own `KeySpace` are dropped and counted (`echo_dropped`):
  this catches the sender's own multicast/broadcast loop and a misconfigured peer with the same identity
  range. (3) Inbound keys already mapped to an owned entity are dropped too (`echo_dropped`), whatever
  their range. Ghosts from adapter A are never published on adapter B.
- **Ghost lifecycle** (per adapter): an inbound `EntityState` for an unknown key creates a ghost
  (`GhostWorld::create_ghost` with the mapped template, the force and name from the message, the
  `RemoteSource` = adapter name, the remote state; ADR 0032), for a known key it updates the ghost's
  track; `EntityRemoved` removes it; a ghost that received nothing for `[ghosts] timeout_s` (default 12 s,
  **simulation time**, ASM-233) is removed at the next ingest. `[ghosts] max` (default 10 000) caps live
  ghosts per adapter; further new keys are dropped (`ghosts_capped`). Creation and removal produce
  `EntityCreated`/`EntityRemoved` events.
- **Inbound order.** Adapters are polled in scenario order, at most `max_inbound_per_tick` messages each
  (default 4096; the rest stay in the adapter's bounded queue), and messages are applied in the order
  the adapter delivered them. That order is the run's network input stream (A-2).
- **Inbound fire and detonation.** `Fire` becomes `SimEvent::Fire` only when shooter and target both map
  to entities and the weapon id is valid; `Detonation` becomes `SimEvent::Detonation` with whatever maps.
  They change no state (damage is a v1 non-goal). Inbound `SimulationControl` is counted and ignored in M6
  (ASM-235).

## Consequences
- Easier: protocol identity stays inside adapters; echo prevention does not depend on an adapter getting
  it right; ghost churn is bounded in memory.
- Harder: two engines configured with the same identity range drop each other's traffic as echo
  (documented, and visible in the counters); timeouts follow simulation time, so a paused engine keeps its
  ghosts and a peer's ghosts of a paused engine time out (documented).
- Revisit if a protocol needs keys wider than 64 bits, or if a wall-clock timeout is wanted in real-time
  mode.
