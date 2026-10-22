# ADR 0034: DIS adapter: in-house codec, v7 and v6 by strategy, golden packets, UDP transport posture

- **Status**: Accepted by the human 2026-10-20 (proposed by the architect, M6 plan `docs/plans/M6.md`, 2026-10-20).
  Settles spec open question 5 (section 19). Records the **human preference of 2026-10-20**: DIS v7
  (IEEE 1278.1-2012) as the primary target, v6 (IEEE 1278.1-1995) as well if reasonably cheap, the version
  chosen by config and isolated in the adapter, golden packets for each supported version.
- **Requirement IDs**: REQ-N-6, REQ-N-1, REQ-N-5, REQ-SDK-3, REQ-A-3, NFR-2, NFR-4, D-9; ADR 0030..0033
- **Affected**: new crate `crates/umbra-net-dis` (`cdylib` + `rlib` for tests, SDK only,
  `#![forbid(unsafe_code)]`), `xtask build-plugins` (package `dis`), `docs/net/dis.md`

## Context
REQ-N-6: UDP unicast, broadcast and multicast; Entity State, Fire, Detonation, Start/Resume, Stop/Freeze
PDUs at least; version v6/v7 configurable; dead reckoning populated; ECEF per the standard; "reuse a
maintained codec crate if license-compatible, otherwise implement with a golden-packet test suite".
The adapter must be built with the SDK exactly as a third party would (REQ-SDK-3), never panic on input
(NFR-2), and keep every DIS notion inside itself (human decision 2026-10-20). No network access is
available to the architect to verify crate licences or maintenance status; any reuse candidate (for
example `dis-rs` on crates.io) needs a human or build-engineer check with `cargo deny` before adoption.

**v6 versus v7 cost.** For the five required PDU families the wire layouts of 1278.1-1995 and -2012 are
the same field for field; v7 differs in the header (`protocol version` 7 instead of 6, byte 7 becomes
the `PDU status` bit field instead of padding) and in the meaning of a few status and appearance bits
(for example the v7 `TEI`/`LVC`/`CEI` status bits). The cost of v6 is a header strategy and one golden
packet per PDU type, an estimated 10-15 % of the codec work. Both are in M6 scope.

## Options
1. **In-house codec for the needed PDUs, version strategy, golden packets (chosen).** About 1.5k lines of
   pure, panic-free byte code with exact control; no new dependency; the plugin crate stays tiny.
2. **Reuse an existing crate (e.g. `dis-rs`).** Broad PDU coverage, but an unverified licence and
   maintenance status from here, extra transitive dependencies in the plugin, behavior on malformed input
   we do not control, and a second type model to map from. Possible later as a dev-dependency cross-check.
3. **v7 only in M6, v6 later.** Saves little (see above); keeps a second variant untested.

## Decision
Option 1, both versions in M6 (human decision requested with this recommendation).
- **Layers inside `umbra-net-dis`:** `codec` (pure: `Pdu` enum for the five families, `encode(&Pdu,
  DisVersion, &mut Vec<u8>)`, `decode(&[u8]) -> Result<(DisVersion, Pdu), DecodeError>`, big-endian,
  bounded lengths, unknown PDU types skipped with a counter, never `unwrap`/index panics; fuzz/proptest),
  `version` (`DisVersion { V6, V7 }` and a `HeaderStrategy` trait with one implementation per version: the
  only place version differences live), `translate` (canonical `UmbraNetMessage` mirror <-> `Pdu`:
  `NetKey` = `site << 32 | app << 16 | entity`, `KeySpace` from the configured site/application,
  entity type tokens `kind.domain.country.category.subcategory.specific.extra` normalised to seven
  decimal fields, force names <-> force ids from config (`friendly` 1, `opposing` 2, `neutral` 3 by
  default), dead-reckoning model <-> DIS DRM 1..9, orientation copied as psi/theta/phi, marking <- name
  (ASCII, 11 bytes, non-ASCII replaced), removal <- `EntityRemoved` sent as a final Entity State with the
  appearance "deactivated" bit; inbound "deactivated" -> `EntityRemoved`; Fire munition type from a
  `weapons` table in config, event ids from an adapter counter; Start/Resume <- `Start`/`Resume`,
  Stop/Freeze <- `Pause` (reason "recess") / `Stop` (reason "termination"); timestamps relative, derived
  from the batch tick and rate (no wall clock), `transport` (sockets and threads).
- **Golden packets:** `crates/umbra-net-dis/tests/fixtures/dis/v7/*.hex` and `v6/*.hex`, one per PDU
  family plus edge cases (marking padding, deactivated appearance, zero entity type), each an annotated
  hex file whose comments name every field, its offset and value, assembled from the standard's field
  tables (provenance in `fixtures/dis/README.md`). Tests: decode(golden) == expected value, encode(value)
  == golden byte for byte, round trip, and truncated / extended / wrong-version / bad-length variants
  return errors. An optional manual cross-check with an independent decoder (Wireshark's DIS dissector,
  used as an external tool only) is recorded in the M6 log if the human runs it (spec section 18 risk).
- **Transport** (adapter-owned threads, `std::net::UdpSocket`, plus `socket2` for `SO_REUSEADDR` /
  multicast options, human decision): one receive thread (blocking `recv_from` with a short read timeout
  so `stop` can join it) feeding a bounded queue (default 4096; when full the new datagram is dropped and counted); one send
  thread draining a bounded queue (default 4096, drop-and-count). Modes: `unicast` (`destinations` list),
  `broadcast` (`broadcast_address`), `multicast` (`group`, `interface`, `ttl`, loopback on). Datagrams
  larger than 8192 bytes are dropped; one datagram per PDU (no bundling).
- **Security posture** (DIS has no authentication; NFR-4, REQ-X-8 spirit): defaults are `mode =
  "unicast"`, `bind = "127.0.0.1:3000"`, no destinations (receive only). A non-loopback bind, broadcast or
  multicast must be configured explicitly and logs one warning ("accepts unauthenticated DIS traffic
  from the network"). Inbound input is bounded at every layer (datagram size, queue length, ghosts per
  adapter, poll per tick).
- **Clarified 2026-10-22 (human-approved, wording only, decision unchanged):**
  - DIS v6 (protocol version 6) is IEEE 1278.1a-1998, not 1278.1-1995 (protocol version 5); read the
    "1278.1-1995" mentions above as 1278.1a-1998 (ASM-416).
  - The v7 `PDU status` field is header byte 10 (v6: padding), not byte 7; byte 11 stays padding in both
    (ASM-416). In v7 it also carries the Fire and Detonation Type Indicators that select the burst
    descriptor form (ASM-424).

## Consequences
- Easier: no dependency or licence risk; full control of malformed-input behavior; v6 and v7 tested side
  by side; the adapter is a pure SDK plugin.
- Harder: golden packets come from our reading of the standard; interoperability with a second DIS tool
  rests on the optional manual check (risk table, spec section 18); more PDUs later are our work.
- Revisit if more PDU types are needed (Transmitter, Signal, Data), if a maintained, permissively licensed
  codec is verified (could replace `codec` behind the same `translate` layer), or after the first
  interoperability test with another tool.
