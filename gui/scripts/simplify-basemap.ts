// Builds the offline basemap assets (M5b WI-13, ADR 0041, H-2) from Natural Earth GeoJSON.
// Usage: bun scripts/simplify-basemap.ts <outDir> <source.geojson>...
// Deterministic: fixed tolerance and precision, no properties, sorted features, compact output,
// so re-running gives byte-identical files. The sources are untrusted data: they are only parsed as JSON.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

/** Douglas-Peucker tolerance in degrees (about 1.1 km at the equator), ASM-500. */
export const TOLERANCE_DEG = 0.01;
/** Decimal places kept in every coordinate (about 110 m), ASM-500. */
export const PRECISION = 3;

/** Source file (without extension) to output file name. */
export const OUTPUT_NAMES: Readonly<Record<string, string>> = {
  ne_50m_land: "ne_50m_land.geojson",
  ne_50m_admin_0_boundary_lines_land: "ne_50m_admin0_lines.geojson",
};

type Position = [number, number];
type Ring = Position[];

export interface OutFeature {
  type: "Feature";
  properties: Record<string, never>;
  geometry: { type: string; coordinates: unknown };
}
export interface OutCollection {
  type: "FeatureCollection";
  features: OutFeature[];
}

const round = (v: number, precision: number): number => {
  const r = Number(v.toFixed(precision));
  return Object.is(r, -0) ? 0 : r;
};

function distToSegment(p: Position, a: Position, b: Position): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  const t =
    len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/** Antimeridian cut vertices and the Antarctic edge are never dropped, so the land outline keeps its seams (ASM-500). */
export const mustKeep = (p: Position): boolean => Math.abs(p[0]) >= 180 || p[1] <= -89.9;

/** Douglas-Peucker on an open polyline (iterative, so long coastlines cannot overflow the stack). */
export function douglasPeucker(points: readonly Position[], tolerance: number): Position[] {
  const n = points.length;
  if (n <= 2) return points.map((p) => [p[0], p[1]]);
  const keep = new Uint8Array(n);
  keep[0] = 1;
  keep[n - 1] = 1;
  const stack: Array<[number, number]> = [];
  let from = 0;
  for (let i = 1; i < n; i++) {
    if (i === n - 1 || mustKeep(points[i] as Position)) {
      keep[i] = 1;
      stack.push([from, i]);
      from = i;
    }
  }
  for (let top = stack.pop(); top; top = stack.pop()) {
    const [lo, hi] = top;
    const a = points[lo] as Position;
    const b = points[hi] as Position;
    let worst = -1;
    let at = -1;
    for (let i = lo + 1; i < hi; i++) {
      const d = distToSegment(points[i] as Position, a, b);
      if (d > worst) {
        worst = d;
        at = i;
      }
    }
    if (at >= 0 && worst > tolerance) {
      keep[at] = 1;
      stack.push([lo, at], [at, hi]);
    }
  }
  return points.flatMap((p, i) => (keep[i] ? [[p[0], p[1]] as Position] : []));
}

const dedupe = (points: Position[]): Position[] =>
  points.filter((p, i) => i === 0 || p[0] !== points[i - 1]?.[0] || p[1] !== points[i - 1]?.[1]);

const roundAll = (points: readonly Position[], precision: number): Position[] =>
  dedupe(points.map((p) => [round(p[0], precision), round(p[1], precision)]));

/** A line: at least two distinct points survive, else null. */
export function simplifyLine(
  line: readonly Position[],
  tolerance: number,
  precision: number,
): Position[] | null {
  const out = roundAll(douglasPeucker(roundAll(line, precision), tolerance), precision);
  return out.length >= 2 ? out : null;
}

