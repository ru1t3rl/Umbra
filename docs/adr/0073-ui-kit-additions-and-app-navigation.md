# ADR 0073: M8 ui-kit additions (Table, Select, AppNav, VirtualList) and app navigation without the router in features

- **Status**: Accepted by the human 2026-10-22 (H-7; proposed by frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22)
- **Requirement IDs**: A-003 ("new components ... only via ADR"), `gui/CLAUDE.md` (design system; layering rules 1-5; `react-router` only in `src/`, M5b WI-2), spec section 13
- **Affected**: `@umbra/ui-kit`, `gui/src` (router composition), every M8 shell, `docs/design-system/UMBRA-IMPLEMENTATION.md`, decision row D-83

## Context
M8 adds four pages (live map, templates, plugins, settings) and needs controls the 14 Vantage reference
components do not cover:
- a data table (plugin capabilities, adapter counters, components by kind, affiliation overrides);
- a select for enumerations with more than a handful of values (schema enums, recent scenario paths);
- page navigation;
- a virtualized list (ADR 0072).

`react-router` is confined to `src/` by a dependency-cruiser rule; shells and features must still
render links and navigate (the inspector links a template, settings links the showcase).

## Options
1. **Add `Table`, `Select`, `AppNav` and `VirtualList` to ui-kit, styled from tokens with `vt-*`
   classes and shown in the showcase; navigation through a ui-kit `NavigationProvider` whose
   `navigate` and items are supplied by `src` from the router (chosen).**
2. Build these controls inside each feature. Cons: duplicates, Vantage drift, `check:tokens` gaps.
3. Allow `react-router` in shells. Cons: loosens a tested rule; shells become router-coupled and harder
   to test; features still could not link.
4. Adopt a component library (Radix, React Aria) for the new controls. Cons: a second styling system
   under Vantage; new runtime dependencies; A-003 asks for ported, token-only components.

## Decision
Option 1.
- `Table`: semantic `<table>` with `caption` (visible or visually hidden), `columns` (`key`, `label`,
  `align`, `mono`), `rows`, `rowKey`, `empty` text, `dense`. No sorting or paging in M8 (YAGNI).
- `Select`: a native `<select>` in the `TextField` visual frame (`label`, `help`, `invalid`, `options`,
  `value`, `onChange`, `disabled`), so keyboard and screen-reader behaviour are the platform's.
- `AppNav`: a `<nav>` landmark with a list of links (`aria-current="page"`), shown inside each page's
  CommandBar; items, the current path and `navigate` come from context.
- `NavigationProvider({ items, currentPath, navigate })`, `useAppNavigate()`, `AppLink({ to, children })`
  (an `<a href>` that calls `navigate` on a plain left click and keeps the browser default for modified
  clicks). `src` builds the items from shell `route` metadata (`path`, `label`, `icon`, `nav`) and
  passes `react-router`'s navigate. No feature or shell imports the router.
- `VirtualList` per ADR 0072.
- All four are added to the showcase in both themes with new screenshot baselines for the human's
  review, and to `UMBRA-IMPLEMENTATION.md` as Umbra additions beyond the Vantage reference.

## Consequences
- Easier: every page uses the same table, select and navigation; the router stays confined.
- Harder: ui-kit grows four components we own outright (no reference to port from), so the reviewer
  checks them against the design rules by hand.
- Revisit when a page needs sorting, paging or column resizing in a table, or Vantage publishes its own
  versions of these components (then port and replace).

## Amendments
- 2026-10-23: `TableColumn` gains an optional `render(row)` cell (M8 WI-13, ASM-961); the default cell is unchanged.
