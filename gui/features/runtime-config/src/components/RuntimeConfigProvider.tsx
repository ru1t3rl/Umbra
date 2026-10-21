import { createContext, type ReactElement, type ReactNode, useContext } from "react";
import { DEFAULT_CONFIG, type RuntimeConfig } from "../model/config.ts";

const RuntimeConfigContext = createContext<RuntimeConfig>(DEFAULT_CONFIG);

export interface RuntimeConfigProviderProps {
  config: RuntimeConfig;
  children?: ReactNode;
}

export function RuntimeConfigProvider({
  config,
  children,
}: RuntimeConfigProviderProps): ReactElement {
  return <RuntimeConfigContext.Provider value={config}>{children}</RuntimeConfigContext.Provider>;
}

export function useRuntimeConfig(): RuntimeConfig {
  return useContext(RuntimeConfigContext);
}
