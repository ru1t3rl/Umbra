# @umbra/entity-inspector

Domain feature (tier `domain`): the entity list and the selected-entity inspector (M8 WI-10, ADR 0078). Depends on `@umbra/data` (entity detail, remove), `@umbra/engine-stream` (frames, run state, control count) and `@umbra/ui-kit`.

## Public API
- `EntityListPanel({ source, selectedId, onSelect, affiliationOf })` (`EntityListPanelProps`): a search field, filter chips (one per force present, Owned, Remote), a polite "N of M entities" line and a virtualized listbox "Entities" (`VirtualList` with presentational `EntityRow`s: affiliation word, name or `#id`, template, status, a Remote badge). The row model is rebuilt at most every 250 ms (the first frame at once); search and chips apply at once. Only the visible rows exist, so 10,000 entities cost a constant number of commits.
- `EntityInspector({ source, selectedId, onSelect, onCenter?, onClose, affiliationOf })` (`EntityInspectorProps`): nothing without a selection. Readouts from the stream (position, altitude, heading, speed, route, status, template; Source and a Remote badge for remote entities), the motion model and contacts (buttons that call `onSelect`) from `GET /entities/{id}`, "Remove entity" (owned only, danger, in-flight guard, the engine's problem inline), "Center on map" (with `onCenter`) and Close. An entity missing from the newest frame keeps its last values with "No longer in the simulation" (`data-stale="true"`); a frame of another session calls `onSelect(null)` once.
- The detail is read when the entity is selected, every second while the run is `running`, and once after every control frame (`useControlSeq()`, ASM-938); never per tick frame.

`source` is the connection's `EntitySource` (`useEntitySource()`); the run state comes from the same stream. Needs an `EngineStreamProvider` and a `DataProvider` above it.

Pure model (not exported): `findById`, `indexById`, `filterEntities` (`NO_CHIPS`), `entityRowModel`, `formatPosition`, `formatHeading`, `formatSpeed`, `formatRoute`.
