// WI-9 review: view-level behaviour of the selection ring and label with the FakeMap. Written before the
// review fixes; each test says whether it fails now (new behaviour) or pins existing behaviour.
import { beforeEach, describe, expect, mock, test } from "bun:test";
import { COLORS, entity, frameOf } from "../model/testing.ts";
import { FakeMap, fakeRasterizer, ManualScheduler } from "./testing.ts";

mock.module("maplibre-gl", () => ({ Map: FakeMap }));
const { createMapLibreView } = await import("./createMapLibreView.ts");

const flushPromises = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
const loadMap = async (map: FakeMap) => {
  map.load();
  await flushPromises();
};

function setup(options: Partial<Parameters<typeof createMapLibreView>[1]> = {}) {
  const scheduler = new ManualScheduler();
  const raster = fakeRasterizer();
  const container = document.createElement("div");
  const view = createMapLibreView(container, {
    colors: COLORS,
    theme: "dark",
    scheduler,
    rasterize: raster.rasterize,
    pixelRatio: 2,
    ...options,
  });
  const map = FakeMap.instances[0] as FakeMap;
  const selection = () => {
    const s = map.sources.get("selection");
    if (!s) throw new Error('no "selection" source');
    return s;
  };
  const label = () => container.querySelector<HTMLElement>(".umbra-live-map-label");
  return { view, map, scheduler, raster, container, selection, label };
}
const frame = (n = 3, tick = 1) => ({
  ...frameOf(Array.from({ length: n }, (_, i) => entity(i + 1))),
  tick,
});
const translate = (el: HTMLElement | null) => {
  const m = /translate(?:3d)?\(\s*(-?[\d.]+)px,\s*(-?[\d.]+)px/.exec(el?.style.transform ?? "");
  if (!m) throw new Error(`no translate in "${el?.style.transform}"`);
  return { x: Number(m[1]), y: Number(m[2]) };
};

beforeEach(() => FakeMap.reset());

describe("selection ring layer at style load (m4)", () => {
  test("inline style: the selection source and layer exist once the style is loaded, with no selection", async () => {
    const { map } = setup();
    await loadMap(map);
    expect(map.sources.has("selection")).toBe(true);
    expect(map.layers.filter((l) => l === "selection")).toHaveLength(1); // fails now: added at the first selection
  });

  test("url style: the same, at load, after the entity layers", async () => {
    const { map } = setup({ basemap: { styleUrl: "https://tiles.test/s.json" } });
    await loadMap(map);
    expect(map.layers).toEqual(["direction", "entities", "selection"]); // fails now: added at the first selection
  });

  test.each([
    ["inline", {}],
    ["url", { basemap: { styleUrl: "https://tiles.test/s.json" } }],
  ])(
    "%s style: select, deselect and reselect never adds the source or layer twice",
    async (_n, extra) => {
      const { view, map, scheduler } = setup(extra);
      const adds: string[] = [];
      const addSource = map.addSource.bind(map);
      map.addSource = (id: string) => {
        adds.push(id);
        addSource(id);
      };
      await loadMap(map);
      view.setEntities(frame());
      scheduler.run();
      for (const id of [2, null, 3, null, 2]) {
        view.setSelection(id);
        scheduler.run();
      }
      expect(map.layers.filter((l) => l === "selection")).toHaveLength(1);
      expect(adds.filter((a) => a === "selection").length).toBeLessThanOrEqual(1);
    },
  );
});

describe("selection source writes (m6)", () => {
  test("frames with no selection never write the selection source", async () => {
    const { view, map, scheduler, selection } = setup();
    await loadMap(map);
    for (let i = 1; i <= 5; i++) {
      view.setEntities(frame(3, i));
      scheduler.run();
    }
    expect(map.entities.setDataCalls).toBe(5);
    expect(selection().setDataCalls).toBe(0); // fails now: written empty on every flush
  });

  test("clearing writes the empty collection once, then later frames write nothing", async () => {
    const { view, map, scheduler, selection } = setup({ selectedId: 2 });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    const withSelection = selection().setDataCalls;
    expect(withSelection).toBeGreaterThanOrEqual(1);
    view.setSelection(null);
    scheduler.run();
    expect(selection().setDataCalls).toBe(withSelection + 1);
    expect(selection().serialise().features).toHaveLength(0);
    for (let i = 2; i <= 4; i++) {
      view.setEntities(frame(3, i));
      scheduler.run();
    }
    expect(selection().setDataCalls).toBe(withSelection + 1);
  });

  test("while selected, each flush writes the selection source once (it follows the entity)", async () => {
    const { view, map, scheduler, selection } = setup({ selectedId: 2 });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    const before = selection().setDataCalls;
    view.setEntities(frame(3, 2));
    scheduler.run();
    expect(selection().setDataCalls).toBe(before + 1); // existing behaviour
  });
});

describe("label follows the camera (m5)", () => {
  test("a map move repositions the label in the same handler, with no animation frame", async () => {
    const { view, map, scheduler, label } = setup({ selectedId: 2 });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    const before = translate(label());
    map.project = ([lon, lat]) => ({ x: lon * 10 + 100, y: lat * 10 - 50 });
    map.emit("move");
    expect(scheduler.pending).toBe(0); // fails now: the move schedules a frame
    const after = translate(label());
    expect(after.x - before.x).toBeCloseTo(100, 1); // fails now: still the old position
    expect(after.y - before.y).toBeCloseTo(-50, 1);
  });

  test("two moves leave the label at the latest position", async () => {
    const { view, map, scheduler, label } = setup({ selectedId: 2 });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    map.project = ([lon, lat]) => ({ x: lon * 10 + 10, y: lat * 10 });
    map.emit("move");
    map.project = ([lon, lat]) => ({ x: lon * 10 + 70, y: lat * 10 });
    map.emit("move");
    scheduler.run();
    expect(translate(label()).x).toBeCloseTo(entity(2).lon_deg * 10 + 70, 1);
  });

  test("a move without a selection creates no label and schedules nothing", async () => {
    const { view, map, scheduler, label } = setup();
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    map.emit("move");
    expect(label()).toBeNull();
    expect(scheduler.pending).toBe(0);
  });
});

describe("selection ring image", () => {
  const ringOps = (map: FakeMap) => map.imageOps.filter((o) => o.id === "umbra-selection");

  test("no selection ever: the nine load-time images only (pins the frozen counts)", async () => {
    const { view, map, scheduler } = setup();
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    await flushPromises();
    expect(map.imageOps).toHaveLength(9);
    expect(ringOps(map)).toHaveLength(0);
  });

  test("the first flush with a selection registers the ring image from the selection colour, at the pixel ratio", async () => {
    const { view, map, scheduler, raster } = setup({ selectedId: 2 });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    await flushPromises();
    const ops = ringOps(map);
    expect(ops).toHaveLength(1); // fails now: no ring image
    expect(ops[0]?.op).toBe("add");
    expect(ops[0]?.pixelRatio).toBe(2);
    expect(raster.svgOf(ops[0]?.image)).toContain(COLORS.selection);
  });

  test("a second selection does not register it again", async () => {
    const { view, map, scheduler } = setup({ selectedId: 2 });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    await flushPromises();
    for (const id of [null, 3, 1]) {
      view.setSelection(id);
      scheduler.run();
      await flushPromises();
    }
    expect(ringOps(map)).toHaveLength(1);
  });

  test("a theme change updates the ring image with the new selection colour", async () => {
    const { view, map, scheduler, raster } = setup({ selectedId: 2 });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    await flushPromises();
    map.imageOps.length = 0;
    view.setTheme({ ...COLORS, selection: "AZURE-DAY" }, "light");
    await flushPromises();
    const ops = ringOps(map);
    expect(ops).toHaveLength(1); // fails now: the ring is a paint property, not an image
    expect(ops[0]?.op).toBe("update");
    expect(raster.svgOf(ops[0]?.image)).toContain("AZURE-DAY");
  });

  test("a theme change before any selection leaves the ring image out; a later selection uses the new colour", async () => {
    const { view, map, scheduler, raster } = setup();
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    await flushPromises();
    view.setTheme({ ...COLORS, selection: "AZURE-DAY" }, "light");
    await flushPromises();
    expect(ringOps(map)).toHaveLength(0);
    view.setSelection(2);
    scheduler.run();
    await flushPromises();
    const ops = ringOps(map);
    expect(ops).toHaveLength(1); // fails now
    expect(raster.svgOf(ops[0]?.image)).toContain("AZURE-DAY");
  });
});

describe("click before the style is ready (m7)", () => {
  test("runs no hit test and does not call onSelect", () => {
    const picked: (number | null)[] = [];
    const { map } = setup({ onSelect: (id) => picked.push(id) });
    let queries = 0;
    map.queryRenderedFeatures = () => {
      queries += 1;
      return [];
    };
    map.emit("click", { point: { x: 10, y: 10 } } as never);
    expect(queries).toBe(0); // fails now
    expect(picked).toEqual([]);
  });

  test("after the style loads the same click runs the hit test", async () => {
    const picked: (number | null)[] = [];
    const { map } = setup({ onSelect: (id) => picked.push(id) });
    await loadMap(map);
    map.queryRenderedFeatures = () => [];
    map.emit("click", { point: { x: 10, y: 10 } } as never);
    expect(picked).toEqual([null]);
  });
});

describe("destroy", () => {
  test("removes the label element and cancels the pending flush and label work", async () => {
    const { view, map, scheduler, container, label } = setup({ selectedId: 2 });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    expect(label()).not.toBeNull();
    view.setEntities(frame(3, 2)); // a flush is pending
    map.emit("move");
    view.destroy();
    expect(container.querySelector(".umbra-live-map-label")).toBeNull();
    expect(scheduler.pending).toBe(0);
    expect(scheduler.cancelled.length).toBeGreaterThanOrEqual(1);
    const writes = map.entities.setDataCalls;
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(writes);
  });
});

const pressEscape = (target: Element) =>
  target.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

describe("Escape handling (follow-ups of the WI-9 re-review)", () => {
  test("Escape on the MapLibre canvas clears the selection (control for the cases below)", async () => {
    const picked: (number | null)[] = [];
    const { map, container } = setup({ selectedId: 2, onSelect: (id) => picked.push(id) });
    await loadMap(map);
    const canvas = container.querySelector("canvas");
    if (!canvas) throw new Error("no canvas");
    pressEscape(canvas);
    expect(picked).toEqual([null]);
  });

  test("after destroy the keydown listener is gone: Escape on the container calls nothing", async () => {
    const picked: (number | null)[] = [];
    const { view, map, container } = setup({ selectedId: 2, onSelect: (id) => picked.push(id) });
    await loadMap(map);
    view.destroy();
    pressEscape(container);
    const canvas = container.querySelector("canvas");
    if (canvas) pressEscape(canvas);
    expect(picked).toEqual([]); // passes now: pins removeEventListener
  });

  test("Escape from another MapLibre control (a button in the container) does not clear the selection", async () => {
    const picked: (number | null)[] = [];
    const { map, container } = setup({ selectedId: 2, onSelect: (id) => picked.push(id) });
    await loadMap(map);
    const control = document.createElement("button");
    container.append(control);
    pressEscape(control);
    expect(picked).toEqual([]); // fails now: any bubbling Escape clears
  });
});

describe("ring image race", () => {
  test("two theme changes whose rasterisations finish in reverse order leave the newest colour", async () => {
    const raster = fakeRasterizer();
    const held: { svg: string; release: () => void }[] = [];
    let hold = false;
    const rasterize = (svg: string, size: number, ratio: number) =>
      hold && svg.includes("AZURE-")
        ? new Promise<Awaited<ReturnType<typeof raster.rasterize>>>((resolve) => {
            held.push({ svg, release: () => resolve(raster.rasterize(svg, size, ratio)) });
          })
        : raster.rasterize(svg, size, ratio);
    const { view, map, scheduler } = setup({ selectedId: 2, rasterize });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    await flushPromises();
    hold = true;
    map.imageOps.length = 0;
    view.setTheme({ ...COLORS, selection: "AZURE-A" }, "light");
    view.setTheme({ ...COLORS, selection: "AZURE-B" }, "dark");
    expect(held.map((h) => h.svg.includes("AZURE-A"))).toEqual([true, false]);
    for (const h of [...held].reverse()) h.release(); // B finishes first, A last
    await flushPromises();
    const ring = map.imageOps.filter((o) => o.id === "umbra-selection");
    const last = ring.at(-1);
    expect(last).toBeDefined();
    expect(raster.svgOf(last?.image)).toContain("AZURE-B");
  });
});

describe("first selection and the ring image", () => {
  /** Rasterizer that holds the ring image (the only svg with the selection colour) until `release`. */
  function gatedRing() {
    const raster = fakeRasterizer();
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    return {
      raster,
      release: () => release(),
      rasterize: async (svg: string, size: number, ratio: number) => {
        if (svg.includes(COLORS.selection)) await gate;
        return raster.rasterize(svg, size, ratio);
      },
    };
  }
  /** Records, per selection write, whether the ring image was installed; emits MapLibre's missing-image event. */
  function watch(map: FakeMap) {
    const writes: { features: number; imageInstalled: boolean }[] = [];
    const source = map.sources.get("selection");
    if (!source) throw new Error('no "selection" source');
    const setData = source.setData.bind(source);
    source.setData = (data: unknown) => {
      const features = (data as { features: unknown[] }).features.length;
      const imageInstalled = map.hasImage("umbra-selection");
      writes.push({ features, imageInstalled });
      if (features > 0 && !imageInstalled) map.emit("styleimagemissing");
      return setData(data);
    };
    return writes;
  }

  test("until the ring image is installed no feature is written; the label shows at once", async () => {
    const ring = gatedRing();
    const { view, map, scheduler, label } = setup({ selectedId: 2, rasterize: ring.rasterize });
    await loadMap(map);
    const writes = watch(map);
    view.setEntities(frame());
    scheduler.run();
    await flushPromises();
    expect(label()?.textContent).toBe("#2");
    expect(writes.filter((w) => w.features > 0)).toHaveLength(0); // fails now: written before the image
    expect(view.inspect().missingImages()).toBe(0);
  });

  test("once the image is installed the next flush writes the feature, and no image is ever missing", async () => {
    const ring = gatedRing();
    const { view, map, scheduler } = setup({ selectedId: 2, rasterize: ring.rasterize });
    await loadMap(map);
    const writes = watch(map);
    view.setEntities(frame());
    scheduler.run();
    ring.release();
    await flushPromises();
    scheduler.run(); // a flush scheduled by the install, if any
    view.setEntities(frame(3, 2));
    scheduler.run();
    const featureWrites = writes.filter((w) => w.features > 0);
    expect(featureWrites.length).toBeGreaterThanOrEqual(1);
    expect(featureWrites.every((w) => w.imageInstalled)).toBe(true); // fails now
    expect(view.inspect().missingImages()).toBe(0);
  });

  test("url style: after the ring is installed one frame makes exactly one selection write, on the same source", async () => {
    const { view, map, scheduler } = setup({
      selectedId: 2,
      basemap: { styleUrl: "https://tiles.test/s.json" },
    });
    await loadMap(map);
    view.setEntities(frame());
    scheduler.run();
    await flushPromises();
    scheduler.run();
    const source = map.sources.get("selection");
    expect(source).toBeDefined();
    const before = source?.setDataCalls ?? 0;
    view.setEntities(frame(3, 2));
    scheduler.run();
    expect(map.sources.get("selection")).toBe(source); // a layer with an inline source would replace it
    expect((source?.setDataCalls ?? 0) - before).toBe(1);
    expect(source?.serialise().features).toHaveLength(1);
  });
});
