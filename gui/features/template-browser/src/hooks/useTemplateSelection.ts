import { useListTemplates } from "@umbra/data";
import { useRunStatus } from "@umbra/engine-stream";
import { useCallback, useEffect, useState } from "react";

export interface TemplateSelection {
  /** The chosen template id, or null (nothing chosen, or the choice no longer applies). */
  selectedId: string | null;
  select: (id: string) => void;
}

interface Picked {
  id: string;
  /** The session the choice was made in (undefined: before the first run frame; that choice ends with the first frame). */
  session: number | undefined;
}

/**
 * The selected template of a page that shows `TemplateList` and `TemplateDetail`. The selection belongs to the
 * scenario: it is cleared when the session changes or the scenario unloads (idle), and a pick made before the first
 * run frame ends when the first session is known, and a selected id that the template list no longer holds is dropped. While the list is not loaded yet the choice is kept (ASM-993).
 */
export function useTemplateSelection(): TemplateSelection {
  const status = useRunStatus();
  const idle = status?.state === "idle";
  const session = status?.session;
  const [picked, setPicked] = useState<Picked | null>(null);
  // The same query as the list's, so it costs no request of its own.
  const listed = useListTemplates({ query: { enabled: !idle } }).data;
  // Strict: a pick made before the first frame (session unknown) ends when the first session is known.
  const sameSession = picked !== null && picked.session === session;
  const stillListed =
    picked !== null && (listed === undefined || listed.some((t) => t.id === picked.id));
  const selectedId = picked !== null && !idle && sameSession && stillListed ? picked.id : null;

  // A choice that no longer applies is forgotten, so it does not come back with the next scenario.
  useEffect(() => {
    if (picked !== null && selectedId === null) setPicked(null);
  }, [picked, selectedId]);

  const select = useCallback((id: string): void => setPicked({ id, session }), [session]);
  return { selectedId, select };
}
