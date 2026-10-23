/** Shown for a missing or non-finite value (never "NaN"). */
export const NO_VALUE = "—";

const fixed = (value: number, decimals: number): string => {
  if (!Number.isFinite(value)) return NO_VALUE;
  const text = value.toFixed(decimals);
  // A value that rounds to zero has no sign.
  return Number(text) === 0 ? (0).toFixed(decimals) : text;
};

export interface PositionText {
  readonly lat: string;
  readonly lon: string;
  readonly alt: string;
}

/** Degrees with 6 decimals, altitude with 1. Numbers only: the Readout carries the unit. */
export function formatPosition(p: {
  readonly lat_deg: number;
  readonly lon_deg: number;
  readonly alt_m: number;
}): PositionText {
  return { lat: fixed(p.lat_deg, 6), lon: fixed(p.lon_deg, 6), alt: fixed(p.alt_m, 1) };
}

/** Degrees clockwise from north, one decimal, normalised into [0, 360). */
export function formatHeading(deg: number): string {
  if (!Number.isFinite(deg)) return NO_VALUE;
  let text = fixed(((deg % 360) + 360) % 360, 1);
  if (text === "360.0") text = "0.0";
  return text;
}

/** Metres per second, one decimal. */
export function formatSpeed(mps: number): string {
  return fixed(mps, 1);
}

/** "next / len", as the stream gives it. */
export function formatRoute(
  route: { readonly next: number; readonly len: number } | null | undefined,
): string {
  return route === undefined || route === null ? NO_VALUE : `${route.next} / ${route.len}`;
}
