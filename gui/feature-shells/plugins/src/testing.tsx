// Test doubles for the plugins shell tests (WI-16; not exported, exempt from check:layout). A shell may import
// features only through their public API, and `@umbra/plugin-status` exports no fixtures, so the stream rig and
// the fake engine are stated here, small (the WI-15 precedent, ASM-991 c): a real engine-stream connection over a
// fake socket (to set the run state), a `fetch` double serving contract-shaped fixtures (a plugin with a faulted
// capability, a faulted adapter), the providers the app mounts.
import { spyOn } from "bun:test";
import { act, render } from "@testing-library/react";
import {
  type AdapterDto,
  type ComponentEntry,
  DataProvider,
  type PluginReport,
  useInvalidateEngineQueries,
} from "@umbra/data";
import {
  connectEngineStream,
  EngineStreamProvider,
  useRunStatus,
  type WebSocketLike,
} from "@umbra/engine-stream";
import { type NavItem, NavigationProvider, ThemeProvider } from "@umbra/ui-kit";
import { type ComponentType, useEffect, useRef } from "react";

/**
 * What the app mounts above its pages (`src/hooks/useSessionInvalidation.ts`): a session change invalidates every
 * engine query, so a page that was unmounted while idle reads again for the next scenario.
 */
function SessionInvalidation(): null {
  const session = useRunStatus()?.session;
  const invalidate = useInvalidateEngineQueries();
  const seen = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (session === undefined) return;
    const previous = seen.current;
    seen.current = session;
    if (previous !== undefined && previous !== session) void invalidate();
  }, [session, invalidate]);
  return null;
}

class FakeSocket implements WebSocketLike {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  send(): void {}
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

const runFrame = (state: string, session = 1, seq = 1) => ({
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

/** `GET /plugins`: a plugin with a healthy and a faulted capability, and one that provides an adapter. */
export const PLUGINS: PluginReport = {
  plugins: [
    {
      id: "acme.radar",
      name: "Acme Radar Pack",
      version: "1.2.0",
      abi_major: 1,
      abi_minor: 3,
      root: "scenario",
      origin: "plugins/radar.so",
      capabilities: [
        { id: "acme.radar.scan", kind: "sensor", version: "1.0.0", faulted: false },
        { id: "acme.radar.track", kind: "behavior", version: "0.4.1", faulted: true },
      ],
    },
    {
      id: "acme.dis-plugin",
      name: "Acme DIS",
      version: "2.0.1",
      abi_major: 1,
      abi_minor: 0,
      root: "plugins[0]",
      origin: "dis.so",
      capabilities: [{ id: "acme.dis", kind: "network_adapter", version: "2.0.0", faulted: false }],
    },
  ],
};

/** The same plugins with every capability healthy. */
export const HEALTHY_PLUGINS: PluginReport = {
  plugins: PLUGINS.plugins.map((p) => ({
    ...p,
    capabilities: p.capabilities.map((c) => ({ ...c, faulted: false })),
  })),
};

const counters = (over: Partial<AdapterDto["counters"]> = {}): AdapterDto["counters"] => ({
  control_ignored: 0,
  decode_errors: 0,
  echo_dropped: 0,
  ghosts_capped: 0,
  invalid_state: 0,
  io_errors: 0,
  keys_exhausted: 0,
  published: 0,
  received: 0,
  rx_dropped: 0,
  tx_dropped: 0,
  unmapped: 0,
  unsupported: 0,
  world_errors: 0,
  ...over,
});

/** `GET /network/adapters`: a healthy built-in adapter and a faulted, disabled plugin adapter. */
export const ADAPTERS: readonly AdapterDto[] = [
  {
    name: "dis-main",
    adapter: "builtin.dis",
    enabled: true,
    faulted: false,
    ghosts: 3,
    key_space: { base: 1000, count: 500 },
    counters: counters({ published: 120, received: 87 }),
  },
  {
    name: "hla-lab",
    adapter: "acme.hla",
    plugin: "acme.hla-plugin",
    enabled: false,
    faulted: true,
    fault_reason: "socket closed",
    ghosts: 0,
    key_space: { base: 5000, count: 100 },
    counters: counters({ io_errors: 9 }),
  },
];

const entry = (
  id: string,
  kind: ComponentEntry["kind"],
  provenance: ComponentEntry["provenance"],
): ComponentEntry => ({
  id,
  kind,
  provenance,
  version: "1.0.0",
  description: `The ${id} component`,
  params_schema: { type: "object" },
});

/** `GET /components`. */
export const COMPONENTS: readonly ComponentEntry[] = [
  entry("builtin.waypoint", "motion_model", "builtin"),
  entry("builtin.patrol", "behavior", "builtin"),
  entry("acme.radar.scan", "sensor", "plugin"),
];

const reply = (body: string, status: number, contentType: string): Response =>
  ({
    ok: status >= 200 && status < 300,
    status,
    headers: new Headers({ "content-type": contentType }),
    text: () => Promise.resolve(body),
    clone: () => reply(body, status, contentType),
  }) as unknown as Response;
const jsonReply = (body: unknown): Response => reply(JSON.stringify(body), 200, "application/json");

export interface EngineScript {
  plugins?: PluginReport;
  adapters?: readonly AdapterDto[];
}

/** Replaces `fetch` with an engine serving the fixtures. Returns the paths asked and the restore function. */
export function fakeEngine(script: EngineScript = {}) {
  const paths: string[] = [];
  const spy = spyOn(globalThis, "fetch").mockImplementation(((url: string) => {
    const path = String(url);
    paths.push(path);
    if (path === "/api/v1/plugins") return Promise.resolve(jsonReply(script.plugins ?? PLUGINS));
    if (path === "/api/v1/network/adapters")
      return Promise.resolve(jsonReply(script.adapters ?? ADAPTERS));
    if (path.startsWith("/api/v1/components")) return Promise.resolve(jsonReply(COMPONENTS));
    return Promise.resolve(
      reply(
        JSON.stringify({ type: "about:blank#nf", code: "nf", title: "Not found", status: 404 }),
        404,
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
];

/** Mounts the page under the providers the app mounts, with the stream in run state `state`. */
export function mountPlugins(
  Page: ComponentType,
  state = "ready",
): { push: (state: string, session?: number) => void } {
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
  let seq = 0;
  const nextSeq = (): number => {
    seq += 1;
    return seq;
  };
  const deliver = (...messages: unknown[]): void => {
    act(() => {
      for (const m of messages) socket.onmessage?.({ data: JSON.stringify(m) });
      const run = pending;
      pending = [];
      for (const f of run) f();
    });
  };
  deliver(HELLO, runFrame(state, 1, nextSeq()));
  render(
    <ThemeProvider>
      <EngineStreamProvider connection={connection}>
        <DataProvider>
          <SessionInvalidation />
          <NavigationProvider items={NAV_ITEMS} currentPath="/plugins" navigate={() => {}}>
            <Page />
          </NavigationProvider>
        </DataProvider>
      </EngineStreamProvider>
    </ThemeProvider>,
  );
  return { push: (next, session = 1) => deliver(runFrame(next, session, nextSeq())) };
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
