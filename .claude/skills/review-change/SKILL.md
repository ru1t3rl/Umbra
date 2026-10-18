---
name: review-change
description: Structured review of an Umbra change (diff or branch) against the spec, architecture rules and coding principles. Use after every implementation work item and before merge.
---

# Review a change

1. `git diff main...HEAD --stat` then the full diff. List the REQ-IDs claimed by the commit messages; if none, that is a Major finding.
2. Work through the checklist and note evidence (file:line):
   - **Spec**: every MUST for the claimed REQ-IDs satisfied; nothing beyond scope; no v1 non-goal work.
   - **Dependencies**: `cargo xtask check-deps` passes; new external crates justified (license, need).
   - **Architecture**: right crate, no protocol/server code in kernel, no second plugin mechanism, wire types separate from domain types.
   - **SOLID/DRY/KISS**: concrete examples of violations, not generalities.
   - **Safety**: `unsafe` location, `// SAFETY:` comments, tests; no `unwrap`/`expect`/`panic` on external input; FFI panic guards; ABI structs append-only.
   - **Determinism**: wall clock, `HashMap` iteration, unseeded RNG, threads in simulation logic, changed system order.
   - **Ownership model**: behaviors/motion only for `Owned`; no re-publish of `Remote`.
   - **Performance**: allocations in hot loops, per-entity FFI calls, unbounded queues, blocking in tick.
   - **Tests**: written for each REQ-ID, failure paths covered, determinism gate green, no flakiness.
   - **Docs**: rustdoc, ADR for new decisions, assumptions logged, generated artifacts regenerated.
3. Output: **Verdict** (APPROVE / REQUEST CHANGES), then findings by severity (Blocker, Major, Minor, Nit) each with file:line, rule or REQ-ID, and a concrete fix. No praise padding, no vague advice.
4. Do not edit code during review. Hand findings to the owning engineer agent.
