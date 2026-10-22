import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { useGetStatus, useListEntities } from "@umbra/data";
import {
  connectEngineStream,
  type EngineStreamConnection,
  useStreamStatus,
  type WebSocketLike,
} from "@umbra/engine-stream";
import { route as dashboard } from "@umbra/shell-dashboard";
import { useTheme } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { createMemoryRouter } from "react-router";
import { appRoutes, createAppRouter, navItems, routes } from "../../router.tsx";
import { NavigationBridge } from "../NavigationBridge/NavigationBridge.tsx";
import { App } from "./App.tsx";

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

test("the showcase route renders", async () => {
  render(<App router={at("/showcase")} connection={idleConnection()} />);
  expect(await screen.findByRole("heading", { name: "Design system", level: 1 })).toBeTruthy();
});

test("/ resolves lazily to the dashboard route and /showcase is kept", () => {
  const root = routes.find((r) => r.path === "/");
  expect(root?.lazy).toBe(dashboard.lazy);
  expect(root?.element).toBeUndefined();
  expect(routes.some((r) => r.path === "/showcase")).toBe(true);
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

test("createAppRouter's root route is the NavigationBridge layout over the shell routes", () => {
  const root = appRoutes[0];
  expect(root?.children).toBe(routes);
  const element = root?.element as ReactElement | undefined;
  expect(element?.type).toBe(NavigationBridge);
  const router = createAppRouter();
  expect(router.routes).toHaveLength(1);
  router.dispose();
});

test("a session change through the App refetches the entity list exactly once", async () => {
  const realFetch = globalThis.fetch;
  let entityCalls = 0;
  globalThis.fetch = ((url: string) => {
    if (String(url).includes("/entities")) entityCalls++;
    return Promise.resolve(
      new Response(JSON.stringify({ api_version: "1.0.0", entities: [], total: 0 }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
  }) as unknown as typeof fetch;
  function Page(): ReactElement {
    const q = useListEntities();
    return <p>{q.isSuccess ? "listed" : "loading"}</p>;
  }
  let socket: WebSocketLike | undefined;
  let pending: (() => void)[] = [];
  const connection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => {
      socket = {
        send: () => {},
        close: () => {},
        onopen: null,
        onmessage: null,
        onclose: null,
        onerror: null,
      };
      return socket;
    },
    scheduleFrame: (cb) => {
      pending.push(cb);
      return pending.length;
    },
    cancelFrame: () => {
      pending = [];
    },
  });
  open.push(connection);
  const send = (m: unknown) => socket?.onmessage?.({ data: JSON.stringify(m) });
  send({
    type: "hello",
    protocol: "umbra.stream.v1",
    api_version: "1.0.0",
    encoding: "json",
    settings: {
      default_rate_hz: 30,
      max_rate_hz: 60,
      max_entity_ids: 10000,
      max_events_per_frame: 4096,
      max_message_bytes: 65536,
      queue_frames: 4,
      send_timeout_s: 10,
    },
  });
  let seq = 0;
  const frame = (session: number) =>
    act(() => {
      send({
        type: "frame",
        kind: "snapshot",
        cause: "tick",
        session,
        seq: seq++,
        tick: 1,
        time_s: 0,
        dropped: 0,
        entities: [],
        events: [],
        events_truncated: false,
        run: {
          state: "running",
          session,
          clock: { mode: "realtime" },
          step_pending: false,
          tick: 1,
          time_s: 0,
        },
      });
      const run = pending;
      pending = [];
      for (const f of run) f();
    });
  try {
    const router = createMemoryRouter([
      {
        element: <NavigationBridge items={navItems} />,
        children: [{ path: "/", element: <Page /> }],
      },
    ]);
    render(<App router={router} connection={connection} />);
    await screen.findByText("listed");
    frame(1);
    const before = entityCalls;
    frame(2);
    await waitFor(() => expect(entityCalls).toBe(before + 1));
    frame(2);
    await act(async () => {
      await Promise.resolve();
    });
    expect(entityCalls).toBe(before + 1);
  } finally {
    globalThis.fetch = realFetch;
  }
});
