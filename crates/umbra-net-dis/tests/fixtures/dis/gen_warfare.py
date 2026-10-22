#!/usr/bin/env python3
"""Generates the Fire, Detonation, Start/Resume and Stop/Freeze golden fixtures (M6 WI-14),
independently of the Rust codec (same method as gen_es.py).

Field offsets come from the PDU tables of IEEE Std 1278.1-2012 (v7) and 1278.1a-1998 (v6); the
header is the 12-byte header of gen_es.py. Each PDU is encoded twice, once field by field at explicit
offsets and once with a whole-PDU `struct` format, and the script asserts both agree and the sizes.
Adjacent same-width fields get distinct values so a swap cannot pass. All floats are exact binary
fractions.

v7 only: the PDU status byte (byte 10) carries the Fire Type Indicator (Fire, bit 4) and the Detonation
Type Indicator (Detonation, bits 4-5) naming the descriptor form: 0 munition, 1 expendable (entity type and
8 zero bytes), 2 non-munition explosion (Detonation only: entity type, explosive material u16, padding
u16, explosive force f32). The non-munition forms exist only in v7 files. Open-DIS ignores the indicators,
so for those forms the whole-PDU `struct` format in this script is the check of the last 8 bytes.

Usage: python3 gen_warfare.py   (writes v6/ and v7/ next to this file)
"""
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))

HEADER = [
    (0, ">B", "protocol_version"), (1, ">B", "exercise_id"), (2, ">B", "pdu_type"),
    (3, ">B", "protocol_family"), (4, ">I", "timestamp"), (8, ">H", "length"),
    (10, ">B", "pdu_status (v7) / padding (v6)"), (11, ">B", "padding"),
]


def ids(base, prefix):
    return [(base, ">H", f"{prefix}.site"), (base + 2, ">H", f"{prefix}.application"),
            (base + 4, ">H", f"{prefix}.entity")]


def entity_type_fields(base, prefix):
    return [
        (base, ">B", f"{prefix}.kind"), (base + 1, ">B", f"{prefix}.domain"),
        (base + 2, ">H", f"{prefix}.country"), (base + 4, ">B", f"{prefix}.category"),
        (base + 5, ">B", f"{prefix}.subcategory"), (base + 6, ">B", f"{prefix}.specific"),
        (base + 7, ">B", f"{prefix}.extra"),
    ]


def burst(base, form="munition"):
    if form == "munition":
        return entity_type_fields(base, "burst.munition_type") + [
            (base + 8, ">H", "burst.warhead"), (base + 10, ">H", "burst.fuse"),
            (base + 12, ">H", "burst.quantity"), (base + 14, ">H", "burst.rate")]
    if form == "expendable":
        return entity_type_fields(base, "descriptor.entity_type") + [
            (base + 8, ">8s", "descriptor.expendable_padding")]
    return entity_type_fields(base, "descriptor.entity_type") + [
        (base + 8, ">H", "descriptor.explosive_material"), (base + 10, ">H", "descriptor.padding"),
        (base + 12, ">f", "descriptor.explosive_force")]


BURST_FMT = {"munition": "BBHBBBBHHHH", "expendable": "BBHBBBB8s", "explosion": "BBHBBBBHHf"}


def triple(base, fmt, prefix, axes="xyz"):
    size = struct.calcsize(fmt)
    return [(base + i * size, fmt, f"{prefix}.{a}") for i, a in enumerate(axes)]


def fire_table(form):
    return (HEADER + ids(12, "firing_entity") + ids(18, "target_entity") + ids(24, "munition_id")
            + ids(30, "event_id") + [(36, ">I", "fire_mission_index")] + triple(40, ">d", "location")
            + burst(64, form) + triple(80, ">f", "velocity") + [(92, ">f", "range")])


def det_table(form):
    return (HEADER + ids(12, "firing_entity") + ids(18, "target_entity") + ids(24, "munition_id")
            + ids(30, "event_id") + triple(36, ">f", "velocity") + triple(48, ">d", "location")
            + burst(72, form) + triple(88, ">f", "location_in_entity", "xyz")
            + [(100, ">B", "detonation_result"), (101, ">B", "number_of_articulation_parameters"),
               (102, ">H", "padding")])


