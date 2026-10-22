# DIS adapter (`umbra-net-dis`)

Mapping between the canonical network model (`umbra_core::net`, seen by the adapter as the
`umbra-sdk` mirror types) and the Distributed Interactive Simulation wire protocol. Requirements:
REQ-N-6, REQ-N-5, REQ-SDK-3, NFR-2; ADR 0030..0034. Status: implemented. The codec for Entity State (v7
and v6) is M6 WI-13, the codec for Fire, Detonation, Start/Resume and Stop/Freeze (v7 and v6) is WI-14, and
the adapter layer (`translate`, `config`, `transport`, `adapter`, packaged as the plugin `dis`) is WI-15.

Sources, named in the golden-packet README (`crates/umbra-net-dis/tests/fixtures/dis/README.md`):

- **v7**: IEEE Std 1278.1-2012, protocol version 7 ("PDU header", "Entity State PDU", "Fire PDU",
  "Detonation PDU", "Start/Resume PDU", "Stop/Freeze PDU").
- **v6**: IEEE Std 1278.1a-1998, protocol version 6 (the same sections). ADR 0034 names the 1995 edition
  for v6; 1278.1-1995 is protocol version 5 and 1278.1a-1998 is version 6 (ASM-416).
- Enumeration values (kind, domain, force, DRM, appearance bits, detonation results): SISO-REF-010.

The wire is **big-endian** throughout. One datagram carries exactly one PDU (no bundling).

## Common header (12 bytes, both versions)

| Offset | Size | Field | v7 | v6 |
|---|---|---|---|---|
| 0 | 1 | Protocol version | 7 | 6 |
| 1 | 1 | Exercise ID | config `exercise` | same |
| 2 | 1 | PDU type | see families | same |
| 3 | 1 | Protocol family | see families | same |
| 4 | 4 | Timestamp | bit 0 = 0 relative (ASM-237) | same |
| 8 | 2 | Length (whole PDU, bytes) | same | same |
| 10 | 1 | **PDU status** | bit field: bit 0 TEI, bits 1-2 LVC, bit 3 CEI, bit 4 Fire Type Indicator (Fire), bits 4-5 Detonation Type Indicator (Detonation) | **padding (0)** |
| 11 | 1 | Padding | 0 | 0 |

The v6/v7 difference in the header is exactly byte 0 and byte 10; `version::HeaderStrategy` is the only
place it lives. The decoder accepts both versions whatever `version` is configured (ASM-249); the
encoder writes the configured one. v6 decode ignores byte 10 (padding, any value) and reports status 0.

| PDU | Type | Family | Version-specific layout |
|---|---|---|---|
| Entity State | 1 | 1 (Entity Information/Interaction) | none |
| Fire | 2 | 2 (Warfare) | none |
| Detonation | 3 | 2 (Warfare) | none |
| Start/Resume | 13 | 5 (Simulation Management) | none |
| Stop/Freeze | 14 | 5 (Simulation Management) | none |

Other types are not an error of the stream: the codec reports `UnsupportedPduType` and the transport
skips the datagram and counts it. Versions other than 6 and 7 are `UnsupportedVersion`.

## Entity State PDU (type 1), 144 bytes plus 16 per articulation parameter

| Offset | Size | Field | Type | Unit / note |
|---|---|---|---|---|
| 12 | 6 | Entity ID | site u16, application u16, entity u16 | key packing below |
| 18 | 1 | Force ID | u8 | 0 other, 1 friendly, 2 opposing, 3 neutral |
| 19 | 1 | Number of articulation parameters | u8 | n; see unsupported |
| 20 | 8 | Entity type | kind u8, domain u8, country u16, category u8, subcategory u8, specific u8, extra u8 | tokens below |
| 28 | 8 | Alternative entity type | same layout | not mapped; preserved by the codec |
| 36 | 12 | Linear velocity | 3 x f32 | m/s, **world (ECEF) axes** |
| 48 | 24 | Location | 3 x f64 | m, **ECEF** (geocentric, WGS-84) |
| 72 | 12 | Orientation | psi, theta, phi: 3 x f32 | radians; frame see below |
| 84 | 4 | Appearance | u32 | bit 23 = state, 1 deactivated (SISO-REF-010 platform appearance) |
| 88 | 1 | Dead reckoning algorithm | u8 | DRM table below |
| 89 | 15 | Dead reckoning other parameters | bytes | preserved by the codec, not mapped |
| 104 | 12 | Linear acceleration | 3 x f32 | m/s^2; world axes for DRM 4, 5, body axes for 8, 9 |
| 116 | 12 | Angular velocity | 3 x f32 | rad/s, **body axes** |
| 128 | 1 | Marking character set | u8 | 1 = ASCII |
| 129 | 11 | Marking characters | 11 bytes | NUL padded |
| 140 | 4 | Capabilities | u32 | not mapped; preserved |
| 144 | 16 n | Articulation parameters | | not supported |

