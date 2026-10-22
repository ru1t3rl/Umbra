import { useContext } from "react";
import { SettingsContext } from "../context/SettingsContext.ts";
import type { Settings } from "../model/settings.ts";

/** The current settings; the defaults outside a `SettingsProvider`. */
export function useSettings(): Settings {
  return useContext(SettingsContext);
}
