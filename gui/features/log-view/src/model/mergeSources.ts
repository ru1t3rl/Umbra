import type { JournalRecord } from "@umbra/engine-stream";

/**
 * Merge the records of several sources by arrival (`received_at`), ties by list order, then each list's own
 * order. Each source numbers its own records, so `seq` alone would interleave them wrongly (ASM-944 b).
 * Linear (a k-way merge of lists that are already in arrival order); one non-empty list comes back as is.
 */
export function mergeSources(
  lists: readonly (readonly JournalRecord[])[],
): readonly JournalRecord[] {
  const filled = lists.filter((l) => l.length > 0);
  const only = filled[0];
  if (only === undefined) return [];
  if (filled.length === 1) return only;
  const at = filled.map(() => 0);
  const out: JournalRecord[] = [];
  const total = filled.reduce((n, l) => n + l.length, 0);
  while (out.length < total) {
    let best = -1;
    let bestTime = 0;
    for (let k = 0; k < filled.length; k++) {
      const next = filled[k]?.[at[k] ?? 0];
      if (next === undefined) continue;
      if (best < 0 || next.received_at < bestTime) {
        best = k;
        bestTime = next.received_at;
      }
    }
    const list = filled[best];
    const record = list?.[at[best] ?? 0];
    if (record === undefined) break;
    out.push(record);
    at[best] = (at[best] ?? 0) + 1;
  }
  return out;
}
