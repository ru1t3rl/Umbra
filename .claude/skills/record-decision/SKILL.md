---
name: record-decision
description: Write an Architecture Decision Record when a decision is needed that the spec does not cover, or when proposing to change an existing decision. Use whenever you would otherwise improvise an architectural choice.
---

# Record a decision

1. Check `docs/SPEC.md` section 2 and `docs/adr/` to be sure no decision exists. If one does, do not override it; propose a change instead.
2. Create `docs/adr/NNNN-short-title.md` (next number) with:
   - **Status**: Proposed | Accepted | Superseded by NNNN
   - **Context**: the forces, requirement IDs, constraints
   - **Options**: at least two, each with pros and cons
   - **Decision**: the chosen option and the reason, tied to principles (KISS, SOLID, DRY) and requirements
   - **Consequences**: what gets easier or harder, migration cost, what would make us revisit
3. Keep it to one page. Link REQ-IDs and affected crates.
4. Status stays **Proposed** until the human approves. Do not implement a Proposed ADR that contradicts the spec.
5. When Accepted, add a row to the decision log in the spec (`D-n`) and note it in `docs/assumptions.md` if it relaxes an assumption.
