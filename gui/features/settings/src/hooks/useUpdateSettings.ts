import { useContext } from "react";
import { UpdateSettingsContext } from "../context/UpdateSettingsContext.ts";
import type { SettingsPatch } from "../model/settings.ts";

/** Stable per-section merge; re-parsed, written through once per effective change; no-op outside a provider. */
export function useUpdateSettings(): (patch: SettingsPatch) => void {
  return useContext(UpdateSettingsContext);
}
