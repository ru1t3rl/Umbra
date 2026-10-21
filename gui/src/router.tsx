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
 * Routes come from shell `route` objects (lazy-loaded with `lazy`). WI-8 (showcase) introduces the first lazy shell
 * route and WI-15 the dashboard; until then there is one placeholder.
 */
export const routes: RouteObject[] = [
  { path: "/", element: <Placeholder /> },
  { path: "*", element: <NotFound /> },
];

export function createAppRouter(): ReturnType<typeof createBrowserRouter> {
  return createBrowserRouter(routes);
}
