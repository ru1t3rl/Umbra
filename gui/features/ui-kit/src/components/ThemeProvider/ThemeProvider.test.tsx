import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { useEffect } from "react";
import { useTheme } from "../../hooks/useTheme.ts";
import { ThemeProvider } from "./ThemeProvider.tsx";

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    data,
  };
}

let mountCount = 0;
function Probe() {
  const { preference, resolved, setPreference } = useTheme();
  return (
    <div>
      <output data-testid="state">{`${preference}/${resolved}`}</output>
      <button type="button" onClick={() => setPreference("light")}>
        light
      </button>
      <button type="button" onClick={() => setPreference("system")}>
        system
      </button>
    </div>
  );
}
function Counted() {
  useEffect(() => {
    mountCount += 1;
  }, []);
  return <Probe />;
}

type Listener = () => void;
function stubMatchMedia(initialDark: boolean) {
  const listeners = new Set<Listener>();
  let dark = initialDark;
  const original = window.matchMedia;
  window.matchMedia = ((query: string) => ({
    get matches() {
      return query.includes("dark") ? dark : !dark;
    },
    media: query,
    addEventListener: (_: string, l: Listener) => listeners.add(l),
    removeEventListener: (_: string, l: Listener) => listeners.delete(l),
  })) as unknown as typeof window.matchMedia;
  return {
    set(next: boolean) {
      dark = next;
      for (const l of listeners) l();
    },
    listenerCount: () => listeners.size,
    restore() {
      window.matchMedia = original;
    },
  };
}

beforeEach(() => {
  document.documentElement.removeAttribute("data-theme");
  mountCount = 0;
});
afterEach(cleanup);

describe("ThemeProvider", () => {
  test("starts in dark without a stored preference", () => {
    render(
      <ThemeProvider storage={memoryStorage()}>
        <Probe />
      </ThemeProvider>,
    );
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(screen.getByTestId("state").textContent).toBe("dark/dark");
  });

  test("restores a stored preference", () => {
    render(
      <ThemeProvider storage={memoryStorage({ "umbra.theme": "light" })}>
        <Probe />
      </ThemeProvider>,
    );
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("ignores an invalid stored value", () => {
    render(
      <ThemeProvider storage={memoryStorage({ "umbra.theme": "purple" })}>
        <Probe />
      </ThemeProvider>,
    );
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("setPreference switches data-theme without remount and persists", () => {
    const storage = memoryStorage();
    render(
      <ThemeProvider storage={storage}>
        <Counted />
      </ThemeProvider>,
    );
    expect(mountCount).toBe(1);
    act(() => screen.getByText("light").click());
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(storage.data.get("umbra.theme")).toBe("light");
    expect(screen.getByTestId("state").textContent).toBe("light/light");
    expect(mountCount).toBe(1); // still the original mount
  });

  test("system follows the media query", () => {
    const mm = stubMatchMedia(true);
    try {
      render(
        <ThemeProvider storage={memoryStorage()}>
          <Probe />
        </ThemeProvider>,
      );
      act(() => screen.getByText("system").click());
      expect(document.documentElement.dataset.theme).toBe("dark");
      expect(mm.listenerCount()).toBe(1);
      act(() => mm.set(false));
      expect(document.documentElement.dataset.theme).toBe("light");
      expect(screen.getByTestId("state").textContent).toBe("system/light");
      act(() => screen.getByText("light").click());
      expect(mm.listenerCount()).toBe(0);
    } finally {
      mm.restore();
    }
  });

  test("unmount removes the media listener", () => {
    const mm = stubMatchMedia(true);
    try {
      const { unmount } = render(
        <ThemeProvider storage={memoryStorage({ "umbra.theme": "system" })}>
          <Probe />
        </ThemeProvider>,
      );
      expect(mm.listenerCount()).toBe(1);
      unmount();
      expect(mm.listenerCount()).toBe(0);
    } finally {
      mm.restore();
    }
  });

  test("setPreference is stable with an inline storage object", () => {
    const seen: Array<(p: "dark" | "light" | "system") => void> = [];
    function Spy() {
      seen.push(useTheme().setPreference);
      return null;
    }
    const make = () => (
      <ThemeProvider storage={memoryStorage()}>
        <Spy />
      </ThemeProvider>
    );
    const { rerender } = render(make());
    rerender(make());
    expect(seen.length).toBeGreaterThan(1);
    expect(new Set(seen).size).toBe(1);
  });

  test("a fixed preference ignores the media query", () => {
    const mm = stubMatchMedia(true);
    try {
      render(
        <ThemeProvider storage={memoryStorage()} defaultPreference="dark">
          <Probe />
        </ThemeProvider>,
      );
      act(() => mm.set(false));
      expect(document.documentElement.dataset.theme).toBe("dark");
    } finally {
      mm.restore();
    }
  });
});
