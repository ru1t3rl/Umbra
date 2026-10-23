// WI-9: the e2e hook gains selectedId() and labelText() (plan WI-9; e2e build only). Written before the
// MapInspection members exist. They read the view's own state, like the other inspection members.
import { afterEach, describe, expect, test } from "bun:test";
import { registerE2eHook } from "./e2eHook.ts";
import type { MapInspection, MapView } from "./MapView.ts";

function viewWith(selected: number | null, label: string | null): MapView {
  const inspection = {
    sourceEntities: () => 0,
    renderedEntities: () => 0,
    sourcePositions: () => [],
    renderedPositions: () => [],
    frameTick: () => null,
    zoom: () => 12,
    project: () => null,
    isIdle: () => true,
    lastFlushMs: () => 0,
    missingImages: () => 0,
    tick: () => 0,
    selectedId: () => selected,
    labelText: () => label,
  } satisfies MapInspection;
  return {
    setEntities: () => undefined,
    setTheme: () => undefined,
    fitTo: () => true,
    inspect: () => inspection,
    destroy: () => undefined,
  } as MapView;
}

afterEach(() => Reflect.deleteProperty(window, "__UMBRA_E2E__"));

describe("registerE2eHook selection members", () => {
  test("exposes selectedId() and labelText() of the view", () => {
    registerE2eHook(viewWith(7, "Alpha"));
    const hook = Reflect.get(window, "__UMBRA_E2E__");
    expect(hook.map.selectedId()).toBe(7);
    expect(hook.map.labelText()).toBe("Alpha");
  });
  test("reports null for no selection and no label", () => {
    registerE2eHook(viewWith(null, null));
    const hook = Reflect.get(window, "__UMBRA_E2E__");
    expect(hook.map.selectedId()).toBeNull();
    expect(hook.map.labelText()).toBeNull();
  });
});
