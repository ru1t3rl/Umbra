import type { EntityFrame } from "@umbra/engine-stream";
import type { ResolvedTheme } from "@umbra/ui-kit";
import type { FitOptions } from "./model/camera.ts";
import type { MapColors } from "./model/colors.ts";
import type { GeoBounds } from "./model/features.ts";

export interface EntityPosition {
  readonly id: number;
  readonly lon: number;
  readonly lat: number;
}

/**
 * What tests and the e2e hook (WI-12) read back from a view without touching the map library. The names
 * are the plan's WI-12 `inspect()` list; the exact return types are WI-12's to settle (ASM-366).
 */
export interface MapInspection {
  /** Features in the entity source. */
  sourceEntities(): number;
  /** Entity features the map actually drew (`queryRenderedFeatures`). */
  renderedEntities(): number;
  /** Positions of the features last handed to the entity source, by entity id (e2e, ASM-310). */
  sourcePositions(): readonly EntityPosition[];
  /** Positions of the entity features the map drew (tile-quantized geometry), unique by id (e2e, ASM-310). */
  renderedPositions(): readonly EntityPosition[];
  /** Tick and session of the frame last handed to the entity source; null before the first flush. */
  frameTick(): { readonly session: number; readonly tick: number } | null;
  /** Current camera zoom level (e2e asserts the zoom behind the rendered-position tolerance, ASM-310). */
  zoom(): number;
  /** Pixel position of a coordinate, or null when it is off the map. */
  project(lon: number, lat: number): { readonly x: number; readonly y: number } | null;
  isIdle(): boolean;
  /** Milliseconds the last flush took on the main thread: feature build plus MapLibre's `setData` hand-off. */
  lastFlushMs(): number;
  /** Times MapLibre asked for an image we did not register (`styleimagemissing`); 0 once images are installed. */
  missingImages(): number;
  /** Increments once per completed render, for tests that wait on a frame. */
  tick(): number;
}

/**
 * The map behind a seam (F-8, ADR 0040). Internal: not exported from the package. The only
 * implementation is MapLibre (WI-12); nothing outside this feature knows the library.
 */
export interface MapView {
  /**
   * Stores the frame and schedules the flush itself: at most one `setData` per animation frame, whatever
   * the number of calls (Performance plan 3). The caller does not batch.
   */
  setEntities(frame: EntityFrame): void;
  /**
   * Re-colours the base style (`setPaintProperty`) and rebuilds the symbol images, without reloading the
   * map. The theme is passed with the colours because the Day graticule differs from Ops (ASM-367).
   */
  setTheme(colors: MapColors, theme: ResolvedTheme): void;
  /** Fits the camera. Returns false (and does nothing) when the map cannot fit, e.g. a zero-size container. */
  fitTo(bounds: GeoBounds, options: FitOptions): boolean;
  inspect(): MapInspection;
  destroy(): void;
}
