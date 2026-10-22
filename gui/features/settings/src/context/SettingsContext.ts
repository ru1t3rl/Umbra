import { createContext } from "react";
import { DEFAULT_SETTINGS, type Settings } from "../model/settings.ts";

export const SettingsContext = createContext<Settings>(DEFAULT_SETTINGS);
