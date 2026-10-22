# ADR 0072: List virtualization: a fixed-row-height `VirtualList` in ui-kit, no new dependency

- **Status**: Proposed (frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22)
- **Requirement IDs**: NFR-1 (10,000 entities), `gui/CLAUDE.md` ("virtualize long lists"), A-003, WAI-ARIA APG listbox, M5b deferral ("EntityList virtualization", "single roving tab stop")
- **Affected**: `@umbra/ui-kit` (new `VirtualList`), `@umbra/entity-inspector`, `@umbra/log-view`, decision row D-82

## Context
The entity list must hold 10,000 rows and the log up to 50,000 records without one DOM node per row.
`EntityRow` and a log line have a fixed height in the Vantage density. The M5b review asked for a single
roving tab stop on any `EntityList` shown with live entities. happy-dom has no layout, so a virtualizer
that depends on measuring DOM rows is hard to test.

## Options
1. **Own `VirtualList` in ui-kit: fixed row height from a token, overscan, viewport height from an
   injectable `ResizeObserver`, `aria-activedescendant` keyboard model (chosen).** About a hundred lines,
   pure window maths in `model/`, deterministic in tests.
2. `@tanstack/react-virtual` (MIT). Pros: variable heights, battle-tested. Cons: measurement-driven, so
   happy-dom tests need stubs; a new runtime dependency for a fixed-height case.
3. `react-window` (MIT). Pros: small. Cons: its own element structure and roles; a11y wiring on top
   anyway; maintenance has slowed.

## Decision
Option 1.
- `VirtualList` props: `label`, `role: "listbox" | "log"`, `count`, `rowHeight` (a token name, resolved
  once), `renderRow(index)`, `getRowId(index)`, `overscan` (default 8), and for `listbox`:
  `activeIndex`, `onActiveIndexChange`, `onActivate`; for `log`: `followEnd` (stick to the newest row
  until the user scrolls up). Rows carry `aria-setsize` and `aria-posinset`.
- Keyboard (listbox): focus stays on the list (one tab stop), `aria-activedescendant` names the active
  row; arrows, Home, End, PageUp, PageDown move it; Enter or click activates (selects). Selection does
  not follow focus (ASM-836).
- Tests assert counts: rendered rows stay at most `ceil(viewport / rowHeight) + 2 * overscan` for
  10,000 items, and scrolling re-renders only the list.

## Consequences
- Easier: no dependency, deterministic tests, one keyboard model for every long list.
- Harder: variable-height rows are not supported (wrapping log messages are truncated with a full-text
  tooltip and the detail shown on activation).
- Revisit when a list needs variable heights; then option 2 behind the same props.
