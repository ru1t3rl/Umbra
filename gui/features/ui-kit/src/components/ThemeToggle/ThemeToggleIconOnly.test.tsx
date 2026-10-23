import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeProvider } from "../ThemeProvider/ThemeProvider.tsx";
import { ThemeToggle } from "./ThemeToggle.tsx";

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.theme;
});

const storage = () => {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
  };
};

test("by default the label is visible text", () => {
  render(
    <ThemeProvider storage={storage()}>
      <ThemeToggle />
    </ThemeProvider>,
  );
  expect(screen.getByRole("switch", { name: "Day theme" })).toHaveTextContent("Day theme");
});

test("iconOnly drops the visible label but keeps the accessible name and the behaviour", async () => {
  render(
    <ThemeProvider storage={storage()}>
      <ThemeToggle iconOnly />
    </ThemeProvider>,
  );
  const sw = screen.getByRole("switch", { name: "Day theme" });
  expect(sw).toHaveAttribute("aria-label", "Day theme");
  expect(sw).not.toHaveTextContent("Day theme");
  expect(sw).toHaveAttribute("aria-checked", "false");
  await userEvent.click(sw);
  expect(sw).toHaveAttribute("aria-checked", "true");
});

// WI-14 review round 2 (ASM-988): the icon-only form shows an icon for the theme, has a title, and a name override.
test("iconOnly shows the icon of the current theme and a title", async () => {
  render(
    <ThemeProvider storage={storage()}>
      <ThemeToggle iconOnly />
    </ThemeProvider>,
  );
  const sw = screen.getByRole("switch", { name: "Day theme" });
  const group = sw.parentElement as HTMLElement;
  expect(group).toHaveAttribute("title", "Day theme");
  // dark now: the dark theme icon; after the click the light one
  expect(group.querySelector(".vt-icon")).toHaveTextContent("dark_mode");
  await userEvent.click(sw);
  expect(group.querySelector(".vt-icon")).toHaveTextContent("light_mode");
});

test("the icon is decorative (hidden from assistive technology)", () => {
  render(
    <ThemeProvider storage={storage()}>
      <ThemeToggle iconOnly />
    </ThemeProvider>,
  );
  expect(document.querySelector(".vt-icon")).toHaveAttribute("aria-hidden", "true");
});

test("the visible form has no icon and no title", () => {
  render(
    <ThemeProvider storage={storage()}>
      <ThemeToggle />
    </ThemeProvider>,
  );
  expect(document.querySelector(".vt-icon")).toBeNull();
  expect(screen.getByRole("switch", { name: "Day theme" }).closest("[title]")).toBeNull();
});

test("label overrides the accessible name (a page with a second switch needs another name)", () => {
  render(
    <ThemeProvider storage={storage()}>
      <ThemeToggle iconOnly label="Day theme, compact" />
    </ThemeProvider>,
  );
  expect(screen.getByRole("switch", { name: "Day theme, compact" })).toBeInTheDocument();
});
