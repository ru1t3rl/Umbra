import { expect, test } from "bun:test";
import { render, screen } from "@testing-library/react";
import { useGetStatus } from "@umbra/data";
import type { ReactElement } from "react";
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

test("a generated data hook resolves under App's providers", async () => {
  const realFetch = globalThis.fetch;
  globalThis.fetch = (() =>
    Promise.resolve(
      new Response(JSON.stringify({ api_version: "1.0.0" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    )) as unknown as typeof fetch;
  function StatusProbe(): ReactElement {
    const q = useGetStatus();
    return <p>{q.data ? `api ${q.data.api_version}` : "loading"}</p>;
  }
  try {
    render(<App router={createMemoryRouter([{ path: "/", element: <StatusProbe /> }])} />);
    expect(await screen.findByText("api 1.0.0")).toBeTruthy();
  } finally {
    globalThis.fetch = realFetch;
  }
});
