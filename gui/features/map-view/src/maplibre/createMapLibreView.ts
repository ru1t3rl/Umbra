import type { EntityFrame } from "@umbra/engine-stream";
import { defaultAffiliationOf } from "@umbra/settings";
import type { ResolvedTheme } from "@umbra/ui-kit";
import {
  type GeoJSONSource,
  type MapGeoJSONFeature,
  Map as MapLibreMap,
  type MapMouseEvent,
} from "maplibre-gl";
import type { EntityPosition, MapInspection, SelectableMapView } from "../MapView.ts";
import type { FitOptions } from "../model/camera.ts";
import type { MapColors } from "../model/colors.ts";
import {
  type AffiliationOf,
  type EntityFeatureCollection,
  emptyFeatureCollection,
  type GeoBounds,
  toFeatureCollection,
} from "../model/features.ts";
import {
  findEntity,
  type HitCandidate,
  hitBox,
  pickNearest,
  selectionLabel,
  toSelectionCollection,
} from "../model/selection.ts";
import {
  type BasemapOptions,
  buildMapStyle,
  ENTITY_LAYER,
  ENTITY_SOURCE,
  paintUpdates,
  SELECTION_SOURCE,
  selectionOverlay,
} from "../model/style.ts";
import { selectionImage, symbolImages } from "../model/symbols.ts";
import { createSelectionLabel } from "./createSelectionLabel.ts";
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
  /** A click on the map: the id of the nearest entity, or null on empty map (ASM-803). */
  readonly onSelect?: ((id: number | null) => void) | undefined;
  /** Force to affiliation for the entity and selection features; read at every flush. Default: the settings default. */
  readonly affiliationOf?: AffiliationOf | undefined;
  /** The selected entity at creation (a re-created view keeps the selection). */
  readonly selectedId?: number | null | undefined;
  /** Clock for `inspect().lastFlushMs()`; default `performance.now`. */
  readonly now?: (() => number) | undefined;
}

const browserScheduler: FrameScheduler = {
  request: (callback) => requestAnimationFrame(callback),
  cancel: (handle) => cancelAnimationFrame(handle),
};

/** Elements whose Escape is their own (MapLibre controls and popups put such elements in the container). */
const CONTROL_SELECTOR =
  "button, input, select, textarea, a[href], [role='button'], .maplibregl-ctrl";

/** The entity id and position of a rendered point feature, or undefined for anything else. */
function pointOf(f: MapGeoJSONFeature): EntityPosition | undefined {
  if (typeof f.id !== "number" || f.geometry.type !== "Point") return undefined;
  const [lon, lat] = f.geometry.coordinates;
  return lon === undefined || lat === undefined ? undefined : { id: f.id, lon, lat };
}

/**
 * MapLibre implementation of `MapView` (ADR 0040). One GeoJSON source, rewritten from a reused buffer at
 * most once per animation frame (Performance plan 3). Never calls `getData()` or `updateData` on the
 * entity source (ASM-366).
 */
