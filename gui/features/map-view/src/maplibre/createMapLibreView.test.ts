import { beforeEach, describe, expect, mock, test } from "bun:test";
import { COLORS, entity, frameOf } from "../model/testing.ts";
import { FakeMap, fakeRasterizer, ManualScheduler } from "./testing.ts";

mock.module("maplibre-gl", () => ({ Map: FakeMap }));
const { createMapLibreView } = await import("./createMapLibreView.ts");

const flushPromises = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
/** Style loaded and the symbol images installed: the view may now send data. */
const loadMap = async (map: FakeMap) => {
  map.load();
  await flushPromises();
};

function setup(options: Partial<Parameters<typeof createMapLibreView>[1]> = {}) {
  const scheduler = new ManualScheduler();
  const raster = fakeRasterizer();
  const errors: unknown[] = [];
  const container = document.createElement("div");
  const view = createMapLibreView(container, {
    colors: COLORS,
    theme: "dark",
    scheduler,
    rasterize: raster.rasterize,
    pixelRatio: 2,
    onError: (e) => errors.push(e),
    ...options,
  });
  const map = FakeMap.instances[0] as FakeMap;
  return { view, map, scheduler, raster, errors, container };
}

beforeEach(() => FakeMap.reset());

describe("createMapLibreView", () => {
  test("creates one map in the container with the offline style: no glyphs, no attribution", () => {
    const { map, container } = setup();
    expect(FakeMap.instances).toHaveLength(1);
    expect(map.options.container).toBe(container);
    expect("glyphs" in (map.options.style as object)).toBe(false);
    expect(map.options.attributionControl).toBe(false);
  });

  test("nothing is sent before the style loads; the first flush happens after load", async () => {
    const { view, map, scheduler } = setup();
    view.setEntities(frameOf([entity(1)]));
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(0);
    await loadMap(map);
    expect(scheduler.pending).toBe(1);
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(1);
    expect(view.inspect().sourceEntities()).toBe(1);
  });

  test("setData is called at most once per animation frame, over 50 frames of 3 setEntities each", async () => {
    const { view, map, scheduler } = setup();
    await loadMap(map);
    for (let i = 0; i < 50; i++) {
      for (let k = 0; k < 3; k++) view.setEntities(frameOf([entity(i * 3 + k)]));
      expect(map.entities.setDataCalls).toBe(i);
      scheduler.run();
      expect(map.entities.setDataCalls).toBe(i + 1);
    }
  });

  test("an animation frame with no new data sends nothing", async () => {
    const { view, map, scheduler } = setup();
    await loadMap(map);
    view.setEntities(frameOf([entity(1)]));
    scheduler.run();
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(1);
  });

  test("two setData calls in flight deliver the last frame (reused buffer, ASM-366)", async () => {
    const { view, map, scheduler } = setup();
    await loadMap(map);
    view.setEntities(frameOf([entity(1), entity(2)]));
    scheduler.run();
    view.setEntities(frameOf([entity(7), entity(8), entity(9)]));
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(2);
    // the "worker" serialises only now, after both calls
    expect(map.entities.serialise().features.map((f) => f.id)).toEqual([7, 8, 9]);
  });

  test("symbol images are rasterised at the pixel ratio and added after load", async () => {
    const { map, raster } = setup();
    expect(map.imageOps).toHaveLength(0);
    await loadMap(map);
    await flushPromises();
    expect(map.imageOps).toHaveLength(9);
    expect(map.imageOps.every((o) => o.op === "add" && o.pixelRatio === 2)).toBe(true);
    expect(raster.svgOf(map.images.get("umbra-symbol-friend-ring"))).toContain("<circle");
    expect(raster.svgOf(map.images.get("umbra-symbol-friend-plain"))).not.toContain("<circle");
  });

  test("a theme change re-colours with setPaintProperty and updates the images; no new map", async () => {
    const { view, map, raster } = setup();
    await loadMap(map);
    await flushPromises();
    map.paint.length = 0;
    map.imageOps.length = 0;
    const day = { ...COLORS, symbolFrame: "FRAME-DAY", lineStrong: "LS-DAY" };
    view.setTheme(day, "light");
    await flushPromises();
    expect(FakeMap.instances).toHaveLength(1);
    expect(map.removed).toBe(false);
    expect(map.paint).toContainEqual({
      layer: "graticule",
      property: "line-color",
      value: "LS-DAY",
    });
    expect(map.paint).toContainEqual({ layer: "graticule", property: "line-opacity", value: 0.55 });
    expect(map.imageOps).toHaveLength(9);
    expect(map.imageOps.every((o) => o.op === "update")).toBe(true);
    expect(raster.svgOf(map.images.get("umbra-symbol-hostile-plain"))).toContain("FRAME-DAY");
  });

  test("a theme change before the style loaded is applied at load", async () => {
    const { view, map } = setup();
    view.setTheme({ ...COLORS, lineStrong: "LS-DAY" }, "light");
    expect(map.paint).toHaveLength(0);
    await loadMap(map);
    await flushPromises();
    expect(map.paint).toContainEqual({
      layer: "graticule",
      property: "line-color",
      value: "LS-DAY",
    });
  });

  test("when two themes overlap, only the newest images are installed", async () => {
    const { view, map, raster } = setup();
    await loadMap(map);
    await flushPromises();
    map.imageOps.length = 0;
    view.setTheme({ ...COLORS, symbolFrame: "FRAME-A" }, "light");
    view.setTheme({ ...COLORS, symbolFrame: "FRAME-B" }, "dark");
    await flushPromises();
    expect(map.imageOps).toHaveLength(9);
    for (const op of map.imageOps) expect(raster.svgOf(op.image)).toContain("FRAME-B");
  });

  test("fitTo passes bounds, padding and max zoom without animation", () => {
    const { view, map } = setup();
    view.fitTo(
      { west: 1, south: 2, east: 3, north: 4 },
      { padding: { top: 64, right: 64, bottom: 64, left: 64 }, maxZoom: 15 },
    );
    expect(map.fits).toEqual([
      {
        bounds: [
          [1, 2],
          [3, 4],
        ],
        options: {
          padding: { top: 64, right: 64, bottom: 64, left: 64 },
          maxZoom: 15,
          animate: false,
        },
      },
    ]);
  });

  test("destroy cancels the pending flush and removes the map", async () => {
    const { view, map, scheduler } = setup();
    await loadMap(map);
    view.setEntities(frameOf([entity(1)]));
    view.destroy();
    scheduler.run();
    expect(scheduler.cancelled).toHaveLength(1);
    expect(map.removed).toBe(true);
    expect(map.entities.setDataCalls).toBe(0);
    view.setEntities(frameOf([entity(2)]));
    expect(scheduler.pending).toBe(0);
  });

  test("inspect: unique rendered ids, project inside the canvas only, idle and tick", async () => {
    const { view, map, scheduler } = setup();
    const inspect = view.inspect();
    expect(inspect.isIdle()).toBe(false);
    await loadMap(map);
    map.emit("idle");
    expect(inspect.isIdle()).toBe(true);
    view.setEntities(frameOf([entity(1)]));
    expect(inspect.isIdle()).toBe(false);
    scheduler.run();
    expect(inspect.isIdle()).toBe(false);
    map.emit("idle");
    expect(inspect.isIdle()).toBe(true);
    map.rendered = [{ id: 1 }, { id: 1 }, { id: 2 }];
    expect(inspect.renderedEntities()).toBe(2);
    expect(inspect.project(10, 10)).toEqual({ x: 100, y: 100 });
    expect(inspect.project(500, 10)).toBeNull();
    expect(inspect.tick()).toBe(0);
    map.emit("render");
    expect(inspect.tick()).toBe(1);
  });

  test("a configured style URL replaces the base; the overlay is added at load", async () => {
    const { map } = setup({ basemap: { styleUrl: "https://tiles.test/s.json" } });
    expect(map.options.style).toBe("https://tiles.test/s.json");
    expect("attributionControl" in map.options).toBe(false);
    await loadMap(map);
    expect([...map.sources.keys()]).toEqual(["entities"]);
    expect(map.layers).toEqual(["direction", "entities"]);
  });

  test("map and image errors are reported, not swallowed", async () => {
    const failing = setup({ rasterize: () => Promise.reject(new Error("no canvas")) });
    await loadMap(failing.map);
    await flushPromises();
    expect(failing.errors.map(String)).toContain("Error: no canvas");
    failing.map.emit("error", { error: new Error("tile") });
    expect(failing.errors.map(String)).toContain("Error: tile");
  });

  test("the first flush waits for the symbol images (no frame with missing symbols)", async () => {
    let release: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const raster = fakeRasterizer();
    const { view, map, scheduler } = setup({
      rasterize: async (svg, size, ratio) => {
        await gate;
        return raster.rasterize(svg, size, ratio);
      },
    });
    map.load();
    view.setEntities(frameOf([entity(1)]));
    expect(scheduler.pending).toBe(0);
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(0);
    release?.();
    await flushPromises();
    expect(map.imageOps).toHaveLength(9);
    expect(scheduler.pending).toBe(1);
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(1);
    expect(view.inspect().missingImages()).toBe(0);
  });

  test("a failed image install is reported and the data still flows", async () => {
    const { view, map, scheduler, errors } = setup({
      rasterize: () => Promise.reject(new Error("no canvas")),
    });
    view.setEntities(frameOf([entity(1)]));
    await loadMap(map);
    expect(errors.map(String)).toContain("Error: no canvas");
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(1);
  });

  test("styleimagemissing is counted and answered with nothing", async () => {
    const { view, map } = setup();
    await loadMap(map);
    map.emit("styleimagemissing");
    map.emit("styleimagemissing");
    expect(view.inspect().missingImages()).toBe(2);
  });

  test("the style is ready on the first styledata with a loaded style, not on every styledata", async () => {
    const { map, view, scheduler } = setup({ basemap: { styleUrl: "https://t.test/s.json" } });
    view.setEntities(frameOf([entity(1)]));
    map.emit("styledata"); // style still loading
    await flushPromises();
    expect(map.layers).toEqual([]);
    expect(map.imageOps).toHaveLength(0);
    await loadMap(map);
    expect(map.layers).toEqual(["direction", "entities"]);
    // later styledata events (our own addLayer calls, tile loads) change nothing
    map.emit("styledata");
    map.emit("styledata");
    await flushPromises();
    expect(map.layers).toEqual(["direction", "entities"]);
    expect(map.imageOps).toHaveLength(9);
    expect(scheduler.pending).toBe(1);
  });

  test("readiness also comes from sourcedata and load when the first styledata was too early", async () => {
    for (const event of ["sourcedata", "load"]) {
      FakeMap.reset();
      const { map } = setup();
      map.emit("styledata"); // sources still loading
      map.styleLoaded = true;
      await flushPromises();
      expect(map.imageOps).toHaveLength(0);
      map.emit(event);
      await flushPromises();
      expect(map.imageOps).toHaveLength(9);
    }
  });

  test("an overlay that already exists is not added again", async () => {
    const { map } = setup({ basemap: { styleUrl: "https://t.test/s.json" } });
    map.addSource("entities");
    await loadMap(map);
    expect(map.layers).toEqual([]);
  });

  test("a lost WebGL context is reported and cleared on restore, and the data is re-sent", async () => {
    const changes: boolean[] = [];
    const { view, map, scheduler, errors } = setup({
      onContextChange: (lost) => changes.push(lost),
    });
    await loadMap(map);
    view.setEntities(frameOf([entity(1)]));
    scheduler.run();
    map.emit("webglcontextlost");
    expect(changes).toEqual([true]);
    expect(errors.map(String)).toContain("Error: map: the WebGL context was lost");
    map.emit("webglcontextrestored");
    expect(changes).toEqual([true, false]);
    expect(scheduler.pending).toBe(1);
    scheduler.run();
    expect(map.entities.setDataCalls).toBe(2);
  });

  test("fitTo reports false and moves nothing when the map has no camera for the bounds", () => {
    const { view, map } = setup();
    const bounds = { west: 1, south: 2, east: 3, north: 4 };
    map.camera = undefined;
    expect(
      view.fitTo(bounds, { padding: { top: 64, right: 64, bottom: 64, left: 64 }, maxZoom: 15 }),
    ).toBe(false);
    expect(map.fits).toHaveLength(0);
    map.camera = { center: [2, 3], zoom: 6 };
    expect(
      view.fitTo(bounds, { padding: { top: 64, right: 64, bottom: 64, left: 64 }, maxZoom: 15 }),
    ).toBe(true);
    expect(map.fits).toHaveLength(1);
  });

  test("lastFlushMs is the main-thread time of the flush, feature build and setData", async () => {
    let t = 100;
    const ticks = [0, 7.5];
    const { view, map, scheduler } = setup({ now: () => (t += ticks.shift() ?? 0) });
    await loadMap(map);
    expect(view.inspect().lastFlushMs()).toBe(0);
    view.setEntities(frameOf([entity(1)]));
    scheduler.run();
    expect(view.inspect().lastFlushMs()).toBe(7.5);
  });
});
