import type { ReactElement, ReactNode } from "react";
import { RuntimeConfigContext } from "../../context/RuntimeConfigContext.ts";
import type { RuntimeConfig } from "../../model/config.ts";

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
