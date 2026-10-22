import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import { radarSchema, radarValue } from "../../../test/fixtures/schemas.ts";
import { SchemaForm } from "./SchemaForm.tsx";

afterEach(cleanup);

const form = (schema: unknown, value: unknown, idPrefix = "f") =>
  render(
    <SchemaForm schema={schema} value={value} label="Parameters" idPrefix={idPrefix} readOnly />,
  );

describe("help is described, not only drawn", () => {
  test("the probe: a JSON field's group is described by Required and the default", () => {
    // Catches: JSON nodes losing description/required/default help.
    form(
      {
        type: "object",
        required: ["x"],
        properties: { x: { oneOf: [], description: "Help!", default: 5 } },
      },
      {},
    );
    const g = screen.getByRole("group", { name: "x" });
    expect(g).toHaveAccessibleDescription("Help! · Required · Using default: 5");
  });
  test("a nested object's description describes its group", () => {
    form(
      {
        type: "object",
        properties: { o: { type: "object", description: "About o.", properties: {} } },
      },
      {},
    );
    expect(screen.getByRole("group", { name: "o" })).toHaveAccessibleDescription("About o.");
  });
  test("a switch is described by its help", () => {
    form(radarSchema, {});
    expect(screen.getByRole("switch", { name: "enabled" })).toHaveAccessibleDescription(
      "Using default: true",
    );
  });
  test("a chip group is described by its help", () => {
    form(radarSchema, radarValue);
    expect(screen.getByRole("group", { name: "mode" })).toHaveAccessibleDescription(/Radar mode\./);
  });
  test("a field named like another field's help id does not steal its description", () => {
    // Catches: id collision between `x-help` (field) and the help element of `x`.
    form(
      {
        type: "object",
        properties: {
          x: { type: "boolean", description: "Real help." },
          "x-help": { type: "number" },
        },
      },
      { x: true },
    );
    expect(screen.getByRole("switch", { name: "x" })).toHaveAccessibleDescription("Real help.");
    const ids = [...document.querySelectorAll("[id]")].map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("structure", () => {
  test("groups use the ui-kit outline Card", () => {
    const { container } = form(radarSchema, radarValue);
    expect(
      within(screen.getByRole("group", { name: "Antenna" })).queryByText("gain_db"),
    ).not.toBeNull();
    expect(container.querySelector(".vt-card-outline")).not.toBeNull();
  });
  test("the component imports its own CSS (ADR 0043 amendment 2026-10-22)", async () => {
    const src = await Bun.file(new URL("./SchemaForm.tsx", import.meta.url)).text();
    expect(src).toMatch(/import\s+["'][^"']+\.css["']/);
  });
});

test("a root that falls back to JSON describes the form group by its help", () => {
  // Catches: the root's description/default help drawn but not linked.
  render(
    <SchemaForm
      schema={{ oneOf: [], description: "Root help." }}
      value={{ a: 1 }}
      label="Parameters"
      idPrefix="r"
      readOnly
    />,
  );
  expect(screen.getByRole("group", { name: "Parameters" })).toHaveAccessibleDescription(
    "Root help.",
  );
});
