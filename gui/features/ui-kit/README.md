# @umbra/ui-kit

Foundation feature (tier `foundation`): the Vantage design system (A-003). Tokens, fonts, theme and the
reference components are added in M5b WI-3..WI-6. The package exports `.` and `./styles.css` (ADR 0043).
Design values change only in `src/tokens/tokens.json`; run `bun run gen:tokens`.
`tokens.ts` exports every token group as a name list and type (`colorTokens`, `spacingTokens`, `radiusTokens`, `shadowTokens`, `blurTokens`, `opacityTokens`, `durationTokens`, `easingTokens`, `typeStyles`) and `cssVar(name)` for any of them (WI-8).

## Components (M5b WI-4)
Props and `vt-*` class names match `reference/vantage`. Import from `@umbra/ui-kit`.
- `Button`, `ButtonProps`: `variant` (`filled` is the one azure command per view), `size`, `icon`. An icon-only button needs `aria-label` or `aria-labelledby` (type error otherwise, dev warning at runtime).
- `Chip`, `ChipProps`: toggle button, `selected` maps to `aria-pressed`; `ChipGroup`, `ChipGroupProps`: `role="group"` with `label`.
- `Badge`, `BadgeProps`, `Tone`: one status word with a dot; `neutral` is the default.
- `AffiliationTag`, `AffiliationTagProps`, `Affiliation`: APP-6D frame shape and fill; children replace the default word.
- `Alert`, `AlertProps`, `AlertTone`: `role="alert"` for danger and warning, `status` otherwise.

## Components B (M5b WI-5)
- `Panel`, `PanelProps`: glass container floating over the map; a `region` labelled by its title; `width` 320px default, 400 for tables. Never place one on another glass layer.
- `Card`, `CardProps`: opaque tonal container; `level` 1 to 3, `variant="outline"`.
- `TextField`, `TextFieldProps`: label, `help` (wired through `aria-describedby`), `mono`, `invalid` (sets `aria-invalid`).
- `Toggle`, `ToggleProps`: `role="switch"`; needs a `label`, `aria-label` or `aria-labelledby` (type error otherwise, dev warning at runtime). `ThemeToggle`, `ThemeToggleProps`: "Day theme" switch over `useTheme`; needs a `ThemeProvider`.
- `Slider`, `SliderProps`: native range input; the `format`ted value is shown and exposed as `aria-valuetext`; arrow keys are native.
- `Tabs`, `TabItem`, `TabsProps`: `tablist` (optional `label` names it; optional `id` derives tab ids `${id}-tab-${value}` and `aria-controls` `${id}-panel-${value}`, give the panel that id) with roving tabindex; the count follows the label after a space (name "Entities 12"); Left, Right, Home and End move focus and select; `count` above 9999 shows "9999+".

## Components C (M5b WI-6)
- `CommandBar`, `CommandBarProps`: the page `banner` (a `header`; render it directly in the body, nested in `main` or `section` it is no longer a banner landmark). The `clock` is a `timer` named "Scenario clock" (", paused" when `paused`; `paused` needs a `clock` and must be paired with a visible word, a "Resume" button or a "Paused" Badge in the children, since colour is never the only signal); cyan while running, `ink-muted` when paused, tabular figures. `brand` defaults to "Umbra". `floating` is glass; glass `Chip`s and `Panel`s inside a floating bar drop to `surface-2` (no glass on glass). A `Readout` inside the bar renders compact (label over a 20px value, no tile) so it fits the bar height; its `unit` is fine, a `delta` row is not supported there (it would not fit 48px). The bar exposes its height as the custom property `--vt-cmdbar-height` (48px) on the `header`, for shells that place things below it. On the Day theme the glass bar is light and the Readout labels (`ink-muted`) sit on glass: the known glass-contrast WARN of `check:contrast`, by design (ASM-509).
- `Readout`, `ReadoutProps`: a `dl` (label is the term, value the definition); `live` is cyan; the unit follows the value after a thin space; a `delta` may carry a `trend` (glyph plus a hidden word, never colour alone).
- `EntityList`, `EntityListProps`, `EntityRow`, `EntityRowProps`: a labelled `listbox` of `option` buttons. Click, Enter and Space call `onSelect`; Up, Down, Home and End move focus between rows without selecting. Every row stays a Tab stop. Rows must contain no interactive content (an option has presentational children). `stale` dims the row and shows the word "Stale" in place of the status Badge; the affiliation is spelled out for assistive technology. Not built: virtualization and a single roving tab stop (M8, ASM-337).
