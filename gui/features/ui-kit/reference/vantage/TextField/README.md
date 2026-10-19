# TextField

A TextField takes one line of text or a single value. Use `mono` for coordinates, identifiers, times and anything else an operator will compare digit by digit.

**You provide** a `label` (uppercase `label` style is applied by the component; write it in sentence case), any native input attributes, optional `help`, `mono` and `invalid`.

**Do**: give every field a label, never a placeholder alone; write help as a full sentence; state the limit in an error ("Exceeds the platform maximum of 65 km/h").

**Don't**: use a TextField for a choice of fixed values (that is a Chip group or a select); shorten the width below 160px; colour help text anything but `ink-muted` or `danger`.

Height 36px, `radius-sm`, `surface-1` fill, `line-strong` edge; the edge turns `cyan` on focus and `danger` when invalid. Width 240px by default; set `style.width` for layouts.
