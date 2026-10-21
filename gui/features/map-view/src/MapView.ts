import type { EntityFrame } from "@umbra/engine-stream";
import type { MapColors } from "./model/colors.ts";
import type { GeoBounds } from "./model/features.ts";

/**
 * What tests and the e2e hook (WI-12) read back from a view without touching the map library. The names
 * are the plan's WI-12 `inspect()` list; the exact return types are WI-12's to settle (ASM-366).
 */
export interface MapInspection {
  /** Features in the entity source. */
  sourceEntities(): number;
  /** Entity features the map actually drew (`queryRenderedFeatures`). */
  renderedEntities(): number;
  /** Pixel position of a coordinate, or null when it is off the map. */
  project(lon: number, lat: number): { readonly x: number; readonly y: number } | null;
  isIdle(): boolean;
  /** Increments once per completed render, for tests that wait on a frame. */
  tick(): number;
}

export interface FitOptions {
  /** Padding in px around the bounds. */
  readonly padding: number;
  readonly maxZoom: number;
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
  /** Re-colours the base style and rebuilds the symbol images, without reloading the map. */
  setTheme(colors: MapColors): void;
  fitTo(bounds: GeoBounds, options: FitOptions): void;
  inspect(): MapInspection;
  destroy(): void;
}
