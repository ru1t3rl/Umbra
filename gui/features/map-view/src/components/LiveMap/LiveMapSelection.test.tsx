// WI-9 (ADR 0078, ASM-802, ASM-803, ASM-933): LiveMap selection, label, centerOn and affiliationOf, with
// maplibre mocked as in LiveMap.test.tsx. Written before the implementation. Contracts pinned here:
// - the entity layer id is "entities", the selection source id is "selection" (one feature at most);
// - the label is one DOM element showing the name or "#id", positioned with `style.transform`
//   `translate(Xpx, Ypx)` from `map.project` (any constant offset is fine: tests compare deltas);
// - a click calls `map.queryRenderedFeatures([[x-6, y-6], [x+6, y+6]], { layers: ["entities"] })` once;
// - Escape (keydown) inside the map region calls onSelect(null); the map is keyboard focusable.
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { defaultAffiliationOf } from "@umbra/settings";
import { ThemeProvider } from "@umbra/ui-kit";
import { createRef, Profiler, type ReactElement, type RefObject } from "react";
import { AnnounceTimersContext } from "../../context/AnnounceTimersContext.ts";
import { FakeEntitySource, FakeMap, fakeRasterizer, frameWith } from "../../maplibre/testing.ts";
import type { Timers } from "../../model/announce.ts";
import { entity, frameOf } from "../../model/testing.ts";
import type { LiveMapHandle } from "./LiveMap.tsx";

const raster = fakeRasterizer();
mock.module("maplibre-gl", () => ({ Map: FakeMap }));
mock.module("../../maplibre/rasterize.ts", () => ({ rasterizeSvg: mock(raster.rasterize) }));
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
  azure: "#00000b", // the selection ring colour (human decision 2026-10-23, M8 WI-9 review)
};
const nextFrame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
const settle = async () => {
  await act(async () => {
    await nextFrame();
    await nextFrame();
  });
};
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
/** The single-feature selection source, created by the style next to the entity source. */
const selectionSource = () => {
  const s = map().sources.get("selection");
  if (!s) throw new Error('no "selection" source on the map');
  return s;
};
const selectionFeatures = () =>
  selectionSource().serialise().features as {
    id: number;
    geometry: { coordinates: [number, number] };
    properties: Record<string, unknown>;
  }[];

const timers: Timers<number> = { setTimer: () => 0, clearTimer: () => undefined };
const wrap = (element: ReactElement) => (
  <AnnounceTimersContext.Provider value={timers}>
    <ThemeProvider defaultPreference="dark">{element}</ThemeProvider>
  </AnnounceTimersContext.Provider>
);

type Hit = { id: number; geometry: { type: string; coordinates: number[] } };
const hit = (id: number): Hit => {
  const e = entity(id);
  return { id, geometry: { type: "Point", coordinates: [e.lon_deg, e.lat_deg] } };
};
/** Pixel of an entity under FakeMap.project (lon * 10, lat * 10). */
const pixel = (id: number) => {
  const e = entity(id);
  return { x: e.lon_deg * 10, y: e.lat_deg * 10 };
};
/** Replaces the map's hit test with a recorder returning `result`. */
function spyQuery(m: FakeMap, result: Hit[]) {
  const calls: unknown[][] = [];
  m.queryRenderedFeatures = ((...args: unknown[]) => {
    calls.push(args);
    return result;
  }) as FakeMap["queryRenderedFeatures"];
  return calls;
}
const click = (m: FakeMap, x: number, y: number) =>
  act(() => {
    m.emit("click", { point: { x, y }, lngLat: { lng: x / 10, lat: y / 10 } } as never);
  });
