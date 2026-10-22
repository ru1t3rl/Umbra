// The bounded event journal (ADR 0071): a ring of records, staged on append and published once per flush.
import type { JournalRecord, JournalSource } from "../types.ts";

/** A record before the journal numbers and stamps it. */
export type JournalDraft = Omit<JournalRecord, "seq" | "received_at">;

export interface Journal {
  readonly source: JournalSource;
  /** Stage a record; it becomes visible at the next `publish`. */
  append(draft: JournalDraft): void;
  /** Swap in the new snapshot and notify once. Returns whether anything changed. */
  publish(): boolean;
}

export function createJournal(capacity: number, now: () => number = Date.now): Journal {
  if (!Number.isInteger(capacity) || capacity < 1) {
    throw new RangeError(`journal capacity must be a positive integer, got ${capacity}`);
  }
  let snapshot: readonly JournalRecord[] = Object.freeze([]);
  let staged: JournalRecord[] = [];
  let nextSeq = 1;
  let evicted = 0;
  const listeners = new Set<() => void>();

  const source: JournalSource = {
    snapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    capacity,
    evicted: () => evicted,
  };

  return {
    source,
    append(draft) {
      staged.push({ ...draft, seq: nextSeq, received_at: now() });
      nextSeq += 1;
      if (staged.length >= capacity * 2) {
        // A burst without a flush: the oldest staged records could never be seen anyway.
        evicted += staged.length - capacity;
        staged = staged.slice(-capacity);
      }
    },
    publish() {
      if (staged.length === 0) return false;
      const merged = snapshot.concat(staged);
      const kept = merged.length > capacity ? merged.slice(merged.length - capacity) : merged;
      evicted += merged.length - kept.length;
      staged = [];
      snapshot = Object.freeze(kept);
      for (const listener of [...listeners]) {
        try {
          listener();
        } catch (error) {
          console.error("engine stream: a journal listener threw", error); // the others still run
        }
      }
      return true;
    },
  };
}
