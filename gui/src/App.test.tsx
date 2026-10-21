import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { useGetStatus } from "@umbra/data";
import {
  connectEngineStream,
  type EngineStreamConnection,
  useStreamStatus,
  type WebSocketLike,
} from "@umbra/engine-stream";
import { useTheme } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { createMemoryRouter } from "react-router";
import { App, ConfigErrorApp } from "./App.tsx";
import { routes } from "./router.tsx";

const open: EngineStreamConnection[] = [];
afterEach(() => {
  cleanup();
  for (const c of open.splice(0)) c.close();
});

/** A connection on a socket that never connects: App tests need the provider, not a server. */
function idleConnection(): EngineStreamConnection {
  const socket: WebSocketLike = {
    send: () => {},
    close: () => {},
    onopen: null,
    onmessage: null,
    onclose: null,
    onerror: null,
  };
  const connection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => socket,
  });
  open.push(connection);
  return connection;
}

const at = (path: string) => createMemoryRouter(routes, { initialEntries: [path] });

test("the config error app shows the configuration alert", () => {
  render(<ConfigErrorApp />);
  expect(screen.getByRole("alert").textContent).toContain(
    "Configuration could not be read. Check /config.json.",
  );
});

test("the placeholder route renders", () => {
  render(<App router={at("/")} connection={idleConnection()} />);
  expect(screen.getByRole("heading", { name: "Umbra" })).toBeTruthy();
});

test("an unknown route shows page not found", () => {
  render(<App router={at("/nowhere")} connection={idleConnection()} />);
  expect(screen.getByRole("heading", { name: "Page not found." })).toBeTruthy();
});

test("App provides theme, data and stream context to every route (provider presence and nesting)", async () => {
  const realFetch = globalThis.fetch;
  globalThis.fetch = (() =>
    Promise.resolve(
      new Response(JSON.stringify({ api_version: "1.0.0" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    )) as unknown as typeof fetch;
  function Probe(): ReactElement {
    const q = useGetStatus();
    const stream = useStreamStatus();
    const { resolved } = useTheme();
    return <p>{`${q.data ? `api ${q.data.api_version}` : "loading"} | ${stream} | ${resolved}`}</p>;
  }
  try {
    render(
      <App
        router={createMemoryRouter([{ path: "/", element: <Probe /> }])}
        connection={idleConnection()}
      />,
    );
    expect(await screen.findByText(/^api 1\.0\.0 \| connecting \| (dark|light)$/)).toBeTruthy();
  } finally {
    globalThis.fetch = realFetch;
  }
});
