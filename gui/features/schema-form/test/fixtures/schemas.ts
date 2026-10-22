import components from "./components.m6.json";

interface Entry {
  kind: string;
  id: string;
  params_schema: Record<string, unknown>;
}

const entries = components as Entry[];

/** The `params_schema` the engine served for a built-in component (ASM-817). */
export function builtinSchema(id: string): Record<string, unknown> {
  const found = entries.find((e) => e.id === id);
  if (!found) throw new Error(`fixture has no component ${id}`);
  return found.params_schema;
}

export function builtinIds(): string[] {
  return entries.map((e) => e.id);
}

/**
 * A plugin-style schema in the shapes the engine's plugins serve (arrays of numbers as in the
 * test network plugin, nested objects, enums, `$defs`). Plugin schemas are not in the M6 capture.
 */
export const radarSchema = {
  title: "example.sensor.radar parameters",
  description: "Radar sensor.",
  type: "object",
  properties: {
    range_m: {
      type: "number",
      exclusiveMinimum: 0,
      maximum: 400000,
      description: "Detection range.",
    },
    scan_period_s: { type: "number", minimum: 0.1, default: 2 },
    mode: {
      type: "string",
      enum: ["search", "track"],
      default: "search",
      description: "Radar mode.",
    },
    band: { type: "string", enum: ["L", "S", "C", "X", "Ku", "Ka"] },
    channels: { type: "integer", minimum: 1, maximum: 16 },
    enabled: { type: "boolean", default: true },
    detonate_position: { type: "array", items: { type: "number" }, minItems: 3, maxItems: 3 },
    antenna: { $ref: "#/$defs/antenna" },
    label: { type: "string", description: "Free text." },
  },
  required: ["range_m"],
  additionalProperties: false,
  $defs: {
    antenna: {
      type: "object",
      title: "Antenna",
      properties: {
        gain_db: { type: "number" },
        beam_width_deg: { type: "number", exclusiveMinimum: 0, maximum: 360 },
        mount: { type: "object", properties: { offset_m: { type: "number" } } },
      },
      required: ["gain_db"],
    },
  },
} as const;

export const radarValue = {
  range_m: 5000,
  mode: "track",
  band: "Ku",
  channels: 4,
  enabled: false,
  detonate_position: [1, 2.5, 3],
  antenna: { gain_db: 30.5, beam_width_deg: 12, mount: { offset_m: 1.5 } },
  label: "north",
};
