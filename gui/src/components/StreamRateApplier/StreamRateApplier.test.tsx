// WI-17 review m4 (ASM-997): the stored stream rate is applied once, by the app, whichever page is first. Before it
// was applied by the dashboard and the settings pages only, so `/templates` or `/plugins` as the first page streamed
// at the engine's default rate until one of them was visited.
import { afterEach, describe, expect, spyOn, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import {
  connectEngineStream,
  type EngineStreamConnection,
  type Subscription,
  type WebSocketLike,
} from "@umbra/engine-stream";
import type { SettingsBackend } from "@umbra/settings";
import { createMemoryRouter } from "react-router";
import { routes } from "../../router.tsx";
import { App } from "../App/App.tsx";

const open: EngineStreamConnection[] = [];
let restore: (() => void) | undefined;
afterEach(() => {
  cleanup();
  for (const c of open.splice(0)) c.close();
  restore?.();
  window.localStorage.clear();
});

const HELLO = {
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
};

function rig() {
  const sent: Array<{ type: string } & Subscription> = [];
  let socket: (WebSocketLike & { onmessage: ((e: { data: unknown }) => void) | null }) | undefined;
  const connection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => {
      socket = {
        send: (data: string) => void sent.push(JSON.parse(data)),
        close: () => {},
        onopen: null,
        onmessage: null,
        onclose: null,
        onerror: null,
      };
      return socket;
    },
  });
  open.push(connection);
  const spy = spyOn(globalThis, "fetch").mockImplementation((() =>
    Promise.resolve(
      new Response(JSON.stringify({ type: "about:blank#e", code: "e", title: "x", status: 404 }), {
        status: 404,
        headers: { "content-type": "application/problem+json" },
      }),
    )) as unknown as typeof fetch);
  restore = () => spy.mockRestore();
  const hello = () => act(() => socket?.onmessage?.({ data: JSON.stringify(HELLO) }));
  return { connection, sent, hello, subscribes: () => sent.filter((m) => m.type === "subscribe") };
}

const stored = (rate: number): SettingsBackend => ({
  read: () => ({ version: 1, stream: { maxRateHz: rate } }),
  write: () => {},
});

describe("the app applies the stored stream rate", () => {
  for (const path of ["/templates", "/plugins", "/settings", "/"]) {
    test(`${path} as the first page streams at the stored rate, once`, async () => {
      const r = rig();
      const router = createMemoryRouter(routes, { initialEntries: [path] });
      render(<App router={router} connection={r.connection} settingsBackend={stored(12)} />);
      r.hello();
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
      });
      const rates = r.subscribes().map((m) => m.max_rate_hz);
      expect(rates.length).toBeGreaterThan(0);
      expect(rates.every((rate) => rate === 12)).toBe(true);
      expect(rates).toHaveLength(1);
    });
  }

  test("an unknown route streams at it too", async () => {
    const r = rig();
    const router = createMemoryRouter(routes, { initialEntries: ["/nowhere"] });
    render(<App router={router} connection={r.connection} settingsBackend={stored(7)} />);
    r.hello();
    expect(screen.getByRole("heading", { name: "Page not found." })).toBeTruthy();
    expect(r.subscribes().at(-1)?.max_rate_hz).toBe(7);
  });

  test("with nothing stored it is the default 30 Hz", async () => {
    const r = rig();
    const router = createMemoryRouter(routes, { initialEntries: ["/templates"] });
    render(<App router={router} connection={r.connection} />);
    r.hello();
    expect(r.subscribes().at(-1)?.max_rate_hz).toBe(30);
  });
});
