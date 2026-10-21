import { route as showcase } from "@umbra/shell-showcase";
import { createBrowserRouter, type RouteObject } from "react-router";
import { NotFound } from "./components/NotFound/NotFound.tsx";
import { Placeholder } from "./components/Placeholder/Placeholder.tsx";

/**
 * Routes come from shell `route` objects: each shell exports `{ path, label, lazy }` and the page is
 * code-split (`lazy`). The showcase (WI-8) is the first; WI-15 adds the dashboard at `/` and replaces the
 * placeholder.
 */
export const routes: RouteObject[] = [
  { path: "/", element: <Placeholder /> },
  { path: showcase.path, lazy: showcase.lazy },
  { path: "*", element: <NotFound /> },
];

export function createAppRouter(): ReturnType<typeof createBrowserRouter> {
  return createBrowserRouter(routes);
}
