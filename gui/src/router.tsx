import { route as dashboard } from "@umbra/shell-dashboard";
import { route as plugins } from "@umbra/shell-plugins";
import { route as settings } from "@umbra/shell-settings";
import { route as showcase } from "@umbra/shell-showcase";
import { route as templates } from "@umbra/shell-templates";
import type { NavItem } from "@umbra/ui-kit";
import { createBrowserRouter, type RouteObject } from "react-router";
import { NavigationBridge } from "./components/NavigationBridge/NavigationBridge.tsx";
import { NotFound } from "./components/NotFound/NotFound.tsx";
import { E2E_NAV_KEY, e2eNavItems } from "./e2eNav.ts";

const shellRoutes = [dashboard, templates, plugins, settings, showcase];

/**
 * Routes come from shell `route` objects: each shell exports `{ path, label, icon, nav, lazy }` and
 * the page is code-split (`lazy`).
 */
export const routes: RouteObject[] = [
  ...shellRoutes.map((r) => ({ path: r.path, lazy: r.lazy })),
  { path: "*", element: <NotFound /> },
];

/** Navigation entries: only routes with `nav: true`, in order (ADR 0073). */
export const navItems: NavItem[] = [
  ...shellRoutes.filter((r) => r.nav).map((r) => ({ path: r.path, label: r.label, icon: r.icon })),
  // The e2e build only: entries a test injects to check the bars with more shells (ASM-992).
  ...(import.meta.env.VITE_UMBRA_E2E === "1" ? e2eNavItems(Reflect.get(window, E2E_NAV_KEY)) : []),
];

/** One layout route provides the navigation context to every page. */
export const appRoutes: RouteObject[] = [
  { element: <NavigationBridge items={navItems} />, children: routes },
];

export function createAppRouter(): ReturnType<typeof createBrowserRouter> {
  return createBrowserRouter(appRoutes);
}
