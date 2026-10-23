import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { route as dashboard } from "@umbra/shell-dashboard";
import { route as plugins } from "@umbra/shell-plugins";
import { route as settings } from "@umbra/shell-settings";
import { route as showcase } from "@umbra/shell-showcase";
import { route as templates } from "@umbra/shell-templates";
import { createMemoryRouter, RouterProvider } from "react-router";
import { appRoutes, navItems, routes } from "./router.tsx";

afterEach(cleanup);

test("nav items come from the routes with nav true, in route order", () => {
  const withNav = [dashboard, templates, plugins, settings, showcase].filter((r) => r.nav);
  expect(navItems).toEqual(withNav.map((r) => ({ path: r.path, label: r.label, icon: r.icon })));
  expect(navItems.length).toBeGreaterThan(0);
});

test("the showcase is routable but not listed in the navigation", () => {
  expect(routes.some((r) => r.path === showcase.path)).toBe(true);
  expect(navItems.some((i) => i.path === showcase.path)).toBe(false);
});

test("every nav item resolves to a shell route (no dead links)", () => {
  for (const item of navItems) expect(routes.some((r) => r.path === item.path)).toBe(true);
});

test("appRoutes is one layout route whose children are the shell routes", () => {
  expect(appRoutes).toHaveLength(1);
  expect(appRoutes[0]?.children).toBe(routes);
});

test("an unknown path inside the layout route still shows page not found", () => {
  render(
    <RouterProvider router={createMemoryRouter(appRoutes, { initialEntries: ["/nowhere"] })} />,
  );
  expect(screen.getByRole("heading", { name: "Page not found." })).toBeTruthy();
});
