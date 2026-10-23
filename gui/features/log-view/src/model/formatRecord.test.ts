// WI-11 (plan "Model: formatRecord ..."): written before model/formatRecord.ts exists.
// `formatRecord(record: JournalRecord): string`. Event records (source "event") are formatted from the event DTO
// the journal keeps as `detail` and from `entity_id`; every other source shows its `message` as is (ASM-944 a).
// Catches: the raw kind shown for a known event, a crash on a missing `detail`, "undefined" or "null" in the
// text, an unknown kind swallowed, the loss records losing their message.
import { describe, expect, test } from "bun:test";
import { eventRec, rec } from "../logTesting.tsx";
import { formatRecord } from "./formatRecord.ts";

describe("formatRecord: events", () => {
  test("entity_created: Created #id (template)", () => {
    const r = eventRec(1, { kind: "entity_created", entity_id: 12, template: "tank" });
    expect(formatRecord(r)).toBe("Created #12 (tank)");
  });
  test("entity_created without a template: Created #id", () => {
    expect(formatRecord(eventRec(1, { kind: "entity_created", entity_id: 12 }))).toBe(
      "Created #12",
    );
    expect(
      formatRecord(eventRec(1, { kind: "entity_created", entity_id: 12, template: null })),
    ).toBe("Created #12");
  });
  test("entity_removed: Removed #id", () => {
    expect(formatRecord(eventRec(1, { kind: "entity_removed", entity_id: 12 }))).toBe(
      "Removed #12",
    );
  });
  test("fire: #shooter fired at #target (weapon)", () => {
    const r = eventRec(1, { kind: "fire", entity_id: 3, target_id: 7, weapon: "rifle" });
    expect(formatRecord(r)).toBe("#3 fired at #7 (rifle)");
  });
  test("fire without a weapon or a target drops that part", () => {
    expect(formatRecord(eventRec(1, { kind: "fire", entity_id: 3, target_id: 7 }))).toBe(
      "#3 fired at #7",
    );
    expect(formatRecord(eventRec(1, { kind: "fire", entity_id: 3, weapon: "rifle" }))).toBe(
      "#3 fired (rifle)",
    );
    expect(formatRecord(eventRec(1, { kind: "fire", entity_id: 3 }))).toBe("#3 fired");
  });
  test("detonation: #shooter detonated at #target; the parts are optional", () => {
    expect(formatRecord(eventRec(1, { kind: "detonation", entity_id: 3, target_id: 7 }))).toBe(
      "#3 detonated at #7",
    );
    expect(formatRecord(eventRec(1, { kind: "detonation", entity_id: 3 }))).toBe("#3 detonated");
    expect(formatRecord(eventRec(1, { kind: "detonation" }))).toBe("Detonation");
  });
  test("simulation_control: the action, with the step count", () => {
    const control = (detail: Record<string, unknown>) =>
      formatRecord(eventRec(1, { kind: "simulation_control", detail }));
    expect(control({ action: "start" })).toBe("Simulation start");
    expect(control({ action: "pause" })).toBe("Simulation pause");
    expect(control({ action: "step", count: 600 })).toBe("Simulation step 600");
  });
  test("simulation_control without an action is the plain label", () => {
    expect(formatRecord(eventRec(1, { kind: "simulation_control" }))).toBe("Simulation control");
  });
  test("custom: custom: <component id>", () => {
    const r = eventRec(1, { kind: "custom", detail: { kind: "wander.leader", payload: { a: 1 } } });
    expect(formatRecord(r)).toBe("custom: wander.leader");
  });
  test("custom without a component id is just custom", () => {
    expect(formatRecord(eventRec(1, { kind: "custom" }))).toBe("custom");
  });
  test("an unknown kind is shown verbatim, whatever its fields (forward compatible)", () => {
    const r = eventRec(1, { kind: "weather_changed", entity_id: 4, extra: { a: 1 } });
    expect(formatRecord(r)).toBe("weather_changed");
  });
  test("an event record without a detail does not crash and never prints undefined or null", () => {
    const bare = rec(1, { source: "event", kind: "fire", message: "fire" });
    expect(() => formatRecord(bare)).not.toThrow();
    const text = formatRecord(bare);
    expect(text).not.toMatch(/undefined|null|NaN/);
  });
  test("the loss records keep their message", () => {
    const truncated = rec(1, {
      source: "event",
      level: "warning",
      kind: "events_truncated",
      message: "Events were lost beyond the per-frame cap",
    });
    const dropped = rec(2, {
      source: "event",
      level: "warning",
      kind: "frames_dropped",
      message: "The server dropped 3 frame(s) for this client",
    });
    expect(formatRecord(truncated)).toBe("Events were lost beyond the per-frame cap");
    expect(formatRecord(dropped)).toBe("The server dropped 3 frame(s) for this client");
  });
});

describe("formatRecord: other sources", () => {
  test("run, connection, engine-process and gui records show their message", () => {
    for (const source of ["run", "connection", "engine-process", "gui"] as const) {
      expect(formatRecord(rec(1, { source, kind: "fire", message: "as is" }))).toBe("as is");
    }
  });
  test("a non-event record named like an event kind is not reformatted", () => {
    expect(
      formatRecord(rec(1, { source: "engine-process", kind: "entity_created", message: "line" })),
    ).toBe("line");
  });
});
