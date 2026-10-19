# Button

A Button triggers an action; the `filled` azure variant is the one command the view exists for, used at most once per view. `tonal` is the default for everything else that matters, `outline` and `ghost` for secondary and tertiary actions, `danger` for destructive ones (always confirmed by a dialog when irreversible).

**You provide** the label as children (verb first, sentence case: "Start exercise"), an optional `icon` (a Material Symbols Rounded name), `variant`, `size` and any native button attributes. An icon-only button needs an `aria-label`.

**Do**: one `filled` per view; put the filled button last in a row, nearest the content it acts on; use `size="sm"` only inside Panel headers and Alerts.

**Don't**: use a filled button for navigation; place two filled buttons side by side; colour a button cyan (cyan is data, never command); use literal white text on azure (the component uses `on-azure`).

Height 36px (`sm` 28px), `radius-sm`, `space-4` horizontal padding. Hover and press paint the Material state layers; focus is the cyan `focus-ring` offset 2px.
