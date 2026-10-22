# ADR 0047: M8 runs alongside M7; the M7-dependent parts wait for M7

- **Status**: Accepted by the human 2026-10-22 (decision: "yes, write the ADR and start M8 in parallel", after the coordinator showed that A-002 ordering rule 4 is a sequencing rule and not a technical dependency for most of M8)
- **Requirement IDs**: D-36, A-002 (ordering rule 4), CLAUDE.md ground rule 1, spec section 17 (M7, M8 rows)
- **Affected**: `docs/amendments/A-002-umbra-and-visual-timeline.md` (ordering rule 4), `docs/SPEC.md` (section 17 ordering paragraph, decision row D-63), `CLAUDE.md` (A-002 ordering line), `docs/milestones.md` (M8 line), `docs/assumptions.md` (parallel-track numbering blocks), `docs/plans/M7.md`, `docs/plans/M8.md`

## Context
A-002 ordering rule 4 says M8 starts after M7 and after M5b. M5b is done (tag `m5b-done`) and M7 has just started. Most of the M8 scope (entity inspector, scenario/run controls, log view, settings, extended dashboard, Tauri packaging for Windows) uses only the M5 control API and the M5b app. Two parts depend on M7 output:
- **plugin/adapter status**: its acceptance criterion lists plugins, capabilities and faults, and M7 adds the `motion_model`, `sensor`, `condition` and `action` kinds (REQ-P-4);
- **template browser parameter forms**, as far as they render parameter schemas M7 adds (air and sea motion models) and any control-API or stream contract M7 adds (for example recording and replay).

M5b already ran alongside M6 and M7 under the same kind of rule (GUI track never blocks or changes engine milestones), with reserved numbering blocks (`docs/assumptions.md`).

## Options
1. **Start M8 now on its own branch; the M7-independent work items run in parallel; the M7-dependent work items, the final acceptance run and the `m8-done` tag wait for `m7-done` (chosen).** Earliest feedback, no rework, M7 stays first among engine milestones.
2. Keep strict order (A-002 rule 4 as written). Cons: the GUI track idles for the whole of M7 although two thirds of M8 has no M7 dependency.
3. Start all of M8, including plugin status and schema forms, against the M6 contract and rework after M7. Cons: known rework; acceptance would be checked against a contract that is about to change.

## Decision
Option 1.
- M8 may start once M5b has passed (already true). It runs on the GUI track on branch `m8`, alongside M7 on branch `m7`.
- **M7-independent work items** (inspector, scenario/run controls, log view, settings, extended dashboard, Tauri packaging) may be implemented and merged before M7 closes.
- **M7-dependent work items** (plugin/adapter status for the new kinds; template-browser forms for parameter schemas or contract additions M7 introduces; any GUI use of recording or replay) start only after the M7 contract change they consume is merged to `develop` and `generate-api-client` has run. Where a work item can be built against the M6 contract and extended additively, the plan says so and its M7 extension is a separate work item.
- M8 changes no engine crate, no `api/` contract file and no engine acceptance criterion. A contract gap found by M8 is raised to the M7 coordinator as a separate, additive `update-openapi` work item on the engine side, never made on the `m8` branch.
- M8's acceptance criteria are run, ticked and tagged `m8-done` only after `m7-done`, against the merged `develop`. M8b still follows M8. M9 still follows M7 and does not wait for M8.
- Numbering (no clashes between the two branches): ADR 0047 and decision row D-63 are this decision; M7 takes ASM-600..ASM-799, ADR 0050..0069, D-64..D-79; M8 takes ASM-800..ASM-999, ADR 0070..0089, D-80..D-95.

## Consequences
- Two branches touch shared docs (`docs/milestones.md`, `docs/assumptions.md`, `docs/SPEC.md` decision rows); merges resolve by keeping both sides, as for M5b and M6.
- The M8 plan must mark each work item as M7-independent or M7-dependent.
- If M7 changes the control API or stream schema, the GUI regenerates its client after the merge; gate 15 (`gen:api --check`) catches drift.
- Revisit if M7's contract changes turn out to touch the surfaces of the M7-independent items (for example changed run states); then those items get a follow-up work item rather than a revert.
