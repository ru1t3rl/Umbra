---
name: implement-milestone
description: Plan and execute one CGF milestone (M0-M9) end to end. Use when asked to start, continue or finish a milestone, or "build the next thing". Coordinates architect, engineers, test-engineer, reviewer and build-engineer.
---

# Implement a milestone

Input: milestone id (e.g. `M2`). If none is given, read `docs/milestones.md` and take the first milestone not marked done.

## Steps
1. **Gate check.** Confirm the previous milestone's acceptance criteria are marked met in `docs/milestones.md` and CI is green. If not, stop and report.
2. **Read** the milestone row in spec section 17 and every REQ-ID it implies (grep `docs/SPEC.md`).
3. **Plan** (delegate to `architect`): numbered work items, each <= one PR, each tied to REQ-IDs, with signatures, test plan and risks. Save as `docs/plans/<milestone>.md`. Get human approval if the plan introduces a new decision (ADR via `record-decision`).
4. **Acceptance tests first** (delegate to `test-engineer`, skill `write-tests`): create failing tests that encode the acceptance criteria.
5. **Implement** work items in order using the matching specialist (`rust-engineer`, `plugin-abi-engineer`, `network-engineer`, `api-engineer`, `frontend-engineer`). One item at a time: failing test, code, green, commit with REQ-IDs.
6. **Review** each item with `code-reviewer` (skill `review-change`). Fix Blockers and Majors before moving on.
7. **Verify** with `build-engineer` (skill `verify-and-build`).
8. **Close out**: tick the acceptance criteria in `docs/milestones.md` with evidence (test names, command output summary), update docs and `docs/assumptions.md`, tag `mN-done`.

## Rules
- Never skip step 4 or 6. Never start the next milestone in the same run unless asked.
- If a task needs something outside the spec, stop and raise it instead of inventing scope.
- Keep a running log in `docs/plans/<milestone>.md` of what is done, deferred and assumed.