const translateOf = (el: HTMLElement) => {
  const m = /translate(?:3d)?\(\s*(-?[\d.]+)px,\s*(-?[\d.]+)px/.exec(el.style.transform);
  if (!m) throw new Error(`label has no translate(Xpx, Ypx): "${el.style.transform}"`);
  return { x: Number(m[1]), y: Number(m[2]) };
};
const canvasOf = () => {
  const region = screen.getByRole("region", { name: "Live map" });
  const canvas = region.querySelector<HTMLElement>(".umbra-live-map-canvas");
  if (!canvas) throw new Error("no map canvas container");
  return { region, canvas };
};

beforeEach(() => {
  FakeMap.reset();
  localStorage.clear();
  for (const [k, v] of Object.entries(TOKENS))
    document.documentElement.style.setProperty(`--${k}`, v);
});
afterEach(cleanup);

/** A mounted, loaded map showing `count` entities. */
async function mounted(
  props: Partial<React.ComponentProps<typeof LiveMap>> = {},
  count = 5,
  source = new FakeEntitySource(),
) {
  const view = render(wrap(<LiveMap source={source} {...props} />));
  await ready(map());
  await act(async () => source.push(frameWith(count, 1, 0)));
  await settle();
  const again = (next: Partial<React.ComponentProps<typeof LiveMap>>) =>
    view.rerender(wrap(<LiveMap source={source} {...props} {...next} />));
  return { source, view, again };
}

describe("LiveMap click selection (ASM-803)", () => {
  test("a click runs one hit test with a 12 px box around the point, on the entity layer only", async () => {
    await mounted({ onSelect: () => undefined });
    const calls = spyQuery(map(), [hit(1)]);
    click(map(), 300, 200);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.[0]).toEqual([
      [294, 194],
      [306, 206],
    ]);
    expect(calls[0]?.[1]).toMatchObject({ layers: ["entities"] });
  });

  test("calls onSelect once with the id of the feature nearest to the click", async () => {
    const picked: (number | null)[] = [];
    await mounted({ onSelect: (id) => picked.push(id) });
    spyQuery(map(), [hit(1), hit(2)]);
    const p2 = pixel(2);
    click(map(), p2.x + 0.001, p2.y);
    expect(picked).toEqual([2]);
    spyQuery(map(), [hit(2), hit(1)]); // order of the hits does not matter
    const p1 = pixel(1);
    click(map(), p1.x, p1.y - 0.001);
    expect(picked).toEqual([2, 1]);
  });

  test("a feature returned twice still selects it once", async () => {
    const picked: (number | null)[] = [];
    await mounted({ onSelect: (id) => picked.push(id) });
    spyQuery(map(), [hit(3), hit(3)]);
    click(map(), pixel(3).x, pixel(3).y);
    expect(picked).toEqual([3]);
  });

  test("a click on empty map calls onSelect(null)", async () => {
    const picked: (number | null)[] = [];
    await mounted({ selectedId: 2, onSelect: (id) => picked.push(id) });
    spyQuery(map(), []);
    click(map(), 10, 10);
    expect(picked).toEqual([null]);
  });

  test("Escape in the map region calls onSelect(null); other keys do nothing", async () => {
    const picked: (number | null)[] = [];
    await mounted({ selectedId: 2, onSelect: (id) => picked.push(id) });
    const { canvas } = canvasOf();
    fireEvent.keyDown(canvas, { key: "a" });
    fireEvent.keyDown(canvas, { key: "Enter" });
    expect(picked).toEqual([]);
    fireEvent.keyDown(canvas, { key: "Escape" });
    expect(picked).toEqual([null]);
  });

  test("the focus target is the MapLibre canvas (tabindex 0); the wrapper and the region are not tab stops", async () => {
    await mounted({ onSelect: () => undefined });
    const { region, canvas } = canvasOf();
    const inner = canvas.querySelector("canvas");
    expect(inner).not.toBeNull();
    expect(inner?.tabIndex).toBe(0);
    expect(canvas.hasAttribute("tabindex")).toBe(false);
    expect(region.hasAttribute("tabindex")).toBe(false);
  });

  test("Escape keydown on the MapLibre canvas bubbles to the map and calls onSelect(null)", async () => {
    const picked: (number | null)[] = [];
    await mounted({ selectedId: 2, onSelect: (id) => picked.push(id) });
    const inner = canvasOf().canvas.querySelector("canvas");
    if (!inner) throw new Error("no map canvas");
    fireEvent.keyDown(inner, { key: "Escape" });
    expect(picked).toEqual([null]);
  });

  test("Escape with nothing selected does not call onSelect", async () => {
    const picked: (number | null)[] = [];
    await mounted({ onSelect: (id) => picked.push(id) });
    const inner = canvasOf().canvas.querySelector("canvas");
    if (!inner) throw new Error("no map canvas");
    fireEvent.keyDown(inner, { key: "Escape" });
    fireEvent.keyDown(canvasOf().canvas, { key: "Escape" });
    expect(picked).toEqual([]);
  });

  test("a missing --azure token gives the colours error state and no map (no silent fallback)", () => {
    const logged = console.error;
    console.error = () => undefined;
    document.documentElement.style.removeProperty("--azure");
    render(wrap(<LiveMap source={new FakeEntitySource()} />));
    console.error = logged;
    expect(screen.getByRole("alert")).toHaveTextContent("The map could not start");
    expect(screen.getByRole("alert")).toHaveTextContent("theme colours");
    expect(FakeMap.instances).toHaveLength(0);
  });

  test("onSelect is not called by mounting, by frames or by a selectedId change", async () => {
    const picked: (number | null)[] = [];
    const { source, again } = await mounted({ onSelect: (id) => picked.push(id) });
    again({ selectedId: 2 });
    await act(async () => source.push(frameWith(5, 1, 1)));
    await settle();
    expect(picked).toEqual([]);
  });

  test("clicking works without onSelect and uses the newest onSelect after a re-render", async () => {
    const first: (number | null)[] = [];
    const second: (number | null)[] = [];
    const { again } = await mounted({ onSelect: (id) => first.push(id) });
    again({ onSelect: (id) => second.push(id) });
    expect(FakeMap.instances).toHaveLength(1); // a new callback never remounts the map
    spyQuery(map(), [hit(1)]);
    click(map(), pixel(1).x, pixel(1).y);
    expect(first).toEqual([]);
    expect(second).toEqual([1]);
    again({ onSelect: undefined });
    expect(() => click(map(), pixel(1).x, pixel(1).y)).not.toThrow();
  });
});

