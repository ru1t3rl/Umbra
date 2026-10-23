// Test doubles for the dashboard shell tests (WI-14; not exported from index.ts, exempt from check:layout).
//
// The map is replaced at the `@umbra/map-view` package boundary by `LiveMapStub` (see the note in
// components/DashboardPage/DashboardPage.test.tsx: the real module loads maplibre-gl, whose mock in the
// map-view tests is process-global). Every test file that uses the stub calls, before importing the page:
//   mock.module("@umbra/map-view", () => ({ LiveMap: LiveMapStub }));
// Everything else is real: the features, the stream (a fake socket), the data hooks (a fake `fetch`),
// the settings provider (an in-memory backend). Nothing outside the stub is mocked.
import { spyOn } from "bun:test";
import { act, render } from "@testing-library/react";
import { DataProvider } from "@umbra/data";
import {
  connectEngineStream,
  type EngineStreamConnection,
  EngineStreamProvider,
  type WebSocketLike,
} from "@umbra/engine-stream";
import { RuntimeConfigProvider } from "@umbra/runtime-config";
import { type SettingsBackend, SettingsProvider } from "@umbra/settings";
import { NavigationProvider, ThemeProvider } from "@umbra/ui-kit";
import {
  type ComponentType,
  Profiler,
  type ReactElement,
  type Ref,
  useImperativeHandle,
} from "react";

// ---- the map stub ------------------------------------------------------------------------------

/** What the shell hands the map (the subset the tests read). */
export interface MapStubProps {
  source?: unknown;
  insets?: { top?: number };
  selectedId?: number | null;
  onSelect?: ((id: number | null) => void) | undefined;
  affiliationOf?: ((force: string | null | undefined) => string) | undefined;
  ref?: Ref<{ centerOn(id: number): boolean }>;
}

/** Props of every render of the stub, oldest first: its length is the number of renders of the shell's map element. */
export const mapRenders: MapStubProps[] = [];
/** Every `centerOn(id)` the shell asked of the map handle. */
export const centerOnCalls: number[] = [];
export let centerOnResult = true;
export const setCenterOnResult = (value: boolean): void => {
  centerOnResult = value;
};
export const resetMapStub = (): void => {
  mapRenders.length = 0;
  centerOnCalls.length = 0;
  centerOnResult = true;
};
/** The newest props the shell gave the map. */
export const lastMapProps = (): MapStubProps => {
  const last = mapRenders.at(-1);
  if (!last) throw new Error("the map stub was never rendered");
  return last;
};

export function LiveMapStub(props: MapStubProps): ReactElement {
  mapRenders.push(props);
  useImperativeHandle(
    props.ref,
    () => ({
      centerOn: (id: number) => {
        centerOnCalls.push(id);
        return centerOnResult;
      },
    }),
    [],
  );
  return (
    <section
      aria-label="Live map"
      data-testid="map"
      data-selected={
        props.selectedId === null || props.selectedId === undefined ? "" : String(props.selectedId)
      }
    />
  );
}

// ---- the stream --------------------------------------------------------------------------------

