# EntityRow

An EntityRow is one unit or platform in the order-of-battle list: affiliation, name, identifier, one mono line of telemetry and a status Badge. Rows live inside `EntityList`.

**You provide** `name`, `id` (DIS entity ID or HLA handle, shown in mono), `affiliation`, optional `echelon` for the tag, `meta` (position, heading, speed in one mono line, separated by " · "), `status` (`label`, `tone`), `selected`, `stale` and `onSelect`.

**Do**: keep `meta` to one line and one coordinate format; mark stale entities with `stale` and the word "Stale" in the badge, never a colour change alone; sync selection with the map's selection ring.

**Don't**: show more than one badge per row; truncate the name without a title attribute; use row colour to show affiliation (the tag does that).

Row padding `space-2`/`space-3`, `radius-xs`; selected rows paint `azure-dim` with a 2px `azure` inset edge; hover paints `state-hover`. `EntityList` is `surface-1`, `radius-md`, with `line` hairlines between rows.
