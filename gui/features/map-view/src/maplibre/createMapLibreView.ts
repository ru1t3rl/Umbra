import type { EntityFrame } from "@umbra/engine-stream";
import { defaultAffiliationOf as affiliationOf } from "@umbra/settings";
import type { ResolvedTheme } from "@umbra/ui-kit";
import { type GeoJSONSource, Map as MapLibreMap } from "maplibre-gl";
import type { EntityPosition, MapInspection, MapView } from "../MapView.ts";
import type { FitOptions } from "../model/camera.ts";
import type { MapColors } from "../model/colors.ts";
import {
  type EntityFeatureCollection,
  emptyFeatureCollection,
  type GeoBounds,
  toFeatureCollection,
} from "../model/features.ts";
import {
  type BasemapOptions,
  buildMapStyle,
  ENTITY_LAYER,
  ENTITY_SOURCE,
  paintUpdates,
} from "../model/style.ts";
import { symbolImages } from "../model/symbols.ts";
import { type Rasterizer, rasterizeSvg } from "./rasterize.ts";

/** Schedules the per-frame flush; the default is `requestAnimationFrame` (ADR 0045). */
export interface FrameScheduler {
  request(callback: () => void): number;
  cancel(handle: number): void;
}

export interface MapLibreViewOptions {
  readonly colors: MapColors;
  readonly theme: ResolvedTheme;
  readonly basemap?: BasemapOptions | undefined;
  readonly scheduler?: FrameScheduler | undefined;
  readonly rasterize?: Rasterizer | undefined;
  readonly pixelRatio?: number | undefined;
  /** Map and image errors (a failed tile, a failed rasterisation, a lost context). Never swallowed silently. */
  readonly onError?: ((error: unknown) => void) | undefined;
  /** The WebGL context was lost (`true`) or restored (`false`); the map recovers by itself on restore. */
  readonly onContextChange?: ((lost: boolean) => void) | undefined;
  /** Clock for `inspect().lastFlushMs()`; default `performance.now`. */
  readonly now?: (() => number) | undefined;
}

const browserScheduler: FrameScheduler = {
  request: (callback) => requestAnimationFrame(callback),
  cancel: (handle) => cancelAnimationFrame(handle),
};

/**
 * MapLibre implementation of `MapView` (ADR 0040). One GeoJSON source, rewritten from a reused buffer at
 * most once per animation frame (Performance plan 3). Never calls `getData()` or `updateData` on the
 * entity source (ASM-366).
 */
