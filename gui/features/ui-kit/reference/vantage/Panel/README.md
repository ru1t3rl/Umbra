# Panel

A Panel is the glass container that floats over the map: entity details, order entry, layer settings, measurements. It is the only place the glass language is used besides the floating CommandBar.

**You provide** a `title` (sentence case, `title-sm` style), optional header `actions` (ghost icon Buttons, size `sm`), optional `width` (320px default, 400px for tables) and the body as children.

**Do**: inset the panel `space-4` from the viewport edge; keep at most three panels open over the map; put the panel's own primary action bottom-right, `filled`, size `sm`.

**Don't**: stack a Panel on a Panel (the inner layer is a `Card` at `surface-2`); use a Panel on a content page where there is no map underneath it, use a `Card` on `surface-1` instead; scroll the whole panel, scroll its body.

`glass` fill, `backdrop-filter: blur(var(--blur-md))`, 1px `glass-stroke`, `shadow-glass`, `radius-lg`. Text inside follows the `surface-1` contrast rules.
