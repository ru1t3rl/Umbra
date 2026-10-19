# CommandBar

The CommandBar is the one persistent strip at the top of every view: product name, scenario title, the scenario clock and the run controls. Docked it sits on `surface-1`; `floating` it becomes glass over the map.

**You provide** `title`, optional `subtitle`, the `clock` string (24-hour Zulu, `data-lg` mono, cyan while running, `ink-muted` when `paused`), `floating`, and the right-hand controls as children: a time-rate Chip, Pause/Resume, and at most one `filled` Button.

**Do**: keep the clock text the same width by using tabular figures (the component does); update it every second without animation; set `paused` the moment the simulation halts.

**Don't**: put navigation tabs in the bar (they belong below it); exceed four controls on the right; use the brand slot for anything but the product name.

Height `space-12` (48px); floating: `glass`, `blur-md`, `glass-stroke`, `shadow-glass`, `radius-xl`, inset `space-4`.
