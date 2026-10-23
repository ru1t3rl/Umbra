// Test doubles for the WI-12 tests (not exported from index.ts; named `browserTesting.tsx`, not `testing.tsx`, so
// `tsc -b` never sees two `testing.d.ts`, ASM-907). A domain feature may not import another domain feature, so
// the stream rig and fetch double of `entity-inspector` are re-stated here in the parts these tests need.
// Contents:
// - `rig`, `hello`, `runFrame`: a real engine-stream connection over a fake socket, to set the run state;
// - `fakeEngine`, `jsonReply`, `problemReply`: a fake engine behind `fetch` that also records the JSON body;
// - `TEMPLATES`, `DETAILS`, `COMPONENTS`, `engineWith`: contract-shaped fixtures (ASM-817: the shapes follow
//   `api/openapi.json`; schemas are the supported JSON Schema subset of `@umbra/schema-form`);
// - `mount`: the feature under the stream, data and navigation providers; `settle`; `installResizeObserver`.
import { spyOn } from "bun:test";
import { act, render } from "@testing-library/react";
import {
  type ComponentEntry,
  DataProvider,
  type EntityDetail,
  type TemplateDetail,
  type TemplateSummary,
} from "@umbra/data";
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

/** A snapshot frame whose run status has `state`. */
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
  /** The parsed JSON request body, if any. */
  body: unknown;
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
    const call: RecordedCall = {
      method: init.method ?? "GET",
      path: String(url),
      body: typeof init.body === "string" ? JSON.parse(init.body) : undefined,
    };
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

const motionSchema = {
  type: "object",
  properties: { speed_mps: { type: "number" }, loop: { type: "boolean" } },
};
const visualSchema = {
  type: "object",
  required: ["range_m"],
  properties: { range_m: { type: "number" }, fov_deg: { type: "number" } },
};
const radarSchema = { type: "object", properties: { range_m: { type: "number" } } };
const behaviorSchema = { type: "object", properties: { route: { type: "string" } } };

const entry = (
  id: string,
  kind: ComponentEntry["kind"],
  params_schema: ComponentEntry["params_schema"],
): ComponentEntry => ({
  id,
  kind,
  params_schema,
  description: `The ${id} component`,
  provenance: "builtin",
  version: "1.0.0",
});

/** `GET /components`: what the three fixture templates below refer to (and one nobody does). */
export const COMPONENTS: readonly ComponentEntry[] = [
  entry("builtin.waypoint", "motion_model", motionSchema),
  entry("builtin.visual", "sensor", visualSchema),
  entry("builtin.radar", "sensor", radarSchema),
  entry("builtin.patrol", "behavior", behaviorSchema),
  entry("builtin.unused", "behavior", behaviorSchema),
];

/** `GET /templates`: a full row, a row without description and behavior, a second behavior. */
export const TEMPLATES: readonly TemplateSummary[] = [
  {
    id: "tank",
    description: "Main battle tank",
    motion: "builtin.waypoint",
    behavior: "builtin.patrol",
    source: "templates/tank.yaml",
  },
  { id: "scout", motion: "builtin.waypoint", behavior: null, source: "templates/scout.yaml" },
  {
    id: "drone",
    description: "Quadcopter",
    motion: "builtin.hover",
    behavior: "builtin.loiter",
    source: "templates/drone.yaml",
  },
];

/** `n` generated templates `t000..`, for the 50/51 threshold (ASM-862). */
export const manyTemplates = (n: number): TemplateSummary[] =>
  Array.from({ length: n }, (_, i) => ({
    id: `t${String(i).padStart(3, "0")}`,
    motion: "builtin.waypoint",
    source: `templates/t${i}.yaml`,
  }));

/** `GET /templates/{id}` for `scout`: chain `scout -> vehicle -> base` (root last), two sensors, no behavior. */
export const SCOUT: TemplateDetail = {
  id: "scout",
  chain: ["scout", "vehicle", "base"],
  source: "templates/scout.yaml",
  description: "Fast recon vehicle",
  motion: { id: "builtin.waypoint", params: { speed_mps: 12, loop: false } },
  sensors: [
    { id: "builtin.visual", params: { range_m: 3000, fov_deg: 120 } },
    { id: "builtin.radar", params: { range_m: 20000 } },
  ],
  behavior: null,
};

