# ADR 0046: GUI file layout: one component or hook per file, in `components/` and `hooks/`

- **Status**: Accepted by the human 2026-10-21 (decision: "For the frontend project, make sure all hooks and components are in their own seperate files so they are easy to find. and that they are in a corresponding hooks or component directory"; layout proposed by the coordinator, implemented in M5b WI-12b)
- **Requirement IDs**: F-3, F-6, `gui/CLAUDE.md` (code standards), `docs/frontend-architecture.md` section 4
- **Affected**: every package under `gui/` (`src/`, `features/*/src`, `feature-shells/*/src`), `gui/scripts/check-layout.ts`, `gui/test/layout.test.ts`, `gui/biome.json` (page override glob), `gui/package.json` (`check:layout`, `verify`), decision row D-62, skills `add-feature`, `add-feature-shell`, `review-frontend-change`, `verify-frontend`, ASM-375..ASM-378

## Context
Components and hooks were spread over ad-hoc places: several in one file (`ComponentSections.tsx` held five components, `hooks.ts` four hooks plus a context, `ThemeProvider.tsx` a provider, a hook and a context), top-level files next to `model/`, and ui-kit already used `components/<Name>/<Name>.tsx`. The human wants every component and hook easy to find by name.

## Options
1. **One component per file in `components/<Name>/<Name>.tsx`, one hook per file in `hooks/use<Name>.ts(x)`, context objects in `context/<Name>Context.ts`, enforced by a script (chosen).** The file name is the symbol name, so "go to file" finds it, and a check keeps it true.
2. Convention only, reviewed by humans. Cons: drifts; no failing signal.
3. Flat `components/<Name>.tsx` (no per-component directory). Cons: CSS and tests beside the component would crowd one directory; ui-kit already uses per-component directories and the reference components ship a CSS file each.
4. Keep hooks beside the component they serve. Cons: shared hooks have no obvious home; "easy to find" is lost.

## Decision
Option 1.
- **Components** (including providers, pages, the app `App`, and small private subcomponents): `src/components/<Name>/<Name>.tsx`, with `<Name>.css` and `<Name>.test.tsx` beside it. One component per file; the file name is the component name. A private subcomponent gets its own directory too (`Shape`, `Section`, `ButtonsDemo`); it is simply not exported from `index.ts`. Pages in shells are `src/components/<Name>Page/<Name>Page.tsx` (the biome `noDefaultExport` override follows).
- **Hooks**: `src/hooks/use<Name>.ts(x)` with `use<Name>.test.ts(x)` beside it. One hook per file, file name is the hook name. Generated orval hooks in `features/data/src/generated/` are exempt (re-exported from the index as before).
- **React contexts** shared by a provider and its hook: `src/context/<Name>Context.ts`. A context object is neither a component nor a hook; giving it its own directory keeps the component and hook files single-purpose and avoids an import cycle between them. Context value types live with the context.
- Everything else stays where it was: `model/` (pure logic), `maplibre/`, `messages.ts`, `types.ts`, `fetcher.ts`, scripts. Helpers shared by several components go to a plain module (`rates.ts` in the showcase), not into a component file.
- Public APIs (`src/index.ts`) and the specifiers other packages import are unchanged. `src/main.tsx` is the bootstrap entry and defines no component.
- **Enforcement**: `bun run check:layout` (`gui/scripts/check-layout.ts`, TypeScript compiler API from the installed `typescript`, no new dependency), a step of `bun run verify` after `lint` and before `check:deps`. It parses every non-exempt `.ts`, `.tsx`, `.js` and `.jsx` file under the three kinds of `src`, walks the whole syntax tree and reports: a component outside `components/<Name>/<Name>.tsx` or two components in a file; a hook outside `hooks/<useName>.ts(x)` or two hooks in a file; a context outside `context/<Name>Context.ts` (the name must end in `Context`, one per file); any file in `components/`, `hooks/` or `context/` whose name does not match what it defines; and `nested-declaration`, a component, hook or context declared inside another declaration or inside an object literal. A component is a PascalCase function, class or const that contains JSX or a `createElement`/`jsx` call (not `document.createElement`), or whose value is a `memo`, `forwardRef`, `lazy` or `with[A-Z]...` call, in any file extension; an anonymous default export (`export default () => <div/>`, `export default function () {}`) is a component named `default` and so never has a valid location. A hook is a `use[A-Z]...` function or const; a context is a `createContext` call. Failing fixtures for each rule and each detection form are under `gui/test/arch-fixtures/layout/` and run by `gui/test/layout.test.ts`.
- **Exempt** (ASM-375): `*.test.*` and `*.test-d.*` files (they define throwaway probe components and hooks), `generated/`, `reference/`, and `testing.ts(x)` test-double modules.

## Consequences
- More files and directories; imports get one segment longer. Moves use `git mv` so history follows.
- A new component or hook cannot be added in the wrong place without `verify` failing.
- Remaining limits of the check (ASM-376): detection is syntactic, so a component reached only through an alias (`const X = Y;`), re-exported from another module under a new name, built by a wrapper whose name is not `memo`, `forwardRef`, `lazy` or `with[A-Z]...`, or named in lower case is not seen; a PascalCase function that merely returns JSX for another purpose counts as a component.
