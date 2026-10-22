# DIS adapter (`umbra-net-dis`)

Mapping between the canonical network model (`umbra_core::net`, seen by the adapter as the
`umbra-sdk` mirror types) and the Distributed Interactive Simulation wire protocol. Requirements:
REQ-N-6, REQ-N-5, REQ-SDK-3, NFR-2; ADR 0030..0034. Status: the codec for Entity State (v7 and v6) is
implemented (M6 WI-13); the sections marked "(WI-15, specified)" describe the adapter layer still to be built; the other PDU families (WI-14), `translate`, `config` and `transport` (WI-15)
follow the tables below. Rows marked **(WI-14)** or **(WI-15)** are specified here and not implemented yet.

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
| 10 | 1 | **PDU status** | bit field (TEI, LVC, CEI) | **padding (0)** |
| 11 | 1 | Padding | 0 | 0 |

The v6/v7 difference in the header is exactly byte 0 and byte 10; `version::HeaderStrategy` is the only
place it lives. The decoder accepts both versions whatever `version` is configured (ASM-249); the
encoder writes the configured one. v6 decode ignores byte 10 (padding, any value) and reports status 0.

| PDU | Type | Family | Version-specific layout |
|---|---|---|---|
| Entity State | 1 | 1 (Entity Information/Interaction) | none |
| Fire **(WI-14)** | 2 | 2 (Warfare) | none |
| Detonation **(WI-14)** | 3 | 2 (Warfare) | none |
| Start/Resume **(WI-14)** | 13 | 5 (Simulation Management) | none |
| Stop/Freeze **(WI-14)** | 14 | 5 (Simulation Management) | none |

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

## Canonical to PDU mapping (WI-15, specified)

| Canonical | PDU | Rule |
|---|---|---|
| `EntityState.key` | Entity ID | key packing below |
| `EntityState.entity_type` | Entity type | seven decimal fields <-> bytes; `None` <-> all zero |
| `EntityState.force` | Force ID | config `forces` table (default `friendly` 1, `opposing` 2, `neutral` 3); unknown id inbound -> `None` |
| `EntityState.name` | Marking | outbound: ASCII, 11 bytes, non-ASCII replaced by `?`, NUL padded, charset 1; inbound: text up to the first NUL, non-ASCII replaced by `?`, empty -> `None` |
| `EntityState.position` | Location | copy, ECEF m |
| `EntityState.velocity` | Linear velocity | f64 -> f32, ECEF m/s |
| `EntityState.acceleration` | DR linear acceleration | f64 -> f32 |
| `EntityState.angular_velocity` | DR angular velocity | f64 -> f32, body axes |
| `EntityState.orientation` | psi, theta, phi | yaw, pitch, roll copied (ADR 0032); **frame caveat below** |
| `EntityState.dead_reckoning` | DR algorithm | DRM table |
| `EntityRemoved { key }` | Entity State | final PDU with appearance bit 23 set (deactivated); inbound deactivated -> `EntityRemoved` **(WI-15)** |
| `Fire` | Fire PDU | firing entity <- shooter; target; munition type from config `weapons`; event ID from an adapter counter; location <- position **(WI-14/15)** |
| `Detonation` | Detonation PDU | firing entity, target, location in world <- position; result 5 "detonation" **(WI-14/15)** |
| `SimulationControl(Start / Resume)` | Start/Resume PDU | **(WI-14/15)** |
| `SimulationControl(Pause / Stop)` | Stop/Freeze PDU | reason 2 recess / 3 termination **(WI-14/15)** |

Inbound Start/Resume and Stop/Freeze are decoded, counted and ignored (ASM-235). Values that are not
finite after conversion (NaN, infinity, f64 outside f32 range for velocity) make the datagram dropped
and counted in `decode_errors`, never a panic and never a ghost.

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
  that needs latitude and longitude and is not done here (ASM-420).