/** `tank`: one sensor, a behavior, and a sensor `acme.lidar` that `GET /components` does not know. */
export const TANK: TemplateDetail = {
  id: "tank",
  chain: ["tank", "base"],
  source: "templates/tank.yaml",
  motion: { id: "builtin.waypoint", params: { speed_mps: 8 } },
  sensors: [
    { id: "builtin.visual", params: { range_m: 2000 } },
    { id: "acme.lidar", params: { rays: 64 } },
  ],
  behavior: { id: "builtin.patrol", params: { route: "north" } },
};

/** The body of a successful `POST /entities`: `EntityDetail` of entity `id`. */
export const spawned = (id: number, name?: string): EntityDetail => ({
  motion_model: "builtin.waypoint",
  contacts: [],
  summary: {
    id,
    alt_m: 0,
    heading_deg: 0,
    lat_deg: 10,
    lon_deg: 20,
    ownership: "owned",
    speed_mps: 0,
    ...(name === undefined ? {} : { name }),
  },
});

export interface EngineScript {
  templates?: readonly TemplateSummary[] | Response;
  details?: Record<string, TemplateDetail | Response>;
  components?: readonly ComponentEntry[] | Response;
  /** `GET /templates/{id}` for these ids answers only after the promise resolves (a slow, uncached template). */
  hold?: Record<string, Promise<unknown>>;
  /** Answers `POST /api/v1/entities`; default: a 201 with entity 42. */
  spawn?: (call: RecordedCall) => Response | Promise<Response>;
}

const asReply = (v: unknown): Response =>
  typeof v === "object" && v !== null && "text" in v && "ok" in v ? (v as Response) : jsonReply(v);

/** A fake engine serving the fixtures; unknown paths answer a 404 problem. */
export function engineWith(script: EngineScript = {}): FakeEngine {
  const details = script.details ?? { scout: SCOUT, tank: TANK };
  return fakeEngine((call) => {
    if (call.method === "GET" && call.path === "/api/v1/templates")
      return asReply(script.templates ?? TEMPLATES);
    if (call.method === "GET" && call.path.startsWith("/api/v1/components"))
      return asReply(script.components ?? COMPONENTS);
    if (call.method === "GET" && call.path.startsWith("/api/v1/templates/")) {
      const found = details[decodeURIComponent(call.path.slice("/api/v1/templates/".length))];
      const id = decodeURIComponent(call.path.slice("/api/v1/templates/".length));
      const held = script.hold?.[id];
      if (found && held) return held.then(() => asReply(found));
      return found
        ? asReply(found)
        : problemReply(404, "template_not_found", "Not found", "no such template");
    }
    if (call.method === "POST" && call.path === "/api/v1/entities")
      return script.spawn ? (script.spawn(call) as Response) : jsonReply(spawned(42), 201);
    return problemReply(404, "not_found", "Not found", `no route ${call.method} ${call.path}`);
  });
}

// ---- mounting --------------------------------------------------------------------------------

/** Makes every `ResizeObserver` report `height` at once; returns the restore function (ASM-873). */
export function installResizeObserver(height: number): () => void {
  const before = Reflect.get(globalThis, "ResizeObserver");
  class FakeResizeObserver {
    constructor(private readonly callback: (entries: unknown[]) => void) {}
    observe(): void {
      this.callback([{ contentRect: { height } }]);
    }
    unobserve(): void {}
    disconnect(): void {}
  }
  Reflect.set(globalThis, "ResizeObserver", FakeResizeObserver);
  return () => {
    if (before === undefined) Reflect.deleteProperty(globalThis, "ResizeObserver");
    else Reflect.set(globalThis, "ResizeObserver", before);
  };
}

export interface MountOptions {
  /**
   * The run state delivered in the first frame. Omit it for a stream that has not sent a frame yet
   * (the run status is then undefined, ASM-947).
   */
  state?: string;
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
  if (opts.state !== undefined) r.push(runFrame(1, opts.state));
  const navigate = opts.navigate ?? (() => {});
  const tree = (element: ReactElement): ReactElement => (
    <EngineStreamProvider connection={r.connection}>
      <DataProvider>
        <NavigationProvider items={[]} currentPath="/templates" navigate={navigate}>
          {element}
        </NavigationProvider>
      </DataProvider>
    </EngineStreamProvider>
  );
  const view = render(tree(ui));
  return { ...r, rerender: (next) => view.rerender(tree(next)) };
}

/**
 * Let pending promises settle: replies resolve in microtasks, and TanStack Query hands results to React through
 * `setTimeout(0)`, so each round also turns one macrotask (no real waiting).
 */
export async function settle(rounds = 6): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await act(async () => {
      for (let k = 0; k < 10; k++) await Promise.resolve();
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
}
