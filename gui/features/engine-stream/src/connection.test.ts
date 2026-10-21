import { describe, expect, test } from "bun:test";
import { connectEngineStream, DEFAULT_SUBSCRIPTION } from "./connection.ts";
import { entity, frame, hello, rig, run } from "./testing.ts";
import type { WebSocketLike } from "./types.ts";

function open(extra = {}) {
  const r = rig(extra);
  const c = connectEngineStream(r.options);
  r.socket().receive(hello());
  return { r, c };
}

describe("subscribe and decode", () => {
  test("starts connecting and sends nothing before hello", () => {
    const r = rig();
    const c = connectEngineStream(r.options);
    expect(c.getState().status).toBe("connecting");
    expect(r.socket().sent).toEqual([]);
  });
  test("sends subscribe after hello", () => {
    const { r, c } = open();
    expect(c.getState().status).toBe("open");
    expect(r.socket().sent).toEqual([{ type: "subscribe", max_rate_hz: 30 }]);
    expect(DEFAULT_SUBSCRIPTION).toEqual({ max_rate_hz: 30 });
  });
  test("an incompatible protocol is not subscribed to and not retried", () => {
    const r = rig();
    const c = connectEngineStream(r.options);
    r.socket().receive(hello("umbra.stream.v2"));
    expect(c.getState().status).toBe("incompatible");
    expect(r.socket().sent).toEqual([]);
    expect(r.socket().closedWith?.code).toBe(4002); // browsers only accept 1000 and 3000-4999
    r.socket().drop(4002);
    expect(r.clock.liveTimers()).toHaveLength(0);
  });
  test("unknown message types and fields are ignored", () => {
    const { r, c } = open();
    r.socket().receive({ type: "telemetry", x: 1 });
    r.socket().receive("{broken");
    r.socket().receive(frame(0, 1, [entity(1, { surprise: true })], { novel: 1 }));
    r.clock.paint();
    expect(c.getState().ignored).toBe(2);
    expect(c.getState().frame?.entities).toHaveLength(1);
    expect(c.getState().status).toBe("open");
  });
  test("a non-text message is counted and ignored", () => {
    const { r, c } = open();
    r.socket().onmessage?.({ data: new ArrayBuffer(4) });
    expect(c.getState().ignored).toBe(1);
  });
  test("an unknown message type is ignored (never applied as a frame)", () => {
    const { r, c } = open();
    r.socket().receive({ ...frame(0, 1, [entity(1)]), type: "frame2" });
    r.clock.paint();
    expect(c.getState().frame).toBeUndefined();
    expect(c.getState().ignored).toBe(1);
  });
  test("a frame with an unknown cause or ownership is handled", () => {
    const { r, c } = open();
    r.socket().receive(
      frame(0, 1, [entity(1, { ownership: "shared" })], {
        cause: "replay",
        run: run(1, { state: "stopped", stop_reason: "abducted" }),
      }),
    );
    r.clock.paint();
    expect(c.getState().frame?.entities[0]?.ownership).toBe("unknown");
    expect(c.getState().run?.state).toBe("stopped");
    expect(c.getState().run?.stop_reason).toBe("unknown");
  });
  test("error messages are counted and the connection stays open", () => {
    const { r, c } = open();
    r.socket().receive({ type: "error", code: "invalid_message", message: "x" });
    r.socket().receive({ type: "error", code: "from_the_future", message: "y" });
    expect(c.getState().errors).toBe(2);
    expect(c.getState().status).toBe("open");
    expect(r.socket().closedWith).toBeUndefined();
  });
});

