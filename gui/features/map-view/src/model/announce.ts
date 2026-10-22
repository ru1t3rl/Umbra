/** Timer functions, injected so tests do not wait. */
export interface Timers<H> {
  setTimer(callback: () => void, ms: number): H;
  clearTimer(handle: H): void;
}

export const browserTimers: Timers<ReturnType<typeof setTimeout>> = {
  setTimer: (callback, ms) => setTimeout(callback, ms),
  clearTimer: (handle) => clearTimeout(handle),
};

/** A screen reader should hear the entity count at most this often (ASM-373). */
export const ANNOUNCE_INTERVAL_MS = 2000;

export interface Announcer {
  /** Records the newest count. Cheap: no React state, no render. */
  report(count: number): void;
  /** Cancels a pending announcement. */
  dispose(): void;
}

/**
 * Trailing throttle: the first change after an announcement starts one timer; when it fires, the newest
 * count is announced if it differs from the last one announced. Counts that change every frame therefore
 * cost one announcement per interval, and a count that returns to the announced value costs none.
 */
export function createAnnouncer<H>(
  initial: number,
  announce: (count: number) => void,
  timers: Timers<H>,
  intervalMs: number = ANNOUNCE_INTERVAL_MS,
): Announcer {
  let announced = initial;
  let latest = initial;
  let handle: H | undefined;
  let armed = false;
  return {
    report(count) {
      latest = count;
      if (armed) return;
      armed = true;
      handle = timers.setTimer(() => {
        armed = false;
        if (latest !== announced) {
          announced = latest;
          announce(latest);
        }
      }, intervalMs);
    },
    dispose() {
      if (armed && handle !== undefined) timers.clearTimer(handle);
      armed = false;
    },
  };
}
