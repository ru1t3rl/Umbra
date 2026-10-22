# @umbra/scenario-control

Domain feature (tier `domain`): scenario run status. M5b ships the read-only slice (ASM-316); M8 adds load/start/pause/step/stop and the clock mode.

## Public API
- `RunStatusBar()`: a fragment of `CommandBar` children. Readouts for sim time (`T+hh:mm:ss`, elapsed, ASM-308; cyan only while the state is `running`), tick and entity count (a placeholder `—` until known), and a run-state `Badge`: Idle (neutral), Ready (info), Running (success), Paused (warning), Stopped (neutral, danger after `plugin_fault` or `error`), Unknown (neutral, a state this client does not know). No Badge before the first run status. No buttons.
- `ConnectionAlert()`: renders nothing while the stream is `open`. `connecting` info; `reconnecting` and `closed` warning; `incompatible` danger and tells the user to reload (ASM-356). The shell anchors it.

Needs an `EngineStreamProvider` above it. The entity count is a primitive `useSyncExternalStore` snapshot of the imperative source, so a frame that changes neither the run nor the count renders nothing.

```tsx
<CommandBar brand="Umbra" title="Live map" floating><RunStatusBar /><ThemeToggle /></CommandBar>
<ConnectionAlert />
```
