import { createContext } from "react";
import type { SettingsPatch } from "../model/settings.ts";

export const UpdateSettingsContext = createContext<(patch: SettingsPatch) => void>(() => {});