describe("LiveMap selection source (ADR 0078)", () => {
  test("holds the selected entity only, at its current position", async () => {
    await mounted({ selectedId: 3 });
    expect(selectionFeatures()).toHaveLength(1);
    expect(selectionFeatures()[0]?.id).toBe(3);
    expect(selectionFeatures()[0]?.geometry.coordinates).toEqual([20.003, 10.003]);
    expect(map().entities.serialise().features).toHaveLength(5);
  });

  test("is empty without a selection, for an id that is not in the frame, and when it is cleared", async () => {
    // Never written while nothing is selected (ASM-935 c): "empty" is an unwritten or empty source.
    const shown = () =>
      (selectionSource().lastRef as { features: unknown[] } | undefined)?.features.length ?? 0;
    const { again } = await mounted({});
    expect(selectionSource().setDataCalls).toBe(0);
    expect(shown()).toBe(0);
    again({ selectedId: 99 });
    await settle();
    expect(shown()).toBe(0);
    again({ selectedId: 2 });
    await settle();
    expect(selectionFeatures()).toHaveLength(1);
    again({ selectedId: null });
    await settle();
    expect(selectionFeatures()).toHaveLength(0);
  });

  test("changing selectedId moves the single feature without remounting and without a new frame", async () => {
    const { again } = await mounted({ selectedId: 1 });
    again({ selectedId: 4 });
    await settle();
    expect(FakeMap.instances).toHaveLength(1);
    expect(selectionFeatures().map((f) => f.id)).toEqual([4]);
  });

  test("is updated in the same flush as the entities: equal setData counts, newest position", async () => {
    const { source } = await mounted({ selectedId: 2 });
    const entitiesBefore = map().entities.setDataCalls;
    const selectionBefore = selectionSource().setDataCalls;
    const moved = frameOf([entity(1), entity(2, { lon_deg: 30, lat_deg: 40 }), entity(3)]);
    act(() => {
      source.push({ ...moved, seq: 1, tick: 2 });
      source.push({ ...moved, seq: 2, tick: 3 });
      source.push({ ...moved, seq: 3, tick: 4 });
    });
    // Nothing is written between animation frames.
    expect(map().entities.setDataCalls).toBe(entitiesBefore);
    expect(selectionSource().setDataCalls).toBe(selectionBefore);
    await settle();
    const entityCalls = map().entities.setDataCalls - entitiesBefore;
    expect(entityCalls).toBeGreaterThanOrEqual(1);
    expect(selectionSource().setDataCalls - selectionBefore).toBe(entityCalls);
    expect(selectionFeatures()[0]?.geometry.coordinates).toEqual([30, 40]);
    expect(selectionFeatures()).toHaveLength(1);
  });

  test("an entity that leaves the frame empties the selection source", async () => {
    const { source } = await mounted({ selectedId: 5 });
    expect(selectionFeatures()).toHaveLength(1);
    await act(async () => source.push(frameWith(3, 1, 1)));
    await settle();
    expect(selectionFeatures()).toHaveLength(0);
  });
});

