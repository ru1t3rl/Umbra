import type { EntitySource } from "../types.ts";
import { useConnection } from "./useConnection.ts";

/** Stable imperative source of entity frames, for consumers that must not re-render per frame. */
export function useEntitySource(): EntitySource {
  return useConnection().source;
}
