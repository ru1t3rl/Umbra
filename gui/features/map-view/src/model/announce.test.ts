import { describe, expect, test } from "bun:test";
import { ANNOUNCE_INTERVAL_MS, createAnnouncer, type Timers } from "./announce.ts";

/** Manual timers: `fire()` runs the one pending timer, if any. */
function manualTimers() {
  let next = 1;
  const pending = new Map<number, { cb: () => void; ms: number }>();
  const timers: Timers<number> = {
    setTimer: (cb, ms) => {
      const h = next++;
      pending.set(h, { cb, ms });
      return h;
    },
    clearTimer: (h) => {
      pending.delete(h);
    },
  };
  return {
    timers,
    get pending() {
      return pending.size;
    },
    fire() {
      for (const [h, t] of [...pending]) {
        pending.delete(h);
        t.cb();
      }
    },
    delays: () => [...pending.values()].map((t) => t.ms),
  };
}

describe("createAnnouncer", () => {
  test("100 changing counts arm one timer and announce once, with the newest count", () => {
    const clock = manualTimers();
    const heard: number[] = [];
    const a = createAnnouncer(0, (n) => heard.push(n), clock.timers);
    for (let n = 1; n <= 100; n++) a.report(n);
    expect(clock.pending).toBe(1);
    expect(clock.delays()).toEqual([ANNOUNCE_INTERVAL_MS]);
    expect(heard).toEqual([]);
    clock.fire();
    expect(heard).toEqual([100]);
  });
  test("each interval announces at most once, however often the count changes", () => {
    const clock = manualTimers();
    const heard: number[] = [];
    const a = createAnnouncer(0, (n) => heard.push(n), clock.timers);
    for (let interval = 1; interval <= 5; interval++) {
      for (let f = 0; f < 60; f++) a.report(interval * 1000 + f);
      clock.fire();
    }
    expect(heard).toHaveLength(5);
  });
  test("a count that returns to the announced value announces nothing", () => {
    const clock = manualTimers();
    const heard: number[] = [];
    const a = createAnnouncer(3, (n) => heard.push(n), clock.timers);
    a.report(9);
    a.report(3);
    clock.fire();
    expect(heard).toEqual([]);
  });
  test("a report after the timer fired arms a new one; dispose cancels", () => {
    const clock = manualTimers();
    const heard: number[] = [];
    const a = createAnnouncer(0, (n) => heard.push(n), clock.timers);
    a.report(1);
    clock.fire();
    a.report(2);
    expect(clock.pending).toBe(1);
    a.dispose();
    expect(clock.pending).toBe(0);
    clock.fire();
    expect(heard).toEqual([1]);
  });
});
