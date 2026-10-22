import { useCallback, useMemo, useSyncExternalStore } from "react";
import { findEntity, sameEntity } from "../model/entityLookup.ts";
import type { EntitySummary } from "../types.ts";
import { useEntitySource } from "./useEntitySource.ts";

/**
 * One entity of the newest frame. Re-renders only when THAT entity's content changes and keeps the previous
 * object while it is equal, although every frame decodes to fresh objects.
 */
export function useEntity(id: number | undefined): EntitySummary | undefined {
  const source = useEntitySource();
  const getSnapshot = useMemo(() => {
    let last: EntitySummary | undefined;
    return (): EntitySummary | undefined => {
      const next = findEntity(source.getFrame()?.entities, id);
      if (last !== undefined && next !== undefined && sameEntity(last, next)) return last;
      last = next;
      return next;
    };
  }, [source, id]);
  const subscribe = useCallback(
    (onChange: () => void) => source.subscribe(() => onChange()),
    [source],
  );
  return useSyncExternalStore(subscribe, getSnapshot);
}
