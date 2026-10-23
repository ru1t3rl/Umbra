import type { EntityFrame, EntitySource, EntitySummary } from "@umbra/engine-stream";
import { useEffect, useState } from "react";
import { findById } from "../model/findById.ts";

export interface SelectedEntity {
  /** The newest frame (for naming contacts), if any arrived. */
  readonly frame: EntityFrame | undefined;
  /** The last values seen; kept when the entity leaves the frame. */
  readonly entity: EntitySummary | undefined;
  /** In the newest frame. */
  readonly present: boolean;
}

interface Held extends SelectedEntity {
  /** The id these values belong to. */
  readonly id: number | null;
}

const EMPTY: SelectedEntity = { frame: undefined, entity: undefined, present: false };
const NOTHING: Held = { ...EMPTY, id: null };

const fromFrame = (frame: EntityFrame | undefined, id: number, previous: Held): Held => {
  const kept = previous.id === id ? previous.entity : undefined;
  if (frame === undefined) return { ...EMPTY, id, entity: kept };
  const found = findById(frame.entities, id);
  return found === undefined
    ? { id, frame, entity: kept, present: false }
    : { id, frame, entity: found, present: true };
};

/**
 * The selected entity from the entity source, one state update per applied frame (a single entity, so one
 * small render, not per entity). The held values carry their id: during the render after an id change they
 * are replaced by the entity derived from the source's current frame.
 */
export function useSelectedEntity(source: EntitySource, id: number | null): SelectedEntity {
  const [held, setHeld] = useState<Held>(NOTHING);
  useEffect(() => {
    if (id === null) {
      setHeld(NOTHING);
      return;
    }
    setHeld((prev) => fromFrame(source.getFrame(), id, prev));
    return source.subscribe((frame) => setHeld((prev) => fromFrame(frame, id, prev)));
  }, [source, id]);
  if (id === null) return EMPTY;
  // After an id change the held values are the previous entity's: derive the new one during render (pure,
  // no state write) so no commit shows the old entity and none shows a false "gone" state.
  return held.id === id ? held : fromFrame(source.getFrame(), id, NOTHING);
}
