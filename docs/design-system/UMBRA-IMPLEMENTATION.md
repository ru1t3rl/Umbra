# Vantage in the Umbra GUI: implementation notes

Vantage (this folder's `README.md` and `tactical-display.md`) is the visual language of the Umbra GUI. The original design lives as an artifact; these files are the repo copy. Where the original assumes a browser mock-up, this page says how it is realised in the real app.

## Where things live
| What | Where |
|---|---|
| Token source of truth | `gui/features/ui-kit/src/tokens/tokens.json` (edit this only) |
| Generated CSS / TS | `tokens.css`, `tokens.ts` next to it, via `bun run gen:tokens` (never hand-edit) |
| Fonts and icons | `gui/features/ui-kit/src/fonts.css` |
| Components | `@umbra/ui-kit` (`gui/features/ui-kit/src/components/<Name>/`), built from the reference in `gui/features/ui-kit/reference/vantage/` |
| Reference (read-only) | `reference/vantage/`: `bundle.css` (styles), `bundle.js` (behaviour), `index.d.ts` (the typed API to match), `<Component>/README.md` + `preview.html` |
| Rules | `docs/design-system/README.md`, `tactical-display.md`, `gui/CLAUDE.md` |

## Deliberate deviations from the original design (keep, they are decisions)
1. **Self-hosted fonts and icons.** The original loads Archivo, JetBrains Mono and Material Symbols Rounded from Google Fonts. The Windows app must work offline, so `fonts.css` uses the npm packages (`@fontsource/archivo`, `@fontsource/jetbrains-mono`, `material-symbols`). Family names and weights are unchanged.
2. **Real components, not `window.Vantage`.** The original ships a global React 18 bundle. Components are re-implemented as typed ES-module components in `@umbra/ui-kit` with the same names, props (see `index.d.ts`), class names (`vt-*`) and visuals. No global, no CDN.
3. **Two light-theme contrast corrections.** The checker (`bun run check:contrast`) found two tokens below the design's own 4.5:1 rule, so they were nudged minimally:
   - `ink-faint` (light) `#5f6c7a` -> `#5c6977` (was 4.39:1 on `surface-2`, now 4.59:1)
   - `danger` (light) `#b8222a` -> `#b21f27` (text on `danger-dim` over `surface-2` was 4.31:1, now 4.56:1)
   To restore the original values, revert those two lines in `tokens.json` and accept the failing checks (not recommended).
4. **Theme switching.** Dark (Ops) is the default. `data-theme="light"` on `<html>` selects Day. A `ThemeProvider` in `ui-kit` offers `dark | light | system` and persists the choice.

## Umbra additions (ADR 0073)
Components the reference does not have, added in M8 WI-1 under the same rules (tokens only, `vt-*` classes, both themes, ASM-837):
- `Table`: `vt-table`, `vt-table-caption`, `vt-table-mono` (data style 13/18), `vt-table-end`, `vt-table-dense`, `vt-table-empty`. Headers use the label style (`vt-label`); cells use `space-2` by `space-3` padding; rules use `line` and `line-strong` on `surface-1`.
- `Select`: `vt-select` on a `vt-field`; shares the frame and control rules with TextField (`shared/Field.css`, `.vt-field :is(input, select)`), label and help classes unchanged.
- `AppNav`: `vt-nav`, `vt-nav-list`, `vt-nav-link`; horizontal, `space-10` high, `ink-muted` at rest, `azure-dim` background for `aria-current="page"`; sits inline in a `CommandBar`.
- `VirtualList` (M8 WI-2, ADR 0072): `vt-vlist` scroller on `surface-1`, `vt-vlist-sizer`, `vt-vlist-row` with the data style 13/18 (`type-data`), `space-3` horizontal padding, `line` rule between rows, `state-hover` on hover, the `azure-dim` background and 2px `azure` edge for the selected row (`aria-selected="true"`, as EntityRow), a 2px inset `focus-ring` outline for the active row (`data-active`). `EntityRow` (M8 WI-10): `badges` are Badges after the status (`vt-entity-badges`, `space-1` gap); `presentational` renders the row content without button or option role as `vt-entity-static` (no padding, no hover, inherited cursor) so a VirtualList row (`row-lg`, two lines) owns the option. Row heights are the tokens `row-sm` (28px), `row-md` (36px) and `row-lg` (56px, two lines) in `tokens.json`. `VirtualList virtualize={false}` (WI-12) mounts all rows with the same classes (short lists); an empty listbox has `tabindex="-1"`.

## Known informational warnings (not failures)
`bun run check:contrast` reports WARN lines for affiliation fills on light surfaces and for text on glass over the worst-case map pixel. They are by design: symbols are always framed with `symbol-frame` (the frame is checked at 3:1), and glass legibility depends on the dark base map required by `tactical-display.md`. Do not "fix" them by changing affiliation fills (APP-6D standard colours).

## Open items for the implementing agents
- Choose the APP-6D symbol renderer for the map (evaluate `milsymbol`; record an ADR).
- Map style (MapLibre/Cesium, decided at M8) must be darkened and desaturated in Ops per `tactical-display.md`; derive it from the tokens, not from hex literals.
- Build a "design system" showcase route in `shell-settings` (or a dev-only shell) rendering every token and component in both themes; add Playwright screenshot tests.
