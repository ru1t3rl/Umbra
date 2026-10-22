import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { connectEngineStream, type EngineStreamConnection } from "@umbra/engine-stream";
import { type SettingsBackend, useAffiliationOf, useSettings } from "@umbra/settings";
import { useTheme } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { createMemoryRouter } from "react-router";
import { App } from "./App.tsx";

// WI-4 wiring: App takes an optional `settingsBackend` (default: createLocalStorageBackend()) and
// renders SettingsProvider above ThemeProvider (ADR 0074 order). The web App does not give
// ThemeProvider the settings adapter (ASM-301, ASM-889).

const open: EngineStreamConnection[] = [];
afterEach(() => {
  cleanup();
  for (const c of open.splice(0)) c.close();
});

function idleConnection(): EngineStreamConnection {
  const c = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => ({
      send: () => {},
      close: () => {},
      onopen: null,
      onmessage: null,
      onclose: null,
      onerror: null,
    }),
  });
  open.push(c);
  return c;
}

function backend(initial: unknown) {
  const writes: unknown[] = [];
  const b: SettingsBackend = { read: () => initial, write: (s) => void writes.push(s) };
  return { b, writes };
}

test("App makes settings from the injected backend available to every route", () => {
  function Probe(): ReactElement {
    const s = useSettings();
    const of = useAffiliationOf();
    return <p>{`${s.stream.maxRateHz}|${of("blue")}`}</p>;
  }
  const { b } = backend({
    stream: { maxRateHz: 12 },
    map: { affiliationOverrides: { blue: "hostile" } },
  });
  render(
    <App
      router={createMemoryRouter([{ path: "/", element: <Probe /> }])}
      connection={idleConnection()}
      settingsBackend={b}
    />,
  );
  expect(screen.getByText("12|hostile")).toBeTruthy();
});

test("the web App keeps the theme in localStorage umbra.theme, not in the settings backend", () => {
  function Probe(): ReactElement {
    const { preference, setPreference } = useTheme();
    return (
      <button type="button" onClick={() => setPreference("light")}>
        {`theme ${preference}`}
      </button>
    );
  }
  window.localStorage.setItem("umbra.theme", "dark");
  const { b, writes } = backend({ appearance: { theme: "system" } });
  try {
    render(
      <App
        router={createMemoryRouter([{ path: "/", element: <Probe /> }])}
        connection={idleConnection()}
        settingsBackend={b}
      />,
    );
    const button = screen.getByRole("button", { name: "theme dark" });
    act(() => button.click());
    expect(screen.getByRole("button", { name: "theme light" })).toBeTruthy();
    expect(window.localStorage.getItem("umbra.theme")).toBe("light");
    expect(writes).toHaveLength(0);
  } finally {
    window.localStorage.removeItem("umbra.theme");
  }
});

test("SettingsProvider sits above ThemeProvider (a theme-level child can read settings)", () => {
  function Probe(): ReactElement {
    useTheme();
    return <p>{String(useSettings().stream.maxRateHz)}</p>;
  }
  const { b } = backend({ stream: { maxRateHz: 9 } });
  render(
    <App
      router={createMemoryRouter([{ path: "/", element: <Probe /> }])}
      connection={idleConnection()}
      settingsBackend={b}
    />,
  );
  expect(screen.getByText("9")).toBeTruthy();
});
