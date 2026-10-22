// WI-5 / ADR 0071: the decoder hands the frame's events to the journal (the store never keeps them).
// Assumed: Decoded "frame" gains `events: readonly EventSummary[]` and `events_truncated: boolean`.
import { describe, expect, test } from "bun:test";
import { entity, ev, frame } from "../testing.ts";
import { decodeJson } from "./decode.ts";

const dec = (m: unknown) => decodeJson(JSON.stringify(m));
function frameOf(m: unknown) {
  const d = dec(m);
  if (d.kind !== "frame") throw new Error("expected a frame");
  return d;
}

describe("decoding frame events", () => {
  test("events keep their order and fields", () => {
    // Mutation caught: reordering, dropping weapon/target_id/template.
    const d = frameOf(
      frame(0, 5, [entity(1)], {
        events: [
          ev("fire", 4, { entity_id: 1, target_id: 2, weapon: "gun" }),
          ev("entity_created", 5, { entity_id: 3, template: "tank" }),
        ],
      }),
    );
    expect(d.events).toEqual([
      { kind: "fire", tick: 4, entity_id: 1, target_id: 2, weapon: "gun" },
      { kind: "entity_created", tick: 5, entity_id: 3, template: "tank" },
    ]);
  });
  test("an unknown kind and its unknown fields are kept verbatim", () => {
    // Mutation caught: filtering to a known-kind list (ADR 0071 forward compatibility).
    const e = ev("jamming_started", 9, { power_w: 12, nested: { a: 1 } });
    expect(frameOf(frame(0, 9, [], { events: [e] })).events).toEqual([e]);
  });
  test("entries without a string kind or a finite tick are dropped, the rest stay", () => {
    // Mutation caught: no validation (a null entry would crash the journal).
    const good = ev("fire", 1);
    const d = frameOf(
      frame(0, 1, [], {
        events: [
          null,
          3,
          "x",
          {},
          { kind: 4, tick: 1 },
          { kind: "fire" },
          { kind: "fire", tick: "1" },
          good,
        ],
      }),
    );
    expect(d.events).toEqual([good]);
  });
  test("events_truncated is read, false when missing or mistyped", () => {
    // Mutation caught: truthiness instead of === true.
    expect(frameOf(frame(0, 1, [], { events_truncated: true })).events_truncated).toBe(true);
    expect(frameOf(frame(0, 1, [], { events_truncated: "yes" })).events_truncated).toBe(false);
    const { events_truncated: _omit, ...rest } = frame(0, 1);
    expect(frameOf(rest).events_truncated).toBe(false);
  });
  test("a frame whose events field is missing or not an array decodes with no events", () => {
    // Mutation caught: treating a bad events field as an invalid frame (the map would lose the entities).
    const { events: _omit, ...rest } = frame(0, 1, [entity(1)]);
    const d = frameOf(rest);
    expect(d.events).toEqual([]);
    expect(d.frame.entities).toHaveLength(1);
    expect(frameOf(frame(0, 1, [], { events: "nope" })).events).toEqual([]);
  });
});
