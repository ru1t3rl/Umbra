# @umbra/data

Foundation feature (tier `foundation`): the only way the GUI reads or writes engine REST data (F-5, ADR 0045). It owns the **orval**-generated TanStack Query hooks for every operation of `api/openapi.json`, the request mutator and the typed error.

## Public API
- `DataProvider({ children, client? })`: the TanStack Query client (a default one with a 4xx-aware retry policy; tests may pass their own). It does not know the engine URL.
- `configureEngineUrl(url)`: sets the engine base URL (`""` = page origin); process-wide. Call it once in the app bootstrap (`src/main.tsx`) or in e2e/scripts; `check:deps` rejects it in features and shells.
- `ApiProblem` (class: `code`, `status`, `title`, `detail`, `diagnostics?`), `isApiProblem`, `CLIENT_PROBLEM_CODES`. Every failed request rejects with an `ApiProblem`: `application/problem+json` is decoded, any other error body is `code: "internal"` with the HTTP status, a failed fetch is `network_error` with status 0.
- Re-export of the generated file: `use<Operation>` hooks (`useGetStatus`, `useStepRun`, ...), `get<Operation>QueryKey` / `QueryOptions` / `MutationOptions` getters, the model types (`EngineStatus`, `Problem`, ...) and the plain request functions (`getStatus`, `stepRun`, ...). **Plain functions are for `gui/e2e/` and scripts only**; `bun run check:deps` rejects them in features, shells and the app.

```tsx
configureEngineUrl(config.engineUrl); // once, in src/main.tsx
<DataProvider><App /></DataProvider>
const status = useGetStatus(); // status.data?.run, status.error instanceof ApiProblem
```

## Generated code
`src/generated/api.ts` comes from `bun run gen:api` (orval, `orval.config.ts`) and is never edited. `bun run gen:api --check` fails on drift and is the first step of `bun run verify`. Contract changes: skill `generate-api-client`.