Decoding checks, in this order: at least 12 bytes (`Truncated`); version 6 or 7 (`UnsupportedVersion`);
PDU type known (`UnsupportedPduType`); family matches the type (`WrongFamily`); the header length equals
the datagram length (`LengthMismatch`: truncated and extended datagrams); the length equals
`144 + 16 n` (`BadLength`). The codec is lossless for every field above except the articulation
records (count and content, validated by length and dropped; the encoder writes none) and header
padding (v6 bytes 10-11, v7 byte 11). In v7 these records are called variable parameter records and
may hold more than articulated and attached parts (for example separation, entity type, association);
every record type is dropped the same way.

## Dead reckoning algorithms (DRM)

The canonical enum (ADR 0032) follows the DIS numbering, so the mapping is `DIS code = canonical index + 1`.

| DIS | Name | Extrapolation | Canonical `DeadReckoning` | Sent by M6 |
|---|---|---|---|---|
| 0 | Other | none | inbound: `Static` | no |
| 1 | Static | none | `Static` | yes |
| 2 | FPW (fixed position, world) | constant velocity | `Velocity` | yes (default) |
| 3 | RPW (rotating position, world) | velocity + rotation | `VelocityRotation` | no |
| 4 | RVW (rotating velocity, world) | acceleration + rotation | `AccelerationRotation` | no |
| 5 | FVW (fixed velocity, world) | acceleration | `Acceleration` | yes |
| 6 | FPB (fixed position, body) | body velocity | `BodyVelocity` | no |
| 7 | RPB | body velocity + rotation | `BodyVelocityRotation` | no |
| 8 | RVB | body acceleration + rotation | `BodyAccelerationRotation` | no |
| 9 | FVB | body acceleration | `BodyAcceleration` | no |
| 10..255 | reserved | | inbound: treated as `Static`, counted | no |

Published models are `static`, `velocity`, `acceleration` (ASM-231). Ghosts honour every received code
through `extrapolate` (ADR 0032; body-frame models approximated, ASM-229).

## Canonical to PDU mapping

| Canonical | PDU | Rule |
|---|---|---|
| `EntityState.key` | Entity ID | key packing below |
| `EntityState.entity_type` | Entity type | seven decimal fields <-> bytes; `None` <-> all zero |
| `EntityState.force` | Force ID | config `forces` table (default `friendly` 1, `opposing` 2, `neutral` 3); unknown id inbound -> `None` |
| `EntityState.name` | Marking | outbound: ASCII, 11 bytes, cut at 11 characters, every character that is not printable ASCII (non-ASCII, control) replaced by `?`, NUL padded, charset 1; inbound: text up to the first NUL, bytes that are not printable ASCII replaced by `?`, empty -> `None` |
| `EntityState.position` | Location | copy, ECEF m |
| `EntityState.velocity` | Linear velocity | f64 -> f32, ECEF m/s |
| `EntityState.acceleration` | DR linear acceleration | f64 -> f32 |
| `EntityState.angular_velocity` | DR angular velocity | f64 -> f32, body axes |
| `EntityState.orientation` | psi, theta, phi | yaw, pitch, roll copied (ADR 0032); **frame caveat below** |
| `EntityState.dead_reckoning` | DR algorithm | DRM table |
| `EntityRemoved { key }` | Entity State | final PDU: the entity's last published state with appearance bit 23 set (deactivated); for an entity never published, an otherwise empty state. Inbound deactivated -> `EntityRemoved` |
| `Fire` | Fire PDU | firing entity <- shooter; target (zero ID for none); munition form with the entity type from config `weapons` (zero type if the weapon has no entry), quantity 1, warhead, fuse and rate 0; event ID from the adapter's counter; location <- position; velocity and range 0. Inbound: the weapon of the first `weapons` entry (smallest weapon ID) with the descriptor's entity type, for the munition and expendable forms; a type in no entry drops the PDU, counted |
| `Detonation` | Detonation PDU | firing entity, target (zero ID for none), location in world <- position; result 5 "detonation"; the canonical event has no weapon, so the descriptor is the munition form with a zero type, warhead and fuse; event ID from the same counter as Fire. Inbound: shooter, target and location of any descriptor form (below) |
| `SimulationControl(Start / Resume)` | Start/Resume PDU | originating entity = site, application, entity 0; receiving entity = all (0xFFFF each); real-world and simulation time from the tick (clock time below); request ID from a per-instance counter. Inbound -> `Start` |
| `SimulationControl(Pause / Stop)` | Stop/Freeze PDU | reason 2 recess / 3 termination, same addressing and request ID. Inbound reason 2 -> `Pause`, any other reason -> `Stop` |

