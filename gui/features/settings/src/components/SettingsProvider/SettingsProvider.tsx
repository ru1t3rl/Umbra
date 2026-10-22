import { parsePreference, THEME_STORAGE_KEY } from "@umbra/ui-kit";
import { type ReactElement, type ReactNode, useCallback, useMemo, useRef, useState } from "react";
import { SettingsContext } from "../../context/SettingsContext.ts";
import { type ThemeStorage, ThemeStorageContext } from "../../context/ThemeStorageContext.ts";
import { UpdateSettingsContext } from "../../context/UpdateSettingsContext.ts";
import type { SettingsBackend } from "../../model/backend.ts";
import { createLocalStorageBackend } from "../../model/localStorageBackend.ts";
import {
  applyPatch,
  DEFAULT_SETTINGS,
  isNewerVersion,
  parseSettings,
  type SettingsPatch,
  settingsEqual,
} from "../../model/settings.ts";

export interface SettingsProviderProps {
  /**
   * Default: the localStorage backend. Read once, before the first render of the children; a changed
   * prop after mount is ignored. Cross-tab: last writer wins, there is no storage-event subscription.
   */
  backend?: SettingsBackend | undefined;
  children?: ReactNode;
}

function safeRead(backend: SettingsBackend): unknown {
  try {
    return backend.read();
  } catch {
    return undefined;
  }
}

export function SettingsProvider({ backend, children }: SettingsProviderProps): ReactElement {
  const backendRef = useRef<SettingsBackend | undefined>(undefined);
  backendRef.current ??= backend ?? createLocalStorageBackend();
  const readOnly = useRef(false);
  const warned = useRef(false);
  const [settings, setSettings] = useState(() => {
    const raw = backendRef.current ? safeRead(backendRef.current) : undefined;
    // Never downgrade a file written by a newer build: keep it in memory, do not write it back.
    readOnly.current = isNewerVersion(raw);
    return backendRef.current ? parseSettings(raw) : DEFAULT_SETTINGS;
  });
  const latest = useRef(settings);

  const update = useCallback((patch: SettingsPatch) => {
    const next = applyPatch(latest.current, patch);
    if (settingsEqual(next, latest.current)) return;
    latest.current = next;
    setSettings(next);
    if (readOnly.current) {
      if (!warned.current) {
        warned.current = true;
        console.warn("umbra settings: stored version is newer than 1; changes are not saved");
      }
      return;
    }
    try {
      backendRef.current?.write(next);
    } catch {
      // A failing backend must not break the UI.
    }
  }, []);

  const themeStorage = useMemo<ThemeStorage>(
    () => ({
      getItem: (key) => (key === THEME_STORAGE_KEY ? latest.current.appearance.theme : null),
      setItem: (key, value) => {
        if (key !== THEME_STORAGE_KEY) return;
        update({ appearance: { theme: parsePreference(value) ?? null } });
      },
    }),
    [update],
  );

  return (
    <UpdateSettingsContext.Provider value={update}>
      <ThemeStorageContext.Provider value={themeStorage}>
        <SettingsContext.Provider value={settings}>{children}</SettingsContext.Provider>
      </ThemeStorageContext.Provider>
    </UpdateSettingsContext.Provider>
  );
}
