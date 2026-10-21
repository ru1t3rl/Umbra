/** Runtime configuration read from `config.json` at start-up (not engine REST). */
export interface RuntimeConfig {
  /** Engine base URL; an empty string means the page origin (the `/api` proxy or a same-origin host). */
  engineUrl: string;
  basemap?: { styleUrl: string };
}

export const DEFAULT_CONFIG: RuntimeConfig = { engineUrl: "" };

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isHttpUrl(value: string): boolean {
  try {
    const { protocol } = new URL(value);
    return protocol === "http:" || protocol === "https:";
  } catch {
    return false;
  }
}

/** Validates the parsed JSON; missing optional fields fall back to the defaults. */
export function parseRuntimeConfig(raw: unknown): RuntimeConfig {
  if (!isRecord(raw)) throw new ConfigError("config must be a JSON object");
  const { engineUrl, basemap } = raw;
  if (engineUrl !== undefined && typeof engineUrl !== "string") {
    throw new ConfigError("engineUrl must be a string");
  }
  if (engineUrl !== undefined && engineUrl !== "" && !isHttpUrl(engineUrl)) {
    throw new ConfigError("engineUrl must be empty or an absolute http(s) URL");
  }
  const config: RuntimeConfig = { engineUrl: engineUrl ?? DEFAULT_CONFIG.engineUrl };
  if (basemap !== undefined) {
    if (!isRecord(basemap) || typeof basemap.styleUrl !== "string" || basemap.styleUrl === "") {
      throw new ConfigError("basemap.styleUrl must be a non-empty string");
    }
    config.basemap = { styleUrl: basemap.styleUrl };
  }
  return config;
}

/**
 * Fetches `${baseUrl}config.json` (baseUrl is the Vite `BASE_URL`, so sub-path deploys work). The file is
 * always emitted, so a 404 means a mis-served path: warn and use the defaults. Any other failure throws.
 */
export async function loadRuntimeConfig(
  fetchImpl: typeof fetch,
  baseUrl = "/",
): Promise<RuntimeConfig> {
  let response: Response;
  try {
    response = await fetchImpl(`${baseUrl}config.json`);
  } catch {
    throw new ConfigError("config request failed");
  }
  if (response.status === 404) {
    console.warn("config.json not found (404); using same-origin defaults");
    return DEFAULT_CONFIG;
  }
  if (!response.ok) throw new ConfigError(`config request returned ${response.status}`);
  let raw: unknown;
  try {
    raw = await response.json();
  } catch {
    throw new ConfigError("config is not valid JSON");
  }
  return parseRuntimeConfig(raw);
}
