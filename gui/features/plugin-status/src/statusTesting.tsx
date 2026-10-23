// Test doubles for the WI-13 tests (not exported from index.ts; named `statusTesting.tsx`, not `testing.tsx`, so
// `tsc -b` never sees two `testing.d.ts`, ASM-907). A domain feature may not import another domain feature, so
// the stream rig and fetch double of `template-browser` are re-stated here in the parts these tests need.
// Contents:
// - `rig`, `hello`, `runFrame`: a real engine-stream connection over a fake socket, to set run state and session;
// - `fakeEngine`, `jsonReply`, `problemReply`: a fake engine behind `fetch`;
// - `PLUGINS`, `ADAPTERS`, `COMPONENTS`, `engineWith`: contract-shaped fixtures (ASM-817, ASM-849: the shapes
//   follow `api/openapi.json`; the engine double keeps adapter state, so enable and disable are visible in the
//   next `GET /network/adapters`);
// - `mount`, `settle`, `fakeTimers`: the feature under the stream, data and navigation providers.
import { jest, spyOn } from "bun:test";
import { act, render } from "@testing-library/react";
import { type AdapterDto, type ComponentEntry, DataProvider, type PluginReport } from "@umbra/data";
import {
  connectEngineStream,
  type EngineStreamConnection,
  EngineStreamProvider,
  type WebSocketLike,
} from "@umbra/engine-stream";
import { NavigationProvider } from "@umbra/ui-kit";
import type { ReactElement } from "react";

// ---- the stream ------------------------------------------------------------------------------

class FakeSocket implements WebSocketLike {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  send(): void {}
  close(): void {}
}

export interface Rig {
  connection: EngineStreamConnection;
  /** Deliver messages, then run the animation frame the connection scheduled. */
  push(...messages: unknown[]): void;
}

export function rig(): Rig {
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
  return {
    connection,
    push(...messages) {
      const s = sockets.at(-1);
      if (!s) throw new Error("no socket");
      for (const m of messages) s.onmessage?.({ data: JSON.stringify(m) });
      const run = pending;
      pending = [];
      for (const f of run) f();
    },
  };
}

export const hello = () => ({
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

/** A snapshot frame whose run status has `state` and `session`; `tick` is `seq`, so every frame differs. */
export const runFrame = (seq: number, state: string, session = 1) => ({
  type: "frame",
  kind: "snapshot",
  cause: "control",
  session,
  seq,
  tick: seq,
  time_s: seq / 30,
  dropped: 0,
  entities: [],
  events: [],
  events_truncated: false,
  run: {
    state,
    session,
    clock: { mode: "realtime" },
    step_pending: false,
    tick: seq,
    time_s: seq / 30,
  },
});

// ---- the engine behind fetch -----------------------------------------------------------------

export interface RecordedCall {
  method: string;
  /** Path and query, without the origin (the base URL is "" in tests). */
  path: string;
}

export interface FakeEngine {
  calls: RecordedCall[];
  /** The calls with this method whose path equals (a string) or matches (a RegExp) `path`. */
  to(method: string, path: string | RegExp): RecordedCall[];
  restore(): void;
}

/** Replace `fetch`. The handler answers each call. */
export function fakeEngine(
  handler: (call: RecordedCall) => Response | Promise<Response>,
): FakeEngine {
  const calls: RecordedCall[] = [];
  const spy = spyOn(globalThis, "fetch").mockImplementation(((
    url: string,
    init: RequestInit = {},
  ) => {
    const call: RecordedCall = { method: init.method ?? "GET", path: String(url) };
    calls.push(call);
    return Promise.resolve(handler(call));
  }) as unknown as typeof fetch);
  return {
    calls,
    to: (method, path) =>
      calls.filter(
        (c) =>
          c.method === method && (typeof path === "string" ? c.path === path : path.test(c.path)),
      ),
    restore: () => spy.mockRestore(),
  };
}

/** A reply whose body resolves in microtasks only (the fetcher uses `ok`, `status`, `headers`, `text()`). */
function reply(body: string, status: number, contentType: string): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: new Headers({ "content-type": contentType }),
    text: () => Promise.resolve(body),
    clone() {
      return reply(body, status, contentType);
    },
  } as unknown as Response;
}

export const jsonReply = (body: unknown, status = 200): Response =>
  reply(JSON.stringify(body), status, "application/json");

/** An `application/problem+json` answer (docs/api.md "Errors"). */
export const problemReply = (
  status: number,
  code: string,
  title: string,
  detail: string,
): Response =>
  reply(
    JSON.stringify({ type: `about:blank#${code}`, code, title, status, detail }),
    status,
    "application/problem+json",
  );

// ---- fixtures --------------------------------------------------------------------------------

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

