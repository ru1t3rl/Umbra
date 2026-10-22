import type { SettingsBackend } from "./backend.ts";
import type { Settings } from "./settings.ts";

export const SETTINGS_STORAGE_KEY = "umbra.settings.v1";

type StorageLike = Pick<Storage, "getItem" | "setItem">;

function defaultStorage(): StorageLike | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage;
  } catch {
    return undefined;
  }
}

/** The web backend (ADR 0074). Storage errors (blocked, quota, corrupt JSON) are swallowed. */
export function createLocalStorageBackend(
  storage: StorageLike | undefined = defaultStorage(),
): SettingsBackend {
  return {
    read(): unknown {
      try {
        const text = storage?.getItem(SETTINGS_STORAGE_KEY);
        return typeof text === "string" ? JSON.parse(text) : undefined;
      } catch {
        return undefined;
      }
    },
    write(settings: Settings): void {
      try {
        storage?.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
      } catch {
        // Persistence is best effort; the in-memory settings stay valid.
      }
    },
  };
}
