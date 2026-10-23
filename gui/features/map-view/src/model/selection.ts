import type { EntityFrame, EntitySummary } from "@umbra/engine-stream";
import {
  type AffiliationOf,
  type EntityFeatureCollection,
  emptyFeatureCollection,
  MOVING_SPEED_MPS,
} from "./features.ts";

/** Click hit box edge in px (ASM-803): a touch-size target on a 1.5 px stroked symbol. */
export const SELECT_BOX_PX = 12;

export interface PixelPoint {
  readonly x: number;
  readonly y: number;
}

/** A hit candidate: the entity id and where its symbol is on screen. */
export interface HitCandidate extends PixelPoint {
  readonly id: number;
}

/** The `queryRenderedFeatures` box around a click: half of `SELECT_BOX_PX` each side. */
export function hitBox(point: PixelPoint): [[number, number], [number, number]] {
  const half = SELECT_BOX_PX / 2;
  return [
    [point.x - half, point.y - half],
    [point.x + half, point.y + half],
  ];
}

/**
 * The id of the candidate nearest to `point` in pixels; equal distances go to the lowest id, so the result
 * does not depend on the order MapLibre returns the hits in. A feature returned twice is one pick.
 */
export function pickNearest(point: PixelPoint, hits: readonly HitCandidate[]): number | null {
  let best: number | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const hit of hits) {
    const distance = (hit.x - point.x) ** 2 + (hit.y - point.y) ** 2;
    if (distance < bestDistance || (distance === bestDistance && best !== null && hit.id < best)) {
      best = hit.id;
      bestDistance = distance;
    }
  }
  return best;
}

/** The name of the entity, or `#id` when it has none (null and empty count as none). */
export function selectionLabel(entity: Pick<EntitySummary, "id" | "name">): string {
  const name = entity.name;
  return name === undefined || name === null || name === "" ? `#${entity.id}` : name;
}

/** Binary search in the id-ascending frame (10,000 entities: about 14 reads). */
export function findEntity(frame: EntityFrame, id: number): EntitySummary | undefined {
  const { entities } = frame;
  let low = 0;
  let high = entities.length - 1;
  while (low <= high) {
    const mid = (low + high) >>> 1;
    const candidate = entities[mid];
    if (candidate === undefined) return undefined;
    if (candidate.id === id) return candidate;
    if (candidate.id < id) low = mid + 1;
    else high = mid - 1;
  }
  return undefined;
}

/**
 * The selection source data: at most one Point feature, the selected entity at its current position. Pass
 * the previous result as `reuse` to rewrite it in place (same rule as `toFeatureCollection`).
 */
export function toSelectionCollection(
  frame: EntityFrame,
  selectedId: number | null,
  affiliationOf: AffiliationOf,
  reuse?: EntityFeatureCollection,
): EntityFeatureCollection {
  const out = reuse ?? emptyFeatureCollection();
  const e = selectedId === null ? undefined : findEntity(frame, selectedId);
  if (e === undefined) {
    out.features.length = 0;
    return out;
  }
  const properties = {
    affiliation: affiliationOf(e.force),
    ownership: e.ownership,
    heading_deg: e.heading_deg,
    moving: e.speed_mps > MOVING_SPEED_MPS,
  };
  const f = out.features[0];
  out.features.length = 1;
  if (f === undefined) {
    out.features[0] = {
      type: "Feature",
      id: e.id,
      geometry: { type: "Point", coordinates: [e.lon_deg, e.lat_deg] },
      properties,
    };
  } else {
    f.id = e.id;
    f.geometry.coordinates[0] = e.lon_deg;
    f.geometry.coordinates[1] = e.lat_deg;
    f.properties.affiliation = properties.affiliation;
    f.properties.ownership = properties.ownership;
    f.properties.heading_deg = properties.heading_deg;
    f.properties.moving = properties.moving;
  }
  return out;
}
