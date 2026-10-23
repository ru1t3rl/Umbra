// WI-14 review round 2 (ASM-988, review minor 4): the measurement-based compact logic, with the observer. The bar's
// size is faked (happy-dom has no layout): the header's width is `client`, its content needs `wanted[level]`.
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { type ReactElement, useRef } from "react";
import { useBarFit } from "./useBarFit.ts";

interface FakeObserver {
  callback: () => void;
  targets: Element[];
  disconnected: boolean;
}
let observers: FakeObserver[] = [];
let before: unknown;
let client = 1000;
let wanted: number[] = [1400, 1250, 1000];
let reads = 0;
let content = 1000;
let fontListeners: Array<() => void> = [];
let fontsReady: Promise<unknown> = new Promise(() => {});
let realFonts: PropertyDescriptor | undefined;

beforeEach(() => {
  observers = [];
  reads = 0;
  content = 1000;
  fontListeners = [];
  fontsReady = new Promise(() => {});
  realFonts = Object.getOwnPropertyDescriptor(document, "fonts");
  Object.defineProperty(document, "fonts", {
    configurable: true,
    value: {
      get ready() {
        return fontsReady;
      },
      addEventListener: (_type: string, l: () => void) => void fontListeners.push(l),
      removeEventListener: (_type: string, l: () => void) => {
        fontListeners = fontListeners.filter((x) => x !== l);
      },
    },
  });
  client = 1000;
  wanted = [1400, 1250, 1000];
  before = Reflect.get(globalThis, "ResizeObserver");
  class Fake {
    private readonly o: FakeObserver;
    constructor(callback: () => void) {
      this.o = { callback, targets: [], disconnected: false };
      observers.push(this.o);
    }
    observe(target: Element): void {
      this.o.targets.push(target);
    }
    unobserve(): void {}
    disconnect(): void {
      this.o.disconnected = true;
    }
  }
  Reflect.set(globalThis, "ResizeObserver", Fake);
});
afterEach(() => {
  cleanup();
  if (realFonts) Object.defineProperty(document, "fonts", realFonts);
  else Reflect.deleteProperty(document, "fonts");
  if (before === undefined) Reflect.deleteProperty(globalThis, "ResizeObserver");
  else Reflect.set(globalThis, "ResizeObserver", before);
});

function Probe({ levels }: { levels?: 3 | 4 }): ReactElement {
  const ref = useRef<HTMLDivElement>(null);
  const level = useBarFit(ref, levels);
  return (
    <div ref={ref}>
      <header
        data-level={level}
        ref={(el) => {
          if (!el) return;
          Object.defineProperty(el, "clientWidth", { configurable: true, get: () => client });
          Object.defineProperty(el, "scrollWidth", {
            configurable: true,
            get: () => {
              reads += 1;
              return Math.max(client, wanted[Number(el.getAttribute("data-level"))] ?? 0);
            },
          });
        }}
      >
        <span data-testid="brand" />
        <div
          data-testid="actions"
          ref={(el) => {
            if (el)
              Object.defineProperty(el, "offsetWidth", { configurable: true, get: () => content });
          }}
        />
      </header>
      <output>{level}</output>
    </div>
  );
}
const level = () => screen.getByRole("status").textContent;
const frame = () => act(async () => void (await new Promise((r) => setTimeout(r, 40))));
/** Every observer fires, as the browser does after a size change. */
const resized = async () => {
  act(() => {
    for (const o of observers) o.callback();
  });
  await frame();
};

