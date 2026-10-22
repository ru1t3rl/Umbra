import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { SettingsProvider } from "../components/SettingsProvider/SettingsProvider.tsx";
import type { SettingsBackend } from "../model/backend.ts";
import { useAffiliationOf } from "./useAffiliationOf.ts";
import { useUpdateSettings } from "./useUpdateSettings.ts";

afterEach(cleanup);

const backendOf = (overrides: Record<string, string>): SettingsBackend => ({
  read: () => ({ map: { affiliationOverrides: overrides } }),
  write: () => {},
});

const fns: Array<ReturnType<typeof useAffiliationOf>> = [];
let update: ReturnType<typeof useUpdateSettings> | undefined;
function Probe({ force }: { force: string | null }) {
  const of = useAffiliationOf();
  update = useUpdateSettings();
  fns.push(of);
  return <output data-testid="a">{of(force)}</output>;
}

describe("useAffiliationOf (ASM-831)", () => {
  test("without overrides it is the ASM-305 table", () => {
    render(
      <SettingsProvider backend={backendOf({})}>
        <Probe force="blue" />
      </SettingsProvider>,
    );
    expect(screen.getByTestId("a").textContent).toBe("friend");
  });
  test("without a provider it is the ASM-305 table", () => {
    render(<Probe force="red" />);
    expect(screen.getByTestId("a").textContent).toBe("hostile");
  });
  test("an override wins over the table", () => {
    render(
      <SettingsProvider backend={backendOf({ blue: "hostile" })}>
        <Probe force="blue" />
      </SettingsProvider>,
    );
    expect(screen.getByTestId("a").textContent).toBe("hostile");
  });
  test("an override matches case-insensitively and trimmed", () => {
    render(
      <SettingsProvider backend={backendOf({ blue: "neutral" })}>
        <Probe force=" BLUE " />
      </SettingsProvider>,
    );
    expect(screen.getByTestId("a").textContent).toBe("neutral");
  });
  test("an override can name a force the table does not know", () => {
    render(
      <SettingsProvider backend={backendOf({ purple: "friend" })}>
        <Probe force="Purple" />
      </SettingsProvider>,
    );
    expect(screen.getByTestId("a").textContent).toBe("friend");
  });
  test("a force without an override and unknown to the table is unknown; null is unknown", () => {
    render(
      <SettingsProvider backend={backendOf({ purple: "friend" })}>
        <Probe force={null} />
      </SettingsProvider>,
    );
    expect(screen.getByTestId("a").textContent).toBe("unknown");
  });
  test("names on Object.prototype are unknown", () => {
    let of: ReturnType<typeof useAffiliationOf> | undefined;
    function P() {
      of = useAffiliationOf();
      return null;
    }
    render(
      <SettingsProvider backend={backendOf({})}>
        <P />
      </SettingsProvider>,
    );
    for (const k of ["constructor", "toString", "__proto__"]) expect(of?.(k)).toBe("unknown");
  });
  test("changing overrides re-renders with the new mapping", () => {
    render(
      <SettingsProvider backend={backendOf({})}>
        <Probe force="blue" />
      </SettingsProvider>,
    );
    act(() => update?.({ map: { affiliationOverrides: { blue: "hostile" } } }));
    expect(screen.getByTestId("a").textContent).toBe("hostile");
  });
  test("the function keeps its identity while the overrides are unchanged", () => {
    fns.length = 0;
    render(
      <SettingsProvider backend={backendOf({})}>
        <Probe force="blue" />
      </SettingsProvider>,
    );
    act(() => update?.({ stream: { maxRateHz: 5 } }));
    expect(fns.length).toBeGreaterThan(1);
    expect(new Set(fns).size).toBe(1);
  });
});
