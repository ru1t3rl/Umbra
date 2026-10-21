// Test doubles shared by this package's tests (not exported from index.ts).
import type { EngineStreamOptions } from "./connection.ts";
import type { WebSocketLike } from "./types.ts";

export class FakeSocket implements WebSocketLike {
  sent: unknown[] = [];
  closedWith: { code?: number; reason?: string } | undefined;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  send(data: string): void {
    this.sent.push(JSON.parse(data));
  }
  close(code?: number, reason?: string): void {
    // Browsers throw InvalidAccessError unless the code is 1000 or in 3000-4999.
    if (code !== undefined && code !== 1000 && !(code >= 3000 && code <= 4999)) {
      throw new DOMException(`invalid close code ${code}`, "InvalidAccessError");
    }
    this.closedWith = {
      ...(code === undefined ? {} : { code }),
      ...(reason === undefined ? {} : { reason }),
    };
  }
  receive(message: unknown): void {
    this.onmessage?.({ data: typeof message === "string" ? message : JSON.stringify(message) });
  }
  /** The peer or the network ended the connection. */
  drop(code = 1006): void {
    this.onclose?.({ code, reason: "" });
  }
}

export class FakeClock {
  frames: (() => void)[] = [];
  timers: { fn: () => void; ms: number; id: number; live: boolean }[] = [];
  cancelledFrames = 0;
  private nextId = 1;
  scheduleFrame = (cb: () => void): number => {
    this.frames.push(cb);
    return this.frames.length;
  };
  cancelFrame = (): void => {
    this.cancelledFrames += 1;
    this.frames = [];
  };
  setTimer = (fn: () => void, ms: number): unknown => {
    const id = this.nextId++;
    this.timers.push({ fn, ms, id, live: true });
    return id;
  };
  clearTimer = (id: unknown): void => {
    for (const t of this.timers) if (t.id === id) t.live = false;
  };
  /** Run the animation-frame callbacks scheduled so far. */
  paint(): void {
    const run = this.frames;
    this.frames = [];
    for (const f of run) f();
  }
  liveTimers() {
    return this.timers.filter((t) => t.live);
  }
  /** Fire the oldest live timer. */
  fireTimer(): number {
    const t = this.liveTimers()[0];
    if (!t) throw new Error("no live timer");
    t.live = false;
    t.fn();
    return t.ms;
  }
}

export interface Rig {
  sockets: FakeSocket[];
  clock: FakeClock;
  options: EngineStreamOptions;
  socket(): FakeSocket;
}

export function rig(extra: Partial<EngineStreamOptions> = {}): Rig {
  const sockets: FakeSocket[] = [];
  const clock = new FakeClock();
  return {
    sockets,
    clock,
    options: {
      url: "ws://engine/api/v1/stream?encoding=json",
      createSocket: () => {
        const s = new FakeSocket();
        sockets.push(s);
        return s;
      },
      scheduleFrame: clock.scheduleFrame,
      cancelFrame: clock.cancelFrame,
      setTimer: clock.setTimer,
      clearTimer: clock.clearTimer,
      ...extra,
    },
    socket() {
      const s = sockets[sockets.length - 1];
      if (!s) throw new Error("no socket yet");
      return s;
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

export const entity = (id: number, extra: Record<string, unknown> = {}) => ({
  id,
  lat_deg: 10 + id / 1e5,
  lon_deg: 20 + id / 1e5,
  alt_m: 0,
  heading_deg: 90,
  speed_mps: 5,
  ownership: "owned",
  ...extra,
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
  extra: Record<string, unknown> = {},
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
  run: run(tick),
  ...extra,
});
