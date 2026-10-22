#!/usr/bin/env python3
"""Optional independent cross-check of the DIS golden fixtures with Open-DIS Python (not a gate).

Usage, in a throwaway venv:  pip install opendis==1.0
    python3 -I xcheck_opendis.py v7/*.hex v6/*.hex
Decodes each fixture with opendis' dis7 `createPdu` (Entity State, Fire, Detonation, Start/Resume,
Stop/Freeze) and compares every field with the `# <offset> <field> = <value>` annotations of the
file. A field that is annotated but not read from the decoded PDU is reported as UNCHECKED and fails
the file, except the ones listed in IGNORED (constants of the layout and padding). Prints MATCH or
MISMATCH per file; exit 1 on any mismatch.
"""
import re
import sys

from opendis.PduFactory import createPdu

# Annotated fields that are layout constants or padding, not information.
IGNORED = {"protocol_version", "pdu_type", "protocol_family", "padding",
           "descriptor.padding", "descriptor.expendable_padding"}


def read(path):
    data, notes, offsets = bytearray(), {}, {}
    for line in open(path):
        code, _, comment = line.partition("#")
        code = code.strip()
        if code:
            data += bytes.fromhex(code)
            m = re.match(r"(\d+) (.+?) = (.*)$", comment.strip())
            if m:
                notes[m.group(2)] = m.group(3)
                offsets[m.group(2)] = int(m.group(1))
    return bytes(data), notes, offsets


def put_id(got, prefix, e):
    got[f"{prefix}.site"], got[f"{prefix}.application"], got[f"{prefix}.entity"] = (
        e.siteID, e.applicationID, e.entityID)


def put_type(got, prefix, t):
    for name, val in (("kind", t.entityKind), ("domain", t.domain), ("country", t.country),
                      ("category", t.category), ("subcategory", t.subcategory),
                      ("specific", t.specific), ("extra", t.extra)):
        got[f"{prefix}.{name}"] = val


def put_xyz(got, prefix, v):
    got[f"{prefix}.x"], got[f"{prefix}.y"], got[f"{prefix}.z"] = v.x, v.y, v.z


def put_burst(got, d, notes):
    """Open-DIS reads every descriptor as the munition form and ignores the v7 type indicators, so for
    the expendable and explosion forms only the entity type is compared here (the last 8 bytes are
    checked by `check_descriptor_tail`)."""
    if "burst.warhead" in notes:
        put_type(got, "burst.munition_type", d.munitionType)
        got["burst.warhead"], got["burst.fuse"] = d.warhead, d.fuse
        got["burst.quantity"], got["burst.rate"] = d.quantity, d.rate
    else:
        put_type(got, "descriptor.entity_type", d.munitionType)


def check_descriptor_tail(data, notes, offsets):
    """Raw-byte check of the 8 bytes after the entity type of a non-munition descriptor against the
    annotated values (Open-DIS cannot verify these forms)."""
    import struct
    if "descriptor.explosive_force" in notes:
        off = offsets["descriptor.explosive_material"]
        want = struct.pack(">HHf", int(notes["descriptor.explosive_material"]),
                           int(notes["descriptor.padding"]), float(notes["descriptor.explosive_force"]))
        return [] if data[off:off + 8] == want else ["descriptor tail: raw bytes differ from annotations"]
    if "descriptor.expendable_padding" in notes:
        off = offsets["descriptor.expendable_padding"]
        want = bytes(eval(notes["descriptor.expendable_padding"]))
        return [] if data[off:off + 8] == want else ["descriptor tail: raw bytes differ from annotations"]
    return []


def entity_state(p, got, notes):
    o = p.entityOrientation
    dr = p.deadReckoningParameters
    put_id(got, "entity_id", p.entityID)
    got["force_id"] = p.forceId
    got["number_of_articulation_parameters"] = p.numberOfVariableParameters
    put_type(got, "entity_type", p.entityType)
    put_type(got, "alt_entity_type", p.alternativeEntityType)
    put_xyz(got, "linear_velocity", p.entityLinearVelocity)
    put_xyz(got, "location", p.entityLocation)
    got["orientation.psi"], got["orientation.theta"], got["orientation.phi"] = o.psi, o.theta, o.phi
    got["appearance"] = p.entityAppearance
    got["dr.algorithm"] = dr.deadReckoningAlgorithm
    got["dr.other_parameters"] = bytes(dr.parameters)
    put_xyz(got, "dr.linear_acceleration", dr.entityLinearAcceleration)
    put_xyz(got, "dr.angular_velocity", dr.entityAngularVelocity)
    got["marking.character_set"] = p.marking.characterSet
    got["marking.characters"] = bytes(p.marking.characters)
    got["capabilities"] = p.capabilities


