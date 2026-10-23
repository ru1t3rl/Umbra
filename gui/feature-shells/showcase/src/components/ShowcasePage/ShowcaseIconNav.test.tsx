// WI-15 review n2 (ASM-992): the icon-only AppNav variant has a showcase entry (A-003: every component and variant).
import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import { ThemeProvider } from "@umbra/ui-kit";
import ShowcasePage from "./ShowcasePage.tsx";

afterEach(cleanup);

test("an icon-only navigation sample has its own section, with names and tooltips, next to the labelled one", () => {
  const { container } = render(
    <ThemeProvider>
      <ShowcasePage />
    </ThemeProvider>,
  );
  const section = container.querySelector("#showcase-appnav-icons");
  expect(section).not.toBeNull();
  const nav = within(section as HTMLElement).getByRole("navigation", { name: "Icon-only pages" });
  const links = within(nav).getAllByRole("link");
  expect(links.length).toBeGreaterThan(1);
  for (const link of links) {
    expect(link).toHaveAttribute("title");
    expect(link.textContent?.trim()).toMatch(/^[a-z_]+$/);
    expect(link).toHaveAccessibleName(link.getAttribute("title") as string);
  }
  expect(nav.querySelectorAll("[aria-current='page']").length).toBe(1);
  // the labelled sample is untouched, and the landmark name "Pages" stays unique
  expect(screen.getAllByRole("navigation", { name: "Pages" })).toHaveLength(1);
});
