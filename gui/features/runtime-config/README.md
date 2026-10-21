# @umbra/runtime-config

Foundation feature (tier `foundation`, no workspace dependencies): the runtime configuration read from
`config.json` at start-up. It is not engine REST (that is `@umbra/data`).

## Public API
- `RuntimeConfig`: `{ engineUrl: string; basemap?: { styleUrl: string } }`. An empty `engineUrl` means the page origin.
- `DEFAULT_CONFIG`, `ConfigError`.
- `parseRuntimeConfig(raw)`: validates parsed JSON; missing optional fields default.
- `loadRuntimeConfig(fetchImpl, baseUrl = "/")`: fetches `${baseUrl}config.json`. A 404 logs a warning and gives the defaults (the file is always emitted, so a 404 means a mis-served path); every other failure throws `ConfigError`.
- `RuntimeConfigProvider`, `useRuntimeConfig()`: the app bootstrap (`src/main.tsx`) loads once and renders the provider.

## Who reads what
- The dashboard shell (WI-15) reads `basemap.styleUrl` through `useRuntimeConfig` and passes it to the map as a prop.
- The `DataProvider` and `EngineStreamProvider` (WI-9, WI-10) get the engine URL from the app (`src/App.tsx`, from the same config) or `useRuntimeConfig`.

```tsx
const config = await loadRuntimeConfig(fetch.bind(globalThis), import.meta.env.BASE_URL);
<RuntimeConfigProvider config={config}><App /></RuntimeConfigProvider>
```
