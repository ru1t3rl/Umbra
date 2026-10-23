// Test doubles for the WI-10 tests (not exported from index.ts; the name differs from `testing.ts` on purpose,
// `tsc -b` refuses two files that emit the same `testing.d.ts`, ASM-907). Contents:
// - `summary`/`wireEntity`: an entity as the GUI holds it / as the stream sends it;
// - `FakeEntitySource`: a hand-written `EntitySource` for the list (no providers needed);
// - `rig`, `hello`, `streamFrame`: a real engine-stream connection over a fake socket (inspector tests);
// - `fakeEngine`, `jsonReply`, `problemReply`: a fake engine behind `fetch`;
// - `installResizeObserver`: a viewport height for `VirtualList` (happy-dom has no layout, ASM-873).
import { spyOn } from "bun:test";
import { act, render } from "@testing-library/react";
import { DataProvider } from "@umbra/data";
import {
  connectEngineStream,
  type EngineStreamConnection,
  EngineStreamProvider,
  type EntityFrame,
  type EntitySource,
  type EntitySummary,
  type WebSocketLike,
} from "@umbra/engine-stream";
import type { ReactElement } from "react";

/** An optional field may be given as `undefined` ("not set"); it is dropped, so no key holds `undefined`. */
type SummaryExtra = { [K in keyof EntitySummary]?: EntitySummary[K] | undefined };

/** An entity as the GUI holds it. */
export const summary = (id: number, extra: SummaryExtra = {}): EntitySummary => {
  const defined = Object.fromEntries(Object.entries(extra).filter(([, v]) => v !== undefined));
  return {
    id,
    alt_m: 12.5,
    heading_deg: 90,
    lat_deg: 10 + id * 0.001,
    lon_deg: 20 + id * 0.001,
    ownership: "owned",
    speed_mps: 5,
    force: "blue",
    ...defined,
  };
};

export const frameOf = (
  entities: readonly EntitySummary[],
  extra: Partial<EntityFrame> = {},
): EntityFrame => ({ session: 1, tick: 1, time_s: 0, seq: 0, dropped: 0, entities, ...extra });

/** A hand-written `EntitySource`; `push` delivers a frame to every listener, like the store flush. */
export class FakeEntitySource implements EntitySource {
  readonly listeners = new Set<(frame: EntityFrame) => void>();
  frame: EntityFrame | undefined;
  getFrame(): EntityFrame | undefined {
    return this.frame;
  }
  subscribe(listener: (frame: EntityFrame) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  push(frame: EntityFrame): void {
    this.frame = frame;
    for (const l of [...this.listeners]) l(frame);
  }
}

/** Makes every `ResizeObserver` report `height` at once; returns the restore function. */
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
  /** The network drops the socket: the status becomes reconnecting. */
  drop(): void;
  /** Fire the backoff timer: a new socket is created (it opens with the next `hello`). */
  reconnect(): void;
}

export function rig(): Rig {
  const sockets: FakeSocket[] = [];
  let pending: (() => void)[] = [];
  let timers: (() => void)[] = [];
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
    setTimer: (fn) => {
      timers.push(fn);
      return timers.length;
    },
    clearTimer: () => {},
  });
  const socket = (): FakeSocket => {
    const s = sockets.at(-1);
    if (!s) throw new Error("no socket");
    return s;
  };
  const paint = (): void => {
    const run = pending;
    pending = [];
    for (const f of run) f();
  };
  return {
    connection,
    push(...messages) {
      for (const m of messages) socket().onmessage?.({ data: JSON.stringify(m) });
      paint();
    },
    drop() {
      socket().onclose?.({ code: 1006, reason: "" });
      paint();
    },
    reconnect() {
      const run = timers;
      timers = [];
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

/** An entity as the stream sends it. */
export const wireEntity = (id: number, extra: Record<string, unknown> = {}) => ({
  id,
  lat_deg: 10 + id * 0.001,
  lon_deg: 20 + id * 0.001,
  alt_m: 12.5,
  heading_deg: 90,
  speed_mps: 5,
  ownership: "owned",
  force: "blue",
  ...extra,
});

export interface FrameOptions {
  session?: number;
  /** The frame cause; "control" frames follow a command. Default "tick". */
  cause?: string;
  state?: string;
  entities?: unknown[];
}

/** A snapshot frame. `state` is the run state (default running). */
export const streamFrame = (seq: number, opts: FrameOptions = {}) => {
  const session = opts.session ?? 1;
  return {
    type: "frame",
    kind: "snapshot",
    cause: opts.cause ?? "tick",
    session,
    seq,
    tick: seq,
    time_s: seq / 30,
    dropped: 0,
    entities: opts.entities ?? [],
    events: [],
    events_truncated: false,
    run: {
      state: opts.state ?? "running",
      session,
      clock: { mode: "realtime" },
      step_pending: false,
      tick: seq,
      time_s: seq / 30,
    },
  };
};

// ---- the engine behind fetch -----------------------------------------------------------------

export interface RecordedCall {
  method: string;
  /** Path and query, without the origin (the base URL is "" in tests). */
  path: string;
}

export interface FakeEngine {
  calls: RecordedCall[];
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
  return { calls, restore: () => spy.mockRestore() };
}

/**
 * A reply whose body resolves in microtasks only. A real `Response.text()` needs an event-loop turn, which
 * jest fake timers cannot be asked to wait for; the fetcher uses just `ok`, `status`, `headers` and `text()`.
 */
function reply(body: string, status: number, contentType: string | null): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: new Headers(contentType === null ? {} : { "content-type": contentType }),
    text: () => Promise.resolve(body),
    clone() {
      return reply(body, status, contentType);
    },
  } as unknown as Response;
}

export const jsonReply = (body: unknown, status = 200): Response =>
  reply(JSON.stringify(body), status, "application/json");

/** 204 No Content. */
export const noContent = (): Response => reply("", 204, null);

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

/** `GET /api/v1/entities/{id}` body (`EntityDetail`). */
export const detailBody = (
  id: number,
  extra: { motion_model?: string; contacts?: number[]; summary?: Record<string, unknown> } = {},
) => ({
  motion_model: extra.motion_model ?? "waypoint",
  contacts: extra.contacts ?? [],
  summary: wireEntity(id, extra.summary),
});

export interface Mounted extends Rig {
  /** Render another element under the same providers (same query client, same connection). */
  rerender(ui: (source: EntitySource) => ReactElement): void;
}

/** Mount `ui` under the stream and data providers with an open connection (hello delivered). */
export function mount(
  ui: (source: EntitySource) => ReactElement,
  /** Runs before the first render (the stream already has its hello and frames when the component mounts). */
  prepare?: (r: Rig) => void,
): Mounted {
  const r = rig();
  if (prepare) {
    r.push(hello());
    prepare(r);
  }
  const tree = (element: ReactElement): ReactElement => (
    <EngineStreamProvider connection={r.connection}>
      <DataProvider>{element}</DataProvider>
    </EngineStreamProvider>
  );
  const view = render(tree(ui(r.connection.source)));
  if (!prepare) act(() => r.push(hello()));
  return {
    ...r,
    rerender: (next) => view.rerender(tree(next(r.connection.source))),
  };
}
