# Assumptions

Ambiguities resolved under KISS (spec section 0). Each entry gives its ID, the assumption, the reason, and when to revisit.
Entries that become architectural decisions move to `docs/adr/`.

The log is split per milestone so that an agent reads only the part it needs. IDs are global and never reused;
find an entry with `grep -rn 'ASM-123 ' docs/assumptions/`.

**Adding an entry:** append a row to the file of the milestone you are working on (create `docs/assumptions/<milestone>.md`
with the same header for a new milestone, e.g. `M5b.md`), take the next free number across all files (`grep -rho 'ASM-[0-9]*' docs/assumptions/ | sort -V | tail -1`),
and update that file's range in the table below. Do not add entries to this index.

| Milestone | File | IDs | Entries |
|---|---|---|---|
| M0 | [`docs/assumptions/M0.md`](assumptions/M0.md) | ASM-001..ASM-025 | 25 |
| M1 | [`docs/assumptions/M1.md`](assumptions/M1.md) | ASM-026..ASM-052 | 27 |
| M2 | [`docs/assumptions/M2.md`](assumptions/M2.md) | ASM-053..ASM-069 | 17 |
| M3 | [`docs/assumptions/M3.md`](assumptions/M3.md) | ASM-070..ASM-110 | 41 |
| M4 | [`docs/assumptions/M4.md`](assumptions/M4.md) | ASM-111..ASM-148 | 38 |
| M5 | [`docs/assumptions/M5.md`](assumptions/M5.md) | ASM-149..ASM-200 | 52 |