- Timestamps: relative (bit 0 = 0), the upper 31 bits count units of 3600/2^31 seconds past the hour,
  derived from the batch tick and rate; no wall clock (ASM-237).

## Identity packing and key spaces (WI-15, specified)

`NetKey = site << 32 | application << 16 | entity` (ASM-225). Entity 0 and 0xFFFF are reserved by the
standard and never issued or accepted; site and application are 1..=65534 (config rejects 0 and
0xFFFF).

The key space reported by a configured adapter is `base = site << 32 | application << 16`,
`count = 65534`: keys `base < k <= base + count`, entities `1..=65534` (ADR 0031). The manager keeps
the `EntityId <-> NetKey` bijection (REQ-N-5); the adapter keeps no map of its own. Inbound PDUs whose
Entity ID falls in the adapter's own key space are echoes of our own traffic (or a clash) and are
dropped by the manager. PDUs of another exercise ID are dropped and counted.

## Entity type tokens (WI-15, specified)

`kind.domain.country.category.subcategory.specific.extra`, seven decimal fields (no signs, no spaces),
for example `1.1.225.1.1.0.0`, normalised on both sides (leading zeros stripped, so `01.1.225.1.1.0.0`
is read but never written). A template's `[network.<adapter name>] entity_type` is checked at
configure time; a bad token is a configure error naming the template. An all-zero entity type
is "no type" (`None`).

## Several adapters in one scenario (ASM-410; WI-15, specified)

Any number of DIS adapters (and other adapters) may run in one scenario. Everything above is
per instance: `site`, `application`, `exercise`, `version`, `forces`, `weapons`, bind address and port,
destinations, group and queues are read from that adapter's own `params`; the plugin keeps no global
state (no statics, one pair of I/O threads per instance, counters per instance). Rules for running two:

- distinct `(site, application)` pairs, otherwise the key spaces overlap and the two instances
  would claim the same keys;
- distinct `bind` ports for unicast (two sockets cannot share a port), or the same multicast
  `group`/port with address reuse;
- a ghost received on one instance is never republished on another (ASM-410; no gateway).

## Configuration (`[[network_adapters]] params`, WI-15, specified)

| Key | Type | Default | Meaning |
|---|---|---|---|
| `version` | 6 or 7 | 7 | version sent; both are accepted inbound |
| `exercise` | 0..255 | 1 | exercise ID sent and required inbound |
| `site`, `application` | 1..65534 | required | key space |
| `mode` | `unicast`, `broadcast`, `multicast` | `unicast` | |
| `bind` | `ip:port` | `127.0.0.1:3000` | receive socket |
| `destinations` | list of `ip:port` | empty (receive only) | unicast targets |
| `broadcast_address` | `ip:port` | | broadcast mode |
| `group`, `interface`, `ttl` | multicast group, interface address, TTL | | multicast mode |
| `forces` | table name -> id | friendly 1, opposing 2, neutral 3 | |
| `weapons` | table weapon -> munition type token | | Fire/Detonation |
| `queue` | sizes | 4096 / 4096 | bounded send and receive queues |

A non-loopback bind, broadcast or multicast logs one warning ("accepts unauthenticated DIS traffic
from the network", NFR-4). Datagrams over 8192 bytes are dropped and counted.

## Further PDU tables (implemented in WI-14, from the same standard sections)

| PDU | Length | Body after the 12-byte header |
|---|---|---|
| Fire | 96 | firing entity ID 6, target entity ID 6, munition ID 6, event ID 6, fire mission index u32, location 3 x f64, burst descriptor 16 (munition type 8, warhead u16, fuse u16, quantity u16, rate u16), velocity 3 x f32, range f32 |
| Detonation | 104 + 16 n | firing 6, target 6, munition 6, event 6, velocity 3 x f32, location 3 x f64, burst descriptor 16, location in entity coordinates 3 x f32, result u8, articulation count u8, padding u16 |
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
