# Chip

A Chip is a compact toggle for filters, map layers and the time-rate selector; a group of chips replaces a row of small buttons or a segmented control.

**You provide** the label as children (one or two words), `selected`, `onClick`, an optional `icon`, and `glass` when the chip floats over the map. Wrap a set in `ChipGroup` with an `aria-label`.

**Do**: keep a group to seven chips or fewer; make exactly one chip selected in a single-choice group (time rate) and any number in a multi-choice group (layers); set the selected chip in `azure-dim` with `azure` text, which the component does.

**Don't**: use a Chip to trigger a one-off action (that is a Button); mix glass and solid chips in one group.

Height 32px, `radius-sm`, `line-strong` border at rest so the boundary meets 3:1; `glass` chips use `blur-sm` and `glass-stroke`.
