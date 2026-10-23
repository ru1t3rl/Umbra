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
| 2026-10-23 | M7 WI-7 | `c_header.rs` (frozen) needs appended tests for the ABI 0.2 handles and a C probe of the new kinds | Add `c_header.rs` to the ASM-679 append-only list and do it in WI-7 (defer to WI-8; no C regression guard) | agent | ASM-679, ASM-641 |
| 2026-10-23 | M7 WI-10 | The ADR 0059 parity test is planned in `umbra-plugin-host/tests`, but the host may not depend on `umbra-sdk` (rule table, also for dev edges) | Compile the SDK's `math.rs` into the test from source (`#[path]`), the file kept self-contained (allow `umbra-sdk` in the host's rule row, which edits a pinned row of the frozen `check_rules.rs`; a dev-only rule-table field) | agent | ASM-713 |
| 2026-10-23 | M7 WI-10 | Review B1: one static blackboard-read slot is clobbered by parallel reads (`kinds_calls` flaky) | One entry per read, keyed by its token, in a `static Mutex<BTreeMap>`, removed by the read also on error or panic (rely on RR-B1 and serialise the test binary; one slot per thread) | agent | ASM-715(b) |
