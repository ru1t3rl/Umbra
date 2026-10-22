# @umbra/shell-showcase

Shell (page composition, no logic): the Vantage design-system reference page at `/showcase` (A-003, M5b WI-8).

- `route` = `{ path: "/showcase", label: "Design system", lazy }`; the app registers it in `src/router.tsx` (ASM-312). It is not in a navigation menu.
- Shows every token group and every ui-kit component in its reference states, plus a floating (glass) `CommandBar` sample with a glass `Chip`, a `Panel` and a compact `Readout` (the no-glass-on-glass guard).
- It is the screenshot target of `e2e/showcase.spec.ts` (Ops and Day baselines under `e2e/__screenshots__/linux/`).
