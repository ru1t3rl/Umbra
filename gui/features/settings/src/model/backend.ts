import type { Settings } from "./settings.ts";

/**
 * Where settings persist (ADR 0074). Synchronous: the desktop backend is awaited before the first
 * render (WI-22) and satisfies this structurally. `read` returns the raw stored value or `undefined`
 * and never throws; the provider parses it.
 */
export interface SettingsBackend {
  read(): unknown;
  write(settings: Settings): void;
}
