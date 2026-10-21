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
