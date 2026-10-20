---
name: write-frontend-tests
description: Write GUI tests with bun test, Testing Library and Playwright: model logic, components, shells with mocked engine client, stream decoding, architecture-rule tests and e2e. Use before or after implementing any frontend work.
---

# Write frontend tests

1. **Trace** requirements (REQ-X-*, F-*) to tests in `docs/test-matrix.md`.
2. **Pick the cheapest level**:
   - `model/` pure logic and reducers -> `bun test` unit tests
   - components/hooks -> Testing Library + happy-dom; query by role/label, not test ids unless unavoidable
   - shells -> render with a mocked `@umbra/data` and a fake stream store; assert wiring between features
   - stream -> frame decoding (json and msgpack), delta application, filter changes, reconnect, dropped-frame handling
   - e2e -> Playwright in `e2e/` against a headless engine started with an example scenario
3. **Setup** (`bunfig.toml`): `[test] preload = ["./test/setup.ts"]` registering happy-dom and jest-dom matchers. Use fake timers; no real sleeps; no external network.
4. **Write the failing test first**, implement, then break the code briefly to confirm the test can fail.
5. **Architecture tests**: for `check:deps`, include fixtures violating each rule (domain->domain, deep import, shell->shell, feature->shell, cycle) and assert the check fails.
6. **Performance test**: feed 10,000 entity updates per frame through the stream store; assert the map layer update stays within the frame budget and React render count does not scale with entity count.
7. **Accessibility**: role/label assertions in component tests; axe check in e2e for each shell.
8. Colocate tests (`*.test.ts(x)`) with source; mock only at `@umbra/data`/`engine-stream`. Run via `bun test` and `bun run test:e2e`.