def event_id(got, e):
    got["event_id.site"], got["event_id.application"] = e.simulationAddress.site, e.simulationAddress.application
    got["event_id.entity"] = e.eventNumber


def fire(p, got, notes):
    put_id(got, "firing_entity", p.firingEntityID)
    put_id(got, "target_entity", p.targetEntityID)
    put_id(got, "munition_id", p.munitionExpendableID)
    event_id(got, p.eventID)
    got["fire_mission_index"] = p.fireMissionIndex
    put_xyz(got, "location", p.location)
    put_burst(got, p.descriptor, notes)
    put_xyz(got, "velocity", p.velocity)
    got["range"] = p.range


def detonation(p, got, notes):
    put_id(got, "firing_entity", p.firingEntityID)
    put_id(got, "target_entity", p.targetEntityID)
    put_id(got, "munition_id", p.explodingEntityID)
    event_id(got, p.eventID)
    put_xyz(got, "velocity", p.velocity)
    put_xyz(got, "location", p.location)
    put_burst(got, p.descriptor, notes)
    put_xyz(got, "location_in_entity", p.locationInEntityCoordinates)
    got["detonation_result"] = p.detonationResult
    got["number_of_articulation_parameters"] = len(p.variableParameters)


def clock(got, prefix, c):
    got[f"{prefix}.hour"], got[f"{prefix}.time_past_hour"] = c.hour, c.timePastHour


def start_resume(p, got, notes):
    put_id(got, "originating_entity", p.originatingEntityID)
    put_id(got, "receiving_entity", p.receivingEntityID)
    clock(got, "real_world_time", p.realWorldTime)
    clock(got, "simulation_time", p.simulationTime)
    got["request_id"] = p.requestID


def stop_freeze(p, got, notes):
    put_id(got, "originating_entity", p.originatingEntityID)
    put_id(got, "receiving_entity", p.receivingEntityID)
    clock(got, "real_world_time", p.realWorldTime)
    got["reason"], got["frozen_behavior"], got["request_id"] = p.reason, p.frozenBehavior, p.requestID


EXTRACTORS = {"EntityStatePdu": entity_state, "FirePdu": fire, "DetonationPdu": detonation,
              "StartResumePdu": start_resume, "StopFreezePdu": stop_freeze}


def same(key, want, value):
    if key in ("marking.characters", "dr.other_parameters"):
        return bytes(eval(want)).rstrip(b"\0") == value.rstrip(b"\0")
    return float(eval(want)) == float(value)


def main(paths):
    bad = 0
    for path in paths:
        data, notes, offsets = read(path)
        p = createPdu(data)
        kind = type(p).__name__
        got = {"length": p.length, "exercise_id": p.exerciseID, "timestamp": p.timestamp}
        if "/v7/" in "/" + path:  # v6 has no status field
            got["pdu_status (v7) / padding (v6)"] = p.pduStatus
        EXTRACTORS[kind](p, got, notes)
        diffs = check_descriptor_tail(data, notes, offsets)
        for key, want in notes.items():
            if key in IGNORED or (key.startswith("pdu_status") and "/v6/" in "/" + path):
                continue
            if key in ("descriptor.explosive_material", "descriptor.explosive_force"):
                continue  # raw-checked by check_descriptor_tail
            if key not in got:
                diffs.append(f"{key}: UNCHECKED")
            elif not same(key, want, got[key]):
                diffs.append(f"{key}: opendis {got[key]!r}, fixture {want}")
        print("MISMATCH" if diffs else "MATCH", path, kind, p.length, f"{len(notes)} fields")
        for d in diffs:
            print("   ", d)
        bad += bool(diffs)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
