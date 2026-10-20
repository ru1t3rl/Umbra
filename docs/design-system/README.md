Vantage is the interface language for a Computer Generated Forces simulation suite: scenario authoring, live tactical display, run control and after-action review. It is built for operators who sit in front of it for hours and for the business people who have to be shown it in a meeting. Two traditions are blended on purpose: Material 3 for structure (tonal surfaces, state layers, a disciplined radius scale, filled and tonal buttons) and Apple's glass language for anything that floats over the map (translucent panels, hairline strokes, blur). The result is high contrast, quiet in colour, loud in type and data; the one brand colour is a cyan-leaning blue, so the palette reads cool and technical.

## Principles

1. **The map is the page.** Everything else floats above it on `surface-1` or `glass`. Never box the map inside chrome.
2. **Contrast before decoration.** Every text colour meets 4.5:1 on the surfaces its token note names, in both themes. If a colour is doing nothing for legibility, drop it.
3. **Two accents, two jobs.** `azure` (a cyan-leaning blue) is for command: the action the operator takes next and what is selected. `cyan` (a teal, leaning green so the two are told apart) is for data: live values, links, own position, focus. They never swap roles and never appear as decoration. Attention is `warning`, a status amber that stays inside badges and alerts.
4. **Standards win over taste.** Force affiliation uses the APP-6D fills (`affil-*`) exactly, always framed by `symbol-frame`. Coordinates, times and IDs are set in the mono `data` styles.
5. **Business-grade restraint.** Sentence case, short sentences, no exclamation marks, no emoji. Tactical does not mean cluttered.

## Content fundamentals

- **Casing.** Sentence case for every UI string, including buttons and tabs: "Start exercise", "Order of battle". Uppercase only in the `label` style for field labels and column headers.
- **Voice.** Imperative for actions ("Assign route"), declarative for state ("12 entities paused"). Address the operator as "you" only in help text; never "we".
- **Numbers and units.** Always with a unit and a thin space: "14 km", "60 Hz", "1:4 time". Times are 24-hour Zulu unless the scenario clock says otherwise: `14:32:07Z`. Coordinates in the format the scenario is set to (MGRS by default), never mixed in one view.
- **Entity names** are shown as the scenario author typed them, in `body-strong`; the DIS or HLA identifier beside them in `data`, `ink-muted`.
- **Status words** are one word, lowercase inside a sentence, capitalised in a Badge: Running, Paused, Faulted, Destroyed, Stale.
- **Errors** say what happened and what to do next, in that order: "Route rejected. The destination lies outside the terrain database."
- No emoji, no exclamation marks, no humour in the product; the brand's character comes from the type and the azure.

## Colour

Two themes: **Ops** (dark, the first and default; used in operations rooms and demos) and **Day** (light; used for authoring at a desk and in printed reviews). Every token holds both values; nothing is hard-coded.

- **Surfaces** are a four-step tonal ladder, `surface-0` through `surface-3`. Elevation is expressed by moving up the ladder, not by shadow: a card on a sidebar is `surface-2` on `surface-1`. Use `line` between rows only when the tonal step is not enough.
- **Glass** is for panels that float over the map and the floating command bar only. Fill `glass`, `backdrop-filter: blur(var(--blur-md))`, a 1px `glass-stroke` border, `shadow-glass`, corners `radius-lg`. Text on glass follows the same rules as text on `surface-1`. Never stack glass on glass; the second layer is `surface-2`.
- **Text.** `ink` for primary text, `ink-muted` for labels and metadata, `ink-faint` for placeholders and disabled labels (not on `surface-3`). Text on an azure or cyan fill is `on-azure` or `on-cyan`, never literal white.
- **Azure** (command). One filled azure Button per view. Selected list rows and chips use `azure-dim` behind `azure` text. The active tab indicator and the active toggle track are azure.
- **Cyan** (data) leans teal (`#5ce1e6` in Ops, `#00666f` in Day) precisely so it is never confused with azure at a glance; where the two meet (a selected row showing a live value) azure is the ground and cyan is the figure.
- Live numbers that change at runtime, hyperlinks, the own-force position marker and track lines, and the focus ring are all `cyan`.
- **Status**: `success`, `warning` (a status amber, with `on-warning` for text on its fill), `danger`, `info`, each with a `-dim` ground for badges and banners. Success and danger differ in lightness as well as hue and always carry a word or icon, so colour is never the only signal.
- **Affiliation** (`affil-friend`, `affil-hostile`, `affil-neutral`, `affil-unknown`) are symbol and track fills, never text and never UI accents. Frame every fill with a 1.5px `symbol-frame` stroke; in the Day theme the standard fills (`#00a8dc`, `#00a000`, `#e1dc00`) sit below 3:1 on the surfaces and the frame is what keeps them visible. See the Tactical display section.
- **State layers.** Hover paints `state-hover` over the control; press and keyboard activation paint `state-pressed`. Disabled content fades to `opacity-disabled`, disabled fills to `opacity-disabled-fill`.
- **Focus.** `outline: 2px solid var(--focus-ring); outline-offset: 2px` on every focusable control. Where outline-offset is impossible use `shadow-focus`. The ring is cyan on both themes and lands on the surface, never on the control's fill, so it is 3:1 everywhere.

