import { createContext } from "react";
import { DEFAULT_CONFIG, type RuntimeConfig } from "../model/config.ts";

export const RuntimeConfigContext = createContext<RuntimeConfig>(DEFAULT_CONFIG);