describe("useBarFit", () => {
  // Catches: compact mode for a bar that fits, or for an unlaid-out page (no layout: widths 0).
  test("a bar that fits stays at level 0", async () => {
    client = 1500;
    render(<Probe />);
    await frame();
    expect(level()).toBe("0");
  });

  test("with no layout at all it stays at level 0", async () => {
    client = 0;
    render(<Probe />);
    await frame();
    expect(level()).toBe("0");
  });

  // Catches: the measurement at mount missing (a page opened on a narrow screen would overflow until a resize).
  test("a bar that overflows at mount steps up until it fits", async () => {
    client = 1200;
    render(<Probe />);
    await frame();
    expect(level()).toBe("2"); // 1400 > 1200, 1250 > 1200, 1000 fits
  });

  // Catches: the observer or its callback removed: the bar would never react to a resize.
  test("a resize (the observer fires) re-measures: down when there is room, up when there is not", async () => {
    client = 1200;
    render(<Probe />);
    await frame();
    client = 1300;
    await resized();
    expect(level()).toBe("1");
    client = 1500;
    await resized();
    expect(level()).toBe("0");
    client = 900;
    await resized();
    expect(level()).toBe("2");
  });

  // Catches: flip-flop right at the boundary.
  test("hysteresis: just wide enough is not wide enough to step back", async () => {
    client = 1200;
    render(<Probe />);
    await frame();
    client = 1270; // level 1 needed 1250; 1250 + 24 = 1274
    await resized();
    expect(level()).toBe("2");
    client = 1274;
    await resized();
    expect(level()).toBe("1");
  });

  // Catches: content changes (more digits in a readout) not noticed, because only the window was watched.
  test("the observer watches the bar and its content, not only the window", async () => {
    render(<Probe />);
    await frame();
    const targets = observers.flatMap((o) => o.targets);
    expect(targets.some((t) => t.tagName === "HEADER")).toBe(true);
    expect(targets.some((t) => t.getAttribute("data-testid") === "actions")).toBe(true);
    expect(targets.some((t) => t.getAttribute("data-testid") === "brand")).toBe(true);
    expect(observers.length).toBeGreaterThan(0);
  });

  test("a wider content at the same level (readout digits) steps up on the next notification", async () => {
    client = 1300;
    wanted = [1400, 1250, 1000];
    render(<Probe />);
    await frame();
    expect(level()).toBe("1");
    wanted = [1400, 1350, 1000]; // 7 digits instead of 3
    await resized();
    expect(level()).toBe("2");
  });

  // Catches: a re-measure storm: one measurement per frame at most, however often the observer fires.
  test("fifty notifications in one frame measure once", async () => {
    client = 1500;
    render(<Probe />);
    await frame();
    reads = 0;
    act(() => {
      for (let i = 0; i < 50; i++) for (const o of observers) o.callback();
    });
    await frame();
    expect(reads).toBeLessThanOrEqual(2);
  });

  // Catches: a leaked observer.
  test("unmounting disconnects every observer", async () => {
    const { unmount } = render(<Probe />);
    await frame();
    unmount();
    expect(observers.every((o) => o.disconnected)).toBe(true);
  });
});

describe("useBarFit after the fonts load (ASM-990)", () => {
  // Catches: the first fit measured before the icon font loaded (ligatures as words, wide), kept forever.
  test("a fonts loadingdone event re-runs the fit from the start", async () => {
    client = 1200;
    render(<Probe />);
    await frame();
    expect(level()).toBe("2");
    wanted = [1100, 900, 800]; // the icons are narrow now
    act(() => {
      for (const l of fontListeners) l();
    });
    await frame();
    expect(level()).toBe("0");
  });

  test("so does fonts.ready, once", async () => {
    client = 1200;
    let ready: () => void = () => {};
    fontsReady = new Promise<void>((r) => {
      ready = r;
    });
    render(<Probe />);
    await frame();
    expect(level()).toBe("2");
    wanted = [1100, 900, 800];
    await act(async () => {
      ready();
      await frame();
    });
    expect(level()).toBe("0");
  });

  test("the listener is removed on unmount", async () => {
    const { unmount } = render(<Probe />);
    await frame();
    expect(fontListeners.length).toBeGreaterThan(0);
    unmount();
    expect(fontListeners).toEqual([]);
  });

  test("without document.fonts nothing breaks", async () => {
    Reflect.deleteProperty(document, "fonts");
    client = 1200;
    render(<Probe />);
    await frame();
    expect(level()).toBe("2");
  });
});

describe("useBarFit when the content shrinks (ASM-990)", () => {
  // Catches: counters reset by an unload leaving the bar one level too compact (stale, inflated needs).
  test("a shrink of more than the hysteresis re-runs the fit from the start", async () => {
    client = 1300;
    content = 1250;
    render(<Probe />);
    await frame();
    expect(level()).toBe("1");
    wanted = [1100, 900, 800];
    content = 1000; // 250 px narrower
    await resized();
    expect(level()).toBe("0");
  });

  // Catches: a restart for every wobble.
  test("a smaller shrink changes nothing", async () => {
    client = 1300;
    content = 1250;
    render(<Probe />);
    await frame();
    wanted = [1100, 900, 800];
    content = 1240;
    await resized();
    expect(level()).toBe("1");
  });
});

// WI-15 (ASM-992, review m2): a restart keeps the length of the ladder; it must not fall back to three levels.
describe("useBarFit with four levels", () => {
  test("it climbs to level 3", async () => {
    client = 1200;
    wanted = [1500, 1400, 1300, 1000];
    render(<Probe levels={4} />);
    await frame();
    expect(level()).toBe("3");
  });

  // Catches: the fonts restart resetting to a three-level ladder (the bar could then never reach level 3 again).
  test("after a fonts restart it still reaches level 3", async () => {
    client = 1200;
    wanted = [1500, 1400, 1300, 1000];
    render(<Probe levels={4} />);
    await frame();
    wanted = [1100, 900, 800, 700];
    act(() => {
      for (const l of fontListeners) l();
    });
    await frame();
    expect(level()).toBe("0");
    wanted = [1500, 1400, 1300, 1000]; // the readouts grow again
    await resized();
    expect(level()).toBe("3");
  });

  // Catches: the shrink restart resetting to a three-level ladder.
  test("after a shrink restart it still reaches level 3", async () => {
    client = 1200;
    content = 1250;
    wanted = [1500, 1400, 1300, 1000];
    render(<Probe levels={4} />);
    await frame();
    expect(level()).toBe("3");
    wanted = [1100, 900, 800, 700];
    content = 1000;
    await resized();
    expect(level()).toBe("0");
    wanted = [1500, 1400, 1300, 1000];
    content = 1250;
    await resized();
    expect(level()).toBe("3");
  });

  // Catches: a changed ladder length keeping the old state (a four-entry ladder run with three entries).
  test("a changed ladder length starts the fit again", async () => {
    client = 1200;
    wanted = [1500, 1400, 1300, 1000];
    const { rerender } = render(<Probe levels={3} />);
    await frame();
    expect(level()).toBe("2");
    rerender(<Probe levels={4} />);
    await frame();
    expect(level()).toBe("3");
  });
});

