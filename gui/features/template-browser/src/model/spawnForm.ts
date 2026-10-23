import type { SpawnRequest } from "@umbra/data";

export const FIELDS = ["lat", "lon", "alt", "heading", "speed", "name", "force", "group"] as const;
export type FieldName = (typeof FIELDS)[number];
/** The fields that hold a number (shown with a decimal keypad). */
export const NUMERIC_FIELDS: readonly FieldName[] = ["lat", "lon", "alt", "heading", "speed"];
export type FormValues = Readonly<Record<FieldName, string>>;
export const EMPTY_VALUES: FormValues = {
  lat: "",
  lon: "",
  alt: "",
  heading: "",
  speed: "",
  name: "",
  force: "",
  group: "",
};

/** A validation problem: a message key of messages.ts plus its params; the field label is added by the caller. */
export type FieldProblem =
  | { readonly kind: "required" }
  | { readonly kind: "number" }
  | { readonly kind: "range"; readonly min: number; readonly max: number };
export type FormProblems = Readonly<Partial<Record<FieldName, FieldProblem>>>;

const RANGES = { lat: [-90, 90], lon: [-180, 180] } as const;

/** A finite number from trimmed text, or undefined (`1,5`, `abc`, `NaN`, `Infinity` are not numbers). */
export function parseNumber(text: string): number | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return undefined;
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : undefined;
}

export type SpawnValidation =
  | { readonly ok: true; readonly body: (template: string) => SpawnRequest }
  | { readonly ok: false; readonly problems: FormProblems };

/** Validates on submit: every problem at once (ASM-949); the engine judges the ranges of alt, heading and speed. */
export function validateSpawn(values: FormValues): SpawnValidation {
  const problems: { -readonly [K in FieldName]?: FieldProblem } = {};
  const check = (field: "lat" | "lon" | "alt" | "heading" | "speed"): number | undefined => {
    const text = values[field];
    if (text.trim() === "") {
      if (field === "lat" || field === "lon") problems[field] = { kind: "required" };
      return undefined;
    }
    const n = parseNumber(text);
    if (n === undefined) {
      problems[field] = { kind: "number" };
      return undefined;
    }
    if (field === "lat" || field === "lon") {
      const [min, max] = RANGES[field];
      if (n < min || n > max) {
        problems[field] = { kind: "range", min, max };
        return undefined;
      }
    }
    return n;
  };
  const lat = check("lat");
  const lon = check("lon");
  const alt = check("alt");
  const heading = check("heading");
  const speed = check("speed");
  if (lat === undefined || lon === undefined || Object.keys(problems).length > 0)
    return { ok: false, problems };
  const text = (field: "name" | "force" | "group"): string | undefined => {
    const v = values[field].trim();
    return v === "" ? undefined : v;
  };
  const name = text("name");
  const force = text("force");
  const group = text("group");
  return {
    ok: true,
    body: (template) => ({
      template,
      lat_deg: lat,
      lon_deg: lon,
      ...(alt === undefined ? {} : { alt_m: alt }),
      ...(heading === undefined ? {} : { heading_deg: heading }),
      ...(speed === undefined ? {} : { speed_mps: speed }),
      ...(name === undefined ? {} : { name }),
      ...(force === undefined ? {} : { force }),
      ...(group === undefined ? {} : { group }),
    }),
  };
}

const SPAWNABLE = new Set(["ready", "running", "paused"]);
/** Spawning is valid in ready, running and paused only (the contract of `POST /entities`). */
export const canSpawn = (state: string | undefined): boolean =>
  state !== undefined && SPAWNABLE.has(state);
