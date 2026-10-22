# ADR 0043: GUI toolchain, npm dependencies and licence policy

- **Status**: Accepted by the human 2026-10-20 (proposed by frontend-architect, M5b plan `docs/plans/M5b.md`, 2026-10-20)
- **Requirement IDs**: F-1..F-8, D-13, NFR-4 (licences), REQ-A-4, A-003 (step 4, fonts and icons), ADR 0004 (no CI)
- **Affected**: `gui/package.json`, every `gui/**/package.json`, `gui/bun.lock`, `gui/scripts/check-licenses.ts`, `docs/build.md` (prerequisites), decision row D-59

## Context
M5b creates the `gui/` Bun monorepo (F-2). F-1..F-8 fix the tools by name but not versions or licences.
The Rust side enforces licences with `cargo deny` (gate 6, `deny.toml` allows MIT, Apache-2.0,
Unicode-3.0, Zlib, ISC, BSD-3-Clause, ...). npm has no equivalent gate yet. Several tools ship with a
`#!/usr/bin/env node` launcher (Playwright, orval, dependency-cruiser) and Playwright supports only the
Node runtime. Installing needs the npm registry once, and Playwright needs a pinned Chromium build.

## Options
1. **The F-1..F-8 tools plus the minimum around them, exact versions in `bun.lock`, an in-repo licence
   check (chosen).**
2. Add helpers (a component library, a CSS framework, a state helper per feature). Cons: conflicts with
   A-003 (ui-kit is the only component source) and KISS.
3. A third-party licence checker (`license-checker` etc.). Cons: another dependency for a 60-line script.

## Decision
Option 1.
- **Package manager and runtimes.** Bun pinned by `"packageManager": "bun@<installed>"` in
  `gui/package.json`. Node.js LTS (22 or 24) is a GUI prerequisite for the node-launched CLIs
  (Playwright, orval, dependency-cruiser); `bun test` and the `scripts/*.mjs` run on Bun.
- **Dependencies** (the list and licences are in the plan's "Decisions for the human" table; exact
  versions recorded here by WI-1 after install). Runtime: `react`, `react-dom`, `react-router`,
  `@tanstack/react-query`, `zustand`, `maplibre-gl`, `@fontsource/archivo`,
  `@fontsource/jetbrains-mono`, `material-symbols`. Dev: `typescript`, `vite`, `@vitejs/plugin-react`,
  `@types/react`, `@types/react-dom`, `@types/bun`, `orval`, `json-schema-to-typescript`,
  `@biomejs/biome`, `dependency-cruiser`, `@playwright/test`, `@happy-dom/global-registrator`,
  `@testing-library/react`, `@testing-library/dom`, `@testing-library/jest-dom`,
  `@testing-library/user-event`, `@axe-core/playwright` (if accepted). Nothing else without an ADR
  amendment.
- **Where they live.** Each dependency is declared in the package that imports it (`maplibre-gl` only in
  `@umbra/map-view`, `zustand` only in `@umbra/engine-stream`, `@tanstack/react-query` and `orval` only in
  `@umbra/data`, fonts and icons only in `@umbra/ui-kit`); shared dev tools in the root package.
- **Licence policy, `bun run check:licenses`** (`gui/scripts/check-licenses.ts`, no dependency): reads
  every installed package's `license` field (SPDX expression) and fails on anything outside the allow
  list. Runtime closure (what ships in the bundle): MIT, ISC, BSD-2-Clause, BSD-3-Clause, Apache-2.0,
  0BSD, and OFL-1.1 for font files. Dev-only closure additionally: MPL-2.0 (`axe-core`), CC-BY-4.0
  (`caniuse-lite` data), Python-2.0 (`argparse`), BlueOak-1.0.0, CC0-1.0. Missing or unknown licences fail.
  The runtime closure is computed from the dependency graph of the non-dev dependencies.
- **Network.** One online `bun install` in WI-1 (and in any later WI that adds an accepted dependency),
  then `bun install --frozen-lockfile` everywhere. One `playwright install chromium` (the browser build
  pinned by `@playwright/test`, about 170 MB under `~/.cache/ms-playwright`). Nothing at test time or at
  app runtime touches the network (ADR 0041).
- **Package entries.** A package's `exports` is `./src/index.ts`; a package that ships CSS may add exactly
  one more entry, `./styles.css` (ui-kit, map-view). The app imports each once. Components never import
  CSS (bun test has no CSS loader). This extends `gui/CLAUDE.md` rule 5. *(Superseded in part
  2026-10-22, see "Amended 2026-10-22": components import their own co-located CSS; `./styles.css` is
  optional.)*

## Amended 2026-10-22 (human decision): components import their own CSS
The 2026-10-20 bullet "Package entries" said:
- "components never import CSS (bun test has no CSS loader)";
- every package that ships CSS exposes it through one `./styles.css` entry, imported once by the app.

M5b shipped something different, and it works:
- every ui-kit component imports its co-located `<Name>.css` and the package-internal
  `../shared/*.css` it needs;
