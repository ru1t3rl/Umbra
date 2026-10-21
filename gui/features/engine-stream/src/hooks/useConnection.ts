import { useContext } from "react";
import type { EngineStreamConnection } from "../connection.ts";
import { EngineStreamContext } from "../context/EngineStreamContext.ts";

export function useConnection(): EngineStreamConnection {
  const connection = useContext(EngineStreamContext);
  if (!connection) throw new Error("EngineStreamProvider is missing above this component");
  return connection;
}
