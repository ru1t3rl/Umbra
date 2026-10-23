# Decisions log (M7 and M8, from 2026-10-23)

Decisions that would otherwise have gone to the human. Since 2026-10-23 the human's standing instruction
is: "Don't ask me anything, just pick the recommended option and write it down somewhere so we can look
back at it later." Each row names who decided: **human** (asked before that instruction) or **agent**
(the recommended option, chosen on the standing instruction). The detail stays in the ASM or ADR named
in the row. The same file is kept on `m7` and `m8`. Add new rows at the bottom.

| Date | Track / WI | Question | Choice (alternatives) | Decided by | Record |
|---|---|---|---|---|---|
| 2026-10-23 | setup | bun 1.4.4 on a PC whose system bun is 1.4.2 | User-local install in `~/.bun/bin`, checksum verified (system upgrade by the human; run with 1.4.2) | human | this log |
| 2026-10-23 | M7 WI-6 | Hover aircraft (`min_speed = 0`) cannot change height | 3D arrival and a vertical rate decoupled from speed, ramped, floor wins (minimum speed while a height error remains) | agent | ASM-707 |
| 2026-10-23 | all | tokio advisory RUSTSEC-2026-0386 fails gate 6 | `cargo update -p tokio` 1.53.2 -> 1.54.1 on develop, merged into m7 and m8 (wait for the human) | agent | develop commit |
| 2026-10-23 | M8 WI-11 | `logTesting.tsx` `mount` has no return type (TS2742) | Explicit `Omit<RenderResult,"rerender"> & {rerender}` return type (tsconfig workaround) | human | ASM-946(h) |
| 2026-10-23 | M8 WI-12 | Private plain listbox duplicates ui-kit | ui-kit `VirtualList` `virtualize?` prop (keep the private list; a new ui-kit `Listbox`) | agent | ASM-953, ADR 0072 amendment line |
| 2026-10-23 | M8 WI-12/13 | Retry policy of REST reads | Data layer default `shouldRetry`, alert above last good data (`retry: false`) | agent | ASM-951(a), ASM-963 |
| 2026-10-23 | M8 WI-13 | ui-kit `Table` cannot hold a Badge | Optional column `render` (raw table; Badge outside the table) | agent | ASM-961, ADR 0073 amendment line |
| 2026-10-23 | M7 WI-7 | Six frozen M6 host tests assume host minor 1 | Rewrite each relative to the minor (leave WI-7 blocked) | human | ASM-641 |
| 2026-10-23 | M7 WI-7 | `required_minor_per_kind` reserved list | `[0, 5, 99]` (blocked) | human | ASM-641 |
| 2026-10-23 | M7 WI-7 | `motion_model_kind_is_unsupported` under ABI 0.2 | (a) expect `Descriptor` at `capabilities[0].vtable.struct_size` ((b) declare minor 1) | human | ASM-641, ASM-708 |
| 2026-10-23 | M7 WI-7 | Additions to `umbra-plugin-api/tests/support/abi_layout.rs` | Approve additions (separate support file) | human | ASM-641 |
| 2026-10-23 | M8 WI-14 | Pinned M5b `DashboardPage.test.tsx` renders without providers | Edit `renderPage` only: add Navigation, Data, Settings providers (fallbacks in the page) | human | ASM-969 |
| 2026-10-23 | M8 WI-14 | Two filled Buttons in the idle dashboard | `ScenarioLoader` `submitVariant` prop, `tonal` in the dashboard | agent | ASM-972, ASM-978 |
| 2026-10-23 | M8 WI-14 | Two pinned M5b lookups are ambiguous with the new panels | Scope them to the command bar (`vt-cmdbar`, `within(banner)`), assertions unchanged (drop panel headers) | human | ASM-979 |
| 2026-10-23 | M8 WI-14 review | Fake-timer leak breaks other files' `waitFor` by file order | Drain timers in the performance file's `afterEach` and run the unit step as `bun test --isolate` (only the afterEach) | agent | ASM-985 |
| 2026-10-23 | M8 WI-14 review | The 48 px command bar overflows (column RunControls, step row off-screen) | `RunControls layout="bar"` with the problem portalled into a shell slot, extracted public `StepControls` in the Clock panel, compact icon-only bar at 1280 px or less (ui-kit `ThemeToggle iconOnly`) (a second row; a menu) | agent | ASM-986 |
| 2026-10-23 | M8 WI-14 review | `react-dom` for the portal in scenario-control | Peer dependency of scenario-control (put the Alert in a shell render prop; export the Alert) | agent | ASM-986 |
| 2026-10-23 | M8 WI-14 review | Pinned export-list tests list exact exports | Append `StepControls` (scenario-control) and `useRunState` (engine-stream) as append-only lines | agent | ASM-986, ASM-987 |
| 2026-10-23 | M8 WI-14 review | Empty list leaves a blank box in the glass panel | `data-empty` and `flex: 0 0 0` in entity-inspector, `.vt-panel .vt-vlist` transparent in ui-kit (one of the two) | agent | ASM-987 |
| 2026-10-23 | M8 WI-14 review | Popovers: exclusivity, focus, Escape, panel placement | One open at a time, `usePopover` (focus in, Escape, focus back), Scenario panel last in the alert column | agent | ASM-987 |
| 2026-10-23 | M8 WI-14 review | Where the recent-list rule lives | `useRecordRecentScenario()` and `withRecentPath` in `@umbra/settings` (shell model) | agent | ASM-987, ASM-975 |
| 2026-10-23 | M8 WI-14 review | Run state selector | `useRunState()` in `@umbra/engine-stream` (a shell hook over `useRunStatus`) | agent | ASM-987 |
| 2026-10-23 | M8 WI-14 review | Screenshot baselines | Regenerated `dashboard-{ops,day}.png` and `dashboard-wide-{ops,day}.png` (compact bar, no blank list box), new `dashboard-running-{ops,day}.png`; no showcase baseline changed | agent | ASM-987, H-8 |
| 2026-10-23 | M8 WI-14 review 2 | The bar overflows at 1366 and 1440 px and with wide readouts | Measured compact mode (`useBarFit`: ResizeObserver, per-frame coalescing, hysteresis), `nowrap` lifecycle row, a second step with icon-only lifecycle buttons, 50-case e2e matrix (a fixed breakpoint; a container query) | agent | ASM-988 |
| 2026-10-23 | M8 WI-14 review 2 | Alert (480 px) overflows the 420 px alert column; Clock panel covers the alert column | Column 512 px with full-width Alerts; Clock panel anchored to the bar's right edge (raise the alert column's stacking order) | agent | ASM-988 |
| 2026-10-23 | M8 WI-14 review 2 | A new command must clear both kinds of error | `CommandProblemProvider` shared slot in scenario-control, inline fallback without `problemContainer` (a module-level store; keep two slots) | agent | ASM-989 |
| 2026-10-23 | M8 WI-14 review 2 | Another append-only edit of a pinned export list | `CommandProblemProvider` appended to scenario-control `index.test.ts` | agent | ASM-989 |
| 2026-10-23 | M8 WI-14 review 2 | Icon-only theme switch needs an icon and a showcase entry; the new name breaks substring queries in pinned e2e specs | `light_mode`/`dark_mode` icon, `title`, `label` prop, showcase name "Compact theme switch" (name "Day theme, compact"; editing the pinned specs) | agent | ASM-989 |
| 2026-10-23 | M8 WI-14 review 2 | Scenario popover after a remote load; ref sync | Close on idle; sync the ref in an effect | agent | ASM-989 |
| 2026-10-23 | M8 WI-14 review 2 | Screenshot baselines | Regenerated `dashboard-{ops,day}.png`, `dashboard-wide-{ops,day}.png`, `dashboard-running-{ops,day}.png`, `showcase-floating-{ops,day}.png`; new `dashboard-worst-{ops,day}.png`; no other showcase baseline changed. The two `showcase-floating-*` baselines change because the existing floating CommandBar sample was edited (it gained the icon-only `ThemeToggle`): the showcase is the one reference for every ui-kit component, and a new sample would have added a section and changed more baselines | agent | ASM-988, ASM-989, H-8 |
| 2026-10-23 | M8 WI-14 review 3 | Fit stays one level too compact after the first load (font loaded late, stale needs) | `useBarFit` restarts from level 0 on `document.fonts` `loadingdone` and `ready`, and when the content shrinks by more than the hysteresis; overflow tolerance 0 (reset only on fonts; keep the 1 px tolerance) | agent | ASM-990 |
| 2026-10-23 | M8 WI-14 review 3 | Who shows the shared command problem | `RunControls` shows it; new `CommandProblemAlert` for a provider with only `StepControls`; two `RunControls` share one Alert, documented (an alert prop on the provider; a `ProblemSlot`) | agent | ASM-990 |
| 2026-10-23 | M8 WI-14 review 3 | Another append-only edit of a pinned export list | `CommandProblemAlert` appended to scenario-control `index.test.ts` | agent | ASM-990 |
| 2026-10-23 | M8 WI-14 review 3 | Screenshot baselines | Regenerated `dashboard-{ops,day}.png`, `dashboard-running-{ops,day}.png`, `dashboard-worst-{ops,day}.png` (the bar now settles at the right level: full bar idle at 1280, labelled lifecycle buttons running); `dashboard-wide-*` unchanged | agent | ASM-990, H-8 |