- the dashboard and showcase pages import their `<Name>Page.css`;
- M8's `SchemaForm` imports `SchemaForm.css`;
- `bun test` loads these imports.

The human chose option (a) on 2026-10-22: the rule follows the code.

**Rule from 2026-10-22**
- **A component imports its own CSS.** It may import its co-located CSS file
  (`./<Name>.css`, beside `<Name>.tsx` per ADR 0046) and package-internal shared CSS
  (`../shared/<x>.css`) by relative path. It never imports another package's CSS file.
- **CSS rules are unchanged.** CSS stays token-only (`check:tokens`) and co-located with its component.
- **`./styles.css` is optional.** A package does not need one. A package may still declare that one
  extra export for styles that must load globally, once.
- **What the app imports globally today** (`gui/src/main.tsx`, the same pair in
  `gui/scripts/measure-map/entry.ts`):
  1. `@umbra/ui-kit/styles.css`: fonts, `tokens/tokens.css`, `base.css`. Its `@import` list of
     component CSS files duplicates the components' own imports; the bundler loads each file once.
  2. `@umbra/map-view/styles.css`: the MapLibre vendor CSS and the `LiveMap` container and error
     rules.
- **No other package has a `./styles.css` entry.** Shells and the other features rely on the
  component-level imports. Shell pages load their CSS with the lazy page chunk.

**Option not chosen: (b) per-package `styles.css` cleanup.** It would have removed every
component-level CSS import, made each package that ships CSS aggregate its files in one
`./styles.css`, and made the app import each entry once.
- Rejected: it rewrites working M5b code and gives up co-location and code-splitting of page CSS.
- Rejected: every new feature with CSS would need a new app import.
- Rejected: the original reason (no CSS loader in `bun test`) no longer holds.

**Consequences of the amendment**
- Easier: a component carries its styles. Lazy pages and features need no change in `src/main.tsx`.
- Harder: the global order of component CSS follows module import order, not one list. Component
  rules therefore stay scoped to their `vt-*` or `umbra-*` class and must not depend on cascade order
  between components. Global tokens and base styles still load first, from the app.
- Unchanged: the dependency-cruiser deep-import rule. A relative import inside one package is not a
  deep import; `@umbra/x/src/...` stays forbidden.
- Superseded: ASM-302 and the old wording of `gui/CLAUDE.md` rule 5. Decision row D-59 carries a
  dated note.
- Optional later cleanup, not required: trim ui-kit's `styles.css` to fonts, tokens and base.

## Consequences
- Easier: reproducible installs, licence regressions caught locally (gate 15), offline afterwards.
- Harder: Node is a second runtime to install; version bumps are deliberate PRs.
- Revisit when Playwright supports the Bun runtime, or when a dependency changes its licence.

## Installed versions (M5b WI-1, 2026-10-20)
Human decision 2026-10-20: when the registry's current major differed from the plan's table (typescript 7,
vite 8, @vitejs/plugin-react 6, happy-dom 21, jest-dom 7), the plan's major was pinned; every other package
takes the latest version within the plan's major. Versions are exact in `gui/package.json` and `gui/bun.lock`.
Only the dependencies WI-1 needs are installed; the others arrive with the WI that uses them (each with
an online `bun install`).

| Package | Version | Licence (installed `license` field) | Where |
|---|---|---|---|
| react, react-dom | 19.3.1 | MIT | root, runtime |
| @fontsource/archivo, @fontsource/jetbrains-mono | 5.3.0 | OFL-1.1 (the plan said MIT; allowed for fonts) | ui-kit, runtime |
| material-symbols | 0.48.0 | Apache-2.0 | ui-kit, runtime |
| typescript | 5.9.3 | Apache-2.0 | root, dev |
| vite | 7.3.7 | MIT | root, dev |
| @vitejs/plugin-react | 5.2.0 | MIT | root, dev |
| @types/react, @types/react-dom | 19.3.0 | MIT | root, dev |
| @types/bun | 1.4.3 | MIT | root, dev |
| @biomejs/biome | 2.5.15 | MIT OR Apache-2.0 | root, dev |
| @happy-dom/global-registrator | 20.14.6 | MIT | root, dev |
| @testing-library/react | 16.3.3 | MIT | root, dev |
| @testing-library/dom | 10.4.2 | MIT | root, dev |
| @testing-library/jest-dom | 6.10.0 | MIT | root, dev |
| @testing-library/user-event | 14.6.8 | MIT | root, dev |
| dependency-cruiser | 17.4.3 (M5b WI-2; the registry's current major is 18, plan major 17 pinned) | MIT | root, dev |

Bun is pinned as `bun@1.4.2`. `bun run check:licenses` passes on the full installed tree (6 runtime and 103
dev-only packages). Not yet installed: react-router, @tanstack/react-query, zustand, maplibre-gl, orval,
json-schema-to-typescript, @playwright/test, @axe-core/playwright.