export function createMapLibreView(
  container: HTMLElement,
  options: MapLibreViewOptions,
): SelectableMapView {
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
  const selectionBuffer: EntityFeatureCollection = emptyFeatureCollection();
  const label = createSelectionLabel(container);
  let selectedId: number | null = options.selectedId ?? null;
  let selectionLayerAdded = false;
  // The selection source holds a feature (so clearing it needs one write) / the ring image was requested.
  let selectionSent = false;
  let ringRequested = false;
  // The ring image is installed: only then does the selection source get its feature (no missing image).
  let ringReady = false;
  let ringGeneration = 0;
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

  /** Puts the label at the selected entity's pixel, or removes it. Pixel work only, no data. */
  function placeLabel(): void {
    const entity =
      latest === undefined || selectedId === null ? undefined : findEntity(latest, selectedId);
    if (entity === undefined) {
      label.show(null, 0, 0);
      return;
    }
    const { x, y } = map.project([entity.lon_deg, entity.lat_deg]);
    label.show(selectionLabel(entity), x, y);
  }

  /**
   * Adds the selection ring above the entity layers, once, at style load. The offline style already holds the
   * `selection` source; a configured style gets it with the layer (MapLibre creates a source named after the
   * layer from an inline spec), so its source list stays the entity source (ASM-935).
   */
  function addSelectionLayer(): void {
    if (selectionLayerAdded) return;
    selectionLayerAdded = true;
    const { sources, layers } = selectionOverlay();
    const spec = sources[SELECTION_SOURCE];
    for (const layer of layers) {
      if (built.kind === "url" && spec !== undefined) map.addLayer({ ...layer, source: spec });
      else map.addLayer(layer);
    }
  }

  function flush(): void {
    pending = undefined;
    if (destroyed || latest === undefined) return;
    const source = map.getSource<GeoJSONSource>(ENTITY_SOURCE);
    if (source === undefined) return;
    const start = now();
    const affiliationOf = options.affiliationOf ?? defaultAffiliationOf;
    toFeatureCollection(latest, affiliationOf, buffer);
    source.setData(buffer);
    // The selection source and the label follow in the same flush (ADR 0078).
    if (selectedId !== null && !ringRequested) {
      ringRequested = true;
      installRing(current.colors)
        .catch(report)
        .then(() => {
          if (destroyed) return;
          ringReady = true;
          schedule();
        });
    }
    toSelectionCollection(latest, selectedId, affiliationOf, selectionBuffer);
    if (!ringReady) selectionBuffer.features.length = 0;
    const has = selectionBuffer.features.length > 0;
    // Nothing is written while the source was empty and stays empty (m6).
    // While selected the source is written each flush (empty until the ring image is installed).
    if (has || selectionSent || selectedId !== null) {
      map.getSource<GeoJSONSource>(SELECTION_SOURCE)?.setData(selectionBuffer);
      selectionSent = selectedId !== null;
    }
    placeLabel();
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

  /** The selection ring image: registered at the first selection, updated on a theme change (ASM-935). */
  async function installRing(colors: MapColors): Promise<void> {
    const generation = ++ringGeneration;
    const image = selectionImage(colors);
    const raster = await rasterize(image.svg, image.size, pixelRatio);
    if (destroyed || generation !== ringGeneration) return;
    if (map.hasImage(image.id)) map.updateImage(image.id, raster);
    else map.addImage(image.id, raster, { pixelRatio });
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
      addSelectionLayer();
    }
    // An overlay that already exists (the configured style carries it) is left alone, ring included.
    if (built.kind === "inline") addSelectionLayer();
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
  map.on("click", (event: MapMouseEvent) => {
    // Before the style is ready there is no entity layer to query.
    if (destroyed || !styleReady) return;
    const hits: HitCandidate[] = [];
    for (const f of map.queryRenderedFeatures(hitBox(event.point), { layers: [ENTITY_LAYER] })) {
      const p = pointOf(f);
      if (p === undefined) continue;
      const { x, y } = map.project([p.lon, p.lat]);
      hits.push({ id: p.id, x, y });
    }
    options.onSelect?.(pickNearest(event.point, hits));
  });
  // A pan or zoom moves the label at once: pixel work only, no data write, no scheduled frame.
  map.on("move", () => {
    if (!destroyed && selectedId !== null) placeLabel();
  });
  // Escape clears the selection. The listener is on the container, which the focusable MapLibre canvas bubbles to.
  const onKeyDown = (event: KeyboardEvent): void => {
    // Escape in another MapLibre control (a popup button, an input) belongs to that control, not to the map.
    const target = event.target;
    if (event.defaultPrevented) return;
    if (target instanceof Element && target.closest(CONTROL_SELECTOR) !== null) return;
    if (event.key === "Escape" && selectedId !== null) options.onSelect?.(null);
  };
  container.addEventListener("keydown", onKeyDown);
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
        const p = pointOf(f);
        if (p !== undefined) unique.set(p.id, p);
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
    selectedId: () => selectedId,
    labelText: () => label.text(),
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
      if (ringRequested) installRing(colors).catch(report);
      installImages(colors).catch(report);
    },
    setSelection(id: number | null): void {
      if (destroyed || id === selectedId) return;
      selectedId = id;
      schedule();
    },
    centerOn(id: number): boolean {
      const entity = latest === undefined ? undefined : findEntity(latest, id);
      if (entity === undefined) return false;
      map.easeTo({ center: [entity.lon_deg, entity.lat_deg] });
      return true;
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
      container.removeEventListener("keydown", onKeyDown);
      label.destroy();
      map.remove();
    },
  };
}
