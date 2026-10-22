import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { SettingsProvider } from "../components/SettingsProvider/SettingsProvider.tsx";
import type { SettingsBackend } from "../model/backend.ts";
import type { Settings } from "../model/settings.ts";
import { useSettings } from "./useSettings.ts";
import { useThemeStorage } from "./useThemeStorage.ts";
import { useUpdateSettings } from "./useUpdateSettings.ts";

afterEach(cleanup);

function memoryBackend(initial: unknown = undefined) {
  let stored: unknown = initial;
  const writes: Settings[] = [];
  const backend: SettingsBackend = {
    read: () => stored,
    write: (s) => {
      stored = s;
      writes.push(s);
    },
  };
  return { backend, writes, stored: () => stored as Settings };
}

let storage: ReturnType<typeof useThemeStorage> | undefined;
let update: ReturnType<typeof useUpdateSettings> | undefined;
const storages: Array<ReturnType<typeof useThemeStorage>> = [];
function Probe() {
  storage = useThemeStorage();
  update = useUpdateSettings();
  storages.push(storage);
  const s = useSettings();
  return <output data-testid="t">{String(s.appearance.theme)}</output>;
}
function mount(initial?: unknown) {
  const m = memoryBackend(initial);
  render(
    <SettingsProvider backend={m.backend}>
      <Probe />
    </SettingsProvider>,
  );
  return m;
}

describe("useThemeStorage (theme storage for the desktop app, ADR 0074, WI-22)", () => {
  test("getItem('umbra.theme') is null while appearance.theme is not set", () => {
    mount();
    expect(storage?.getItem("umbra.theme")).toBeNull();
  });
  test("getItem returns appearance.theme from the backend", () => {
    mount({ version: 1, appearance: { theme: "dark" } });
    expect(storage?.getItem("umbra.theme")).toBe("dark");
  });
  test("an invalid stored value reads as null", () => {
    mount({ version: 1, appearance: { theme: "purple" } });
    expect(storage?.getItem("umbra.theme")).toBeNull();
  });
  test("setItem writes appearance.theme once, getItem and useSettings see it", () => {
    const m = mount();
    act(() => storage?.setItem("umbra.theme", "light"));
    expect(m.writes).toHaveLength(1);
    expect(m.writes[0]?.appearance.theme).toBe("light");
    expect(storage?.getItem("umbra.theme")).toBe("light");
    expect(screen.getByTestId("t").textContent).toBe("light");
  });
  test("setItem of an invalid theme value stores null", () => {
    const m = mount({ appearance: { theme: "dark" } });
    act(() => storage?.setItem("umbra.theme", "purple"));
    expect(m.writes.at(-1)?.appearance.theme).toBeNull();
  });
  test("setItem keeps the other settings", () => {
    const m = mount({ version: 1, stream: { maxRateHz: 7 } });
    act(() => storage?.setItem("umbra.theme", "dark"));
    expect(m.writes[0]?.stream.maxRateHz).toBe(7);
  });
  test("any other key reads as null and is ignored on write", () => {
    const m = mount();
    act(() => storage?.setItem("other", "x"));
    expect(storage?.getItem("other")).toBeNull();
    expect(storage?.getItem("constructor")).toBeNull();
    expect(m.writes).toHaveLength(0);
  });
  test("one shared record: a later settings update does not revert the theme", () => {
    const m = mount();
    act(() => storage?.setItem("umbra.theme", "dark"));
    act(() => update?.({ stream: { maxRateHz: 5 } }));
    expect(m.stored().appearance.theme).toBe("dark");
    expect(m.stored().stream.maxRateHz).toBe(5);
    expect(screen.getByTestId("t").textContent).toBe("dark");
  });
  test("the storage object keeps its identity across updates", () => {
    storages.length = 0;
    mount();
    act(() => update?.({ stream: { maxRateHz: 5 } }));
    expect(storages.length).toBeGreaterThan(1);
    expect(new Set(storages).size).toBe(1);
  });
  test("outside a provider it is inert", () => {
    render(<Probe />);
    expect(storage?.getItem("umbra.theme")).toBeNull();
    expect(() => storage?.setItem("umbra.theme", "dark")).not.toThrow();
  });
});
