# @umbra/ui-kit

Foundation feature (tier `foundation`): the Vantage design system (A-003). Tokens, fonts, theme and the
reference components are added in M5b WI-3..WI-6. The package exports `.` and `./styles.css` (ADR 0043).
Design values change only in `src/tokens/tokens.json`; run `bun run gen:tokens`.

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