export class FakeSocket implements WebSocketLike {
  sent: Record<string, unknown>[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  send(data: string): void {
    this.sent.push(JSON.parse(data) as Record<string, unknown>);
  }
  close(): void {}
}

export const HELLO = {
  type: "hello",
  protocol: "umbra.stream.v1",
  api_version: "1.0.0",
  encoding: "json",
  settings: {
    default_rate_hz: 30,
    max_rate_hz: 60,
    max_entity_ids: 10000,
    max_events_per_frame: 4096,
    max_message_bytes: 16_000_000,
    queue_frames: 4,
    send_timeout_s: 10,
  },
};

export type RunState = "idle" | "ready" | "running" | "paused" | "stopped";

export const wireEntity = (id: number, extra: Record<string, unknown> = {}) => ({
  id,
  lat_deg: 1 + id * 0.0001,
  lon_deg: 2 + id * 0.0001,
  alt_m: 0,
  heading_deg: 0,
  speed_mps: 1,
  ownership: "owned",
  force: "blue",
  ...extra,
});

export const wireFrame = (
  seq: number,
  entities: unknown[],
  state: RunState = "running",
  clock: Record<string, unknown> = { mode: "realtime" },
) => ({
  type: "frame",
  kind: "snapshot",
  cause: "tick",
  session: 1,
  seq,
  tick: seq + 1,
  time_s: seq + 1,
  dropped: 0,
  entities,
  events: [],
  events_truncated: false,
  run: { state, session: 1, clock, step_pending: false, tick: seq + 1, time_s: seq + 1 },
});

// ---- fetch -------------------------------------------------------------------------------------

export interface FetchCall {
  method: string;
  /** The url as given to `fetch` (the runtime base URL is "" in tests). */
  url: string;
  body: unknown;
}

export const problem = (status: number, code: string): Response =>
  new Response(
    JSON.stringify({ type: `about:blank#${code}`, code, title: code, status, detail: code }),
    { status, headers: { "content-type": "application/problem+json" } },
  );

export const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

/** Replaces `fetch`; the default answer is a 404 problem. Restore in `afterEach`. */
export function fakeFetch(
  handler: (call: FetchCall) => Response | Promise<Response> = () => problem(404, "not_found"),
) {
  const calls: FetchCall[] = [];
  const spy = spyOn(globalThis, "fetch").mockImplementation(((
    url: string,
    init: RequestInit = {},
  ) => {
    const call: FetchCall = {
      method: init.method ?? "GET",
      url: String(url),
      body: typeof init.body === "string" ? (JSON.parse(init.body) as unknown) : undefined,
    };
    calls.push(call);
    return Promise.resolve(handler(call));
  }) as unknown as typeof fetch);
  return { calls, restore: () => spy.mockRestore() };
}

// ---- the page ----------------------------------------------------------------------------------

export interface Dashboard {
  socket: FakeSocket;
  connection: EngineStreamConnection;
  /** What the settings backend was asked to store, oldest first. */
  writes: unknown[];
  /** `navigate` calls of the NavigationProvider. */
  navigated: string[];
  /** Deliver one wire message, then run the animation frame the connection scheduled. */
  send(message: unknown): void;
  /** The stream opens (socket open + hello). */
  open(): void;
  /** Open, then a first frame with the given run state and entities. */
  start(entities?: unknown[], state?: RunState, clock?: Record<string, unknown>): void;
  close(): void;
}

export interface DashboardOptions {
  /** The raw stored settings the backend returns on `read()`. */
  storedSettings?: unknown;
  config?: { engineUrl: string; basemap?: { styleUrl: string } };
  /** Do not open the stream: the page renders in `connecting`. */
  unopened?: boolean;
  /**
   * Open the stream and deliver a first frame BEFORE the page renders, so the list shows its rows on the
   * throttle's leading edge and a test needs no fake timers (bun 1.4.4 leaves `waitFor` confused after
   * `jest.useRealTimers()`, so tests that wait on a request must not use fake timers at all).
   */
  preload?: { entities?: unknown[]; state?: RunState; clock?: Record<string, unknown> };
  /** Called for every commit of the page's tree (React `Profiler`), to count renders. */
  onCommit?: () => void;
}

/**
 * Mounts the page under the providers the app mounts (theme, runtime config, settings, stream, data,
 * navigation with the "Live map" item). `Page` is the default export of DashboardPage.tsx, imported by the
 * caller after `mock.module`.
 */
export function mountDashboard(Page: ComponentType, options: DashboardOptions = {}): Dashboard {
  const socket = new FakeSocket();
  let paint: (() => void)[] = [];
  const connection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => socket,
    scheduleFrame: (cb) => {
      paint.push(cb);
      return paint.length;
    },
    cancelFrame: () => {
      paint = [];
    },
    setTimer: () => 0,
    clearTimer: () => {},
  });
  const writes: unknown[] = [];
  const navigated: string[] = [];
  const backend: SettingsBackend = {
    read: () => options.storedSettings,
    write: (settings) => {
      writes.push(settings);
    },
  };
  const send = (message: unknown): void =>
    act(() => {
      socket.onmessage?.({ data: JSON.stringify(message) });
      const run = paint;
      paint = [];
      for (const f of run) f();
    });
  const open = (): void => {
    act(() => socket.onopen?.());
    send(HELLO);
  };
  let seq = 0;
  if (options.preload && !options.unopened) {
    open();
    const { entities = [], state = "running", clock = { mode: "realtime" } } = options.preload;
    send(wireFrame(seq++, entities, state, clock));
  }
  render(
    <ThemeProvider>
      <RuntimeConfigProvider config={options.config ?? { engineUrl: "" }}>
        <SettingsProvider backend={backend}>
          <EngineStreamProvider connection={connection}>
            <DataProvider>
              <NavigationProvider
                items={[{ path: "/", label: "Live map", icon: "map" }]}
                currentPath="/"
                navigate={(to) => navigated.push(to)}
              >
                <Profiler id="page" onRender={() => options.onCommit?.()}>
                  <Page />
                </Profiler>
              </NavigationProvider>
            </DataProvider>
          </EngineStreamProvider>
        </SettingsProvider>
      </RuntimeConfigProvider>
    </ThemeProvider>,
  );
  if (!options.unopened && !options.preload) open();
  return {
    socket,
    connection,
    writes,
    navigated,
    send,
    open,
    start(entities = [], state = "running", clock = { mode: "realtime" }) {
      if (options.unopened) open();
      send(wireFrame(seq++, entities, state, clock));
    },
    close: () => connection.close(),
  };
}

// ---- layout ------------------------------------------------------------------------------------

/**
 * happy-dom has no layout: `VirtualList` reads its viewport height from a ResizeObserver, so a fake one
 * reports `height` for every observed element. Returns the restore function.
 */
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
