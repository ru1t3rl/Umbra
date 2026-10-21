import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeProvider } from "../ThemeProvider/ThemeProvider.tsx";
import { ThemeToggle } from "./ThemeToggle.tsx";

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.theme;
});

const memory = () => {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
  };
};

test("is a switch labelled Day theme, off in the dark default", () => {
  render(
    <ThemeProvider storage={memory()}>
      <ThemeToggle />
    </ThemeProvider>,
  );
  const sw = screen.getByRole("switch", { name: "Day theme" });
  expect(sw).toHaveAttribute("aria-checked", "false");
  expect(document.documentElement.dataset.theme).toBe("dark");
});

test("toggling sets data-theme to light and back", async () => {
  const user = userEvent.setup();
  render(
    <ThemeProvider storage={memory()}>
      <ThemeToggle />
    </ThemeProvider>,
  );
  const sw = screen.getByRole("switch", { name: "Day theme" });
  await user.click(sw);
  expect(document.documentElement.dataset.theme).toBe("light");
  expect(sw).toHaveAttribute("aria-checked", "true");
  await user.keyboard(" ");
  expect(document.documentElement.dataset.theme).toBe("dark");
  expect(sw).toHaveAttribute("aria-checked", "false");
});

test("reflects a stored light preference", () => {
  const store = memory();
  store.setItem("umbra.theme", "light");
  render(
    <ThemeProvider storage={store}>
      <ThemeToggle />
    </ThemeProvider>,
  );
  expect(screen.getByRole("switch", { name: "Day theme" })).toHaveAttribute("aria-checked", "true");
});
