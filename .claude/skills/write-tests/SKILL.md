---
name: write-tests
description: Design and write tests for Umbra requirements or milestone acceptance criteria: unit, property, determinism, plugin failure, network, API contract, end-to-end and benchmarks. Use before implementing a feature or when coverage of a REQ-ID is questioned.
---

# Write tests

1. **Trace**: list the REQ-IDs in scope. For each, write one row in `docs/test-matrix.md`: REQ-ID | test type | test name | status.
2. **Choose the cheapest level that proves it**:
   - pure logic -> unit test; invariants -> `proptest`
   - tick pipeline or anything touching state -> **determinism test** (two runs plus different clock modes, per-tick state hash, report first divergence)
   - plugin boundary -> failure matrix (ABI mismatch, missing symbol, wrong struct size, panic in each callback, double load, unload order, bad params)
   - network -> golden packets, round-trip, loopback, echo-loop, ghost timeout, malformed input
   - server -> OpenAPI contract, WebSocket back-pressure (slow client does not slow ticks), error shape
   - whole system -> CLI end-to-end on example scenarios with recorded expected output
   - performance -> `criterion` benchmark per tick phase and the NFR-1 scenario
3. **Write the test first and watch it fail** for the right reason. After implementation, break the code briefly to confirm the test can fail.
4. **Rules**: deterministic (no sleeps, no wall clock, no external network); one behavior per test; names like `given_x_when_y_then_z`; mock only at trait boundaries (`Clock`, `TerrainProvider`, `NetworkAdapter`); fixtures in `tests/fixtures/` with provenance notes.
5. **Reference scenarios** for determinism live in `tests/scenarios/` and must grow with each milestone (M1: waypoint; M3: patrol-and-engage; M6: two-instance DIS).
6. Update `docs/test-matrix.md` and report uncovered REQ-IDs explicitly.
