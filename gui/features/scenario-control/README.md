# @umbra/scenario-control

Domain feature (tier `domain`): scenario run status (M5b, ASM-316) and, from M8 WI-7, the run and clock controls. Depends on `@umbra/data` (commands), `@umbra/engine-stream` (the run status is the only truth) and `@umbra/ui-kit`.

## Public API
- `RunStatusBar()`: a fragment of `CommandBar` children. Readouts for sim time (`T+hh:mm:ss`, elapsed, ASM-308; cyan only while the state is `running`), tick and entity count (a placeholder `—` until known), and a run-state `Badge`: Idle (neutral), Ready (info), Running (success), Paused (warning), Stopped (neutral, danger after `plugin_fault` or `error`), Unknown (neutral, a state this client does not know). No Badge before the first run status. No buttons.
- `ConnectionAlert()`: renders nothing while the stream is `open`. `connecting` info; `reconnecting` and `closed` warning; `incompatible` danger and tells the user to reload (ASM-356). The shell anchors it.
- `RunControls({ onLoadRequested? })`: group "Run controls" with Load, Start, Pause, Resume, Stop, Unload. Enabled exactly per the docs/api.md command matrix (`availableCommands`); the next lifecycle step is the one filled azure button; a disabled button is described by its reason. Load only calls `onLoadRequested` (the loader is WI-8). For a running stepped clock a group "Step" adds presets 1/10/100/1000 and a "Tick count" field (1..1,000,000). No optimistic update: buttons follow the next stream frame; a button is disabled while its request is in flight; a failed command shows a danger Alert with the engine's `detail` until the next command.
- `ClockControl()`: "Clock: <label>", a "Clock mode" chip group (As fast as possible, Real time, Stepped, Scaled; the chip sends at once, the active one sends nothing) and, for a scaled clock, a logarithmic "Scale" slider (0.1x..100x) plus a "Scale factor" field (0.001..1,000,000) edited as a draft and sent by "Apply scale". Off in idle and stopped.
- `RunFaultAlert()`: a danger Alert (role alert) for a stopped run with a plugin fault (plugin, capability, call, message), a network fault (adapter, reason), `error`, or an unknown reason ("Run stopped: <word as received>"). Nothing for command or duration, or while the run is not stopped.

Needs an `EngineStreamProvider` (above a `DataProvider` for the controls) above it. The entity count is a primitive `useSyncExternalStore` snapshot of the imperative source, so a frame that changes neither the run nor the count renders nothing.

```tsx
<CommandBar brand="Umbra" title="Live map" floating><RunStatusBar /><ThemeToggle /></CommandBar>
<ConnectionAlert />
```

Pure model (not exported): `runCommands` (`availableCommands`, `primaryCommand`, `disabledReason`), `clockLabel`, `parseStepCount`, `clockScale`, `problemText`.

```tsx
<RunControls onLoadRequested={openLoader} />
<ClockControl />
<RunFaultAlert />
```
