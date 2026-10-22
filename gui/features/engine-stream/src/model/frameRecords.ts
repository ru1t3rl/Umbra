// Pure mapping from a decoded frame to journal drafts (ADR 0071). The order within one frame is: session,
// run, events, then loss records.

import type { JournalLevel, RunStatus } from "../types.ts";
import type { Decoded } from "./decode.ts";
import type { JournalDraft } from "./journal.ts";
import { sameFault } from "./store.ts";

type FrameMessage = Extract<Decoded, { kind: "frame" }>;

/** What the previous frames showed, to emit session and run records only on change. */
export interface RecorderState {
  readonly session: number | undefined;
  readonly runState: RunStatus["state"] | undefined;
  readonly stopReason: RunStatus["stop_reason"];
  readonly fault: RunStatus["fault"];
}

export const INITIAL_RECORDER_STATE: RecorderState = {
  session: undefined,
  runState: undefined,
  stopReason: undefined,
  fault: undefined,
};

const ERROR_REASONS: readonly string[] = ["plugin_fault", "network_fault", "error"];

function runLevel(run: RunStatus): JournalLevel {
  const reason = run.stop_reason;
  if (reason !== undefined && reason !== null) {
    if (ERROR_REASONS.includes(reason)) return "error";
    if (reason === "unknown") return "warning";
  }
  return run.fault && run.state !== "stopped" ? "warning" : "info";
}

function runMessage(run: RunStatus): string {
  let text = `Run ${run.state}`;
  if (run.stop_reason) text += ` (${run.stop_reason})`;
  if (run.fault) {
    text += `: ${run.fault.plugin} ${run.fault.capability}/${run.fault.call}: ${run.fault.message}`;
  }
  return text;
}

function eventMessage(kind: string, entityId: unknown, targetId: unknown): string {
  let text = kind;
  if (typeof entityId === "number") text += ` entity ${entityId}`;
  if (typeof targetId === "number") text += ` -> ${targetId}`;
  return text;
}

export function recordFrame(
  prev: RecorderState,
  message: FrameMessage,
): { drafts: JournalDraft[]; next: RecorderState } {
  const { frame, run, events } = message;
  const drafts: JournalDraft[] = [];
  const session = frame.session;

  if (session !== prev.session) {
    drafts.push({
      source: "run",
      level: "info",
      kind: "session",
      message: `Session ${session}`,
      session,
      tick: frame.tick,
    });
  }

  let runState = prev.runState;
  let stopReason = prev.stopReason;
  let fault = prev.fault;
  if (
    run &&
    (run.state !== prev.runState ||
      run.stop_reason !== prev.stopReason ||
      !sameFault(run.fault, prev.fault))
  ) {
    drafts.push({
      source: "run",
      level: runLevel(run),
      kind: run.state,
      message: runMessage(run),
      session,
      ...(typeof run.tick === "number" ? { tick: run.tick } : {}),
      ...(typeof run.time_s === "number" ? { time_s: run.time_s } : {}),
    });
  }
  if (run) {
    runState = run.state;
    stopReason = run.stop_reason;
    fault = run.fault;
  }

  for (const event of events) {
    drafts.push({
      source: "event",
      level: "info",
      kind: event.kind,
      message: eventMessage(event.kind, event.entity_id, event.target_id),
      tick: event.tick,
      session,
      ...(typeof event.entity_id === "number" ? { entity_id: event.entity_id } : {}),
      detail: event,
    });
  }

  if (message.events_truncated) {
    drafts.push({
      source: "event",
      level: "warning",
      kind: "events_truncated",
      message: "Events were lost beyond the per-frame cap",
      tick: frame.tick,
      session,
    });
  }
  if (frame.dropped > 0) {
    drafts.push({
      source: "event",
      level: "warning",
      kind: "frames_dropped",
      message: `The server dropped ${frame.dropped} frame(s) for this client`,
      tick: frame.tick,
      session,
      detail: { dropped: frame.dropped },
    });
  }

  return { drafts, next: { session, runState, stopReason, fault } };
}
