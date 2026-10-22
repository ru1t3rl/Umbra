# ADR 0078: Entity selection: lifted into the dashboard shell, map selection by hit test, one DOM label

- **Status**: Accepted by the human 2026-10-22 (with the M8 plan; proposed by frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22)
- **Requirement IDs**: M8 acceptance ("selecting an entity shows its details in the inspector"), `gui/CLAUDE.md` rules 2 and 4 (shells wire, domain features never import each other), NFR-1, ADR 0045, ASM-306, ASM-314
- **Affected**: `@umbra/map-view`, `@umbra/entity-inspector`, `@umbra/shell-dashboard`, decision row D-88

## Context
Three domain features must agree on one selected entity:
- the map (click a marker);
- the entity list (click or keyboard);
- the inspector (shows it; clicking a contact selects that contact).

Domain features cannot import each other. The map draws no text (no glyphs, ASM-314), and the selected
entity needs a ring and a name label. At 10,000 entities nothing may render per entity.

## Options
1. **Selection is lifted state in the dashboard shell (`EntityId | null`), passed down as
   `selectedId` plus `onSelect` (chosen).** It is pure wiring, with no derivation in the shell. The
   rules about selection live in features:
   - the inspector reports when the selected entity left the frame or the session changed;
   - map-view does hit testing.
2. A selection store in a foundation package. Cons: global state for a single page's concern; it
   needs a new store library location (`zustand` is confined to engine-stream).
3. Selection in the URL (`/?entity=42`). Cons: the router is confined to `src`; it can be added later
   on top of option 1 by `src` passing the value in.

## Decision
Option 1.
- **map-view** gains:
  - `selectedId?: number | null`, `onSelect?: (id: number | null) => void`,
    `affiliationOf?: (force: string | null) => Affiliation`;
  - a `ref` of type `LiveMapHandle` with `centerOn(id): boolean`.

  Clicks hit-test with `queryRenderedFeatures` in a 12 px box and pick the nearest feature. A click
  on empty map clears the selection, and so does Escape on the focused map (ASM-803). The selected
  entity gets a ring in a second, single-feature layer, and a name label drawn as one absolutely
  positioned DOM element that is moved once per animation frame (ASM-802).
- **entity-inspector** takes `selectedId` and `onSelect`. When the selected entity is missing from the
  latest frame, it keeps the last values and shows "No longer in the simulation". When the session
  changes, it calls `onSelect(null)`.
- **shell-dashboard** holds `useState<number | null>` and passes it to `LiveMap`, `EntityListPanel`
  and `EntityInspector`. "Center on map" calls `mapRef.current?.centerOn(id)`.

## Consequences
- Easier: the layering rules hold, and every rule about selection has a unit test in its feature.
- Harder: selection does not survive navigation between pages (acceptable in M8, ASM-801).
- Revisit when selection must be shared across pages or deep-linked.
