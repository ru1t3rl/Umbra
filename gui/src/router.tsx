import { route as dashboard } from "@umbra/shell-dashboard";
import { route as showcase } from "@umbra/shell-showcase";
import { createBrowserRouter, type RouteObject } from "react-router";
import { NotFound } from "./components/NotFound/NotFound.tsx";

/**
 * Routes come from shell `route` objects: each shell exports `{ path, label, lazy }` and the page is
 * code-split (`lazy`): the dashboard (WI-15) at `/` and the showcase (WI-8) at `/showcase`.
 */
export const routes: RouteObject[] = [
  { path: dashboard.path, lazy: dashboard.lazy },
  { path: showcase.path, lazy: showcase.lazy },
  { path: "*", element: <NotFound /> },
];

export function createAppRouter(): ReturnType<typeof createBrowserRouter> {
  return createBrowserRouter(routes);
}
