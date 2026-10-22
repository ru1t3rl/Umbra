// Test doubles for this package's tests (not exported from index.ts): a real engine-stream connection over a fake socket.
import {
  connectEngineStream,
  type EngineStreamConnection,
  type WebSocketLike,
} from "@umbra/engine-stream";

class FakeSocket implements WebSocketLike {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  send(): void {}
  close(): void {}
}

export interface Rig {
  connection: EngineStreamConnection;
  socket: FakeSocket;
  /** Deliver messages, then run the animation frame the connection scheduled. */
  push(...messages: unknown[]): void;
  /** Make the socket drop (status becomes reconnecting). */
  drop(): void;
  /** Fire the backoff timer: a new socket is created. */
  reconnect(): void;
}

export function rig(): Rig {
  const sockets: FakeSocket[] = [];
  let pending: (() => void)[] = [];
  let timers: (() => void)[] = [];
  const connection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    scheduleFrame: (cb) => {
      pending.push(cb);
      return pending.length;
    },
    cancelFrame: () => {
      pending = [];
    },
    setTimer: (fn) => {
      timers.push(fn);
      return timers.length;
    },
    clearTimer: () => {},
  });
  const socket = (): FakeSocket => {
    const s = sockets.at(-1);
    if (!s) throw new Error("no socket");
    return s;
  };
  return {
    connection,
    get socket() {
      return socket();
    },
    push(...messages) {
      for (const m of messages) socket().onmessage?.({ data: JSON.stringify(m) });
      const run = pending;
      pending = [];
      for (const f of run) f();
    },
    drop() {
      socket().onclose?.({ code: 1006, reason: "" });
    },
    reconnect() {
      const run = timers;
      timers = [];
      for (const f of run) f();
    },
  };
}

export const hello = (protocol = "umbra.stream.v1") => ({
  type: "hello",
  protocol,
  api_version: "1.0.0",
  encoding: "json",
  settings: {
    default_rate_hz: 30,
    max_rate_hz: 60,
    max_entity_ids: 10000,
    max_events_per_frame: 4096,
    max_message_bytes: 65536,
    queue_frames: 4,
    send_timeout_s: 10,
  },
});

export const entity = (id: number) => ({
  id,
  lat_deg: 10,
  lon_deg: 20,
  alt_m: 0,
  heading_deg: 90,
  speed_mps: 5,
  ownership: "owned",
});

export const run = (tick: number, extra: Record<string, unknown> = {}) => ({
  state: "running",
  session: 1,
  clock: { mode: "realtime" },
  step_pending: false,
  tick,
  time_s: tick / 30,
  ...extra,
});

export const frame = (
  seq: number,
  tick: number,
  entities: unknown[] = [],
  runExtra: Record<string, unknown> = {},
) => ({
  type: "frame",
  kind: "snapshot",
  cause: "tick",
  session: 1,
  seq,
  tick,
  time_s: tick / 30,
  dropped: 0,
  entities,
  events: [],
  events_truncated: false,
  run: run(tick, runExtra),
});
