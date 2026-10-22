import { route as dashboard } from "@umbra/shell-dashboard";
import { route as showcase } from "@umbra/shell-showcase";
import type { NavItem } from "@umbra/ui-kit";
import { createBrowserRouter, type RouteObject } from "react-router";
import { NavigationBridge } from "./components/NavigationBridge/NavigationBridge.tsx";
import { NotFound } from "./components/NotFound/NotFound.tsx";

const shellRoutes = [dashboard, showcase];

/**
 * Routes come from shell `route` objects: each shell exports `{ path, label, icon, nav, lazy }` and
 * the page is code-split (`lazy`).
 */
export const routes: RouteObject[] = [
  ...shellRoutes.map((r) => ({ path: r.path, lazy: r.lazy })),
  { path: "*", element: <NotFound /> },
];

/** Navigation entries: only routes with `nav: true`, in order (ADR 0073). */
export const navItems: NavItem[] = shellRoutes
  .filter((r) => r.nav)
  .map((r) => ({ path: r.path, label: r.label, icon: r.icon }));

/** One layout route provides the navigation context to every page. */
export const appRoutes: RouteObject[] = [
  { element: <NavigationBridge items={navItems} />, children: routes },
];

export function createAppRouter(): ReturnType<typeof createBrowserRouter> {
  return createBrowserRouter(appRoutes);
}