/** No plugin at all (the examples load none). */
export const NO_PLUGINS: PluginReport = { plugins: [] };

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
    counters: counters({ published: 120, received: 87, rx_dropped: 4, decode_errors: 2 }),
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
  version = "1.0.0",
): ComponentEntry => ({
  id,
  kind,
  provenance,
  version,
  description: `The ${id} component`,
  params_schema: { type: "object" },
});

/** `GET /components`: kinds listed out of table order on purpose, ids out of alphabetical order. */
export const COMPONENTS: readonly ComponentEntry[] = [
  entry("builtin.visual", "sensor", "builtin"),
  entry("builtin.patrol", "behavior", "builtin"),
  entry("builtin.waypoint", "motion_model", "builtin"),
  entry("acme.radar.scan", "sensor", "plugin", "1.0.0"),
  entry("file.loiter", "behavior", "file", "0.2.0"),
  entry("builtin.dis", "network_adapter", "builtin"),
];

export interface EngineScript {
  plugins?: PluginReport | Response;
  adapters?: readonly AdapterDto[] | Response;
  components?: readonly ComponentEntry[] | Response;
  /** Answers `POST .../{name}/enable|disable`; default: flips the stored adapter and answers it. */
  toggle?: (name: string, enable: boolean) => Response | Promise<Response>;
}

const isResponse = (v: unknown): v is Response =>
  typeof v === "object" && v !== null && "text" in v && "ok" in v;

/** A fake engine serving the fixtures with a stateful adapter list; unknown paths answer a 404 problem. */
export function engineWith(script: EngineScript = {}): FakeEngine {
  const adapters: AdapterDto[] | Response = isResponse(script.adapters)
    ? script.adapters
    : (script.adapters ?? ADAPTERS).map((a) => ({ ...a }));
  return fakeEngine((call) => {
    if (call.method === "GET" && call.path === "/api/v1/plugins")
      return isResponse(script.plugins) ? script.plugins : jsonReply(script.plugins ?? PLUGINS);
    if (call.method === "GET" && call.path === "/api/v1/network/adapters")
      return isResponse(adapters) ? adapters : jsonReply(adapters);
    if (call.method === "GET" && call.path.startsWith("/api/v1/components"))
      return isResponse(script.components)
        ? script.components
        : jsonReply(script.components ?? COMPONENTS);
    const toggle = /^\/api\/v1\/network\/adapters\/([^/]+)\/(enable|disable)$/.exec(call.path);
    if (call.method === "POST" && toggle) {
      const name = decodeURIComponent(toggle[1] ?? "");
      const enable = toggle[2] === "enable";
      if (script.toggle) return script.toggle(name, enable);
      const found = Array.isArray(adapters) ? adapters.find((a) => a.name === name) : undefined;
      if (!found) return problemReply(404, "not_found", "Not found", "no such adapter");
      found.enabled = enable;
      return jsonReply(found);
    }
    return problemReply(404, "not_found", "Not found", `no route ${call.method} ${call.path}`);
  });
}

// ---- mounting --------------------------------------------------------------------------------

export interface MountOptions {
  /** The run state of the first frame. Omit it for a stream that has not sent a frame yet. */
  state?: string;
  session?: number;
  navigate?: (to: string) => void;
}

export interface Mounted extends Rig {
  /** Render another element under the same providers (same query client, same connection). */
  rerender(ui: ReactElement): void;
}

/** Mount `ui` under the stream, data and navigation providers; the hello (and the first frame) are delivered. */
export function mount(ui: ReactElement, opts: MountOptions = {}): Mounted {
  const r = rig();
  r.push(hello());
  if (opts.state !== undefined) r.push(runFrame(1, opts.state, opts.session ?? 1));
  const navigate = opts.navigate ?? (() => {});
  const tree = (element: ReactElement): ReactElement => (
    <EngineStreamProvider connection={r.connection}>
      <DataProvider>
        <NavigationProvider items={[]} currentPath="/plugins" navigate={navigate}>
          {element}
        </NavigationProvider>
      </DataProvider>
    </EngineStreamProvider>
  );
  const view = render(tree(ui));
  return { ...r, rerender: (next) => view.rerender(tree(next)) };
}

let fake = false;

/** Switch this test to fake timers; `settle` then turns the zero timers by hand. Call `restoreTimers` after. */
export function fakeTimers(): void {
  jest.useFakeTimers();
  fake = true;
}

export function restoreTimers(): void {
  jest.useRealTimers();
  fake = false;
}

/** Advance fake time by `ms` (inside `act`), then settle. */
export async function advance(ms: number): Promise<void> {
  await act(async () => void jest.advanceTimersByTime(ms));
  await settle();
}

/**
 * Let pending promises settle: replies resolve in microtasks, and TanStack Query hands results to React through
 * `setTimeout(0)`, so each round also turns one macrotask (advanced by hand under fake timers; no real waiting).
 */
export async function settle(rounds = 6): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await act(async () => {
      for (let k = 0; k < 10; k++) await Promise.resolve();
      if (fake) jest.advanceTimersByTime(0);
      else await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
}
