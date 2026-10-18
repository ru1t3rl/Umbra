---
name: review-frontend-change
description: Structured review of a GUI change against the frontend architecture rules (features vs shells, tiers, public APIs), React/TypeScript quality, performance and test coverage. Use after each frontend work item, before merge.
---

# Review a frontend change

1. `git diff main...HEAD -- gui/`; list affected packages. Confirm commit messages cite REQ-X-* / F-* IDs.
2. **Structure**
   - Right place: feature (library with logic) vs shell (thin composition) vs `src` (bootstrapping only)?
   - Layering: src -> shells -> domain features -> foundation features. Domain features never import each other. No shell->shell, feature->shell, deep imports (`@umbra/x/src/...`) or cycles. `bun run check:deps` passes.
   - Public API minimal, documented in the package README, exported only via `src/index.ts`. `package.json` has correct name, `exports`, `umbra.tier`, `workspace:*` deps.
3. **Shells**: no fetch, no generated-client use, no business logic.
4. **TypeScript/React**: strict, no `any`/`@ts-ignore`; generated files untouched; logic in `model/`/hooks; small components; stable keys; effects clean up; no state derived in effects that could be computed.
5. **Performance**: no per-entity React renders; imperative map layer; batched per frame; virtualized lists; narrow store selectors; lazy-loaded shells.
6. **Contract**: types from the generated client; `gen:api --check` passes; problem-details errors handled; stream reconnect and slow-client behavior handled.
7. **UX/a11y/i18n**: semantic elements, labels, keyboard use, text via the `ui-kit` helper, loading/error/empty states.
8. **Tooling**: bun only; `bun.lock` is the sole lockfile; new dependency justified (size, license, maintenance).
9. **Tests**: written first, cover the public API plus failure and empty states, architecture tests still catch violations, e2e updated for user-facing pages.
10. **Output**: Verdict (APPROVE / REQUEST CHANGES), then findings by severity (Blocker, Major, Minor, Nit) with file:line, rule and concrete fix. Read-only: do not edit code.
