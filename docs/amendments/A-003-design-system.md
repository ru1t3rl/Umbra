# Amendment A-003: Adopt the Vantage design system for the GUI

Status: Accepted. Affects: `gui/` only (milestones M5b/M8/M8b). No engine impact.

## Decision
The Umbra GUI implements the **Vantage** design system (Material 3 structure plus Apple-style glass; cyan-leaning blue "azure" for command, teal "cyan" for data; dark "Ops" default and light "Day"; APP-6D affiliation colours). It is the single visual language for every feature, shell and the Tauri app.

## Why
One coherent, already-designed language prevents each agent inventing its own style. Token-driven theming gives dark/light for free, the contrast rules are testable, and the tactical-display rules fit the map-centric product.

## What is delivered with this amendment
- `docs/design-system/`: Vantage README, tactical-display rules, `UMBRA-IMPLEMENTATION.md` (deviations and open items)
- `gui/features/ui-kit/src/tokens/tokens.json` (source), generated `tokens.css`/`tokens.ts`, `fonts.css`
- `gui/features/ui-kit/reference/vantage/`: the original component CSS, behaviour, typings and per-component guidelines (read-only reference to port from)
- `gui/scripts/`: `gen-tokens.mjs`, `check-tokens.mjs`, `check-contrast.mjs`
- Skill `apply-design-system`

## Deviations from the original (decided)
Self-hosted fonts/icons (offline desktop app), ES-module components instead of `window.Vantage`, two light-theme contrast corrections found by `check-contrast`. Details in `UMBRA-IMPLEMENTATION.md`.

## Apply it (give to `frontend-architect`, then `frontend-engineer`)
1. **Add to `gui/package.json` scripts:**
   ```json
   "gen:tokens": "node scripts/gen-tokens.mjs",
   "check:tokens": "node scripts/check-tokens.mjs",
   "check:contrast": "node scripts/check-contrast.mjs"
   ```
   (`bun scripts/...` also works.) Add `bun run gen:tokens --check`, `check:tokens` and `check:contrast` to the `verify-frontend` skill and the CI frontend job.
2. **Append to `gui/CLAUDE.md`:**
   ```
   ## Design system (Vantage, amendment A-003)
   All UI follows docs/design-system/ (README.md, tactical-display.md, UMBRA-IMPLEMENTATION.md). Use the apply-design-system skill for any UI work.
   - Colours, fonts, sizes, radii, shadows, spacing, motion come only from @umbra/ui-kit tokens (var(--azure), .type-body, ...). No hex/rgb/hsl, font-size or font-family literals outside ui-kit/src/tokens. `bun run check:tokens` enforces this.
   - Use @umbra/ui-kit components as-is. New components are ported from ui-kit/reference/vantage with identical props and class names.
   - tokens.json is the only place to change design values; run `bun run gen:tokens`, then `check:contrast`. Never hand-edit tokens.css/tokens.ts.
   - Azure = command (one filled azure Button per view), cyan = data. Glass only for floating panels/command bar, never glass on glass. Affiliation colours only for framed symbols.
   - ui-kit is a foundation feature; other packages depend on it, never the reverse.
   ```
3. **Update agents** (append one line each): `frontend-architect`: "Plan UI with the Vantage design system; propose new components only via ADR." `frontend-engineer`: "Always use the apply-design-system skill for UI; run gen:tokens --check, check:tokens, check:contrast before finishing." `frontend-test-engineer`: "Add Playwright screenshot tests for each ui-kit component and each shell in both themes (Ops and Day)." And add to `review-frontend-change`: "Design system compliance: tokens only, accents used for their jobs, no glass-on-glass, content rules, both themes checked."
4. **Install packages** in `ui-kit`: `bun add --cwd gui/features/ui-kit @fontsource/archivo @fontsource/jetbrains-mono material-symbols`, import `fonts.css` and `tokens.css` once in `src/` (app entry) via `ui-kit`'s public entry. Verify the package import paths.
5. **Plan the ui-kit work** (first GUI work items, before feature packages): ThemeProvider (dark/light/system, persisted, sets `data-theme`), port all 14 components from the reference with tests, showcase route with both themes and screenshot tests, base styles (body, focus), then the map style and APP-6D symbol layer (ADR for the symbol library) when `map-view` starts.

## Acceptance criteria
- [ ] `gen:tokens --check`, `check:tokens`, `check:contrast` run in CI and pass
- [ ] All 14 reference components exist in `@umbra/ui-kit` with the reference props and tests
- [ ] The showcase renders every token and component correctly in Ops and Day
- [ ] A deliberate hex literal in a feature makes `check:tokens` fail (tested)
- [ ] The app starts in Ops (dark) and can switch to Day without a reload