def fire_fmt(form):
    return (">BBBB I H BB HHH HHH HHH HHH I ddd " + BURST_FMT[form] + " fff f").replace(" ", "")


def det_fmt(form):
    return (">BBBB I H BB HHH HHH HHH HHH fff ddd " + BURST_FMT[form] + " fff B B H").replace(" ", "")


START = (HEADER + ids(12, "originating_entity") + ids(18, "receiving_entity")
         + [(24, ">i", "real_world_time.hour"), (28, ">I", "real_world_time.time_past_hour"),
            (32, ">i", "simulation_time.hour"), (36, ">I", "simulation_time.time_past_hour"),
            (40, ">I", "request_id")])
START_FMT = ">BBBB I H BB HHH HHH iI iI I".replace(" ", "")
STOP = (HEADER + ids(12, "originating_entity") + ids(18, "receiving_entity")
        + [(24, ">i", "real_world_time.hour"), (28, ">I", "real_world_time.time_past_hour"),
           (32, ">B", "reason"), (33, ">B", "frozen_behavior"), (34, ">H", "padding"),
           (36, ">I", "request_id")])
STOP_FMT = ">BBBB I H BB HHH HHH iI BB H I".replace(" ", "")

FIRE_V = dict(
    type=(2, 2), exercise=1, ts=0x01020304, firing=(1, 2, 3), target=(4, 5, 6), munition=(7, 8, 9),
    event=(10, 11, 12), form="munition", status=0, mission=0x0A0B0C0D, loc=(3900100.5, 1100200.25, 4800300.125),
    burst=(2, 2, 225, 3, 4, 5, 6, 0x1201, 0x1302, 0x1403, 0x1504),
    vel=(100.5, -20.25, 3.125), range=2500.5)
DET_V = dict(
    type=(3, 2), exercise=1, ts=0x01020304, firing=(1, 2, 3), target=(4, 5, 6), munition=(7, 8, 9),
    event=(10, 11, 12), form="munition", status=0x03, vel=(30.5, -4.25, 1.125), loc=(3900100.5, 1100200.25, 4800300.125),
    burst=(2, 2, 225, 3, 4, 5, 6, 0x1201, 0x1302, 0x1403, 0x1504), local=(0.5, 1.5, -2.25),
    result=5, articulations=0)
START_V = dict(type=(13, 5), exercise=1, ts=0x01020304, status=0x06, orig=(1, 2, 3), recv=(4, 5, 6),
               real=(13, 0x01020304), sim=(7, 0x0A0B0C0D), request=0x11223344)
STOP_V = dict(type=(14, 5), exercise=1, ts=0x01020304, status=0x01, orig=(1, 2, 3), recv=(4, 5, 6),
              real=(13, 0x01020304), reason=3, frozen=6, request=0x11223344)


def descriptor_values(v):
    form = v["form"]
    if form == "munition":
        return list(v["burst"])
    if form == "expendable":
        return [*v["dtype"], bytes(8)]
    return [*v["dtype"], v["material"], 0, v["force"]]


def values(kind, v, version, length):
    status = v["status"] if version == 7 else 0
    head = [version, v["exercise"], *v["type"], v["ts"], length, status, 0]
    if kind == "fire":
        return head + [*v["firing"], *v["target"], *v["munition"], *v["event"], v["mission"],
                       *v["loc"], *descriptor_values(v), *v["vel"], v["range"]]
    if kind == "det":
        return head + [*v["firing"], *v["target"], *v["munition"], *v["event"], *v["vel"], *v["loc"],
                       *descriptor_values(v), *v["local"], v["result"], v["articulations"], 0]
    if kind == "start":
        return head + [*v["orig"], *v["recv"], *v["real"], *v["sim"], v["request"]]
    return head + [*v["orig"], *v["recv"], *v["real"], v["reason"], v["frozen"], 0, v["request"]]


def tables(kind, v):
    form = v.get("form", "munition")
    return {"fire": (fire_table(form), fire_fmt(form), 96), "det": (det_table(form), det_fmt(form), 104),
            "start": (START, START_FMT, 44), "stop": (STOP, STOP_FMT, 40)}[kind]


