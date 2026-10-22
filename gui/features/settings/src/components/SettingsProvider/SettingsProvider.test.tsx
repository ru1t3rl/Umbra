import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { useRef } from "react";
import { useSettings } from "../../hooks/useSettings.ts";
import { useUpdateSettings } from "../../hooks/useUpdateSettings.ts";
import type { SettingsBackend } from "../../model/backend.ts";
import { DEFAULT_SETTINGS, type Settings } from "../../model/settings.ts";
import { SettingsProvider } from "./SettingsProvider.tsx";

afterEach(cleanup);

function memoryBackend(initial?: unknown) {
  const writes: Settings[] = [];
  let reads = 0;
  const backend: SettingsBackend = {
    read: () => {
      reads += 1;
      return initial;
    },
    write: (s) => void writes.push(s),
  };
  return { backend, writes, reads: () => reads };
}

let update: ReturnType<typeof useUpdateSettings> | undefined;
let updateIdentities = 0;
function Probe() {
  const s = useSettings();
  const u = useUpdateSettings();
  const last = useRef(u);
  if (last.current !== u) updateIdentities += 1;
  update = u;
  return <output data-testid="s">{`${s.stream.maxRateHz}|${s.scenarios.recent.join(",")}`}</output>;
}
const text = () => screen.getByTestId("s").textContent;

describe("SettingsProvider, useSettings, useUpdateSettings (ADR 0074)", () => {
  test("without a provider useSettings returns the defaults", () => {
    render(<Probe />);
    expect(text()).toBe(`${DEFAULT_SETTINGS.stream.maxRateHz}|`);
  });
  test("outside a provider the update function is a no-op", () => {
    render(<Probe />);
    expect(() => act(() => update?.({ stream: { maxRateHz: 5 } }))).not.toThrow();
    expect(text()).toBe(`${DEFAULT_SETTINGS.stream.maxRateHz}|`);
  });
  test("it reads the backend once and parses it", () => {
    const m = memoryBackend({ stream: { maxRateHz: 999 }, scenarios: { recent: ["a"] } });
    render(
      <SettingsProvider backend={m.backend}>
        <Probe />
      </SettingsProvider>,
    );
    expect(text()).toBe("60|a");
    expect(m.reads()).toBe(1);
    expect(m.writes).toHaveLength(0);
  });
  test("corrupt stored data gives the defaults and does not throw", () => {
    const m = memoryBackend("garbage");
    render(
      <SettingsProvider backend={m.backend}>
        <Probe />
      </SettingsProvider>,
    );
    expect(text()).toBe(`${DEFAULT_SETTINGS.stream.maxRateHz}|`);
  });
  test("an update merges the patch per section, re-renders and writes through once", () => {
    const m = memoryBackend({ scenarios: { recent: ["a"] } });
    render(
      <SettingsProvider backend={m.backend}>
        <Probe />
      </SettingsProvider>,
    );
    act(() => update?.({ stream: { maxRateHz: 10 } }));
    expect(text()).toBe("10|a");
    expect(m.writes).toHaveLength(1);
    expect(m.writes[0]?.stream.maxRateHz).toBe(10);
    expect(m.writes[0]?.scenarios.recent).toEqual(["a"]);
  });
  test("an update is clamped by the parser before it is stored", () => {
    const m = memoryBackend();
    render(
      <SettingsProvider backend={m.backend}>
        <Probe />
      </SettingsProvider>,
    );
    act(() => update?.({ stream: { maxRateHz: 1000 } }));
    expect(text()).toBe("60|");
    expect(m.writes[0]?.stream.maxRateHz).toBe(60);
  });
  test("two updates write twice, in order", () => {
    const m = memoryBackend();
    render(
      <SettingsProvider backend={m.backend}>
        <Probe />
      </SettingsProvider>,
    );
    act(() => update?.({ stream: { maxRateHz: 5 } }));
    act(() => update?.({ stream: { maxRateHz: 6 } }));
    expect(m.writes.map((w) => w.stream.maxRateHz)).toEqual([5, 6]);
  });
  test("two updates in one batch both apply (functional merge, no lost update)", () => {
    const m = memoryBackend();
    render(
      <SettingsProvider backend={m.backend}>
        <Probe />
      </SettingsProvider>,
    );
    act(() => {
      update?.({ stream: { maxRateHz: 5 } });
      update?.({ scenarios: { recent: ["x"] } });
    });
    expect(text()).toBe("5|x");
  });
  test("an unchanged update does not write", () => {
    const m = memoryBackend();
    render(
      <SettingsProvider backend={m.backend}>
        <Probe />
      </SettingsProvider>,
    );
    act(() => update?.({ stream: { maxRateHz: DEFAULT_SETTINGS.stream.maxRateHz } }));
    expect(m.writes).toHaveLength(0);
  });
  test("the update function keeps its identity across updates", () => {
    updateIdentities = 0;
    const m = memoryBackend();
    render(
      <SettingsProvider backend={m.backend}>
        <Probe />
      </SettingsProvider>,
    );
    act(() => update?.({ stream: { maxRateHz: 5 } }));
    expect(updateIdentities).toBe(0);
  });
  test("a backend that throws on write does not break the UI", () => {
    const backend: SettingsBackend = {
      read: () => undefined,
      write: () => {
        throw new Error("disk full");
      },
    };
    render(
      <SettingsProvider backend={backend}>
        <Probe />
      </SettingsProvider>,
    );
    expect(() => act(() => update?.({ stream: { maxRateHz: 5 } }))).not.toThrow();
    expect(text()).toBe("5|");
  });
  test("a backend whose read throws gives the defaults", () => {
    const backend: SettingsBackend = {
      read: () => {
        throw new Error("denied");
      },
      write: () => {},
    };
    render(
      <SettingsProvider backend={backend}>
        <Probe />
      </SettingsProvider>,
    );
    expect(text()).toBe(`${DEFAULT_SETTINGS.stream.maxRateHz}|`);
  });
  test("a file from a newer version is used but never written back (one warning)", () => {
    const m = memoryBackend({ version: 2, stream: { maxRateHz: 8 } });
    const warn = console.warn;
    const warnings: unknown[] = [];
    console.warn = (...a: unknown[]) => void warnings.push(a);
    try {
      render(
        <SettingsProvider backend={m.backend}>
          <Probe />
        </SettingsProvider>,
      );
      const before = warnings.length;
      act(() => update?.({ stream: { maxRateHz: 9 } }));
      act(() => update?.({ stream: { maxRateHz: 10 } }));
      expect(text()).toBe("10|");
      expect(m.writes).toHaveLength(0);
      expect(warnings.length - before).toBe(1);
    } finally {
      console.warn = warn;
    }
  });
});
