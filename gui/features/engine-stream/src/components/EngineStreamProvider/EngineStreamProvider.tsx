import type { ReactElement, ReactNode } from "react";
import type { EngineStreamConnection } from "../../connection.ts";
import { EngineStreamContext } from "../../context/EngineStreamContext.ts";

export interface EngineStreamProviderProps {
  /** Created once by the app bootstrap (`connectEngineStream`); the provider does not own its lifetime. */
  connection: EngineStreamConnection;
  children?: ReactNode;
}

export function EngineStreamProvider({
  connection,
  children,
}: EngineStreamProviderProps): ReactElement {
  return <EngineStreamContext.Provider value={connection}>{children}</EngineStreamContext.Provider>;
}
