import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { NavItem } from "../NavigationProvider/NavigationProvider.tsx";
import { NavigationProvider } from "../NavigationProvider/NavigationProvider.tsx";
import { AppNav } from "./AppNav.tsx";

afterEach(cleanup);

const items: NavItem[] = [
  { path: "/", label: "Live map", icon: "map" },
  { path: "/templates", label: "Templates", icon: "dataset" },
  { path: "/plugins", label: "Plugins" },
];

function renderNav(currentPath = "/", navigate: (to: string) => void = () => {}) {
  return render(
    <NavigationProvider items={items} currentPath={currentPath} navigate={navigate}>
      <AppNav />
    </NavigationProvider>,
  );
}

test("is a navigation landmark labelled Pages", () => {
  renderNav();
  expect(screen.getByRole("navigation", { name: "Pages" })).toBeInTheDocument();
});

test("lists one link per item with its label and href", () => {
  renderNav();
  const nav = screen.getByRole("navigation", { name: "Pages" });
  const links = within(nav).getAllByRole("link");
  expect(links.map((l) => l.textContent)).toEqual([
    expect.stringContaining("Live map"),
    expect.stringContaining("Templates"),
    expect.stringContaining("Plugins"),
  ]);
  expect(screen.getByRole("link", { name: /Templates/ })).toHaveAttribute("href", "/templates");
});

test("only the current item has aria-current=page", () => {
  renderNav("/templates");
  expect(screen.getByRole("link", { name: /Templates/ })).toHaveAttribute("aria-current", "page");
  expect(screen.getByRole("link", { name: /Live map/ })).not.toHaveAttribute("aria-current");
  expect(screen.getByRole("link", { name: /Plugins/ })).not.toHaveAttribute("aria-current");
});

test("the current item follows currentPath", () => {
  const { rerender } = renderNav("/");
  expect(screen.getByRole("link", { name: /Live map/ })).toHaveAttribute("aria-current", "page");
  rerender(
    <NavigationProvider items={items} currentPath="/plugins" navigate={() => {}}>
      <AppNav />
    </NavigationProvider>,
  );
  expect(screen.getByRole("link", { name: /Plugins/ })).toHaveAttribute("aria-current", "page");
  expect(screen.getByRole("link", { name: /Live map/ })).not.toHaveAttribute("aria-current");
});

test("no item is current on an unlisted path", () => {
  renderNav("/showcase");
  for (const link of screen.getAllByRole("link")) expect(link).not.toHaveAttribute("aria-current");
});

test("a plain click on an item calls navigate with its path", async () => {
  const user = userEvent.setup();
  const calls: string[] = [];
  renderNav("/", (to) => calls.push(to));
  await user.click(screen.getByRole("link", { name: /Plugins/ }));
  expect(calls).toEqual(["/plugins"]);
});

test("icons are decorative and do not join the link name", () => {
  renderNav();
  expect(screen.getByRole("link", { name: "Live map" })).toBeInTheDocument();
});

test("iconOnly shows the icon alone; each link keeps its name and gets a title (a link without an icon keeps its text)", () => {
  render(
    <NavigationProvider items={items} currentPath="/" navigate={() => {}}>
      <AppNav iconOnly />
    </NavigationProvider>,
  );
  const live = screen.getByRole("link", { name: "Live map" });
  expect(live).toHaveAttribute("title", "Live map");
  expect(live).toHaveAttribute("aria-label", "Live map");
  expect(live.textContent).not.toContain("Live map");
  expect(screen.getByRole("link", { name: "Templates" })).toHaveAttribute("title", "Templates");
  const plain = screen.getByRole("link", { name: "Plugins" });
  expect(plain.textContent).toBe("Plugins");
  expect(plain).not.toHaveAttribute("title");
});
