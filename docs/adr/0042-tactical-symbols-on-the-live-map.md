# ADR 0042: Tactical symbols on the live map (APP-6D frames, no symbol library yet)

- **Status**: Accepted by the human 2026-10-20 (proposed by frontend-architect, M5b plan `docs/plans/M5b.md`, 2026-10-20)
- **Requirement IDs**: A-003 (step 5: "ADR for the symbol library"; `tactical-display.md`), REQ-K-5 (owned and remote entities), A-002 (M5b deliverable 3), NFR-1, `UMBRA-IMPLEMENTATION.md` open item 1
- **Affected**: `@umbra/map-view`, `docs/design-system/tactical-display.md` (one added rule), decision row D-58

## Context
`tactical-display.md` asks for APP-6D symbols: affiliation fill (`affil-*`), 1.5px `symbol-frame` stroke,
24px default (20-40px by zoom). `UMBRA-IMPLEMENTATION.md` suggests evaluating `milsymbol`. The M5
contract carries no symbol identification code (SIDC) and no entity type: an `EntitySummary` has `force`
(free text or null), `ownership` (`owned` | `remote`), `heading_deg`, `speed_mps`. A-002 requires owned
and remote entities to be styled differently (remote entities appear from M6). The design system has no
rule for remote entities yet. Stale and destroyed rules need data the contract lacks (stale threshold,
damage state).

## Options
1. **`milsymbol` (MIT) renders full APP-6D/2525 symbols from SIDCs.** Cons: without SIDCs it can only draw
   the affiliation frame; a dependency (about 1 MB unminified) for four shapes; its colours are its own,
   not our tokens, unless overridden per call.
2. **Draw the four APP-6D affiliation frames ourselves (chosen).** Friend: rectangle; hostile: diamond;
   neutral: square; unknown: quatrefoil. A pure function returns an SVG string per (affiliation,
   ownership, theme) with fill and stroke taken from the resolved token colours; the map adds them with
   `addImage` (rebuilt on theme change). Cons: no function icons or modifiers until the contract carries
   a type; we own about 100 lines of geometry.
3. **Plain coloured circles.** Cons: violates `tactical-display.md` (APP-6D).

## Decision
Option 2 for M5b. `milsymbol` is reconsidered by a new ADR when the contract gains an entity type or SIDC
(M8 or later).
- **Affiliation** from `force` through a fixed default mapping (case-insensitive): `blue`, `friend`,
  `friendly` -> friend; `red`, `hostile`, `opfor` -> hostile; `green`, `neutral`, `white` -> neutral;
  anything else or null -> unknown (ASM-305). No runtime override in M5b (M8 settings).
- **Size** 24px at zoom 12, interpolated 20-40px by zoom (`icon-size` expression).
- **Direction of movement**: an APP-6D direction-of-movement line (`symbol-frame`, 1.5px, length 1.5x the
  symbol) drawn by a second icon layer rotated by `heading_deg` (`icon-rotation-alignment: map`), only
  when `speed_mps` > 0.1.
- **Remote entities (proposed design-language addition, needs the human):** same frame and fill, plus a
  1px `ink-muted` outer ring 3px outside the frame ("external" ring). Added to `tactical-display.md` in
  WI-11 when accepted. Rejected alternatives: dashed frame (APP-6D means "planned"), `opacity-track`
  (reserved for stale), a colour change (forbidden on the map).
- **Not in M5b:** selection ring and label pill (needs selection, M8), stale (needs a threshold), destroyed
  modifier (needs damage state), map text (ADR 0041).

## Consequences
- Easier: no new dependency, colours come only from tokens, theme switch rebuilds four to eight images.
- Harder: every entity of one affiliation looks the same until types exist.
- Revisit when the contract carries an entity type or SIDC, or when the human chooses a different remote
  marker.