/** A closed ring: split at the point farthest from the start, simplify both halves, rejoin; null if it collapses. */
export function simplifyRing(
  ring: readonly Position[],
  tolerance: number,
  precision: number,
): Ring | null {
  const pts = roundAll(ring, precision);
  const first = pts[0];
  if (!first || pts.length < 4) return null;
  const open = pts.slice(
    0,
    pts[pts.length - 1]?.[0] === first[0] && pts[pts.length - 1]?.[1] === first[1] ? -1 : undefined,
  );
  let far = 0;
  let farD = -1;
  open.forEach((p, i) => {
    const d = Math.hypot(p[0] - first[0], p[1] - first[1]);
    if (d > farD) {
      farD = d;
      far = i;
    }
  });
  const a = douglasPeucker(open.slice(0, far + 1), tolerance);
  const b = douglasPeucker([...open.slice(far), first], tolerance);
  const out = roundAll([...a, ...b.slice(1)], precision);
  return out.length >= 4 ? out : null;
}

interface Geometry {
  type?: unknown;
  coordinates?: unknown;
}

function simplifyGeometry(
  g: Geometry,
  tolerance: number,
  precision: number,
): OutFeature["geometry"] | null {
  const polygon = (rings: Ring[]): Ring[] | null => {
    const [outer, ...holes] = rings.map((r) => simplifyRing(r, tolerance, precision));
    if (!outer) return null;
    return [outer, ...holes.filter((h): h is Ring => h !== null)];
  };
  const lines = (ls: Position[][]): Position[][] =>
    ls.flatMap((l) => {
      const s = simplifyLine(l, tolerance, precision);
      return s ? [s] : [];
    });
  switch (g.type) {
    case "Polygon": {
      const c = polygon(g.coordinates as Ring[]);
      return c ? { type: "Polygon", coordinates: c } : null;
    }
    case "MultiPolygon": {
      const c = (g.coordinates as Ring[][]).flatMap((p) => {
        const s = polygon(p);
        return s ? [s] : [];
      });
      return c.length === 0
        ? null
        : c.length === 1
          ? { type: "Polygon", coordinates: c[0] }
          : { type: "MultiPolygon", coordinates: c };
    }
    case "LineString": {
      const c = simplifyLine(g.coordinates as Position[], tolerance, precision);
      return c ? { type: "LineString", coordinates: c } : null;
    }
    case "MultiLineString": {
      const c = lines(g.coordinates as Position[][]);
      return c.length === 0
        ? null
        : c.length === 1
          ? { type: "LineString", coordinates: c[0] }
          : { type: "MultiLineString", coordinates: c };
    }
    default:
      return null;
  }
}

/** Simplifies a parsed GeoJSON FeatureCollection; features are sorted by their serialised geometry. */
export function simplifyCollection(
  input: unknown,
  tolerance = TOLERANCE_DEG,
  precision = PRECISION,
): OutCollection {
  const features = (input as { features?: Array<{ geometry?: Geometry | null }> }).features;
  if (!Array.isArray(features)) throw new Error("not a FeatureCollection");
  const out: OutFeature[] = [];
  for (const f of features) {
    if (!f.geometry) continue;
    const geometry = simplifyGeometry(f.geometry, tolerance, precision);
    if (geometry) out.push({ type: "Feature", properties: {}, geometry });
  }
  const keyed = out.map((f) => ({ f, key: JSON.stringify(f.geometry) }));
  keyed.sort((x, y) => (x.key < y.key ? -1 : x.key > y.key ? 1 : 0));
  return { type: "FeatureCollection", features: keyed.map((k) => k.f) };
}

/** Compact, stable serialisation (key order is fixed by construction), with a trailing newline. */
export const serialise = (c: OutCollection): string => `${JSON.stringify(c)}\n`;

if (import.meta.main) {
  const [outDir, ...sources] = process.argv.slice(2);
  if (!outDir || sources.length === 0) {
    console.error("usage: bun scripts/simplify-basemap.ts <outDir> <source.geojson>...");
    process.exit(2);
  }
  mkdirSync(outDir, { recursive: true });
  for (const src of sources) {
    const name = basename(src).replace(/\.geojson$/, "");
    const target = OUTPUT_NAMES[name];
    if (!target)
      throw new Error(
        `unknown source ${name}; expected one of ${Object.keys(OUTPUT_NAMES).join(", ")}`,
      );
    const text = serialise(simplifyCollection(JSON.parse(readFileSync(src, "utf8"))));
    writeFileSync(join(outDir, target), text);
    console.log(`${target}: ${text.length} bytes`);
  }
}
