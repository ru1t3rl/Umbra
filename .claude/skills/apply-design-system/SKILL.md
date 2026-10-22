---
name: apply-design-system
description: Build or change any GUI UI with the Vantage design system: use @umbra/ui-kit components and tokens, follow the colour, glass, typography, tactical-display and content rules, and pass the design checks. Use for every frontend task that renders UI, adds a component, or touches styling.
---

# Apply the Vantage design system

Authority: `docs/design-system/README.md`, `docs/design-system/tactical-display.md`, `docs/design-system/UMBRA-IMPLEMENTATION.md`, `gui/CLAUDE.md`. Tokens: `gui/features/ui-kit/src/tokens/tokens.json`.

## Before building
1. Read the three design docs above (skim if you know them). They are the brief; do not improvise a style.
2. Check `@umbra/ui-kit` for an existing component (Button, Chip/ChipGroup, Panel, Card, TextField, Toggle, Slider, Tabs, CommandBar, Badge, Alert, Readout, EntityRow, AffiliationTag, Icon). Use it as is. Do not restyle it from outside.

## Hard rules (enforced by `bun run check:tokens` where possible)
- **No literals.** Colours, font families, font sizes, radii, shadows, blur, spacing and durations come from tokens (`var(--azure)`, `.type-body`, `var(--radius-sm)`, `var(--space-4)`). Never hex/rgb/hsl, never `font-size: 13px`. A justified exception needs `umbra-allow` on that line plus a comment why.
- **Two accents, two jobs.** `azure` = command and selection (at most one filled azure Button per view). `cyan` = data: live values, links, own position, focus ring. Never swap, never decorate. `warning` amber only in Badges, Alerts and map alert markers.
- **Surfaces are tonal.** Elevation = moving up `surface-0..3`. Shadows only on things floating over the map.
- **Glass** only for panels floating over the map and the floating command bar. Never glass on glass (second layer is `surface-2`). Max three floating panels; a fourth docks into the sidebar.
- **Affiliation colours** (`affil-*`) only for symbol and track fills, always with the 1.5px `symbol-frame`, never text, never UI accents. Symbols are APP-6D, never Material icons.
- **Type.** Archivo for display/text, JetBrains Mono (`data*`, `readout`) for coordinates, times, IDs, headings, speeds, counts, with tabular figures. Sentence case everywhere; uppercase only the `label` style.
- **Shape.** Radius scale only; a control inside a container is one step sharper than the container; data cells, badges and affiliation tags stay `radius-xs`.
- **Motion.** `duration-fast/base/slow` with `ease-standard` (`ease-exit` for closes). Live values never tween. Respect reduced motion (tokens already go to 0ms).
- **Focus.** `outline: 2px solid var(--focus-ring); outline-offset: 2px` on every focusable control.
- **Content.** Units with a thin space ("14 km", "60 Hz"), Zulu times ("14:32:07Z"), no emoji, no exclamation marks, errors say what happened then what to do. Colour is never the only signal: status colours always carry a word or icon.
- **Icons:** Material Symbols Rounded via `Icon`; icon-only buttons need `aria-label`.

## Adding or porting a component
1. Read `reference/vantage/<Name>/README.md` and `preview.html`, the styles in `reference/vantage/bundle.css` (`.vt-<name>` rules), the behaviour in `bundle.js`, and the props in `reference/vantage/index.d.ts`.
2. Tests first (role/label queries, keyboard operation, disabled and invalid states, both themes where visual).
3. Implement `src/components/<Name>/<Name>.tsx` + `<Name>.css` with the **same props and class names** as the reference; styles use tokens only. The component imports its own `./<Name>.css` (and any `../shared/*.css` it uses) by relative path (ADR 0043, amended 2026-10-22); never import another package's CSS. Export from `src/index.ts`.
4. Add it to the showcase route and a Playwright screenshot in both themes.
5. If the design needs a component Vantage does not have, compose from existing ones; if truly new, propose it (ADR) and describe it in `docs/design-system/` before coding.

## Missing token or colour
Never invent a literal. Change `tokens.json` (including its `usage` text), run `bun run gen:tokens`, then `bun run check:contrast`. A new colour or a changed accent role is a design-language change: write an ADR first.

## Finish checklist
- [ ] `bun run gen:tokens --check`, `bun run check:tokens`, `bun run check:contrast` pass
- [ ] Looked at the result in both Ops (dark) and Day (light)
- [ ] One filled azure Button per view; accents used for their jobs only
- [ ] No glass-on-glass; panels over the map use `Panel`
- [ ] Keyboard focus visible and reachable; labels and `aria-label`s present
- [ ] Copy follows the content rules
