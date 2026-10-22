import { type Affiliation, parsePreference, type ThemePreference } from "@umbra/ui-kit";

export type EngineMode = "managed" | "remote";

/** Settings v1 (ADR 0074, ASM-829, ASM-889). */
export interface Settings {
  version: 1;
  appearance: { theme: ThemePreference | null };
  stream: { maxRateHz: number };
  map: { affiliationOverrides: Record<string, Affiliation> };
  journal: { capacity: number };
  scenarios: { recent: string[] };
  engine: { mode: EngineMode; remoteUrl: string; scenarioRoot: string; pluginPath: string };
}

/** A per-section partial update; `version` is not patchable. */
export type SettingsPatch = {
  [K in Exclude<keyof Settings, "version">]?: Partial<Settings[K]>;
};

export const DEFAULT_SETTINGS: Settings = {
  version: 1,
  appearance: { theme: null },
  stream: { maxRateHz: 30 },
  map: { affiliationOverrides: {} },
  journal: { capacity: 10_000 },
  scenarios: { recent: [] },
  engine: { mode: "managed", remoteUrl: "", scenarioRoot: "", pluginPath: "" },
};

export const MAX_RECENT = 10;
export const MAX_OVERRIDES = 256;
export const MAX_STRING = 4096;
const AFFILIATIONS: readonly string[] = ["friend", "hostile", "neutral", "unknown"];
const FORBIDDEN_KEYS: readonly string[] = ["__proto__", "constructor", "prototype"];

type Rec = Record<string, unknown>;
const isRec = (v: unknown): v is Rec => typeof v === "object" && v !== null && !Array.isArray(v);

/** Property read that never throws (hostile getters). */
function get(o: unknown, key: string): unknown {
  if (!isRec(o)) return undefined;
  try {
    return o[key];
  } catch {
    return undefined;
  }
}

function num(v: unknown, min: number, max: number, fallback: number): number {
  if (typeof v !== "number" || !Number.isFinite(v)) return fallback;
  return Math.min(max, Math.max(min, Math.round(v)));
}

function str(v: unknown, fallback: string): string {
  return typeof v === "string" ? v.slice(0, MAX_STRING) : fallback;
}

function overrides(v: unknown): Record<string, Affiliation> {
  // Null prototype while building, so no key can hit Object.prototype; copied to a plain object below.
  const out: Record<string, Affiliation> = Object.create(null);
  if (!isRec(v)) return {};
  let keys: string[];
  try {
    keys = Object.keys(v);
  } catch {
    return {};
  }
  let count = 0;
  for (const raw of keys) {
    if (raw.length > MAX_STRING) continue;
    const key = raw.trim().toLowerCase();
    const value = get(v, raw);
    if (key === "" || FORBIDDEN_KEYS.includes(key)) continue;
    if (typeof value !== "string" || !AFFILIATIONS.includes(value)) continue;
    // Keys that fold to the same lower-case key: the last one wins.
    if (!(key in out)) {
      if (count >= MAX_OVERRIDES) continue;
      count += 1;
    }
    out[key] = value as Affiliation;
  }
  return { ...out };
}

function recent(v: unknown): string[] {
  if (!Array.isArray(v)) return [];
  const out: string[] = [];
  for (const item of v) {
    if (typeof item !== "string") continue;
    const entry = item.trim().slice(0, MAX_STRING);
    if (entry !== "" && !out.includes(entry)) out.push(entry);
    if (out.length === MAX_RECENT) break;
  }
  return out;
}

/**
 * Validates untrusted stored data (ADR 0074). Never throws; drops unknown keys, clamps ranges, falls
 * back per key; idempotent. A `version` above 1 is read as v1 and `warn` is called once.
 */
export function parseSettings(
  raw: unknown,
  warn: (message: string) => void = console.warn,
): Settings {
  try {
    return parseUnsafe(raw, warn);
  } catch {
    warn("umbra settings: stored settings are unreadable; using the defaults");
    return DEFAULT_SETTINGS;
  }
}

/** True when the stored data declares a version newer than this build understands. */
export function isNewerVersion(raw: unknown): boolean {
  const version = get(raw, "version");
  return typeof version === "number" && version > 1;
}

function parseUnsafe(raw: unknown, warn: (message: string) => void): Settings {
  const d = DEFAULT_SETTINGS;
  const version = get(raw, "version");
  if (typeof version === "number" && version > 1) {
    warn(`umbra settings: version ${version} is newer than 1; reading it as version 1`);
  }
  const stream = get(raw, "stream");
  const journal = get(raw, "journal");
  const engine = get(raw, "engine");
  const mode = get(engine, "mode");
  const theme = get(get(raw, "appearance"), "theme");
  return {
    version: 1,
    appearance: {
      theme: typeof theme === "string" ? (parsePreference(theme) ?? null) : null,
    },
    stream: { maxRateHz: num(get(stream, "maxRateHz"), 1, 60, d.stream.maxRateHz) },
    map: { affiliationOverrides: overrides(get(get(raw, "map"), "affiliationOverrides")) },
    journal: { capacity: num(get(journal, "capacity"), 1000, 50_000, d.journal.capacity) },
    scenarios: { recent: recent(get(get(raw, "scenarios"), "recent")) },
    engine: {
      mode: mode === "managed" || mode === "remote" ? mode : d.engine.mode,
      remoteUrl: str(get(engine, "remoteUrl"), d.engine.remoteUrl),
      scenarioRoot: str(get(engine, "scenarioRoot"), d.engine.scenarioRoot),
      pluginPath: str(get(engine, "pluginPath"), d.engine.pluginPath),
    },
  };
}

/** Per-section shallow merge of a patch onto `current`, then re-parsed (clamped). */
export function applyPatch(current: Settings, patch: SettingsPatch): Settings {
  const merged: Record<string, unknown> = { ...current };
  for (const key of Object.keys(patch) as Array<keyof SettingsPatch>) {
    merged[key] = { ...current[key], ...patch[key] };
  }
  const next = parseSettings(merged);
  // Structural sharing: unchanged sections keep their identity (stable memo inputs).
  const shared: Record<string, unknown> = { ...next };
  for (const key of Object.keys(next) as Array<keyof Settings>) {
    if (JSON.stringify(next[key]) === JSON.stringify(current[key])) shared[key] = current[key];
  }
  return shared as unknown as Settings;
}

export function settingsEqual(a: Settings, b: Settings): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
