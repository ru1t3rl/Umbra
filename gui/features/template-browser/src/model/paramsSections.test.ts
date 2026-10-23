// WI-12 `paramsSections(template, index)`: one section per ComponentRef of a template, in display order: the
// motion model, the sensors in template order, then the behavior if any. A section carries the merged params of
// the template and the `params_schema` joined by id, or `schema: undefined` when the component is unknown
// (ASM-816). Section shape (ASM-947): `{ key, kind: "motion" | "sensor" | "behavior", id, params, schema }`,
// `key` unique within the result (it is the React key and the SchemaForm id prefix).
import { describe, expect, test } from "bun:test";
import type { TemplateDetail } from "@umbra/data";
import { COMPONENTS, SCOUT, TANK } from "../browserTesting.tsx";
import { componentIndex } from "./componentIndex.ts";
import { paramsSections } from "./paramsSections.ts";

const index = componentIndex(COMPONENTS);
const schemaOf = (id: string) => COMPONENTS.find((c) => c.id === id)?.params_schema;

describe("paramsSections", () => {
  test("orders motion, then sensors in template order, then behavior", () => {
    const sections = paramsSections(TANK, index);
    expect(sections.map((s) => [s.kind, s.id])).toEqual([
      ["motion", "builtin.waypoint"],
      ["sensor", "builtin.visual"],
      ["sensor", "acme.lidar"],
      ["behavior", "builtin.patrol"],
    ]);
  });

  test("keeps the sensor order of the template, not the registry order", () => {
    const reversed: TemplateDetail = { ...SCOUT, sensors: [...SCOUT.sensors].reverse() };
    expect(
      paramsSections(reversed, index)
        .filter((s) => s.kind === "sensor")
        .map((s) => s.id),
    ).toEqual(["builtin.radar", "builtin.visual"]);
  });

  test("a null or absent behavior gives no behavior section", () => {
    expect(paramsSections(SCOUT, index).map((s) => s.kind)).toEqual(["motion", "sensor", "sensor"]);
    const { behavior: _omitted, ...without } = SCOUT;
    expect(paramsSections(without, index).some((s) => s.kind === "behavior")).toBe(false);
  });

  test("a template without sensors has only the motion section", () => {
    expect(paramsSections({ ...SCOUT, sensors: [] }, index).map((s) => s.kind)).toEqual(["motion"]);
  });

  test("carries the merged params of the template, not defaults", () => {
    const sections = paramsSections(SCOUT, index);
    expect(sections[0]?.params).toEqual({ speed_mps: 12, loop: false });
    expect(sections[1]?.params).toEqual({ range_m: 3000, fov_deg: 120 });
    expect(sections[2]?.params).toEqual({ range_m: 20000 });
  });

  test("joins each section to the schema of its own component id", () => {
    const sections = paramsSections(SCOUT, index);
    expect(sections[0]?.schema).toEqual(schemaOf("builtin.waypoint"));
    expect(sections[1]?.schema).toEqual(schemaOf("builtin.visual"));
    expect(sections[2]?.schema).toEqual(schemaOf("builtin.radar"));
    expect(sections[1]?.schema).not.toEqual(sections[2]?.schema);
  });

  test("a component missing from the index has an undefined schema and keeps its params", () => {
    const lidar = paramsSections(TANK, index).find((s) => s.id === "acme.lidar");
    expect(lidar?.schema).toBeUndefined();
    expect(lidar?.params).toEqual({ rays: 64 });
  });

  test("an empty index leaves every schema undefined", () => {
    const sections = paramsSections(TANK, componentIndex([]));
    expect(sections).toHaveLength(4);
    expect(sections.every((s) => s.schema === undefined)).toBe(true);
  });

  test("every section key is unique, also for two sensors with one id", () => {
    const twin: TemplateDetail = {
      ...SCOUT,
      sensors: [
        { id: "builtin.radar", params: { range_m: 1 } },
        { id: "builtin.radar", params: { range_m: 2 } },
      ],
    };
    const keys = paramsSections(twin, index).map((s) => s.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  test("does not modify the template", () => {
    const before = JSON.stringify(SCOUT);
    paramsSections(SCOUT, index);
    expect(JSON.stringify(SCOUT)).toBe(before);
  });
});
