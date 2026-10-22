import { useGetStatus, useInvalidateEngineQueries } from "@umbra/data";
import { useRunStatus } from "@umbra/engine-stream";
import { useEffect, useRef } from "react";

/**
 * Refetches engine data when the run session changes (WI-3, ASM-851/885). The first session seen is
 * compared with the cached REST status session (if any), which closes the startup gap; later changes
 * of the stream session always invalidate. The status query is read from the cache only.
 */
export function useSessionInvalidation(): void {
  const session = useRunStatus()?.session;
  const cached = useGetStatus({ query: { enabled: false } }).data?.run?.session;
  const invalidate = useInvalidateEngineQueries();
  const seen = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (session === undefined) return;
    const previous = seen.current ?? cached;
    seen.current = session;
    if (previous !== undefined && previous !== session) void invalidate();
  }, [session, cached, invalidate]);
}