export function createMapLibreView(container: HTMLElement, options: MapLibreViewOptions): MapView {
  const scheduler = options.scheduler ?? browserScheduler;
  const rasterize = options.rasterize ?? rasterizeSvg;
  const pixelRatio = options.pixelRatio ?? 1;
  const report = options.onError ?? (() => undefined);
  const now = options.now ?? (() => performance.now());
  const built = buildMapStyle(options.colors, options.theme, options.basemap);

  const map = new MapLibreMap({
    container,
    style: built.kind === "inline" ? built.style : built.styleUrl,
    // The offline style is public domain (ADR 0041); a configured style shows its own attribution.
    ...(built.kind === "url" ? {} : { attributionControl: false as const }),
  });

  const buffer: EntityFeatureCollection = emptyFeatureCollection();
  let latest: EntityFrame | undefined;
  let pending: number | undefined;
  // The style is ready (sources and layers exist), then the symbol images are installed; the first flush waits for both.
  let styleReady = false;
  let imagesReady = false;
  let idle = false;
  let destroyed = false;
  let sourceCount = 0;
  let flushedSession = 0;
  let flushedTick = -1;
  let renders = 0;
  let flushMs = 0;
  let missing = 0;
  let imageGeneration = 0;
  let current = { colors: options.colors, theme: options.theme };

  function flush(): void {
    pending = undefined;
    if (destroyed || latest === undefined) return;
    const source = map.getSource<GeoJSONSource>(ENTITY_SOURCE);
    if (source === undefined) return;
    const start = now();
    toFeatureCollection(latest, affiliationOf, buffer);
    source.setData(buffer);
    flushMs = now() - start;
    sourceCount = buffer.features.length;
    flushedSession = latest.session;
    flushedTick = latest.tick;
    idle = false;
  }

  function schedule(): void {
    if (pending !== undefined || destroyed || !imagesReady) return;
    pending = scheduler.request(flush);
  }

  async function installImages(colors: MapColors): Promise<void> {
    const generation = ++imageGeneration;
    const images = await Promise.all(
      symbolImages(colors).map(async (image) => ({
        id: image.id,
        raster: await rasterize(image.svg, image.size, pixelRatio),
      })),
    );
    // A newer theme started while this one rasterised: it installs its own images.
    if (destroyed || generation !== imageGeneration) return;
    for (const { id, raster } of images) {
      if (map.hasImage(id)) map.updateImage(id, raster);
      else map.addImage(id, raster, { pixelRatio });
    }
  }

  function applyPaint(colors: MapColors, theme: ResolvedTheme): void {
    for (const { layer, property, value } of paintUpdates(colors, theme)) {
      map.setPaintProperty(layer, property, value);
    }
  }

  // `isStyleLoaded()` is true once the style and its sources have loaded, which is usually after the first
  // `styledata` (GeoJSON sources load in the worker), so `sourcedata` is a trigger too. The first event with a
  // loaded style is the signal; `load` is only the last resort. Gating on `load` alone would wait for the
  // tiles of a slow configured style as well.
  function onStyleData(): void {
    if (styleReady || destroyed || !map.isStyleLoaded()) return;
    styleReady = true;
    if (built.kind === "url" && !map.getSource(ENTITY_SOURCE)) {
      for (const [id, source] of Object.entries(built.overlay.sources)) map.addSource(id, source);
      for (const layer of built.overlay.layers) map.addLayer(layer);
    }
    // A theme switch before the style finished loading is applied now (setPaintProperty needs a loaded style).
    if (built.kind === "inline") applyPaint(current.colors, current.theme);
    // Images first, then the first flush: no frame is drawn with missing symbols. A failed install is
    // reported and the data flows anyway.
    installImages(current.colors)
      .catch(report)
      .then(() => {
        if (destroyed) return;
        imagesReady = true;
        if (latest !== undefined) schedule();
      });
  }
  map.on("styledata", onStyleData);
  map.on("sourcedata", onStyleData);
  map.on("load", onStyleData);
  map.on("styleimagemissing", () => {
    missing += 1;
  });
  map.on("webglcontextlost", () => {
    report(new Error("map: the WebGL context was lost"));
    options.onContextChange?.(true);
  });
  map.on("webglcontextrestored", () => {
    options.onContextChange?.(false);
    // MapLibre re-uploads its images and tiles; make sure the newest entity data is sent too.
    if (latest !== undefined) schedule();
  });
  map.on("render", () => {
    renders += 1;
  });
  map.on("idle", () => {
    idle = true;
  });
  map.on("error", (event) => report(event.error));

  const inspection: MapInspection = {
    sourceEntities: () => sourceCount,
    renderedEntities: () =>
      new Set(map.queryRenderedFeatures({ layers: [ENTITY_LAYER] }).map((f) => f.id)).size,
    sourcePositions: () =>
      buffer.features.map((f) => ({
        id: f.id,
        lon: f.geometry.coordinates[0],
        lat: f.geometry.coordinates[1],
      })),
    renderedPositions: () => {
      const unique = new Map<number, EntityPosition>();
      for (const f of map.queryRenderedFeatures({ layers: [ENTITY_LAYER] })) {
        if (typeof f.id !== "number" || f.geometry.type !== "Point") continue;
        const [lon, lat] = f.geometry.coordinates;
        if (lon !== undefined && lat !== undefined) unique.set(f.id, { id: f.id, lon, lat });
      }
      return [...unique.values()];
    },
    frameTick: () =>
      flushedTick < 0 ? null : Object.freeze({ session: flushedSession, tick: flushedTick }),
    zoom: () => map.getZoom(),
    project: (lon, lat) => {
      const { x, y } = map.project([lon, lat]);
      const canvas = map.getCanvas();
      const inside = x >= 0 && y >= 0 && x <= canvas.clientWidth && y <= canvas.clientHeight;
      return inside ? { x, y } : null;
    },
    isIdle: () => styleReady && imagesReady && idle && pending === undefined,
    lastFlushMs: () => flushMs,
    missingImages: () => missing,
    tick: () => renders,
  };

  return {
    setEntities(frame: EntityFrame): void {
      latest = frame;
      schedule();
    },
    setTheme(colors: MapColors, theme: ResolvedTheme): void {
      if (destroyed) return;
      current = { colors, theme };
      if (!styleReady) return;
      if (built.kind === "inline") applyPaint(colors, theme);
      installImages(colors).catch(report);
    },
    fitTo(bounds: GeoBounds, fit: FitOptions): boolean {
      const box: [[number, number], [number, number]] = [
        [bounds.west, bounds.south],
        [bounds.east, bounds.north],
      ];
      // No camera when the container is too small for the padding (zero-size, hidden): do not pretend to fit.
      if (map.cameraForBounds(box, { padding: fit.padding, maxZoom: fit.maxZoom }) === undefined) {
        return false;
      }
      map.fitBounds(box, { padding: fit.padding, maxZoom: fit.maxZoom, animate: false });
      return true;
    },
    inspect: () => inspection,
    destroy(): void {
      destroyed = true;
      if (pending !== undefined) scheduler.cancel(pending);
      pending = undefined;
      map.remove();
    },
  };
}