describe("frames", () => {
  test("a frame is applied only on the animation frame", () => {
    const { r, c } = open();
    r.socket().receive(frame(0, 1, [entity(1)]));
    expect(c.getState().frame).toBeUndefined();
    expect(r.clock.frames).toHaveLength(1);
    r.clock.paint();
    expect(c.getState().frame?.tick).toBe(1);
    expect(c.getState().run?.tick).toBe(1);
  });
  test("latest frame wins within one animation frame", () => {
    const { r, c } = open();
    for (let i = 0; i < 5; i++) r.socket().receive(frame(i, i + 1, [entity(i + 1)]));
    expect(r.clock.frames).toHaveLength(1); // one scheduled flush for five messages
    let notifications = 0;
    c.store.subscribe(() => {
      notifications += 1;
    });
    r.clock.paint();
    expect(notifications).toBe(1);
    expect(c.getState().frame?.tick).toBe(5);
    expect(c.getState().frame?.entities.map((e) => e.id)).toEqual([5]);
    expect(c.getState().run?.tick).toBe(5);
  });
  test("a new frame schedules a new flush after the previous one ran", () => {
    const { r, c } = open();
    r.socket().receive(frame(0, 1));
    r.clock.paint();
    r.socket().receive(frame(1, 2));
    expect(r.clock.frames).toHaveLength(1);
    r.clock.paint();
    expect(c.getState().frame?.tick).toBe(2);
  });
  test("a new session replaces all entities", () => {
    const { r, c } = open();
    r.socket().receive(frame(0, 5, [entity(1), entity(2), entity(3)]));
    r.clock.paint();
    r.socket().receive(frame(0, 1, [entity(9)], { session: 2, run: run(1, { session: 2 }) }));
    r.clock.paint();
    expect(c.getState().frame?.session).toBe(2);
    expect(c.getState().frame?.entities.map((e) => e.id)).toEqual([9]);
  });
  test("seq gaps accumulate dropped, also across coalesced frames", () => {
    const { r, c } = open();
    r.socket().receive(frame(0, 1));
    r.socket().receive(frame(3, 4, [], { dropped: 2 }));
    r.socket().receive(frame(4, 5));
    r.clock.paint(); // frames 0, 3 and 4 collapse into one flush
    expect(c.getState().frame?.seq).toBe(4);
    expect(c.getState().dropped).toBe(2);
    r.socket().receive(frame(9, 10, [], { dropped: 4 }));
    r.clock.paint();
    expect(c.getState().dropped).toBe(6);
  });
  test("an unchanged run keeps its identity while the frame changes", () => {
    const { r, c } = open();
    r.socket().receive(frame(0, 1, [entity(1)]));
    r.clock.paint();
    const before = c.getState().run;
    r.socket().receive(frame(1, 1, [entity(1), entity(2)], { cause: "control" }));
    r.clock.paint();
    expect(c.getState().frame?.entities).toHaveLength(2);
    expect(c.getState().run).toBe(before);
  });
  test("a frame whose run state is not understood still moves tick and time", () => {
    const { r, c } = open();
    r.socket().receive(frame(0, 1));
    r.clock.paint();
    r.socket().receive(frame(1, 2, [], { run: run(2, { state: "hibernating" }) }));
    r.clock.paint();
    expect(c.getState().frame?.tick).toBe(2);
    expect(c.getState().run?.state).toBe("unknown");
    expect(c.getState().run?.tick).toBe(2);
  });
  test("skipped entities accumulate across coalesced frames", () => {
    const { r, c } = open();
    const bad = { ...entity(9), lat_deg: null };
    r.socket().receive(frame(0, 1, [entity(1), bad]));
    r.socket().receive(frame(1, 2, [entity(1), bad, bad]));
    r.clock.paint();
    expect(c.getState().skipped).toBe(3);
    expect(c.getState().frame?.entities).toHaveLength(1);
    r.socket().receive(frame(2, 3, [bad]));
    r.clock.paint();
    expect(c.getState().skipped).toBe(4);
  });
  test("a throwing entity listener does not stop the others or the store", () => {
    const { r, c } = open();
    const seen: number[] = [];
    const logged: unknown[] = [];
    const realError = console.error;
    console.error = (...args: unknown[]) => logged.push(args);
    try {
      c.source.subscribe(() => {
        throw new Error("listener bug");
      });
      c.source.subscribe((f) => seen.push(f.tick));
      r.socket().receive(frame(0, 1));
      r.clock.paint();
      r.socket().receive(frame(1, 2));
      r.clock.paint();
    } finally {
      console.error = realError;
    }
    expect(seen).toEqual([1, 2]);
    expect(logged).toHaveLength(2);
    expect(c.getState().frame?.tick).toBe(2);
  });
  test("EntitySource notifies once per applied frame and unsubscribes", () => {
    const { r, c } = open();
    const seen: number[] = [];
    const off = c.source.subscribe((f) => seen.push(f.tick));
    r.socket().receive(frame(0, 1));
    r.socket().receive(frame(1, 2));
    r.clock.paint();
    expect(seen).toEqual([2]);
    expect(c.source.getFrame()?.tick).toBe(2);
    off();
    r.socket().receive(frame(2, 3));
    r.clock.paint();
    expect(seen).toEqual([2]);
  });
});

