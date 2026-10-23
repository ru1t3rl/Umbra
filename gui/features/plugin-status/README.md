# @umbra/plugin-status

Domain feature (tier `domain`): what the engine has loaded and how it is doing, for the plugins page (M8 WI-13). Depends on `@umbra/data` (`useGetPlugins`, `useListNetworkAdapters`, `useListComponents`, `useEnableNetworkAdapter`, `useDisableNetworkAdapter`), `@umbra/engine-stream` (`useRunStatus`) and `@umbra/ui-kit`. The M7 capability kinds are added in WI-25.

## Public API
The four lists take no props: each reads its own data. `PluginStatusGate` takes `children`.
- `PluginList`: one Card per plugin (name, id, version, "ABI major.minor", root, origin) with a Table "Capabilities of <name>" (kind label, id, version, Status: "OK" or a danger Badge "Faulted"). "No plugins loaded", "No capabilities".
- `AdapterList`: one Card per network adapter: the Enabled switch (name "<name> enabled"), a "Faulted" Badge and the fault reason, the adapter component, the plugin or "built-in", ghosts, the key space and a Table "Counters of <name>" (the 14 counters of the contract, then unknown ones by key). "No network adapters in this scenario".
- `FaultSummary`: a success Badge "No faults" (only when both reads succeeded), or a list "Faults" with one danger row per fault: plugin fault, network fault, faulted capability, faulted adapter. The report's plugin fault wins over the run status's; a capability that carries the plugin fault and an adapter that carries the network fault have no row of their own (ASM-966). The stream's faults show while REST is pending or failed.
- `ComponentList`: one Card per component kind that has entries (contract order, unknown kinds after in first-seen order, titled verbatim), entries by id with version, description and a provenance Badge (neutral for `builtin`, info otherwise, the word verbatim).
- `PluginStatusGate({ children })`: renders the idle state once while the engine is `idle` (the children are not mounted, so nothing is requested), the children otherwise. For a page that composes several exports (the plugins shell, ASM-994).

Unknown kinds and provenance render verbatim (ASM-822). Until the first answer each export shows "Loading..." (ASM-967). A failed switch is cleared when the session changes.

## Run-state rules (ASM-957, ASM-958)
- `idle`: every export shows "Load a scenario to see plugin status" with an "Open the dashboard" link to `/`, and requests nothing. An unknown run state (no frame yet) and `stopped` request normally.
- Refetch: plugins and adapters are read again when the run state or the session changes; components only when the session changes. Tick-only frames do nothing; mounting makes one request; leaving idle fetches once.
- Polling: only the adapters, every 2 s while the state is `running`; unmounting stops it. A plugin fault arrives with a state change, so plugins are not polled.
- `no_scenario` (409) from the adapters is an empty list: `AdapterList` shows the empty state, `FaultSummary` shows no adapter fault. Any other failed read is a danger Alert with the problem's title and detail after the data layer's retries (4xx never, other errors twice, ASM-963); when data has already arrived the Alert shows above it and the last good data stays.
- The Enabled switch calls disable when on and enable when off, then reads the list again (no optimistic state). It is disabled while the adapter is faulted and while its request runs; a failed call is a danger Alert and the switch keeps its state (ASM-959).

## Providers
Needs an `EngineStreamProvider`, a `DataProvider` and a `NavigationProvider` (the idle link) above it.

Pure model (not exported): `kindLabel`, `faultRows`, `counterRows`, `componentGroups`. The ui-kit `Table` column `render` draws the Badge in the capability Status cell (ASM-956, ASM-961).
