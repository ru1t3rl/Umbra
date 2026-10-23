import type { EntitySource, EntitySummary } from "@umbra/engine-stream";
import { useEffect, useState } from "react";

/** The row model is rebuilt at most this often (performance plan 2, ASM-804). */
export const LIST_INTERVAL_MS = 250;

const NONE: readonly EntitySummary[] = [];

/**
 * The entities of the newest frame, republished at most every `LIST_INTERVAL_MS`. The first frame always shows
 * at once (the interval starts after a rebuild, not at mount); a frame inside the interval shows after its
 * remainder (newest wins). Frames reach React only through this state, so the list renders a handful of times
 * per second whatever the stream rate (ADR 0045).
 */
export function useThrottledEntities(source: EntitySource): readonly EntitySummary[] {
  const [entities, setEntities] = useState<readonly EntitySummary[]>(
    () => source.getFrame()?.entities ?? NONE,
  );
  useEffect(() => {
    let latest = source.getFrame()?.entities;
    let dirty = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setEntities(latest ?? NONE);
    const cooldown = (): void => {
      timer = setTimeout(() => {
        timer = undefined;
        if (!dirty || latest === undefined) return;
        dirty = false;
        setEntities(latest);
        cooldown();
      }, LIST_INTERVAL_MS);
    };
    if (latest !== undefined) cooldown();
    const unsubscribe = source.subscribe((frame) => {
      latest = frame.entities;
      if (timer !== undefined) {
        dirty = true;
        return;
      }
      setEntities(frame.entities);
      cooldown();
    });
    return () => {
      unsubscribe();
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [source]);
  return entities;
}