Inbound Start/Resume and Stop/Freeze whose originating site and application are this adapter's own (its packet came back over broadcast or multicast) are dropped and counted. The others are translated as above and then counted and ignored by the manager
(`control_ignored`, ASM-235). Values that are not finite make a message not exist, never a panic and never
a ghost: outbound, a value that is not finite as an f32 (NaN, infinity, f64 outside the f32 range) makes
the message unsendable and it is counted in `tx_dropped`; inbound, a NaN or infinity in position, velocity,
acceleration, angular velocity or orientation drops the PDU, counted in `decode_errors`. A key outside the
adapter's own key space is never sent as an Entity State, a removal, or the shooter of a Fire or Detonation.

**Descriptor forms, inbound.** A Fire or Detonation of any form (munition, expendable, non-munition
explosion) becomes the canonical event from the fields the canonical model has: shooter (firing entity,
`None` for entity number 0 or 0xFFFF), target (same rule) and location. Warhead, fuse, quantity, rate,
explosive material and explosive force have no canonical counterpart and are not read, so an explosion or
an expendable never produces a made-up warhead or fuse. A Fire needs a weapon: its descriptor's entity type
(the munition type, or the expendable's type) is looked up in `weapons`; there is no stand-in weapon.
**Descriptors, outbound.** Always the munition form (also the only form v6 has).

## Units and frames

- Positions are ECEF metres, as the standard defines the world coordinate system (geocentric,
  x through the equator at the prime meridian, z to the north pole); this equals Umbra's frame (D-15).
- Velocity and acceleration are metres per second (squared) in ECEF axes for the world DRMs; angular
  velocity is radians per second about the entity's body axes (x forward, y right, z down, FRD).
- Angles are radians on the wire (not degrees).
- Orientation: the Entity Orientation record holds psi, theta, phi, the z-y'-x'' rotation from the DIS
  world coordinate system (geocentric, ECEF axes) to the entity coordinate system (x forward, y right,
  z down, positive theta raises the nose). `WorldEuler` (ADR 0032) is the same sequence and the same
  body axes (`Quaternion::{to,from}_world_euler`, yaw about ECEF z, pitch about the new y, roll about
  the new x), so the adapter copies yaw, pitch, roll to and from psi, theta, phi (f64 <-> f32) with no
  trigonometry. Local heading, pitch and roll (relative to north-east-down) are a separate conversion
  that needs latitude and longitude and is not done here (ASM-420). Example (checked in `tests/translate.rs` against the local north-east-down basis): a level entity at latitude 52, longitude 5 heading east has psi = 95 degrees (90 + longitude), theta = 0 and phi = -142 degrees (-(90 + latitude)).
- Timestamps: relative (bit 0 = 0), the upper 31 bits count units of 3600/2^31 seconds past the hour,
  derived from the batch tick and rate; no wall clock (ASM-237).

## Identity packing and key spaces

`NetKey = site << 32 | application << 16 | entity` (ASM-225). Entity 0 and 0xFFFF are reserved by the
standard and never issued or accepted; site and application are 1..=65534 (config rejects 0 and
0xFFFF).

The key space reported by a configured adapter is `base = site << 32 | application << 16`,
`count = 65534`: keys `base < k <= base + count`, entities `1..=65534` (ADR 0031). The manager keeps
the `EntityId <-> NetKey` bijection (REQ-N-5); the adapter keeps no map of its own. Inbound PDUs whose
Entity ID falls in the adapter's own key space are echoes of our own traffic (or a clash) and are
dropped by the manager. PDUs of another exercise ID are dropped and counted.

## Entity type tokens

`kind.domain.country.category.subcategory.specific.extra`, seven decimal fields (no signs, no spaces),
for example `1.1.225.1.1.0.0`, normalised on both sides (leading zeros stripped, so `01.1.225.1.1.0.0`
is read but never written). A template's `[network.<adapter name>] entity_type` is checked at
configure time; a bad token is a configure error naming the template. An all-zero entity type
is "no type" (`None`). Fields: kind, domain and the last four fields at most 255, country at most 65535.

## Several adapters in one scenario (ASM-410)

Any number of DIS adapters (and other adapters) may run in one scenario. Everything above is
per instance: `site`, `application`, `exercise`, `version`, `forces`, `weapons`, bind address and port,
destinations, group and queues are read from that adapter's own `params`; the plugin keeps no global
state (no statics, one pair of I/O threads per instance, counters per instance). Rules for running two:

- distinct `(site, application)` pairs, otherwise the key spaces overlap and the two instances
  would claim the same keys;
- distinct `bind` ports for unicast (two sockets cannot share a port), or the same multicast
  `group`/port with address reuse;
- distinct ports for unicast instances: a broadcast or multicast socket also receives unicast sent to its port, and with `SO_REUSEADDR` a unicast datagram reaches only one of the sockets sharing the port;
- two multicast instances may share a port in different groups: each socket receives only the groups it joined (`IP_MULTICAST_ALL` off on Linux);
- a ghost received on one instance is never republished on another (ASM-410; no gateway).

Tests: two DIS instances exchanging unicast and three instances with per-instance counters, event IDs and
request IDs (`tests/transport.rs`); one plugin image configured twice under two scenario names, and DIS
next to the loopback in one process (`umbra-engine/tests/dis_plugin.rs`).

## Configuration (`[[network_adapters]] params`)

| Key | Type | Default | Meaning |
|---|---|---|---|
| `version` | 6 or 7 | 7 | version sent; both are accepted inbound |
| `exercise` | 0..255 | 1 | exercise ID sent and required inbound (other exercises are dropped and counted) |
| `site`, `application` | 1..65534 | required | key space |
| `mode` | `unicast`, `broadcast`, `multicast` | `unicast` | |
| `bind` | `ip:port` (IPv4) | `127.0.0.1:3000` for unicast, `0.0.0.0:3000` for broadcast and multicast | receive socket; the port is also the port of the multicast group |
| `destinations` | list of `ip:port` | empty (receive only) | unicast targets (port 0 refused) |
| `broadcast_address` | `ip:port` | required in broadcast mode | broadcast target |
| `group`, `interface`, `ttl` | multicast group, interface address, TTL | `group` required, `interface` `0.0.0.0` (the system chooses), `ttl` 1 | multicast mode; the socket joins `group` on `interface`, sends with that interface and TTL, loopback on |
| `forces` | table name -> id (1..255, unique) | friendly 1, opposing 2, neutral 3 | replaces the default table; names are engine force IDs (`[a-z][a-z0-9_]*`) |
| `weapons` | table weapon -> munition type token | empty | weapon IDs have at least two dot-separated segments; Fire munition type, both ways |
| `queue` | `{ send, receive }`, 1..1000000 | 4096 / 4096 | bounded send and receive queues, in datagrams |

A broadcast or multicast adapter with a loopback `bind` is a configure error naming the adapter (a loopback socket never receives those packets). A key that belongs to another mode (`destinations` with broadcast, `group` with unicast, ...) is a
configure error, not ignored; so is an unknown key, an IPv6 address, and a `[network.<name>]` table of a
template that has anything but a valid `entity_type` token (the message names the template).

A non-loopback bind, broadcast or multicast logs one warning from `start`, never from `configure` (a dry
run opens and says nothing), and never from an I/O thread: "DIS adapter `<name>` accepts unauthenticated
DIS traffic from the network (<reason>, bind <address>)" (NFR-4). Datagrams over 8192 bytes are dropped
and counted.

## Transport

One UDP socket per instance, cloned for two threads (ADR 0033: neither calls the host API). The receive
thread reads with a 50 ms timeout so it sees the stop flag, and puts datagrams into a bounded queue; when
the queue is full the new datagram is dropped and counted. The send thread drains a second bounded queue to
every target of the mode; a full queue drops the new datagram and counts it, and a failed `sendto` is
counted and the adapter goes on. `stop` closes the send queue (the send thread first sends what is queued),
sets the stop flag and joins both threads; the port is free again when it returns. Sends have a 50 ms write timeout (a timed-out send is an I/O error and a drop), and the drain after `stop` stops after 500 ms: what is still queued is dropped and counted in `tx_dropped`, so `stop` returns within about a second even when the network blocks. `publish` and `poll`
never block. Broadcast and multicast sockets set `SO_REUSEADDR` so peers on one host can share the port;
unicast keeps the port exclusive. A broadcast or multicast sender hears its own packets: the manager drops
them as echoes (REQ-K-5, ASM-410).

Counters (the ABI carries four): `tx_dropped` = send queue full, oversized datagram, failed send, message
that cannot be sent; `rx_dropped` = receive queue full, oversized datagram; `decode_errors` = malformed
datagram, other protocol version, unknown PDU type, other exercise, unusable content (reserved entity
number, value not finite, Fire of an unlisted munition); `io_errors` = socket errors. `DisAdapter::stats`
has the detail for tests.

## Fire, Detonation, Start/Resume and Stop/Freeze (WI-14)

| PDU | Length | Body after the 12-byte header |
|---|---|---|
| Fire | 96 | firing entity ID 6, target entity ID 6, munition ID 6, event ID 6, fire mission index u32, location 3 x f64, descriptor 16 (see the forms below; munition form: type 8, warhead u16, fuse u16, quantity u16, rate u16), velocity 3 x f32, range f32 |
| Detonation | 104 + 16 n | firing 6, target 6, munition 6, event 6, velocity 3 x f32, location 3 x f64, descriptor 16, location in entity coordinates 3 x f32, result u8, articulation count u8, padding u16 |
| Start/Resume | 44 | originating 6, receiving 6, real-world time (hour i32, time past hour u32), simulation time 8, request ID u32 |
| Stop/Freeze | 40 | originating 6, receiving 6, real-world time 8, reason u8, frozen behavior u8, padding u16, request ID u32 |

## Unsupported

- Articulation parameters (v7: variable parameter records of any type; Entity State, Detonation):
  validated by length, count and content dropped.
- Entity State Update, Transmitter, Signal, Receiver, Data, Event Report, Comment, Collision, other
  PDU types: skipped and counted. Protocol versions below 6 and above 7.
- Alternative entity type, DR other parameters and capabilities are carried by the codec and not
  mapped to anything canonical.
- Absolute timestamps, authentication, encryption (DIS has none), PDU bundling, fragmentation.

## Warfare and simulation management notes (WI-14)

- v6 and v7 lay these four PDUs out identically; only the common header differs (version byte and
  v7 status byte), so the golden files of the two versions differ in byte 0 and, when the v7 file has a
  non-zero status, in byte 10. The decoder checks the exact length (Fire 96, Start/Resume 44,
  Stop/Freeze 40) or `104 + 16 n` (Detonation, `n` = byte 101).
- Descriptor forms (v7 only; v6 has the munition form alone and ignores byte 10). The 16-byte descriptor of
  Fire and Detonation has three forms, named by the v7 PDU status: the Fire Type Indicator (Fire, bit 4:
  0 munition, 1 expendable) and the Detonation Type Indicator (Detonation, bits 4-5: 0 munition,
  1 expendable, 2 non-munition explosion, 3 reserved). Munition: entity type 8, warhead u16, fuse u16,
  quantity u16, rate u16. Expendable: entity type 8, 8 bytes of padding. Explosion: entity type 8,
  explosive material u16, padding u16, explosive force f32. The codec's `Descriptor` enum carries the
  form; the decoder reads the form from the status bits (a reserved Detonation indicator is
  `DecodeError::ReservedDescriptorForm`, counted as malformed), the encoder refuses a form that the status
  bits do not name (`EncodeError::DescriptorStatusMismatch`) and any non-munition form for v6
  (`DescriptorNeedsV7`). Other status bits (TEI, LVC, CEI, Fire bit 5) are carried unchanged. Padding is
  ignored on decode and written as zero.
- Detonation's munition field is called "exploding entity" in v7. Detonation parameter records after the
  fixed part are validated by length and dropped (v7 calls them variable parameter records), padding
  bytes (Detonation 102-103, Stop/Freeze 34-35) are ignored on decode and written as zero.
- `codec::DecodeCounters` is the per-adapter-instance counter: PDU types the codec does not know are
  skipped and counted by type byte, other versions and malformed datagrams are counted apart.
- Cross-checked against Open-DIS Python (dis7 classes decode all five families, v6 files included).
  Open-DIS ignores the type indicators and reads every descriptor as the munition form, so it cannot verify
  the expendable and explosion forms beyond their entity type; for those the generator's second whole-PDU
  `struct` format and a raw-byte check in `xcheck_opendis.py` are the check.
