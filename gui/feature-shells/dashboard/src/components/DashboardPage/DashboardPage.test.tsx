import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import {
  connectEngineStream,
  type EngineStreamConnection,
  EngineStreamProvider,
  type EntitySource,
  type WebSocketLike,
} from "@umbra/engine-stream";
import { RuntimeConfigProvider } from "@umbra/runtime-config";
import { ThemeProvider } from "@umbra/ui-kit";
import type { ReactElement } from "react";

/**
 * The map is tested in its own package (maplibre mocked there); this test replaces `@umbra/map-view` at the
 * package boundary with a stub that records the props the shell gives it. Deviation from "maplibre mocked":
 * the shell only wires, so the boundary is the right seam. Bun's `mock.module` is process-global and
 * `mock.restore()` does not undo it. Restoring in `afterAll` was tried (capture the real module first) and
 * rejected: importing the real module loads the real `maplibre-gl`, which breaks the `maplibre-gl` mock of
 * the map-view tests that run later in the same process (26 failures). So the stub stays installed, which is
 * harmless: no other test file imports `@umbra/map-view` through the package name (map-view's own tests use
 * relative imports) and `App.test.tsx` only compares the route's `lazy` function without loading the page.
 */
const mapProps: {
  source?: EntitySource;
  basemapStyleUrl?: string | undefined;
  insets?: { top?: number };
}[] = [];
mock.module("@umbra/map-view", () => ({
  LiveMap: (props: { source: EntitySource; basemapStyleUrl?: string }): ReactElement => {
    mapProps.push(props);
    return <section aria-label="Live map" data-testid="map" />;
  },
}));
const { default: DashboardPage } = await import("./DashboardPage.tsx");
const { route } = await import("../../index.ts");

class FakeSocket implements WebSocketLike {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  send(): void {}
  close(): void {}
}

let socket: FakeSocket;
let connection: EngineStreamConnection;
let paint: (() => void)[];
let fetchCalls: number;
const realFetch = globalThis.fetch;

beforeEach(() => {
  mapProps.length = 0;
  paint = [];
  fetchCalls = 0;
  globalThis.fetch = (() => {
    fetchCalls += 1;
    return Promise.reject(new Error("no fetch in the dashboard"));
  }) as unknown as typeof fetch;
  socket = new FakeSocket();
  connection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => socket,
    scheduleFrame: (cb) => paint.push(cb),
    cancelFrame: () => {
      paint = [];
    },
    setTimer: () => 0,
    clearTimer: () => {},
  });
});
afterEach(() => {
  cleanup();
  connection.close();
  globalThis.fetch = realFetch;
});

const send = (message: unknown) =>
  act(() => {
    socket.onmessage?.({ data: JSON.stringify(message) });
    const run = paint;
    paint = [];
    for (const f of run) f();
  });
const hello = {
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
const entity = (id: number) => ({
  id,
  lat_deg: 1,
  lon_deg: 2,
  alt_m: 0,
  heading_deg: 0,
  speed_mps: 1,
  ownership: "owned",
});
const frame = (seq: number, tick: number, entities: unknown[]) => ({
  type: "frame",
  kind: "snapshot",
  cause: "tick",
  session: 1,
  seq,
  tick,
  time_s: tick,
  dropped: 0,
  entities,
  events: [],
  events_truncated: false,
  run: {
    state: "running",
    session: 1,
    clock: { mode: "realtime" },
    step_pending: false,
    tick,
    time_s: tick,
  },
});

function renderPage(
  config: { engineUrl: string; basemap?: { styleUrl: string } } = { engineUrl: "" },
) {
  return render(
    <ThemeProvider>
      <RuntimeConfigProvider config={config}>
        <EngineStreamProvider connection={connection}>
          <DashboardPage />
        </EngineStreamProvider>
      </RuntimeConfigProvider>
    </ThemeProvider>,
  );
}

describe("DashboardPage", () => {
  test("has the banner, the main landmark with one h1 and the map region", () => {
    renderPage();
    const banner = screen.getByRole("banner");
    expect(banner).toHaveTextContent("Umbra");
    expect(banner).toHaveTextContent("Live map");
    expect(banner).toHaveClass("vt-cmdbar-floating");
    expect(screen.getByRole("main")).toContainElement(screen.getByTestId("map"));
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.getByRole("switch", { name: "Day theme" })).toBeInTheDocument();
  });

  test("gives the map the stream source and the configured basemap", () => {
    renderPage({ engineUrl: "", basemap: { styleUrl: "https://tiles.example/style.json" } });
    expect(mapProps[0]?.source).toBe(connection.source);
    expect(mapProps[0]?.basemapStyleUrl).toBe("https://tiles.example/style.json");
  });

  test("leaves the basemap to the map when none is configured", () => {
    renderPage();
    expect(mapProps[0] && "basemapStyleUrl" in mapProps[0]).toBe(false);
  });

  test("shows the connecting status first, the reconnect alert after a drop, nothing when open", () => {
    renderPage();
    const main = screen.getByRole("main");
    expect(screen.getByRole("status", { name: "Connecting to the engine" })).toBeInTheDocument();
    act(() => socket.onopen?.());
    send(hello);
    expect(screen.queryByRole("alert")).toBeNull();
    act(() => socket.onclose?.({ code: 1006, reason: "" }));
    const alert = screen.getByRole("alert", { name: "Engine not reachable" });
    expect(main).toContainElement(alert);
  });

  test("a frame updates the readouts and the badge", () => {
    renderPage();
    send(hello);
    send(frame(0, 7, [entity(1), entity(2)]));
    const dd = (label: string) =>
      screen.getByText(label).closest("dl")?.querySelector(".vt-readout-value");
    expect(dd("Entities")).toHaveTextContent("2");
    expect(dd("Tick")).toHaveTextContent("7");
    expect(dd("Sim time")).toHaveTextContent("T+00:00:07");
    expect(screen.getByText("Running")).toBeInTheDocument();
    send(frame(1, 8, [entity(1), entity(2), entity(3)]));
    expect(dd("Entities")).toHaveTextContent("3");
  });

  test("passes the measured bar height plus margins as the top inset", () => {
    const realGetComputedStyle = window.getComputedStyle.bind(window);
    window.getComputedStyle = ((el: Element) => {
      const real = realGetComputedStyle(el);
      if (!(el instanceof HTMLElement && el.tagName === "HEADER")) return real;
      const values: Record<string, string> = {
        "margin-top": "16px",
        "margin-bottom": "16px",
        "--vt-cmdbar-height": "48px",
      };
      return { getPropertyValue: (name: string) => values[name] ?? "" };
    }) as typeof window.getComputedStyle;
    try {
      renderPage();
    } finally {
      window.getComputedStyle = realGetComputedStyle;
    }
    expect(mapProps.at(-1)?.insets).toEqual({ top: 80 });
    expect(screen.getByRole("main").style.getPropertyValue("--dashboard-inset-top")).toBe("80px");
  });

  test("makes no fetch call", () => {
    renderPage();
    send(hello);
    send(frame(0, 1, [entity(1)]));
    expect(fetchCalls).toBe(0);
  });

  test("the map is not re-rendered by frames", () => {
    renderPage();
    send(hello);
    const before = mapProps.length;
    for (let i = 0; i < 20; i += 1) send(frame(i, i + 1, [entity(1)]));
    expect(mapProps.length).toBe(before);
  });
});

describe("route", () => {
  test("is / and lazy-loads the page", async () => {
    expect(route.path).toBe("/");
    expect((await route.lazy()).Component).toBe(DashboardPage);
  });
});
