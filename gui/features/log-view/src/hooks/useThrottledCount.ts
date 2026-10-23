import { useEffect, useRef, useState } from "react";
import type { LogFilter } from "../model/filterRecords.ts";

/** Records arriving change the shown count at most this often (trailing), ASM-945. */
export const THROTTLE_MS = 1000;

export interface Count {
  readonly shown: number;
  readonly total: number;
}

/**
 * The count of the status line. It follows at once on the first render and when the filter changes (the user
 * did something and wants the answer); when only the records changed it is republished at most once per
 * `THROTTLE_MS`, so a screen reader is not told about every frame.
 */
export function useThrottledCount(shown: number, total: number, filter: LogFilter): Count {
  const [displayed, setDisplayed] = useState<Count>({ shown, total });
  const latest = useRef<Count>({ shown, total });
  latest.current = { shown, total };
  const current = useRef(displayed);
  current.current = displayed;
  const lastFilter = useRef(filter);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // biome-ignore lint/correctness/useExhaustiveDependencies: shown and total are the triggers; the values are read from refs
  useEffect(() => {
    const filterChanged = lastFilter.current !== filter;
    lastFilter.current = filter;
    const differs = (): boolean =>
      latest.current.shown !== current.current.shown ||
      latest.current.total !== current.current.total;
    if (filterChanged) {
      if (timer.current !== undefined) clearTimeout(timer.current);
      timer.current = undefined;
      if (differs()) setDisplayed(latest.current);
      return;
    }
    if (timer.current !== undefined || !differs()) return;
    timer.current = setTimeout(() => {
      timer.current = undefined;
      setDisplayed(latest.current);
    }, THROTTLE_MS);
  }, [shown, total, filter]);

  useEffect(
    () => () => {
      if (timer.current !== undefined) clearTimeout(timer.current);
    },
    [],
  );
  return displayed;
}
