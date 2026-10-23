import type { JournalRecord, JournalSource } from "@umbra/engine-stream";
import { useEffect, useRef, useState } from "react";
import { mergeSources } from "../model/mergeSources.ts";

type Snapshots = readonly (readonly JournalRecord[])[];

const snapshots = (sources: readonly JournalSource[]): Snapshots =>
  sources.map((s) => s.snapshot());

/**
 * The merged records of the sources. A journal notification (at most one per animation frame) is one state
 * update here, and only when a snapshot actually changed; the lists are never copied per record, and a single
 * source's array is passed through.
 */
export function useRecords(sources: readonly JournalSource[]): readonly JournalRecord[] {
  const [state, setState] = useState(() => {
    const snaps = snapshots(sources);
    return { snaps, records: mergeSources(snaps) };
  });
  const last = useRef(state.snaps);
  useEffect(() => {
    const update = (): void => {
      const next = snapshots(sources);
      const prev = last.current;
      if (next.length === prev.length && next.every((s, i) => s === prev[i])) return;
      last.current = next;
      setState({ snaps: next, records: mergeSources(next) });
    };
    update(); // a source may have changed or published between render and subscribe
    const unsubscribes = sources.map((s) => s.subscribe(update));
    return () => {
      for (const unsubscribe of unsubscribes) unsubscribe();
    };
  }, [sources]);
  return state.records;
}
