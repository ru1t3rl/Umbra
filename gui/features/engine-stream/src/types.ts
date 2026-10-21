import type {
  Hello,
  RouteProgress,
  Subscription,
  EntitySummary as WireEntitySummary,
  RunStatus as WireRunStatus,
} from "./generated/stream";

export type { Subscription };

/**
 * GUI domain types (ASM-359). The generated wire types close their enums; a newer server may send a value
 * this client does not know, so the decoder maps those to "unknown" instead of guessing.
 */

/**
 * An entity as the GUI holds it. **Control gating must test `ownership === "owned"`**: both `"remote"` and
 * `"unknown"` mean "do not offer controls".
 */
export type EntitySummary = Readonly<
  Omit<WireEntitySummary, "ownership" | "route"> & {
    ownership: "owned" | "remote" | "unknown";
    route?: Readonly<RouteProgress> | null;
  }
>;

export type RunState = WireRunStatus["state"] | "unknown";
export type Clock = WireRunStatus["clock"] | { mode: "unknown" };

/** The run status; `tick` and `time_s` are always kept, even when `state` or `clock` is unknown. */
export type RunStatus = Readonly<
  Omit<WireRunStatus, "state" | "clock" | "stop_reason"> & {
    state: RunState;
    clock: Clock;
    stop_reason?: NonNullable<WireRunStatus["stop_reason"]> | "unknown" | null;
  }
>;

/** Connection lifecycle as the UI sees it. `open` means the server's `hello` arrived. */
export type StreamStatus = "connecting" | "open" | "reconnecting" | "closed" | "incompatible";

/** The newest snapshot (ADR 0045). `entities` is frozen and ascending by id. */
export interface EntityFrame {
  readonly session: number;
  readonly tick: number;
  readonly time_s: number;
  readonly seq: number;
  readonly dropped: number;
  readonly entities: readonly EntitySummary[];
}

export interface StreamState {
  status: StreamStatus;
  hello?: Hello;
  run?: RunStatus;
  frame?: EntityFrame;
  /** `error` messages received from the server. */
  errors: number;
  /** Messages that were not understood: bad JSON, unknown `type`, or an invalid shape. */
  ignored: number;
  /** Entities left out of frames because a required number was missing or mistyped. */
  skipped: number;
  /** Frames the server dropped for this client, summed over every received frame (also coalesced ones). */
  dropped: number;
}

/** Imperative read access for consumers that must not re-render per frame (the map layer). */
export interface EntitySource {
  getFrame(): EntityFrame | undefined;
  /** Called once per applied frame (at most once per animation frame). Returns the unsubscribe function. */
  subscribe(listener: (frame: EntityFrame) => void): () => void;
}

/** The subset of the browser `WebSocket` the connection uses; tests supply a fake. */
export interface WebSocketLike {
  send(data: string): void;
  close(code?: number, reason?: string): void;
  onopen: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code: number; reason: string }) => void) | null;
  onerror: (() => void) | null;
}
