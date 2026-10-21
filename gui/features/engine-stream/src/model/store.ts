import { createStore, type StoreApi } from "zustand";
import type { EntityFrame, RunStatus, StreamState } from "../types.ts";

export type StreamStore = StoreApi<StreamState>;

export function createStreamStore(): StreamStore {
  return createStore<StreamState>(() => ({
    status: "connecting",
    errors: 0,
    ignored: 0,
    skipped: 0,
    dropped: 0,
  }));
}

const sameClock = (a: RunStatus["clock"], b: RunStatus["clock"]): boolean =>
  a.mode === b.mode && (a.mode !== "scaled" || (b.mode === "scaled" && a.scale === b.scale));

const sameFault = (a: RunStatus["fault"], b: RunStatus["fault"]): boolean =>
  a === b ||
  (!!a &&
    !!b &&
    a.call === b.call &&
    a.capability === b.capability &&
    a.plugin === b.plugin &&
    a.message === b.message);

/** Field-wise equality, so an unchanged run keeps its object identity and its subscribers stay quiet. */
export function sameRun(a: RunStatus, b: RunStatus): boolean {
  return (
    a.state === b.state &&
    a.session === b.session &&
    a.tick === b.tick &&
    a.time_s === b.time_s &&
    a.tick_rate_hz === b.tick_rate_hz &&
    a.tick_rate_source === b.tick_rate_source &&
    a.step_pending === b.step_pending &&
    a.stop_reason === b.stop_reason &&
    sameClock(a.clock, b.clock) &&
    sameFault(a.fault, b.fault)
  );
}

export interface PendingFrame {
  frame: EntityFrame;
  run: RunStatus | undefined;
  /** Server-reported drops of every frame received since the last flush, including coalesced ones. */
  dropped: number;
  /** Entities skipped as invalid in every frame received since the last flush. */
  skipped: number;
}

/**
 * Apply the newest pending frame in ONE store update. Frames replace each other (a new `session`
 * therefore drops every entity of the old one); an unchanged run keeps its identity.
 */
export function flushPending(store: StreamStore, pending: PendingFrame): void {
  const s = store.getState();
  const run =
    pending.run === undefined ? s.run : s.run && sameRun(s.run, pending.run) ? s.run : pending.run;
  store.setState({
    frame: pending.frame,
    dropped: s.dropped + pending.dropped,
    skipped: s.skipped + pending.skipped,
    ...(run === undefined ? {} : { run }),
  });
}
