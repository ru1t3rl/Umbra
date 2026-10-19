# Card

A Card is an opaque tonal container on a content page or in a sidebar: a formation summary, a scenario in a library, a settings group. Elevation is tonal, never a shadow.

**You provide** an optional `title` and `subtitle`, a `level` (1 to 3, which surface it sits on: default 2 for a card on `surface-1`), `variant="outline"` for an empty or placeholder card, and children.

**Do**: choose `level` one step above the surface the card rests on; keep the card's padding at `space-4`; put metadata in `caption` (`ink-muted`) under the title.

**Don't**: nest more than two levels; add a coloured left border or a shadow; put a Card over the map (that is a `Panel`).

`radius-md`, `space-4` padding, `surface-1` to `surface-3` fill or a `line` outline.
