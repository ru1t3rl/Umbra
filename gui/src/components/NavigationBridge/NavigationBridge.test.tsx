import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppNav, type NavItem, useAppNavigate } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { NavigationBridge } from "./NavigationBridge.tsx";

afterEach(cleanup);

const items: NavItem[] = [
  { path: "/", label: "Live map", icon: "map" },
  { path: "/b", label: "Other" },
];

function Go(): ReactElement {
  const navigate = useAppNavigate();
  return (
    <button type="button" onClick={() => navigate("/b")}>
      go
    </button>
  );
}

function renderAt(path: string) {
  const router = createMemoryRouter(
    [
      {
        element: <NavigationBridge items={items} />,
        children: [
          {
            path: "/",
            element: (
              <>
                <AppNav />
                <Go />
                <p>page a</p>
              </>
            ),
          },
          {
            path: "/b",
            element: (
              <>
                <AppNav />
                <p>page b</p>
              </>
            ),
          },
        ],
      },
    ],
    { initialEntries: [path] },
  );
  render(<RouterProvider router={router} />);
  return router;
}

test("the bridge renders the child route and provides the items to AppNav", () => {
  renderAt("/");
  expect(screen.getByText("page a")).toBeTruthy();
  const nav = screen.getByRole("navigation", { name: "Pages" });
  expect(nav.querySelectorAll("a")).toHaveLength(2);
});

test("the current path is the router location (aria-current on the matching link)", () => {
  renderAt("/b");
  expect(screen.getByRole("link", { name: /Other/ }).getAttribute("aria-current")).toBe("page");
  expect(screen.getByRole("link", { name: /Live map/ }).getAttribute("aria-current")).toBeNull();
});

test("clicking a nav link navigates through the router without a page load", async () => {
  const router = renderAt("/");
  await userEvent.click(screen.getByRole("link", { name: /Other/ }));
  expect(router.state.location.pathname).toBe("/b");
  expect(screen.getByText("page b")).toBeTruthy();
});

test("useAppNavigate calls the router navigate", async () => {
  const router = renderAt("/");
  await userEvent.click(screen.getByRole("button", { name: "go" }));
  expect(router.state.location.pathname).toBe("/b");
});