def build(kind, v, version):
    table, whole_fmt, base = tables(kind, v)
    n = v.get("articulations", 0)
    length = base + 16 * n
    vals = values(kind, v, version, length)
    buf = bytearray(length)
    for (off, fmt, _n), val in zip(table, vals):
        struct.pack_into(fmt, buf, off, val)
    assert struct.pack(whole_fmt, *vals) == bytes(buf[:base]), f"{kind}: offsets and format disagree"
    assert struct.calcsize(whole_fmt) == base
    # offsets must tile the fixed part without gaps or overlap
    pos = 0
    for off, fmt, name in table:
        assert off == pos, f"{kind}: gap before {name}"
        pos += struct.calcsize(fmt)
    assert pos == base
    for i in range(n):
        buf[base + 16 * i:base + 16 * i + 16] = bytes(0xB0 + 16 * i + j for j in range(16))
    return bytes(buf), vals


def write(name, kind, v, version, note):
    data, vals = build(kind, v, version)
    table = tables(kind, v)[0]
    lines = [
        f"# {name} ({'v7, IEEE 1278.1-2012' if version == 7 else 'v6, IEEE 1278.1a-1998'}): {note}",
        "# Generated by gen_warfare.py from the standard's field table; see README.md.",
        "# format: <hex bytes>  # <offset> <field> = <value>",
    ]
    for (off, fmt, fname), val in zip(table, vals):
        size = struct.calcsize(fmt)
        lines.append(f"{data[off:off + size].hex(' '):<48} # {off} {fname} = {val!r}")
    base = tables(kind, v)[2]
    for i in range(v.get("articulations", 0)):
        off = base + 16 * i
        lines.append(f"{data[off:off + 16].hex(' '):<48} # {off} articulation_parameter[{i}] (opaque)")
    with open(os.path.join(HERE, f"v{version}", f"{name}.hex"), "w") as f:
        f.write("\n".join(lines) + "\n")


EXPENDABLE_TYPE = (3, 2, 225, 3, 4, 5, 6)
EXPLOSION_TYPE = (4, 1, 225, 7, 8, 9, 10)


def scenarios():
    """(name, kind, values, note, versions)"""
    both, v7 = (7, 6), (7,)
    yield "fire_basic", "fire", dict(FIRE_V), "all fields distinct", both
    yield "fire_no_target", "fire", {**FIRE_V, "target": (0, 0, 0)}, "target entity 0.0.0 (no target)", both
    yield "fire_status", "fire", {**FIRE_V, "status": 0x0D}, "v7 status 0x0D: TEI, LVC 2, CEI; munition", v7
    yield "fire_expendable", "fire", {**FIRE_V, "form": "expendable", "dtype": EXPENDABLE_TYPE,
                                      "status": 0x11}, "v7 Fire Type Indicator 1 (status 0x11, with TEI): expendable", v7
    yield "det_basic", "det", dict(DET_V), "status 0x03 (v7), result 5, no parameter records", both
    yield "det_result_1", "det", {**DET_V, "result": 1}, "result 1 (entity impact)", both
    yield "det_no_target", "det", {**DET_V, "target": (0, 0, 0)}, "target entity 0.0.0", both
    yield "det_articulated", "det", {**DET_V, "articulations": 2}, "2 parameter records (decode only)", both
    yield "det_expendable", "det", {**DET_V, "form": "expendable", "dtype": EXPENDABLE_TYPE,
                                    "status": 0x10}, "v7 Detonation Type Indicator 1: expendable", v7
    yield "det_explosion", "det", {**DET_V, "form": "explosion", "dtype": EXPLOSION_TYPE, "material": 0x1234,
                                   "force": 12.5, "status": 0x21}, "v7 Detonation Type Indicator 2 (with TEI): explosion", v7
    yield "start_resume_basic", "start", dict(START_V), "status 0x06 (v7), all fields distinct", both
    yield "stop_freeze_basic", "stop", dict(STOP_V), "status 0x01 (v7), reason 3, frozen behavior 6", both
    yield "stop_freeze_recess", "stop", {**STOP_V, "reason": 2}, "status 0x01 (v7), reason 2 (recess)", both


if __name__ == "__main__":
    for name, kind, v, note, versions in scenarios():
        for version in versions:
            write(name, kind, v, version, note)
    print("ok")
