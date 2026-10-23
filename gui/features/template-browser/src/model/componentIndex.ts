import type { ComponentEntry } from "@umbra/data";

/** Component id to its entry; for a duplicate id the first entry wins (ASM-947). */
export function componentIndex(
  components: readonly ComponentEntry[],
): ReadonlyMap<string, ComponentEntry> {
  const index = new Map<string, ComponentEntry>();
  for (const c of components) if (!index.has(c.id)) index.set(c.id, c);
  return index;
}
