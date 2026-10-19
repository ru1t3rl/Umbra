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

## Known informational warnings (not failures)
`bun run check:contrast` reports WARN lines for affiliation fills on light surfaces and for text on glass over the worst-case map pixel. They are by design: symbols are always framed with `symbol-frame` (the frame is checked at 3:1), and glass legibility depends on the dark base map required by `tactical-display.md`. Do not "fix" them by changing affiliation fills (APP-6D standard colours).

## Open items for the implementing agents
- Choose the APP-6D symbol renderer for the map (evaluate `milsymbol`; record an ADR).
- Map style (MapLibre/Cesium, decided at M8) must be darkened and desaturated in Ops per `tactical-display.md`; derive it from the tokens, not from hex literals.
- Build a "design system" showcase route in `shell-settings` (or a dev-only shell) rendering every token and component in both themes; add Playwright screenshot tests.
