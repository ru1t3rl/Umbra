# Readout

A Readout shows one number the operator watches: the clock, an entity count, a message rate, a distance. The number is set in the `readout` mono style; `live` values are `cyan` to say they change at runtime.

**You provide** a `label`, the `value` (already formatted, fixed width where possible), an optional `unit`, `live`, and an optional `delta` line with a `trend` (`up` success, `down` danger, `flat` muted).

**Do**: format the value before passing it (pad the clock, round the rate); use `live` only for values that update on their own; colour trend by meaning, so a falling fault count can still be `trend="up"` if that is good.

**Don't**: animate value changes; show more than one number per tile; use `readout` size inside a dense list (use `data` or `data-lg`).

`surface-2` fill, `radius-sm`, min-width 140px, `space-3`/`space-4` padding, tabular figures.
