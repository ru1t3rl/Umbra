#!/usr/bin/env python3
"""Optional independent cross-check of the Entity State fixtures with Open-DIS Python (not a gate).

Usage, in a throwaway venv:  pip install opendis==1.0
    python3 -I xcheck_opendis.py v7/*.hex v6/*.hex
Decodes each fixture with opendis' dis7 `createPdu` and compares the result with the
`# <offset> <field> = <value>` annotations of the file. Prints MATCH or MISMATCH per file; exit 1 on
any mismatch.
"""
import re
import sys

from opendis.PduFactory import createPdu


def read(path):
    data, notes = bytearray(), {}
    for line in open(path):
        code, _, comment = line.partition("#")
        code = code.strip()
        if code:
            data += bytes.fromhex(code)
            m = re.match(r"(\d+) (.+?) = (.*)$", comment.strip())
            if m:
                notes[m.group(2)] = m.group(3)
    return bytes(data), notes


def main(paths):
    bad = 0
    for path in paths:
        data, n = read(path)
        p = createPdu(data)
        loc, o, t, v = p.entityLocation, p.entityOrientation, p.entityType, p.entityLinearVelocity
        a, dr = p.alternativeEntityType, p.deadReckoningParameters
        marking = bytes(p.marking.characters).rstrip(b"\0")
        got = {
            "length": p.length,
            "entity_id.site": p.entityID.siteID,
            "entity_id.application": p.entityID.applicationID,
            "entity_id.entity": p.entityID.entityID,
            "force_id": p.forceId,
            "number_of_articulation_parameters": p.numberOfVariableParameters,
            "entity_type.kind": t.entityKind, "entity_type.domain": t.domain,
            "entity_type.country": t.country, "entity_type.category": t.category,
            "entity_type.subcategory": t.subcategory, "entity_type.specific": t.specific,
            "entity_type.extra": t.extra,
            "linear_velocity.x": v.x, "linear_velocity.y": v.y, "linear_velocity.z": v.z,
            "location.x": loc.x, "location.y": loc.y, "location.z": loc.z,
            "orientation.psi": o.psi, "orientation.theta": o.theta, "orientation.phi": o.phi,
            "appearance": p.entityAppearance,
            "dr.algorithm": p.deadReckoningParameters.deadReckoningAlgorithm,
            "marking.characters": marking,
            "exercise_id": p.exerciseID,
            "timestamp": p.timestamp,
            "capabilities": p.capabilities,
            "alt_entity_type.kind": a.entityKind, "alt_entity_type.domain": a.domain,
            "alt_entity_type.country": a.country, "alt_entity_type.category": a.category,
            "alt_entity_type.subcategory": a.subcategory, "alt_entity_type.specific": a.specific,
            "alt_entity_type.extra": a.extra,
            "dr.linear_acceleration.x": dr.entityLinearAcceleration.x,
            "dr.linear_acceleration.y": dr.entityLinearAcceleration.y,
            "dr.linear_acceleration.z": dr.entityLinearAcceleration.z,
            "dr.angular_velocity.x": dr.entityAngularVelocity.x,
            "dr.angular_velocity.y": dr.entityAngularVelocity.y,
            "dr.angular_velocity.z": dr.entityAngularVelocity.z,
            "dr.other_parameters": bytes(dr.parameters),
            "marking.character_set": p.marking.characterSet,
        }
        if "/v7/" in "/" + path:  # v6 has no status field
            got["pdu_status (v7) / padding (v6)"] = p.pduStatus
        diffs = []
        for key, value in got.items():
            want = n.get(key)
            if want is None:
                continue
            if key in ("marking.characters", "dr.other_parameters"):
                ok = bytes(eval(want)).rstrip(b"\0") == value.rstrip(b"\0")
            else:
                ok = float(eval(want)) == float(value)
            if not ok:
                diffs.append(f"{key}: opendis {value!r}, fixture {want}")
        status = "MISMATCH" if diffs else "MATCH"
        bad += bool(diffs)
        print(status, path, p.length, p.numberOfVariableParameters, hex(p.entityAppearance),
              p.deadReckoningParameters.deadReckoningAlgorithm, marking)
        for d in diffs:
            print("   ", d)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
