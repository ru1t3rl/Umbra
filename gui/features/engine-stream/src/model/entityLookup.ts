import type { EntitySummary } from "../types.ts";

/** Binary search; `entities` ascend by id (ADR 0045). */
export function findEntity(
  entities: readonly EntitySummary[] | undefined,
  id: number | undefined,
): EntitySummary | undefined {
  if (entities === undefined || id === undefined) return undefined;
  let lo = 0;
  let hi = entities.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >>> 1;
    const e = entities[mid];
    if (e === undefined) return undefined;
    if (e.id === id) return e;
    if (e.id < id) lo = mid + 1;
    else hi = mid - 1;
  }
  return undefined;
}

/** Every compared field; a new wire field makes this fail to compile until it is added (and compared). */
const COMPARED: Record<Exclude<keyof EntitySummary, "route">, true> = {
  id: true,
  lat_deg: true,
  lon_deg: true,
  alt_m: true,
  heading_deg: true,
  speed_mps: true,
  ownership: true,
  name: true,
  template: true,
  force: true,
  group: true,
  behavior: true,
  status: true,
  remote_source: true,
};
const FIELDS = Object.keys(COMPARED) as (keyof typeof COMPARED)[];

/** Shallow field equality; `route` by `len` and `next`, absent and null alike. */
export function sameEntity(a: EntitySummary, b: EntitySummary): boolean {
  if (a === b) return true;
  for (const f of FIELDS) if (a[f] !== b[f]) return false;
  const ra = a.route;
  const rb = b.route;
  if (ra == null || rb == null) return ra == null && rb == null;
  return ra.len === rb.len && ra.next === rb.next;
}
