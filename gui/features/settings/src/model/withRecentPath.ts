import { MAX_RECENT } from "./settings.ts";

/** The recent scenario paths after loading `path`: it goes first, appears once, and the list is capped (ASM-975). */
export function withRecentPath(recent: readonly string[], path: string): string[] {
  return [path, ...recent.filter((p) => p !== path)].slice(0, MAX_RECENT);
}
