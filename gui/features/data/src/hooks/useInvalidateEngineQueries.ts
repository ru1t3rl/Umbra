import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { getGetStatusQueryKey } from "../generated/api.ts";

const STATUS_KEY = getGetStatusQueryKey()[0];

/**
 * Returns a stable function that invalidates every engine query (entities, plugins, templates, ...)
 * except the status query, which the run controls own. Used on a session change (WI-3, ASM-884).
 */
export function useInvalidateEngineQueries(): () => Promise<void> {
  const client = useQueryClient();
  return useCallback(
    () =>
      client.invalidateQueries({
        predicate: (query) => query.queryKey[0] !== STATUS_KEY,
      }),
    [client],
  );
}
