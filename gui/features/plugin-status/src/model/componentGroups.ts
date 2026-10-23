import { type ComponentEntry, ComponentKind } from "@umbra/data";

export interface ComponentGroup {
  readonly kind: string;
  readonly entries: readonly ComponentEntry[];
}

/**
 * Entries grouped by kind: the contract's kinds in table order, then unknown kinds in first-seen order; entries
 * by id. A kind without entries has no group (ASM-955 e).
 */
export function componentGroups(entries: readonly ComponentEntry[]): ComponentGroup[] {
  const byKind = new Map<string, ComponentEntry[]>();
  for (const entry of entries) {
    const list = byKind.get(entry.kind);
    if (list) list.push(entry);
    else byKind.set(entry.kind, [entry]);
  }
  const known: string[] = Object.values(ComponentKind);
  const order = [...known, ...[...byKind.keys()].filter((k) => !known.includes(k))];
  return order.flatMap((kind) => {
    const list = byKind.get(kind);
    return list
      ? [{ kind, entries: [...list].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)) }]
      : [];
  });
}
