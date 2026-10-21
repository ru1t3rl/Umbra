import { route as showcase } from "@umbra/shell-showcase";
import type { ReactElement } from "react";
import { createBrowserRouter, type RouteObject } from "react-router";
import { t } from "./messages.ts";

function Placeholder(): ReactElement {
  return (
    <main>
      <h1>{t("app.placeholder")}</h1>
    </main>
  );
}

function NotFound(): ReactElement {
  return (
    <main>
      <h1>{t("app.notFound")}</h1>
    </main>
  );
}

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
