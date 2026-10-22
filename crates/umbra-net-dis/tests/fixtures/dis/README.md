# DIS golden packets

Entity State PDUs (M6 WI-13) for DIS version 7 (`v7/`) and version 6 (`v6/`), used by
`tests/golden_es.rs`. Fixtures for the other families come with WI-14.

## Provenance

These bytes were **not** produced by the Rust codec (that would be circular). They are produced by
`gen_es.py` in this directory (python3, `struct`, big-endian), which places every field at an explicit
offset taken from the standard's field tables:

| Fixture set | Source of the field table |
|---|---|
| `v7/` | IEEE Std 1278.1-2012, "PDU header" (12 bytes, protocol version 7, byte 10 = PDU status) and "Entity State PDU" (fixed part 144 bytes, 16 bytes per articulation parameter) |
| `v6/` | IEEE Std 1278.1a-1998, "PDU header" (byte 10 and 11 = padding) and "Entity State PDU" (same layout) |
| enumerations | SISO-REF-010: entity kind/domain, force ID, dead reckoning algorithm 0..9, appearance bit 23 (state: deactivated), marking character set 1 = ASCII |

The standards are not in this repository and no network access was available while writing them: the
offsets come from the implementer's knowledge of the tables, double-checked inside `gen_es.py` by a
second, independent encoding of the same PDU with a single `struct` format string (the script asserts
both give the same bytes and that the fixed part is 144 bytes). The field layout agrees with the
widely documented 144-byte Entity State PDU.

Independent cross-check: `xcheck_opendis.py` decodes every fixture with Open-DIS Python and compares
the result with the annotations of the file (optional dev tool, not run by the gates; in a throwaway
venv `pip install opendis==1.0`, then `python3 -I xcheck_opendis.py v7/*.hex v6/*.hex`). Wireshark/`tshark`
is not installed here. To use it: `text2pcap` each hex (strip the `#` comments) as a UDP payload to port 3000
and open it with the DIS dissector, or run `tshark -d udp.port==3000,dis`. Record the result below if
it is ever run.

| Date | Tool and version | Result |
|---|---|---|
| 2026-10-22 | Open-DIS Python `opendis` 1.0 (dis7 `createPdu`), `xcheck_opendis.py` | all 31 MATCH (16 v7, 15 v6) after the field-distinctness revision: exercise, timestamp, ids, force, both entity types, location, velocity, orientation, appearance, DRM 0..9, other parameters, acceleration, angular velocity, marking, capabilities, v7 status, lengths 144/176, 2 articulation parameters |
| 2026-10-22 | `tshark` | not available |

## Format

One `.hex` file per packet; `#` starts a comment; the remaining text is hex byte pairs separated by
white space. Each line holds one field with a comment `# <offset> <field> = <value>`, so a reader can
check the packet against the standard by eye. Regenerate with `python3 -I gen_es.py`; the Rust tests
carry their own hand-written expected values and fail if the files drift.

## Files (each in `v7/` and `v6/` unless noted)

| File | Edge case |
|---|---|
| `es_basic.hex` | Land platform, DRM 2 (FPW), marking `T72-01` NUL padded, non-zero DR "other parameters" |
| `es_status.hex` (v7 only) | PDU status byte 0x0B; v6 has no such field |
| `es_deactivated.hex` | appearance bit 23 set |
| `es_zero_type.hex` | entity type and alternative type all zero |
| `es_marking_full.hex` | 11 marking characters, no padding |
| `es_articulated.hex` | two articulation parameters, length 176 (decode only: the codec drops them) |
| `es_drm_0.hex` .. `es_drm_9.hex` | dead reckoning algorithm codes 0 to 9 |

Every field of `es_basic` has a value different from its neighbours of the same width (entity types `1.2.225.3.4.5.6` and alternative `1.2.222.7.8.9.10`, distinct velocity, acceleration, angular velocity components, a multi-byte appearance and capabilities, distinct articulation bytes), so a swapped pair of fields cannot pass the golden tests.
