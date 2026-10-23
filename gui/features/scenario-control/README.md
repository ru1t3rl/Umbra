# @umbra/scenario-control

Domain feature (tier `domain`): scenario run status (M5b, ASM-316) and, from M8 WI-7, the run and clock controls and from WI-8 the scenario loader and summary. Depends on `@umbra/data` (commands), `@umbra/engine-stream` (the run status is the only truth) and `@umbra/ui-kit`.

## Public API
- `RunStatusBar()`: a fragment of `CommandBar` children. Readouts for sim time (`T+hh:mm:ss`, elapsed, ASM-308; cyan only while the state is `running`), tick and entity count (a placeholder `—` until known), and a run-state `Badge`: Idle (neutral), Ready (info), Running (success), Paused (warning), Stopped (neutral, danger after `plugin_fault` or `error`), Unknown (neutral, a state this client does not know). No Badge before the first run status. No buttons.
- `ConnectionAlert()`: renders nothing while the stream is `open`. `connecting` info; `reconnecting` and `closed` warning; `incompatible` danger and tells the user to reload (ASM-356). The shell anchors it.
- `RunControls({ onLoadRequested? })`: group "Run controls" with Load, Start, Pause, Resume, Stop, Unload. Enabled exactly per the docs/api.md command matrix (`availableCommands`); the next lifecycle step is the one filled azure button; a disabled button is described by its reason. Load only calls `onLoadRequested` (the shell opens the `ScenarioLoader`). For a running stepped clock a group "Step" adds presets 1/10/100/1000 and a "Tick count" field (1..1,000,000). No optimistic update: buttons follow the next stream frame; a button is disabled while its request is in flight; a failed command shows a danger Alert with the engine's `detail` until the next command.
- `ClockControl()`: "Clock: <label>", a "Clock mode" chip group (As fast as possible, Real time, Stepped, Scaled; the chip sends at once, the active one sends nothing) and, for a scaled clock, a logarithmic "Scale" slider (0.1x..100x) plus a "Scale factor" field (0.001..1,000,000) edited as a draft and sent by "Apply scale". Off in idle and stopped.
- `RunFaultAlert()`: a danger Alert (role alert) for a stopped run with a plugin fault (plugin, capability, call, message), a network fault (adapter, reason), `error`, or an unknown reason ("Run stopped: <word as received>"). Nothing for command or duration, or while the run is not stopped.
- `ScenarioLoader({ recentPaths?, onLoaded, suggestions? })` (`ScenarioLoaderProps`): a form that sends `POST /scenario`. Fields: "Recent scenarios" Select (only with recent paths), "Suggested scenarios" Select (defaults to the four shipped examples), "Scenario path" (required, trimmed), "Clock override" (default sends no clock; Scaled adds "Scale factor", 0.001..1,000,000), "Seed" (optional decimal string, `^[0-9]{1,20}$` and at most 18446744073709551615, sent as a string). Invalid input is marked inline with a specific message and nothing is sent. The button is disabled while a request is in flight. A failure shows the engine's title and detail; `scenario_invalid` adds a "Diagnostics" list (`file:line:column`, code, message, severity Badge, notes; at most 50, then a count) beside the Alert. Warnings of a successful load show in a dismissible warning Alert plus the same list. `onLoaded(path)` runs after the engine accepted the scenario (the shell records the recent path). Unload is in `RunControls`.
- `ScenarioSummary()`: name, path, seed, tick rate and its source, duration (`T+hh:mm:ss` and ticks) and the entity count at load, from `GET /scenario`; a loading status and, for `no_scenario`, "No scenario is loaded.".

`suggestions` has the shape of a future scenario listing (contract gap G-2, `GET /api/v1/scenarios`, `{ path, name }`): when it exists the shell maps the rows to `{ path, label }` and passes them in; nothing else changes (ASM-931).

Needs an `EngineStreamProvider` (above a `DataProvider` for the controls) above it. The entity count is a primitive `useSyncExternalStore` snapshot of the imperative source, so a frame that changes neither the run nor the count renders nothing.

```tsx
<CommandBar brand="Umbra" title="Live map" floating><RunStatusBar /><ThemeToggle /></CommandBar>
<ConnectionAlert />
```

Pure model (not exported): `runCommands` (`availableCommands`, `primaryCommand`, `disabledReason`), `clockLabel`, `parseStepCount`, `clockScale`, `problemText`, `loadRequest` (form validation, seed rules, request body), `diagnostics` (defensive decoding, `file:line:column`, the list cap), `exampleScenarios`.

```tsx
<RunControls onLoadRequested={openLoader} />
<ClockControl />
<RunFaultAlert />
```

```tsx
<ScenarioLoader recentPaths={recent} onLoaded={(path) => remember(path)} />
<ScenarioLoader suggestions={listing.map((s) => ({ path: s.path, label: s.name }))} onLoaded={remember} />
<ScenarioSummary />
```