// WI-17 (ASM-997): a readout deep inside the bar gets narrower (the counters reset) while the bar's own children keep
// their width (one of them takes the spare width, so the summed-width rule of ASM-990 sees nothing). The widths
// remembered per level are stale then, and the bar must measure again from level 0.
describe("useBarFit when a readout inside the bar gets narrower (ASM-997)", () => {
  const BASE = [1000, 950, 900, 850];
  let readout = 700;
  let rendered: string[] = [];

  function ReadoutProbe(): ReactElement {
    const ref = useRef<HTMLDivElement>(null);
    const fit = useBarFit(ref, 4);
    rendered.push(String(fit));
    return (
      <div ref={ref}>
        <header
          data-level={fit}
          ref={(el) => {
            if (!el) return;
            Object.defineProperty(el, "clientWidth", { configurable: true, get: () => client });
            Object.defineProperty(el, "scrollWidth", {
              configurable: true,
              get: () =>
                Math.max(client, (BASE[Number(el.getAttribute("data-level"))] ?? 0) + readout),
            });
          }}
        >
          <span data-testid="brand" />
          <div data-testid="actions">
            <span
              data-testid="readout"
              ref={(el) => {
                if (el)
                  Object.defineProperty(el, "offsetWidth", {
                    configurable: true,
                    get: () => readout,
                  });
              }}
            />
          </div>
        </header>
        <output>{fit}</output>
      </div>
    );
  }

  beforeEach(() => {
    readout = 700;
    rendered = [];
  });

  const settleOnWorst = async () => {
    client = 1580; // the levels need 1700, 1650, 1600, 1550: level 3 is the first that fits
    render(<ReadoutProbe />);
    await frame();
    expect(level()).toBe("3");
  };

  // Catches: the bar staying at the compact level after the widest readouts reset (stale remembered needs).
  test("the level that fits again is taken", async () => {
    await settleOnWorst();
    readout = 600; // the levels need 1600, 1550, 1500, 1450: level 1 is the first that fits
    await resized();
    expect(level()).toBe("1");
  });

  // Catches (ASM-997, review B1): a nested element's size entry not being observed, so a readout that shrinks inside a
  // child that takes the spare width is never noticed (the child keeps its size and fires nothing).
  test("the elements inside the bar's children are observed too", async () => {
    render(<ReadoutProbe />);
    await frame();
    const targets = observers.flatMap((o) => o.targets);
    expect(targets.some((t) => t.getAttribute("data-testid") === "readout")).toBe(true);
  });

  // Catches (review B1): a restart for every 1 px a nested element narrows (a tick alternating 999,999 and
  // 1,000,000 at 30 fps changed the level 18 times in 20 frames). The width of the bar's content, counted from the
  // elements inside its children, goes through the one margin rule of ASM-990: 12 px is under it.
  test("a readout alternating between two widths across a level boundary changes the level at most once", async () => {
    client = 1605; // 1000 + 600 fits level 0, 1000 + 612 does not (level 1: 950 + 612 fits)
    readout = 600;
    render(<ReadoutProbe />);
    await frame();
    expect(level()).toBe("0");
    rendered = [];
    for (let i = 0; i < 10; i++) {
      readout = i % 2 === 0 ? 612 : 600;
      await resized();
    }
    const changes = rendered.filter((l, k) => k > 0 && l !== rendered[k - 1]).length;
    expect(changes).toBeLessThanOrEqual(1);
  });

  // Catches: a growing readout (5 digits to 7) changing the level more than once, or downwards.
  test("growth changes the level at most once and never downwards", async () => {
    client = 1600;
    readout = 600;
    render(<ReadoutProbe />);
    await frame();
    expect(level()).toBe("0"); // 1600 fits
    rendered = [];
    readout = 700;
    await resized();
    const seen = rendered.filter((l, i) => i === 0 || l !== rendered[i - 1]);
    expect(Number(level())).toBeGreaterThan(0);
    expect(seen.every((l, i) => i === 0 || Number(l) >= Number(seen[i - 1]))).toBe(true);
  });

  test("a shrink while the bar shows level 0 changes nothing", async () => {
    client = 1800;
    render(<ReadoutProbe />);
    await frame();
    readout = 600;
    await resized();
    expect(level()).toBe("0");
  });
});
