import { useContext } from "react";
import { RuntimeConfigContext } from "../context/RuntimeConfigContext.ts";
import type { RuntimeConfig } from "../model/config.ts";

export function useRuntimeConfig(): RuntimeConfig {
  return useContext(RuntimeConfigContext);
}
