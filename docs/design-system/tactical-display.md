# Tactical display

Rules for the live map and everything drawn on it. The map is `surface-0`; base-map tiles are desaturated and darkened in the Ops theme so symbols and tracks carry the colour.

## Symbols

- Render units and platforms as APP-6D symbols. Fill with the affiliation token (`affil-friend`, `affil-hostile`, `affil-neutral`, `affil-unknown`) and frame with a 1.5px `symbol-frame` stroke. Modifiers and amplifiers are set in `symbol-frame` as well, in `data-sm`.
- Standard symbol sizes: 24px for point symbols at the default zoom, 32px when selected or hovered; scale symbols by zoom only between 20px and 40px.
- Selection draws a 2px `azure` ring at `radius-xs` outside the frame, plus a `glass` label pill (`data`, `ink`) with the entity's name.
- Own position and the own-force route are `cyan`; planned routes of friendly units are `affil-friend` at `opacity-track`; executed tracks fade to `opacity-track` over their last 60 seconds.
- Stale entities (no update within the scenario's stale threshold) keep their fill but drop to `opacity-track` and gain the word "Stale" in their label; destroyed entities add the APP-6D destroyed modifier and a `danger` badge in the entity list, never a colour change on the map.

## Overlays and panels

- Panels over the map are the `Panel` component: `glass`, `blur-md`, `glass-stroke`, `shadow-glass`, `radius-lg`. Keep at most three open at once; a fourth docks into the sidebar.
- Measurement, range rings and sector arcs are `cyan` at 1.5px with `data` labels on a `glass` pill.
- Areas of interest are hairline `ink-muted` outlines with a `cyan-dim` fill; never a saturated fill that competes with affiliation colours.
- Grid lines (MGRS, lat/long) are `line` at the Ops theme and `line-strong` at `opacity-track` in the Day theme.

## Time

- The scenario clock is a `Readout` in the CommandBar, `readout` style, `cyan` while the simulation is running and `ink-muted` while paused.
- Time-rate controls are a Chip group (1:1, 1:2, 1:4, 1:8) with the current rate selected in `azure`.
- The timeline scrubber in review mode uses `cyan` for the playhead and `azure` for bookmarks.

## Alerts on the map

- Engagements, detonations and faults raise an `Alert` banner anchored to the top of the map, below the CommandBar; banners stack at most two deep and the rest go to the event log.
- A map marker for an alert is a 12px `danger` or `warning` disc framed in `symbol-frame`, with the count in `data-sm`, `on-warning` on a warning disc or `ink` on a danger disc.

## Remote entities

- An entity that is not known to be owned by this simulation (ownership `remote`, or `unknown` when the client cannot tell) keeps the affiliation frame and fill and gains a 1px `ink-muted` ring 3px outside the frame. Only entities known to be owned have no ring; the ring matches the rule that controls are offered for owned entities only.
- The ring is the only marker: no dashed frame (APP-6D means "planned"), no `opacity-track` (reserved for stale) and no colour change (forbidden on the map).
