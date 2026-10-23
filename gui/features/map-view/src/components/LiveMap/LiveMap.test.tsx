import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { ThemeProvider, useTheme } from "@umbra/ui-kit";
import { Profiler, type ReactElement, StrictMode } from "react";
import { AnnounceTimersContext } from "../../context/AnnounceTimersContext.ts";
import { FakeEntitySource, FakeMap, fakeRasterizer, frameWith } from "../../maplibre/testing.ts";
import { ANNOUNCE_INTERVAL_MS, type Timers } from "../../model/announce.ts";
import { DIRECTION_IMAGE_ID } from "../../model/symbols.ts";

const raster = fakeRasterizer();
const rasterize = mock(raster.rasterize);
mock.module("maplibre-gl", () => ({ Map: FakeMap }));
mock.module("../../maplibre/rasterize.ts", () => ({ rasterizeSvg: rasterize }));
const { LiveMap } = await import("./LiveMap.tsx");

const TOKENS: Record<string, string> = {
  "surface-0": "#000001",
  "surface-1": "#000002",
  line: "#000003",
  "line-strong": "#000004",
  "ink-muted": "#000005",
  "symbol-frame": "#000006",
  "affil-friend": "#000007",
  "affil-hostile": "#000008",
  "affil-neutral": "#000009",
  "affil-unknown": "#00000a",
  "opacity-track": "0.55",
  "space-16": "64px",
  azure: "#00000b", // human decision 2026-10-23, M8 WI-9 review
};
const setTokens = (omit?: string) => {
  const root = document.documentElement;
  for (const [k, v] of Object.entries(TOKENS)) {
    if (k === omit) root.style.removeProperty(`--${k}`);
    else root.style.setProperty(`--${k}`, v);
  }
};
const nextFrame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
const settle = async () => {
  await act(async () => {
    await nextFrame();
    await nextFrame();
  });
};
/** Style loaded and symbol images installed: the map may receive data. */
const ready = async (m: FakeMap) => {
  await act(async () => {
    m.load();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
  });
};
const map = (): FakeMap => {
  const m = FakeMap.instances.at(-1);
  if (!m) throw new Error("no map created");
  return m;
};

/** Manual timers for the count announcements: `fire()` is "2 seconds passed". */
const clock = (() => {
  let next = 1;
  const pending = new Map<number, () => void>();
  const timers: Timers<number> = {
    setTimer: (cb) => {
      const h = next++;
      pending.set(h, cb);
      return h;
    },
    clearTimer: (h) => {
      pending.delete(h);
    },
  };
  return {
    timers,
    reset: () => pending.clear(),
    get pending() {
      return pending.size;
    },
    fire() {
      for (const [h, cb] of [...pending]) {
        pending.delete(h);
        cb();
      }
    },
  };
})();

let consoleError: typeof console.error;
beforeEach(() => {
  FakeMap.reset();
  clock.reset();
  localStorage.clear();
  setTokens();
  consoleError = console.error;
});
afterEach(() => {
  cleanup();
  console.error = consoleError;
});

const wrap = (element: ReactElement, preference: "dark" | "light" = "dark") => (
  <AnnounceTimersContext.Provider value={clock.timers}>
    <ThemeProvider defaultPreference={preference}>{element}</ThemeProvider>
  </AnnounceTimersContext.Provider>
);
const renderMap = (source: FakeEntitySource, element = <LiveMap source={source} />) =>
  render(wrap(element));

function themeSwitch() {
  const ref: { set?: (p: "dark" | "light") => void } = {};
  function Probe() {
    ref.set = useTheme().setPreference;
    return null;
  }
  return { ref, Probe };
}