describe("LiveMap selection label (ASM-802)", () => {
  test("shows #id for an entity without a name", async () => {
    await mounted({ selectedId: 3 });
    expect(screen.getByText("#3")).toBeInTheDocument();
  });

  test("shows the name when the entity has one", async () => {
    const source = new FakeEntitySource();
    render(wrap(<LiveMap source={source} selectedId={2} />));
    await ready(map());
    await act(async () =>
      source.push(frameOf([entity(1), entity(2, { name: "Alpha" }), entity(3)])),
    );
    await settle();
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.queryByText("#2")).toBeNull();
  });

  test("is absent without a selection, and when the selected entity is not in the frame", async () => {
    const { again } = await mounted({}, 3);
    expect(screen.queryByText(/^#\d+$/)).toBeNull();
    again({ selectedId: 99 });
    await settle();
    expect(screen.queryByText(/^#\d+$/)).toBeNull();
  });

  test("is one node, kept across flushes, and moves by the entity's pixel delta once per flush", async () => {
    const { source } = await mounted({ selectedId: 2 });
    const label = screen.getByText("#2");
    const before = translateOf(label);
    expect(before.x).toBeCloseTo(pixel(2).x, 1);
    const moved = frameOf([entity(1), entity(2, { lon_deg: 21, lat_deg: 11 }), entity(3)]);
    await act(async () => source.push({ ...moved, seq: 9, tick: 9 }));
    await settle();
    expect(screen.getByText("#2")).toBe(label); // same element, not re-created
    const after = translateOf(label);
    expect(after.x - before.x).toBeCloseTo(21 * 10 - pixel(2).x, 1);
    expect(after.y - before.y).toBeCloseTo(11 * 10 - pixel(2).y, 1);
  });

  test("does not move between animation frames, however many frames arrive", async () => {
    const { source } = await mounted({ selectedId: 2 });
    const label = screen.getByText("#2");
    const before = label.style.transform;
    const moved = frameOf([entity(1), entity(2, { lon_deg: 25, lat_deg: 12 }), entity(3)]);
    act(() => {
      for (let i = 0; i < 20; i++) source.push({ ...moved, seq: 20 + i, tick: 20 + i });
    });
    expect(label.style.transform).toBe(before);
    await settle();
    expect(label.style.transform).not.toBe(before);
  });

  test("follows the selection when selectedId changes", async () => {
    const { again } = await mounted({ selectedId: 1 });
    again({ selectedId: 4 });
    await settle();
    expect(screen.queryByText("#1")).toBeNull();
    expect(translateOf(screen.getByText("#4")).x).toBeCloseTo(pixel(4).x, 1);
  });
});

describe("LiveMap handle: centerOn", () => {
  /** Records any camera move, whichever MapLibre call the implementation uses. */
  function spyCamera(m: FakeMap) {
    const moves: [number, number][] = [];
    const take = (arg: unknown) => {
      const c = Array.isArray(arg)
        ? arg
        : (arg as { center?: unknown; lng?: number; lat?: number }).center;
      if (Array.isArray(c)) moves.push([c[0] as number, c[1] as number]);
      else if (c && typeof c === "object") {
        const ll = c as { lng: number; lat: number };
        moves.push([ll.lng, ll.lat]);
      }
    };
    for (const name of ["easeTo", "flyTo", "jumpTo", "panTo", "setCenter"]) {
      Reflect.set(m, name, (arg: unknown) => take(arg));
    }
    return moves;
  }
  const handleOf = (ref: RefObject<LiveMapHandle | null>) => {
    if (!ref.current) throw new Error("LiveMap did not set its ref");
    return ref.current;
  };

  test("returns false for an unknown id and leaves the camera alone", async () => {
    const ref = createRef<LiveMapHandle>();
    const source = new FakeEntitySource();
    render(wrap(<LiveMap source={source} ref={ref} />));
    await ready(map());
    await act(async () => source.push(frameWith(3, 1, 0)));
    await settle();
    const moves = spyCamera(map());
    expect(handleOf(ref).centerOn(99)).toBe(false);
    expect(moves).toHaveLength(0);
  });

  test("returns false before any frame has arrived", async () => {
    const ref = createRef<LiveMapHandle>();
    render(wrap(<LiveMap source={new FakeEntitySource()} ref={ref} />));
    await ready(map());
    const moves = spyCamera(map());
    expect(handleOf(ref).centerOn(1)).toBe(false);
    expect(moves).toHaveLength(0);
  });

  test("returns true for a known id and moves the camera once to its longitude and latitude", async () => {
    const ref = createRef<LiveMapHandle>();
    const source = new FakeEntitySource();
    render(wrap(<LiveMap source={source} ref={ref} />));
    await ready(map());
    await act(async () => source.push(frameWith(3, 1, 0)));
    await settle();
    const moves = spyCamera(map());
    expect(handleOf(ref).centerOn(2)).toBe(true);
    expect(moves).toEqual([[20.002, 10.002]]);
  });

  test("uses the newest frame: false once the entity has left it", async () => {
    const ref = createRef<LiveMapHandle>();
    const source = new FakeEntitySource();
    render(wrap(<LiveMap source={source} ref={ref} />));
    await ready(map());
    await act(async () => source.push(frameWith(3, 1, 0)));
    await settle();
    await act(async () => source.push(frameWith(1, 1, 1)));
    await settle();
    expect(handleOf(ref).centerOn(3)).toBe(false);
  });
});

describe("LiveMap affiliationOf", () => {
  const features = () =>
    map().entities.serialise().features as unknown as { properties: { affiliation: string } }[];

  test("is used for the entity features when given, with the entity's force", async () => {
    const seen: (string | null | undefined)[] = [];
    await mounted({
      affiliationOf: (force) => {
        seen.push(force);
        return "hostile";
      },
    });
    expect(seen).toContain("blue");
    expect(features().every((f) => f.properties.affiliation === "hostile")).toBe(true);
  });

  test("is used for the selection feature too", async () => {
    await mounted({ selectedId: 2, affiliationOf: () => "neutral" });
    expect(selectionFeatures()[0]?.properties.affiliation).toBe("neutral");
  });

  test("defaults to the settings default when not given", async () => {
    await mounted({});
    expect(features()[0]?.properties.affiliation).toBe(defaultAffiliationOf("blue"));
  });

  test("a new function applies from the next flush and never remounts the map", async () => {
    const { again, source } = await mounted({ affiliationOf: () => "friend" });
    again({ affiliationOf: () => "hostile" });
    expect(FakeMap.instances).toHaveLength(1);
    await act(async () => source.push(frameWith(5, 1, 1)));
    await settle();
    expect(features().every((f) => f.properties.affiliation === "hostile")).toBe(true);
  });
});

describe("LiveMap render count with a selection", () => {
  test("is constant over 100 frames of 10,000 entities with a selected entity", async () => {
    const source = new FakeEntitySource();
    let commits = 0;
    render(
      wrap(
        <Profiler id="map" onRender={() => commits++}>
          <LiveMap source={source} selectedId={5_000} onSelect={() => undefined} />
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
    expect(selectionFeatures()).toHaveLength(1);
  }, 30_000);
});
