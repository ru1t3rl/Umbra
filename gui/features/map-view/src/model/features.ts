import type { EntityFrame, EntitySummary } from "@umbra/engine-stream";
import type { Affiliation } from "@umbra/ui-kit";

/** An entity counts as moving above this speed (ADR 0042: direction line only when `speed_mps` > 0.1). */
export const MOVING_SPEED_MPS = 0.1;

// A type alias, not an interface: only an alias is assignable to GeoJSON's index-signature properties.
export type EntityFeatureProperties = {
  affiliation: Affiliation;
  ownership: EntitySummary["ownership"];
  heading_deg: number;
  moving: boolean;
};

export interface EntityFeature {
  type: "Feature";
  id: number;
  geometry: { type: "Point"; coordinates: [number, number] };
  properties: EntityFeatureProperties;
}

export interface EntityFeatureCollection {
  type: "FeatureCollection";
  features: EntityFeature[];
}

export type AffiliationOf = (force: string | null | undefined) => Affiliation;

export const emptyFeatureCollection = (): EntityFeatureCollection => ({
  type: "FeatureCollection",
  features: [],
});

/**
 * One GeoJSON collection (`FeatureCollection<Point>`, assignable to MapLibre's GeoJSON source data) for a
 * whole frame (Performance plan 3). Pass the previous result as `reuse` to rewrite it in place: no
 * allocation per entity per frame once the count is stable. MapLibre keeps the reference passed to
 * `setData` and may serialise a pending update later, so an in-place rewrite delivers the newest complete
 * frame (JavaScript is single-threaded; ASM-366). Never call `getData()` or `updateData` on this source.
 */
export function toFeatureCollection(
  frame: EntityFrame,
  affiliationOf: AffiliationOf,
  reuse?: EntityFeatureCollection,
): EntityFeatureCollection {
  const out = reuse ?? emptyFeatureCollection();
  const { entities } = frame;
  const features = out.features;
  features.length = entities.length;
  for (let i = 0; i < entities.length; i++) {
    const e = entities[i];
    if (e === undefined) continue;
    const moving = e.speed_mps > MOVING_SPEED_MPS;
    const affiliation = affiliationOf(e.force);
    const f = features[i];
    if (f === undefined) {
      features[i] = {
        type: "Feature",
        id: e.id,
        geometry: { type: "Point", coordinates: [e.lon_deg, e.lat_deg] },
        properties: { affiliation, ownership: e.ownership, heading_deg: e.heading_deg, moving },
      };
    } else {
      f.id = e.id;
      f.geometry.coordinates[0] = e.lon_deg;
      f.geometry.coordinates[1] = e.lat_deg;
      f.properties.affiliation = affiliation;
      f.properties.ownership = e.ownership;
      f.properties.heading_deg = e.heading_deg;
      f.properties.moving = moving;
    }
  }
  return out;
}

export interface GeoBounds {
  readonly west: number;
  readonly south: number;
  readonly east: number;
  readonly north: number;
}

/** Bounds of the entities, or null for an empty frame. Ignores the antimeridian (ASM-307 trigger). */
export function frameBounds(frame: EntityFrame): GeoBounds | null {
  let west = Number.POSITIVE_INFINITY;
  let south = Number.POSITIVE_INFINITY;
  let east = Number.NEGATIVE_INFINITY;
  let north = Number.NEGATIVE_INFINITY;
  for (const e of frame.entities) {
    if (e.lon_deg < west) west = e.lon_deg;
    if (e.lon_deg > east) east = e.lon_deg;
    if (e.lat_deg < south) south = e.lat_deg;
    if (e.lat_deg > north) north = e.lat_deg;
  }
  return frame.entities.length === 0 ? null : { west, south, east, north };
}