describe("reconnect", () => {
  test("reconnects with backoff and resets on hello", () => {
    const { r, c } = open();
    const delays: number[] = [];
    for (let i = 0; i < 6; i++) {
      r.socket().drop();
      expect(c.getState().status).toBe("reconnecting");
      delays.push(r.clock.fireTimer());
    }
    expect(delays).toEqual([500, 1000, 2000, 4000, 8000, 8000]);
    expect(r.sockets).toHaveLength(7);
    r.socket().receive(hello());
    expect(c.getState().status).toBe("open");
    r.socket().drop();
    expect(r.clock.fireTimer()).toBe(500); // reset by hello
  });
  test("the subscription is sent again on the new socket", () => {
    const { r } = open();
    r.socket().drop();
    r.clock.fireTimer();
    r.socket().receive(hello());
    expect(r.socket().sent).toEqual([{ type: "subscribe", max_rate_hz: 30 }]);
  });
  test("a socket that never reached hello keeps backing off", () => {
    const r = rig();
    const c = connectEngineStream(r.options);
    r.socket().drop();
    expect(r.clock.fireTimer()).toBe(500);
    r.socket().drop();
    expect(r.clock.fireTimer()).toBe(1000);
    expect(c.getState().status).toBe("reconnecting");
  });
  test("events from a replaced socket are ignored", () => {
    const { r, c } = open();
    const old = r.socket();
    old.drop();
    r.clock.fireTimer();
    old.receive(frame(0, 1));
    r.clock.paint();
    expect(c.getState().frame).toBeUndefined();
  });
  test("close stops reconnecting", () => {
    const { r, c } = open();
    r.socket().drop();
    expect(r.clock.liveTimers()).toHaveLength(1);
    c.close();
    expect(r.clock.liveTimers()).toHaveLength(0);
    expect(r.sockets).toHaveLength(1);
    expect(c.getState().status).toBe("closed");
  });
  test("close closes the socket, cancels the pending flush and ignores a late close event", () => {
    const { r, c } = open();
    r.socket().receive(frame(0, 1, [entity(1)]));
    const s = r.socket();
    c.close();
    expect(s.closedWith?.code).toBe(1000);
    expect(r.clock.cancelledFrames).toBe(1);
    r.clock.paint();
    expect(c.getState().frame).toBeUndefined();
    s.drop(1000);
    expect(r.clock.liveTimers()).toHaveLength(0);
    expect(c.getState().status).toBe("closed");
  });
});

describe("subscription and options", () => {
  test("setSubscription sends a new subscribe", () => {
    const { r, c } = open();
    c.setSubscription({ max_rate_hz: 10, filter: { entity_ids: [1, 2] } });
    expect(r.socket().sent).toEqual([
      { type: "subscribe", max_rate_hz: 30 },
      { type: "subscribe", max_rate_hz: 10, filter: { entity_ids: [1, 2] } },
    ]);
  });
  test("setSubscription before hello is sent with the hello, and survives reconnects", () => {
    const r = rig();
    const c = connectEngineStream(r.options);
    c.setSubscription({ max_rate_hz: 5 });
    expect(r.socket().sent).toEqual([]);
    r.socket().receive(hello());
    r.socket().drop();
    r.clock.fireTimer();
    r.socket().receive(hello());
    expect(r.socket().sent).toEqual([{ type: "subscribe", max_rate_hz: 5 }]);
  });
  test("a custom initial subscription is used", () => {
    const r = rig({ subscription: { max_rate_hz: 12 } });
    connectEngineStream(r.options);
    r.socket().receive(hello());
    expect(r.socket().sent).toEqual([{ type: "subscribe", max_rate_hz: 12 }]);
  });
  test("a throwing socket factory retries with backoff", () => {
    const r = rig();
    let attempts = 0;
    const realError = console.error;
    console.error = () => {};
    try {
      const c = connectEngineStream({
        ...r.options,
        createSocket: (url) => {
          attempts += 1;
          if (attempts < 3) throw new SyntaxError("bad url");
          return (r.options.createSocket as (u: string) => WebSocketLike)(url);
        },
      });
      expect(c.getState().status).toBe("reconnecting");
      expect(r.clock.fireTimer()).toBe(500);
      expect(c.getState().status).toBe("reconnecting");
      expect(r.clock.fireTimer()).toBe(1000);
      expect(r.sockets).toHaveLength(1);
      r.socket().receive(hello());
      expect(c.getState().status).toBe("open");
      c.close();
    } finally {
      console.error = realError;
    }
  });
  test("encoding other than json is rejected at construction", () => {
    const r = rig({ encoding: "msgpack" });
    expect(() => connectEngineStream(r.options)).toThrow("msgpack");
    expect(r.sockets).toHaveLength(0);
  });
});

describe("10,000 entities", () => {
  test("one store notification per flush, however many messages arrived", () => {
    const { r, c } = open();
    const entities = Array.from({ length: 10_000 }, (_, i) => entity(i + 1));
    let notifications = 0;
    c.store.subscribe(() => {
      notifications += 1;
    });
    for (let flush = 0; flush < 3; flush++) {
      for (let m = 0; m < 4; m++)
        r.socket().receive(frame(flush * 4 + m, flush * 4 + m + 1, entities));
      r.clock.paint();
    }
    expect(notifications).toBe(3);
    expect(c.getState().frame?.entities).toHaveLength(10_000);
  });
});
