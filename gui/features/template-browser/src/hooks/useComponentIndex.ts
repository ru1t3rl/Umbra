import { type ComponentEntry, useListComponents } from "@umbra/data";
import { useMemo } from "react";
import { componentIndex } from "../model/componentIndex.ts";

export interface ComponentIndexState {
  readonly index: ReadonlyMap<string, ComponentEntry>;
  /** The answer has not arrived (and has not failed) yet. */
  readonly pending: boolean;
}

const NONE: readonly ComponentEntry[] = [];

/**
 * `GET /components` once per session (ASM-947 d): never stale, never garbage collected, so remounts and other
 * templates reuse it; a session change invalidates it through `useInvalidateEngineQueries`. The provider's default retry applies (transient failures retry, 4xx do not,
 * ASM-951); a final failure falls back to JSON.
 */
export function useComponentIndex(enabled: boolean): ComponentIndexState {
  const query = useListComponents(undefined, {
    query: {
      enabled,
      staleTime: Number.POSITIVE_INFINITY,
      gcTime: Number.POSITIVE_INFINITY,
    },
  });
  const data = query.data;
  const index = useMemo(() => componentIndex(data ?? NONE), [data]);
  return { index, pending: enabled && query.isPending };
}