describe("LiveMap", () => {
  test("the offline style carries the bundled Natural Earth land and borders by URL", () => {
    renderMap(new FakeEntitySource());
    const style = map().options.style as {
      sources: Record<string, { data: unknown }>;
      layers: { id: string; source?: string }[];
    };
    const land = style.sources.land?.data;
    const borders = style.sources.borders?.data;
    expect(typeof land === "string" && land.endsWith("ne_50m_land.geojson")).toBe(true);
    expect(typeof borders === "string" && borders.endsWith("ne_50m_admin0_lines.geojson")).toBe(
      true,
    );
    expect(style.layers.find((l) => l.id === "coast")?.source).toBe("land");
  });

  test("creates one view per mount and destroys it on unmount", () => {
    const { unmount } = renderMap(new FakeEntitySource());
    expect(FakeMap.instances).toHaveLength(1);
    unmount();
    expect(map().removed).toBe(true);
  });

  test("under StrictMode the first view is destroyed before the second is used", () => {
    const source = new FakeEntitySource();
    render(<StrictMode>{wrap(<LiveMap source={source} />)}</StrictMode>);
    expect(FakeMap.instances.filter((m) => !m.removed)).toHaveLength(1);
    expect(source.listeners.size).toBe(1);
  });

  test("subscribes to the source and unsubscribes on unmount", () => {
    const source = new FakeEntitySource();
    const { unmount } = renderMap(source);
    expect(source.listeners.size).toBe(1);
    unmount();
    expect(source.listeners.size).toBe(0);
  });

  test("shows the frame already in the source on mount", async () => {
    const source = new FakeEntitySource();
    source.frame = frameWith(2);
    renderMap(source);
    await ready(map());
    await settle();
    expect(map().entities.setDataCalls).toBe(1);
    expect(map().entities.serialise().features).toHaveLength(2);
  });

  test("symbol images go through the (mocked) rasteriser, so the module mock is effective", async () => {
    renderMap(new FakeEntitySource());
    await ready(map());
    expect(rasterize).toHaveBeenCalled();
    expect(map().hasImage(DIRECTION_IMAGE_ID)).toBe(true);
  });

  test("has a named region; the count is announced politely and at most every interval", async () => {
    const source = new FakeEntitySource();
    renderMap(source);
    expect(screen.getByRole("region", { name: "Live map" })).toBeInTheDocument();
    const live = screen.getByText("0 entities on the map");
    expect(live).toHaveAttribute("aria-live", "polite");
    await act(async () => source.push(frameWith(1)));
    expect(live).toHaveTextContent("0 entities on the map"); // not yet: the interval has not passed
    act(() => clock.fire());
    expect(live).toHaveTextContent("1 entity on the map");
    await act(async () => source.push(frameWith(5)));
    act(() => clock.fire());
    expect(live).toHaveTextContent("5 entities on the map");
  });

  test("100 frames with changing counts: one announcement, and few renders", async () => {
    const source = new FakeEntitySource();
    let commits = 0;
    render(
      wrap(
        <Profiler id="map" onRender={() => commits++}>
          <LiveMap source={source} />
        </Profiler>,
      ),
    );
    await ready(map());
    const mounted = commits;
    act(() => {
      for (let i = 1; i <= 100; i++) source.push(frameWith(i, 1, i));
    });
    await settle();
    expect(commits).toBe(mounted); // the changing count cost no render
    expect(clock.pending).toBe(1);
    act(() => clock.fire());
    expect(commits).toBe(mounted + 1); // one announcement, one render
    expect(screen.getByText("100 entities on the map")).toBeInTheDocument();
    expect(ANNOUNCE_INTERVAL_MS).toBe(2000);
  });

  test("a custom label names the region", () => {
    renderMap(
      new FakeEntitySource(),
      <LiveMap source={new FakeEntitySource()} label="Operations" />,
    );
    expect(screen.getByRole("region", { name: "Operations" })).toBeInTheDocument();
  });

  test("50 frames reach the map with at most one setData per animation frame, newest last", async () => {
    const source = new FakeEntitySource();
    renderMap(source);
    await ready(map());
    await settle();
    const before = map().entities.setDataCalls;
    act(() => {
      for (let i = 1; i <= 50; i++) source.push(frameWith(i, 1, i));
    });
    await settle();
    const calls = map().entities.setDataCalls - before;
    expect(calls).toBeGreaterThanOrEqual(1);
    expect(calls).toBeLessThanOrEqual(2); // 50 synchronous frames, 2 animation frames waited
    expect(map().entities.serialise().features).toHaveLength(50);
  });

  test("the render count is constant over 100 frames of 10,000 entities", async () => {
    const source = new FakeEntitySource();
    let commits = 0;
    render(
      wrap(
        <Profiler id="map" onRender={() => commits++}>
          <LiveMap source={source} />
        </Profiler>,
      ),
    );
    await ready(map());
    await act(async () => source.push(frameWith(10_000, 1, 0)));
    await settle();
    const settled = commits;
    await act(async () => {
      for (let i = 1; i <= 100; i++) source.push(frameWith(10_000, 1, i));
    });
    await settle();
    expect(commits).toBe(settled);
    expect(FakeMap.instances).toHaveLength(1);
  }, 30_000);

  test("a theme change calls setTheme on the same view; no new map; the mount applies nothing", async () => {
    const { ref, Probe } = themeSwitch();
    render(
      wrap(
        <>
          <Probe />
          <LiveMap source={new FakeEntitySource()} />
        </>,
      ),
    );
    await ready(map());
    expect(map().paint).toHaveLength(6); // the load applied the initial theme once; the mount did not add another
    map().paint.length = 0;
    act(() => ref.set?.("light"));
    expect(FakeMap.instances).toHaveLength(1);
    expect(map().paint).toContainEqual({
      layer: "graticule",
      property: "line-color",
      value: "#000004",
    });
    expect(map().paint).toContainEqual({
      layer: "graticule",
      property: "line-opacity",
      value: 0.55,
    });
  });

  test("the camera fits the first non-empty frame of each session and otherwise stays put", () => {
    const source = new FakeEntitySource();
    renderMap(source);
    act(() => source.push(frameWith(2, 1, 0)));
    expect(map().fits).toHaveLength(1);
    expect(map().fits[0]?.options).toEqual({
      padding: { top: 64, right: 64, bottom: 64, left: 64 },
      maxZoom: 15,
      animate: false,
    });
    act(() => source.push(frameWith(3, 1, 1)));
    expect(map().fits).toHaveLength(1);
    act(() => source.push(frameWith(0, 2, 0)));
    expect(map().fits).toHaveLength(1); // empty first frame of session 2 waits
    act(() => source.push(frameWith(2, 2, 1)));
    expect(map().fits).toHaveLength(2);
    act(() => source.push(frameWith(4, 2, 2)));
    expect(map().fits).toHaveLength(2);
  });

  test("a fit that could not happen (zero-size container) is retried on the next frame", () => {
    const source = new FakeEntitySource();
    renderMap(source);
    map().camera = undefined;
    act(() => source.push(frameWith(2, 1, 0)));
    act(() => source.push(frameWith(2, 1, 1)));
    expect(map().fits).toHaveLength(0);
    map().camera = { center: [0, 0], zoom: 5 };
    act(() => source.push(frameWith(2, 1, 2)));
    expect(map().fits).toHaveLength(1);
    act(() => source.push(frameWith(2, 1, 3)));
    expect(map().fits).toHaveLength(1); // fitted once it worked
  });

  test("insets add to the fit padding on each side", () => {
    const source = new FakeEntitySource();
    renderMap(source, <LiveMap source={source} insets={{ top: 80, left: 12 }} />);
    act(() => source.push(frameWith(2, 1, 0)));
    expect(map().fits[0]?.options).toEqual({
      padding: { top: 144, right: 64, bottom: 64, left: 76 },
      maxZoom: 15,
      animate: false,
    });
  });

  test("the latest insets are used at fit time without remounting the map", () => {
    const source = new FakeEntitySource();
    const view = renderMap(source, <LiveMap source={source} insets={{ top: 10 }} />);
    view.rerender(wrap(<LiveMap source={source} insets={{ top: 90 }} />));
    expect(FakeMap.instances).toHaveLength(1);
    act(() => source.push(frameWith(2, 1, 0)));
    expect(map().fits[0]?.options).toMatchObject({ padding: { top: 154 } });
  });

  test("the error banner sits below the top inset", () => {
    console.error = () => undefined;
    setTokens("affil-hostile");
    const source = new FakeEntitySource();
    renderMap(source, <LiveMap source={source} insets={{ top: 80 }} />);
    const banner = screen.getByRole("alert").parentElement;
    expect(banner).toHaveClass("umbra-live-map-error");
    expect(banner?.style.top).toBe("calc(80px + var(--space-4))");
  });

  test("unreadable colours show the colour error and create no map", () => {
    console.error = () => undefined;
    setTokens("affil-hostile");
    renderMap(new FakeEntitySource());
    expect(screen.getByRole("alert")).toHaveTextContent("The map could not start");
    expect(screen.getByRole("alert")).toHaveTextContent("theme colours");
    expect(FakeMap.instances).toHaveLength(0);
  });

  test("a colour error at mount recovers on the next theme change", () => {
    console.error = () => undefined;
    setTokens("affil-hostile");
    const { ref, Probe } = themeSwitch();
    render(
      wrap(
        <>
          <Probe />
          <LiveMap source={new FakeEntitySource()} />
        </>,
      ),
    );
    expect(FakeMap.instances).toHaveLength(0);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    setTokens();
    act(() => ref.set?.("light"));
    expect(FakeMap.instances).toHaveLength(1);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("a colour error at mount does not retry by itself, only on a theme change", () => {
    const logged: unknown[][] = [];
    console.error = (...args: unknown[]) => logged.push(args);
    setTokens("affil-hostile");
    renderMap(new FakeEntitySource());
    expect(FakeMap.instances).toHaveLength(0);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(logged).toHaveLength(1); // one attempt, not a retry loop
  });

  test("unmounting cancels a pending count announcement", async () => {
    const source = new FakeEntitySource();
    const { unmount } = renderMap(source);
    await act(async () => source.push(frameWith(3)));
    expect(clock.pending).toBe(1);
    unmount();
    expect(clock.pending).toBe(0);
  });

  test("a device without WebGL shows the WebGL message, not the colour message", () => {
    console.error = () => undefined;
    FakeMap.failNext = new Error("Failed to initialize WebGL");
    renderMap(new FakeEntitySource());
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("The map needs WebGL, which this device does not provide.");
    expect(alert).not.toHaveTextContent("theme colours");
    expect(FakeMap.instances).toHaveLength(0);
  });

  test("a lost WebGL context shows the error state and clears when it is restored", async () => {
    console.error = () => undefined;
    renderMap(new FakeEntitySource());
    await ready(map());
    expect(screen.queryByRole("alert")).toBeNull();
    act(() => map().emit("webglcontextlost"));
    expect(screen.getByRole("alert")).toHaveTextContent("The map lost its graphics context");
    act(() => map().emit("webglcontextrestored"));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(map().removed).toBe(false);
  });

  test("a context restore does not clear a colour error", async () => {
    console.error = () => undefined;
    const { ref, Probe } = themeSwitch();
    render(
      wrap(
        <>
          <Probe />
          <LiveMap source={new FakeEntitySource()} />
        </>,
      ),
    );
    await ready(map());
    setTokens("line");
    act(() => ref.set?.("light"));
    expect(screen.getByRole("alert")).toHaveTextContent("theme colours");
    act(() => map().emit("webglcontextrestored"));
    expect(screen.getByRole("alert")).toHaveTextContent("theme colours");
  });

  test("a colour error on a theme change shows the error state and the map stays; a good change clears it", async () => {
    console.error = () => undefined;
    const { ref, Probe } = themeSwitch();
    render(
      wrap(
        <>
          <Probe />
          <LiveMap source={new FakeEntitySource()} />
        </>,
      ),
    );
    await ready(map());
    expect(screen.queryByRole("alert")).toBeNull();
    setTokens("line");
    act(() => ref.set?.("light"));
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(map().removed).toBe(false);
    setTokens();
    act(() => ref.set?.("dark"));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("a style URL option reaches the view", () => {
    renderMap(
      new FakeEntitySource(),
      <LiveMap source={new FakeEntitySource()} basemapStyleUrl="https://t.test/s.json" />,
    );
    expect(map().options.style).toBe("https://t.test/s.json");
  });

  test("the e2e global is absent outside the e2e build", () => {
    renderMap(new FakeEntitySource());
    expect(Reflect.has(window, "__UMBRA_E2E__")).toBe(false);
  });
});
