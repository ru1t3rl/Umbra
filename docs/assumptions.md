# Assumptions

Ambiguities resolved under KISS (spec section 0). Each entry gives its ID, the assumption, the reason, and when to revisit.
Entries that become architectural decisions move to `docs/adr/`.

The log is split per milestone so that an agent reads only the part it needs. IDs are global and never reused;
find an entry with `grep -rn 'ASM-123 ' docs/assumptions/`.

**Adding an entry:** append a row to the file of the milestone you are working on (create `docs/assumptions/<milestone>.md`
with the same header for a new milestone, e.g. `M5b.md`), take the next free number across all files (`grep -rho 'ASM-[0-9]*' docs/assumptions/ | sort -V | tail -1`),
and update that file's range in the table below. Do not add entries to this index.

**Parallel tracks (M5b and M6, from 2026-10-20).** M5b and M6 run at the same time on separate branches, so each takes
numbers from its own reserved block instead of the next free number:
- M6: ASM-224..ASM-299, ADR 0030..0039, decision rows D-49..D-55;
- M5b: ASM-300..ASM-379, ADR 0040..0049, decision rows D-56..D-62.
- Overflow blocks (coordinator decision 2026-10-21, when the first blocks were nearly used up): M6 continues at
  ASM-400..ASM-499, M5b at ASM-500..ASM-599. ADR and decision-row blocks are unchanged.
**Parallel tracks (M7 and M8, from 2026-10-22, ADR 0047).** ADR 0047 and decision row D-63 are the ordering decision itself.
- M7: ASM-600..ASM-799, ADR 0050..0069, decision rows D-64..D-79;
- M8: ASM-800..ASM-999, ADR 0070..0089, decision rows D-80..D-95.
Gaps left in a block stay unused. After both close, numbering continues after the highest number used.

| Milestone | File | IDs | Entries |
|---|---|---|---|
| M0 | [`docs/assumptions/M0.md`](assumptions/M0.md) | ASM-001..ASM-025 | 25 |
| M1 | [`docs/assumptions/M1.md`](assumptions/M1.md) | ASM-026..ASM-052 | 27 |
| M2 | [`docs/assumptions/M2.md`](assumptions/M2.md) | ASM-053..ASM-069 | 17 |
| M3 | [`docs/assumptions/M3.md`](assumptions/M3.md) | ASM-070..ASM-110 | 41 |
| M4 | [`docs/assumptions/M4.md`](assumptions/M4.md) | ASM-111..ASM-148 | 38 |
| M5 | [`docs/assumptions/M5.md`](assumptions/M5.md) | ASM-149..ASM-223 | 75 |
| M5b | [`docs/assumptions/M5b.md`](assumptions/M5b.md) | ASM-300..ASM-378 and ASM-500..ASM-517 (blocks ASM-300..ASM-379 and ASM-500..ASM-599) | 97 |
| M6 | [`docs/assumptions/M6.md`](assumptions/M6.md) | ASM-224..ASM-299, ASM-400..ASM-458 | 135 |
| M8 | [`docs/assumptions/M8.md`](assumptions/M8.md) | ASM-800..ASM-930, ASM-932 (block ASM-800..ASM-999) | 116 |