## Typography

Two families, both hosted on Google Fonts; load `Archivo` (weights 400 to 700) and `JetBrains Mono` (400, 500). There are no font files in this system.

- **Archivo** is the display and text face (`display`, `sans`). Its slightly condensed, square letterforms give the business copy a signage-like, tactical edge without resorting to a stencil face. Headings carry negative tracking (`display-lg` -0.02em, `headline` -0.015em, `title` -0.01em).
- **JetBrains Mono** is the data face (`mono`). Every coordinate, time, identifier, heading, speed and count is set in a `data` style with `font-variant-numeric: tabular-nums`, so columns of changing values do not jitter.
- Scale: `display-lg` 44/48 · `headline` 28/34 · `title` 20/26 · `title-sm` 16/22 · `body-lg` 16/24 · `body` 14/20 · `body-strong` 14/20 · `label` 12/16 uppercase · `caption` 12/16 · `readout` 32/36 · `data-lg` 20/24 · `data` 13/18 · `data-sm` 11/14.
- Minimum text size in the tactical display is `data-sm` (11px) and only for axis labels and dense telemetry in `ink-muted` or stronger.

## Spacing and layout

- A 4px grid: `space-1` (4) through `space-16` (64). Containers pad with `space-4`; dense lists pad rows with `space-2` vertical and `space-3` horizontal.
- The **command bar** is `space-12` tall, `surface-1` when docked, `glass` with `radius-xl` when floating over the map.
- **Panels** over the map are 320px or 400px wide, inset `space-4` from the viewport edge, `radius-lg`.
- Sidebars are 320px, `surface-1`, with a `line` hairline on the map side.
- Content pages (authoring, review) use a 12-column grid, `space-5` gutters, `space-8` page margins up to 1280px and `space-12` beyond.

## Shape

Material 3's radius ladder: `radius-xs` 4 · `radius-sm` 8 · `radius-md` 12 · `radius-lg` 16 · `radius-xl` 24 · `radius-full`. A control inside a container is one step sharper than the container (Button `radius-sm` inside a Card `radius-md` inside a Panel `radius-lg`). Data cells, badges and affiliation tags stay at `radius-xs`: the data layer is squarer than the chrome.

## Elevation and borders

Tonal first. Shadows (`shadow-glass`, `shadow-menu`) appear only on things that float over the map. `line` separates; `line-strong` bounds: an outline Button or TextField edge is `line-strong` because it carries meaning and must meet 3:1.

## Motion

Short and functional: `duration-fast` (120ms) for state layers and toggles, `duration-base` (200ms) for panels and tab indicators, `duration-slow` (320ms) for dialogs, all on `ease-standard`; closes on `ease-exit`. Live values in Readouts and EntityRows change instantly, never tween; a value that animates lies about the simulation clock. Respect `prefers-reduced-motion` by setting every duration to 0.

## Iconography

Material Symbols Rounded (outlined style, weight 400, optical size 20 and 24) from Google Fonts, set in the current text colour. Icons stand beside a label; an icon-only Button carries an `aria-label`. Military unit symbols are not icons: they are rendered by the symbol layer in APP-6D with the `affil-*` fills and the `symbol-frame` stroke, and are never substituted by Material icons. The product mark is `umbra_icon.svg` at the repository root (human decision 2026-10-20). It is the single source for every Umbra icon: the README header, the GUI favicon and window/app icon, and the Tauri bundle icons (M8: generate the platform sizes from this file, e.g. `bun tauri icon ../umbra_icon.svg`; never redraw or recolour it). Beside the mark, the product name is set in `display-lg` in plain type; the mark is never substituted by a Material Symbol.

## Components

Components live in `components/bundle.js` as `window.Vantage` and expect React 18 on the page; `components/bundle.css` styles them from the tokens. Each component's guidelines sit beside its preview. Compose views from them in this order: CommandBar at the top, the map or page as `surface-0`, Panels and Cards above it, Buttons and Chips inside those. One filled azure Button per view; use `tonal`, `outline` and `ghost` for everything else.
