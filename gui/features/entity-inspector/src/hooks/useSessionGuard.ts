import type { EntitySource } from "@umbra/engine-stream";
import { useEffect, useRef } from "react";

/**
 * Calls `onChange` once when a frame of another session than the first one seen arrives and `active` is true
 * (something is selected): the selected id belongs to the old session (ASM-807). The first frame (or the one
 * already in the source) sets the baseline; sessions are tracked also while inactive.
 */
export function useSessionGuard(source: EntitySource, active: boolean, onChange: () => void): void {
  const callback = useRef(onChange);
  callback.current = onChange;
  const enabled = useRef(active);
  enabled.current = active;
  useEffect(() => {
    let session = source.getFrame()?.session;
    return source.subscribe((frame) => {
      if (session === undefined) session = frame.session;
      else if (frame.session !== session) {
        session = frame.session;
        if (enabled.current) callback.current();
      }
    });
  }, [source]);
}
