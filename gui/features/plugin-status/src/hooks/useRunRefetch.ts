import { useEffect, useRef } from "react";

type Refetch = (options: { cancelRefetch: boolean }) => Promise<unknown>;

/**
 * Read a query again when `key` changes while the query stays enabled (ASM-957). A key of `undefined` (no frame
 * yet) and a change of `enabled` start nothing: mounting and leaving idle already fetch. `cancelRefetch: false`
 * joins a request that is already running, so two readers of one query cause one request.
 */
export function useRunRefetch(refetch: Refetch, enabled: boolean, key: string | undefined): void {
  const previous = useRef({ enabled, key });
  useEffect(() => {
    const before = previous.current;
    previous.current = { enabled, key };
    if (
      before.enabled &&
      enabled &&
      before.key !== undefined &&
      key !== undefined &&
      before.key !== key
    )
      void refetch({ cancelRefetch: false });
  }, [enabled, key, refetch]);
}
