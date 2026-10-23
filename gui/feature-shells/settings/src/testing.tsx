// Test doubles for the settings shell tests (WI-17; not exported, exempt from check:layout). A shell may import
// features only through their public API and a domain feature exports no fixtures, so the rig is stated here, small
// (the WI-15/WI-16 precedent, ASM-991 c): a real engine-stream connection over a fake socket that records what the
// client sends (the subscribes), a `fetch` double serving `GET /status`, a recording settings backend, and the
// providers the app mounts (SettingsProvider above ThemeProvider, ADR 0074).
import { spyOn } from "bun:test";
import { act, render } from "@testing-library/react";
import { DataProvider, type EngineStatus } from "@umbra/data";
import {
  connectEngineStream,
  EngineStreamProvider,
  type Subscription,
  type WebSocketLike,
} from "@umbra/engine-stream";
import { RuntimeConfigProvider } from "@umbra/runtime-config";
import {
  type Settings,
  type SettingsBackend,
  SettingsProvider,
  useAffiliationOf,
} from "@umbra/settings";
import { type NavItem, NavigationProvider, ThemeProvider } from "@umbra/ui-kit";
import type { ComponentType, ReactElement } from "react";

class FakeSocket implements WebSocketLike {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  sent: Array<{ type: string } & Subscription> = [];
  send(data: string): void {
    this.sent.push(JSON.parse(data));
  }
  close(): void {}
}

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

const runFrame = (state: string, session: number, seq: number) => ({
  type: "frame",
  kind: "snapshot",
  cause: "control",
  session,
  seq,
  tick: 1,
  time_s: 0,
  dropped: 0,
  entities: [],
  events: [],
  events_truncated: false,
  run: { state, session, clock: { mode: "realtime" }, step_pending: false, tick: 1, time_s: 0 },
});

/** The contract version the status double reports (differs from the stream's hello, so a test can tell them apart). */
export const API_VERSION = "1.2.3";

const status = (state: string): EngineStatus => ({
  api_version: API_VERSION,
  run: {
    state,
    session: 1,
    clock: { mode: "realtime" },
    step_pending: false,
  } as EngineStatus["run"],
});

const reply = (body: string, code: number, contentType: string): Response =>
  ({
    ok: code >= 200 && code < 300,
    status: code,
    headers: new Headers({ "content-type": contentType }),
    text: () => Promise.resolve(body),
    clone: () => reply(body, code, contentType),
  }) as unknown as Response;

export interface EngineScript {
  /** `GET /status` fails with a 503 problem. */
  statusFails?: boolean;
}

/** Replaces `fetch` with an engine that serves `GET /status`. Returns the paths asked and the restore function. */
export function fakeEngine(script: EngineScript = {}, state = "ready") {
  const paths: string[] = [];
  const spy = spyOn(globalThis, "fetch").mockImplementation(((url: string) => {
    const path = String(url);
    paths.push(path);
    if (path === "/api/v1/status" && !script.statusFails)
      return Promise.resolve(reply(JSON.stringify(status(state)), 200, "application/json"));
    const code = path === "/api/v1/status" ? 503 : 404;
    return Promise.resolve(
      reply(
        JSON.stringify({ type: "about:blank#e", code: "e", title: "Unavailable", status: code }),
        code,
        "application/problem+json",
      ),
    );
  }) as unknown as typeof fetch);
  return { paths, restore: () => spy.mockRestore() };
}

export const NAV_ITEMS: NavItem[] = [
  { path: "/", label: "Live map", icon: "map" },
  { path: "/templates", label: "Templates", icon: "category" },
  { path: "/plugins", label: "Plugins", icon: "extension" },
  { path: "/settings", label: "Settings", icon: "settings" },
];

/** Prints `useAffiliationOf()("blue")` next to the page: an override must recolour through the app's hook. */
function BlueProbe(): ReactElement {
  const of = useAffiliationOf();
  return <output aria-label="Affiliation of blue">{of("blue")}</output>;
}

export interface MountOptions {
  /** The run state of the first frame; default `ready`. */
  state?: string;
  /** What the settings backend holds before the first render. */
  stored?: unknown;
  /** The runtime config's engine URL; default `http://engine.test:8080`. */
  engineUrl?: string;
}

export interface Mounted {
  /** Every settings object written through the backend, in order. */
  writes: Settings[];
  /** The `subscribe` messages the client sent on the socket, in order. */
  subscribes: () => Array<{ type: string } & Subscription>;
}

/** Mounts the page under the providers the app mounts, with a real stream connection in run state `state`. */
export function mountSettingsPage(Page: ComponentType, options: MountOptions = {}): Mounted {
  const sockets: FakeSocket[] = [];
  let pending: (() => void)[] = [];
  const connection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    scheduleFrame: (cb) => {
      pending.push(cb);
      return pending.length;
    },
    cancelFrame: () => {
      pending = [];
    },
    setTimer: () => 1,
    clearTimer: () => {},
  });
  const socket = sockets.at(-1);
  if (!socket) throw new Error("no socket");
  act(() => {
    for (const m of [HELLO, runFrame(options.state ?? "ready", 1, 1)])
      socket.onmessage?.({ data: JSON.stringify(m) });
    const run = pending;
    pending = [];
    for (const f of run) f();
  });
  const writes: Settings[] = [];
  const backend: SettingsBackend = {
    read: () => options.stored,
    write: (s) => void writes.push(s),
  };
  render(
    <SettingsProvider backend={backend}>
      <ThemeProvider>
        <RuntimeConfigProvider
          config={{ engineUrl: options.engineUrl ?? "http://engine.test:8080" }}
        >
          <EngineStreamProvider connection={connection}>
            <DataProvider>
              <NavigationProvider items={NAV_ITEMS} currentPath="/settings" navigate={() => {}}>
                <Page />
                <BlueProbe />
              </NavigationProvider>
            </DataProvider>
          </EngineStreamProvider>
        </RuntimeConfigProvider>
      </ThemeProvider>
    </SettingsProvider>,
  );
  return { writes, subscribes: () => socket.sent.filter((m) => m.type === "subscribe") };
}

/** Lets pending promises settle (replies resolve in microtasks; TanStack Query notifies through `setTimeout(0)`). */
export async function settle(rounds = 6): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await act(async () => {
      for (let k = 0; k < 10; k++) await Promise.resolve();
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
}
