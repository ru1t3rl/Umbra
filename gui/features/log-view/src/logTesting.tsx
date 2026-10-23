// Test doubles for the WI-11 tests (not exported from index.ts; named `logTesting` because `tsc -b` refuses two
// files that emit the same `testing.d.ts`, ASM-907):
// - `rec`/`eventRec`: journal records as the journal builds them (events carry the event DTO as `detail`);
// - `FakeJournal`: a hand-written `JournalSource` (the log view reads nothing else of the connection);
// - `mount`: LogView under an `EngineStreamProvider` whose connection is only `{ journal }` (cast);
// - `installResizeObserver`: a viewport height for `VirtualList` (happy-dom has no layout, ASM-873);
// - `engineOutput`: what the mocked `@umbra/desktop` `useEngineOutputSource()` returns (undefined = web build).
import { render } from "@testing-library/react";
import {
  type EngineStreamConnection,
  EngineStreamProvider,
  type JournalLevel,
  type JournalRecord,
  type JournalSource,
  type JournalSourceName,
} from "@umbra/engine-stream";
import type { ReactElement } from "react";

let clock = 1_000;
const tick = (): number => {
  clock += 1;
  return clock;
};
/** A record. `seq` and `received_at` default to rising numbers; pass them to control the order. */
export const rec = (
  seq: number,
  extra: Partial<JournalRecord> & { source?: JournalSourceName; level?: JournalLevel } = {},
): JournalRecord => ({
  seq,
  source: "run",
  level: "info",
  kind: "running",
  message: `message ${seq}`,
  received_at: tick(),
  ...extra,
});

/** An `event` record the way `frameRecords.ts` builds it: `detail` is the whole event DTO. */
export const eventRec = (
  seq: number,
  event: { kind: string; tick?: number; entity_id?: number; target_id?: number } & Record<
    string,
    unknown
  >,
  extra: Partial<JournalRecord> = {},
): JournalRecord =>
  rec(seq, {
    source: "event",
    kind: event.kind,
    message: event.kind, // the journal's own fallback text; the log view formats from `detail`
    tick: event.tick ?? 1,
    ...(typeof event.entity_id === "number" ? { entity_id: event.entity_id } : {}),
    detail: { tick: 1, ...event },
    ...extra,
  });

export class FakeJournal implements JournalSource {
  private records: readonly JournalRecord[];
  private readonly listeners = new Set<() => void>();
  constructor(
    initial: readonly JournalRecord[] = [],
    readonly capacity = 50_000,
  ) {
    this.records = Object.freeze([...initial]);
  }
  snapshot(): readonly JournalRecord[] {
    return this.records;
  }
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  evicted(): number {
    return 0;
  }
  /** Appends and notifies once (one flush). */
  push(...more: JournalRecord[]): void {
    this.records = Object.freeze([...this.records, ...more]);
    for (const l of [...this.listeners]) l();
  }
  get listenerCount(): number {
    return this.listeners.size;
  }
}

/** Set by a test; read by the mocked `@umbra/desktop` (`undefined` is the web build). */
export const engineOutput: { current: JournalSource | undefined } = { current: undefined };

export function mount(ui: ReactElement, journal: JournalSource) {
  const connection = { journal } as unknown as EngineStreamConnection;
  const wrap = (element: ReactElement) => (
    <EngineStreamProvider connection={connection}>{element}</EngineStreamProvider>
  );
  const view = render(wrap(ui));
  return { ...view, rerender: (next: ReactElement) => view.rerender(wrap(next)) };
}

/** Makes every `ResizeObserver` report `height` at once; returns the restore function. */
export function installResizeObserver(height: number): () => void {
  const before = Reflect.get(globalThis, "ResizeObserver");
  class FakeResizeObserver {
    constructor(private readonly callback: (entries: unknown[]) => void) {}
    observe(): void {
      this.callback([{ contentRect: { height } }]);
    }
    unobserve(): void {}
    disconnect(): void {}
  }
  Reflect.set(globalThis, "ResizeObserver", FakeResizeObserver);
  return () => {
    if (before === undefined) Reflect.deleteProperty(globalThis, "ResizeObserver");
    else Reflect.set(globalThis, "ResizeObserver", before);
  };
}
