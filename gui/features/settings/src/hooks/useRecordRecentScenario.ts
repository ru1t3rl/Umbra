import { useCallback, useEffect, useRef } from "react";
import { withRecentPath } from "../model/withRecentPath.ts";
import { useSettings } from "./useSettings.ts";
import { useUpdateSettings } from "./useUpdateSettings.ts";

/**
 * A stable callback that records a loaded scenario path as the newest recent one and persists it (ASM-975).
 * It reads the latest list (a ref synced in an effect, not during render), so two calls in a row keep both.
 */
export function useRecordRecentScenario(): (path: string) => void {
  const update = useUpdateSettings();
  const recent = useSettings().scenarios.recent;
  const latest = useRef<readonly string[]>(recent);
  useEffect(() => {
    latest.current = recent;
  }, [recent]);
  return useCallback(
    (path: string) => {
      const next = withRecentPath(latest.current, path);
      latest.current = next;
      update({ scenarios: { recent: next } });
    },
    [update],
  );
}
