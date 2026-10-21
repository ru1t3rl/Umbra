import { expect, test } from "bun:test";
import { render, screen } from "@testing-library/react";
import { createMemoryRouter } from "react-router";
import { App, ConfigErrorApp } from "./App.tsx";
import { routes } from "./router.tsx";

const at = (path: string) => createMemoryRouter(routes, { initialEntries: [path] });

test("the config error app shows the configuration alert", () => {
  render(<ConfigErrorApp />);
  expect(screen.getByRole("alert").textContent).toContain(
    "Configuration could not be read. Check /config.json.",
  );
});

test("the placeholder route renders", () => {
  render(<App router={at("/")} />);
  expect(screen.getByRole("heading", { name: "Umbra" })).toBeTruthy();
});

test("an unknown route shows page not found", () => {
  render(<App router={at("/nowhere")} />);
  expect(screen.getByRole("heading", { name: "Page not found." })).toBeTruthy();
});
