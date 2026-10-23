import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { SettingsProvider } from "../components/SettingsProvider/SettingsProvider.tsx";
import type { SettingsBackend } from "../model/backend.ts";
import { useRecordRecentScenario } from "./useRecordRecentScenario.ts";
import { useSettings } from "./useSettings.ts";

afterEach(cleanup);

let record: ReturnType<typeof useRecordRecentScenario> | undefined;
const records: Array<ReturnType<typeof useRecordRecentScenario>> = [];
function Probe() {
  record = useRecordRecentScenario();
  records.push(record);
  return <output data-testid="r">{useSettings().scenarios.recent.join("|")}</output>;
}
const mount = (stored?: unknown) => {
  const writes: unknown[] = [];
  const backend: SettingsBackend = { read: () => stored, write: (s) => void writes.push(s) };
  render(
    <SettingsProvider backend={backend}>
      <Probe />
    </SettingsProvider>,
  );
  return writes;
};

describe("useRecordRecentScenario (ASM-975)", () => {
  test("records a path first and persists it", () => {
    const writes = mount({ version: 1, scenarios: { recent: ["old"] } });
    act(() => record?.("new"));
    expect(screen.getByTestId("r").textContent).toBe("new|old");
    expect((writes.at(-1) as { scenarios: { recent: string[] } }).scenarios.recent).toEqual([
      "new",
      "old",
    ]);
  });
  test("a known path moves to the front once", () => {
    mount({ version: 1, scenarios: { recent: ["a", "b"] } });
    act(() => record?.("b"));
    expect(screen.getByTestId("r").textContent).toBe("b|a");
  });
  test("recording twice in a row keeps the newest first (reads the latest list)", () => {
    mount();
    act(() => record?.("one"));
    act(() => record?.("two"));
    expect(screen.getByTestId("r").textContent).toBe("two|one");
  });
  test("two calls in the same tick keep both (the second reads the first)", () => {
    mount();
    act(() => {
      record?.("one");
      record?.("two");
    });
    expect(screen.getByTestId("r").textContent).toBe("two|one");
  });
  test("is a no-op outside a provider (the defaults, like useUpdateSettings)", () => {
    render(<Probe />);
    expect(() => act(() => record?.("x"))).not.toThrow();
  });
});
