// Public API of @umbra/data: the only way features read or write engine REST data (F-5, ADR 0045).
// Hooks, query-key getters and model types come from the orval output. The plain request functions in it
// (getStatus, stepRun, ...) are for gui/e2e and scripts only; `check:deps` rejects them in features,
// shells and the app.

export type { DataProviderProps } from "./components/DataProvider/DataProvider.tsx";
export { DataProvider } from "./components/DataProvider/DataProvider.tsx";
export { configureEngineUrl } from "./fetcher.ts";
export * from "./generated/api.ts";
export type { ApiProblemInit } from "./problem.ts";
export { ApiProblem, CLIENT_PROBLEM_CODES, isApiProblem } from "./problem.ts";
