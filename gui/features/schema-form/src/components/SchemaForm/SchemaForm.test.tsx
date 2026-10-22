import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import {
  builtinIds,
  builtinSchema,
  radarSchema,
  radarValue,
} from "../../../test/fixtures/schemas.ts";
import { SchemaForm } from "./SchemaForm.tsx";

afterEach(cleanup);

/*
 * Assumed API (ASM-897): <SchemaForm schema={unknown} value={unknown} label={string} idPrefix={string} readOnly />
 * `readOnly` is the literal `true` in M8 (ADR 0070). Markup contract the tests query, all through ui-kit:
 *   whole form  role=group, name = label
 *   number/integer/string  TextField (role textbox), readOnly and disabled, value = model text, mono for numbers
 *   boolean  Toggle (role switch, aria-checked), disabled
 *   enum chips  ChipGroup (role group named by the field) with disabled Chips (aria-pressed marks the selection)
 *   enum select  Select (role combobox) disabled
 *   array  <ul aria-label=field> of <li>, plus a "...and N more" text when `more` > 0
 *   nested object  role=group named by the field label
 *   JSON fallback  role=group named by the field label with the note text "Shown as JSON" and a <pre>
 *   extras  role=group named "Other parameters", one role=group per key containing a <pre>
 * Help text (TextField `help`, so aria-describedby) joins, in this order, with " · ":
 *   description, "Required" (if required), "Unit: <unit>", bounds ("greater than 0, max 1000"),
 *   "Default: <d>" when a value is given or "Using default: <d>" when it is absent.
 * Ids: every generated id starts with `${idPrefix}-` and is unique within the document.
 */

const form = (schema: unknown, value: unknown, extra: { label?: string; idPrefix?: string } = {}) =>
  render(
    <SchemaForm
      schema={schema}
      value={value}
      label={extra.label ?? "Parameters"}
      idPrefix={extra.idPrefix ?? "f"}
      readOnly
    />,
  );

describe("SchemaForm rendering of the engine's built-in schemas", () => {
  test("is a named group", () => {
    // Catches: missing group name (axe: form fields need a landmark/group context).
    form(
      builtinSchema("builtin.visual"),
      { range_m: 3000, fov_deg: 120 },
      { label: "Sensor parameters" },
    );
    expect(screen.getByRole("group", { name: "Sensor parameters" })).toBeInTheDocument();
  });

  test("a number field is a labelled, read-only and disabled textbox with the value", () => {
    // Catches: editable inputs in M8 (read-only), missing label, value not shown.
    form(builtinSchema("builtin.visual"), { range_m: 3000, fov_deg: 120 });
    const input = screen.getByRole("textbox", { name: "range_m" });
    expect(input).toHaveValue("3000");
    expect(input).toHaveAttribute("readonly");
    expect(input).toBeDisabled();
  });

  test("required fields are described as required and optional ones are not", () => {
    // Catches: required marker missing or on every field.
    form(builtinSchema("builtin.visual"), {});
    expect(screen.getByRole("textbox", { name: "range_m" })).toHaveAccessibleDescription(
      /required/i,
    );
    expect(screen.getByRole("textbox", { name: "eye_height_m" })).not.toHaveAccessibleDescription(
      /required/i,
    );
  });

  test("description, unit and bounds are in the accessible description", () => {
    // Catches: help text not wired via aria-describedby, unit or bounds omitted.
    form(builtinSchema("builtin.ground.kinematic"), {
      max_speed_mps: 18,
      max_turn_rate_dps: 40,
      max_accel_mps2: 2.5,
    });
    const d = screen.getByRole("textbox", { name: "max_speed_mps" });
    expect(d).toHaveAccessibleDescription(/Maximum speed in meters per second\./);
    expect(d).toHaveAccessibleDescription(/Unit: m\/s/);
    expect(d).toHaveAccessibleDescription(/greater than 0, max 1000/);
    expect(screen.getByRole("textbox", { name: "max_turn_rate_dps" })).toHaveAccessibleDescription(
      /Unit: °\/s/,
    );
  });

  test("an absent value with a default shows the default and says 'Using default'", () => {
    // Catches: empty input for an absent value (plan: defaults shown when the value is absent).
    form(builtinSchema("builtin.visual"), { range_m: 1, fov_deg: 1 });
    const input = screen.getByRole("textbox", { name: "eye_height_m" });
    expect(input).toHaveValue("2");
    expect(input).toHaveAccessibleDescription(/Using default: 2/);
  });

  test("a given value with a default shows the value and only 'Default: 2'", () => {
    // Catches: the default overwriting the value, or 'Using default' on a set value.
    form(builtinSchema("builtin.visual"), { range_m: 1, fov_deg: 1, eye_height_m: 1.5 });
    const input = screen.getByRole("textbox", { name: "eye_height_m" });
    expect(input).toHaveValue("1.5");
    expect(input).toHaveAccessibleDescription(/Default: 2/);
    expect(input).not.toHaveAccessibleDescription(/Using default/);
  });

  test("an absent value without a default is an empty field", () => {
    form(builtinSchema("builtin.visual"), {});
    expect(screen.getByRole("textbox", { name: "range_m" })).toHaveValue("");
  });

  test("every built-in schema renders without a JSON fallback and with only disabled controls", () => {
    // Catches: a control that is not disabled (read-only mode), a built-in outside the subset.
    for (const id of builtinIds()) {
      const { container, unmount } = form(builtinSchema(id), {}, { label: id });
      expect(screen.queryByText("Shown as JSON")).toBeNull();
      for (const el of container.querySelectorAll("input, select, textarea, button"))
        expect(el).toBeDisabled();
      unmount();
    }
  });

  test("a no-parameter schema (follow_route) renders an empty group and no controls", () => {
    form(builtinSchema("builtin.action.follow_route"), {});
    expect(screen.getByRole("group", { name: "Parameters" })).toBeInTheDocument();
    expect(screen.queryAllByRole("textbox")).toHaveLength(0);
  });
});

describe("SchemaForm field kinds (plugin-style schema)", () => {
  test("chips for a short enum mark the selection and are disabled", () => {
    // Catches: wrong selected chip, active chips.
    form(radarSchema, radarValue);
    const g = screen.getByRole("group", { name: "mode" });
    const track = within(g).getByRole("button", { name: "track" });
    expect(track).toHaveAttribute("aria-pressed", "true");
    expect(within(g).getByRole("button", { name: "search" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(track).toBeDisabled();
  });

  test("a long enum is a disabled select showing the value", () => {
    // Catches: chips for six values, or a select that is enabled.
    form(radarSchema, radarValue);
    const sel = screen.getByRole("combobox", { name: "band" });
    expect(sel).toHaveValue("Ku");
    expect(sel).toBeDisabled();
  });

  test("a boolean is a disabled switch with the state", () => {
    form(radarSchema, radarValue);
    const sw = screen.getByRole("switch", { name: "enabled" });
    expect(sw).toHaveAttribute("aria-checked", "false");
    expect(sw).toBeDisabled();
  });

  test("a boolean that is absent shows its default state", () => {
    form(radarSchema, {});
    expect(screen.getByRole("switch", { name: "enabled" })).toHaveAttribute("aria-checked", "true");
  });

  test("an array of numbers is a named list of its items", () => {
    // Catches: joined text, missing list semantics.
    form(radarSchema, radarValue);
    const list = screen.getByRole("list", { name: "detonate_position" });
    expect(
      within(list)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["1", "2.5", "3"]);
  });

  test("a long array shows 200 items and says how many are not shown", () => {
    // Catches: silent truncation or an unbounded DOM.
    form(radarSchema, { detonate_position: Array.from({ length: 1000 }, (_, i) => i) });
    expect(
      within(screen.getByRole("list", { name: "detonate_position" })).getAllByRole("listitem"),
    ).toHaveLength(200);
    expect(screen.getByText(/800 more/)).toBeInTheDocument();
  });

  test("a $ref'd nested object is a group named by its title with its own fields and units", () => {
    // Catches: flattening the nested object into the parent, wrong label, lost nested unit.
    form(radarSchema, radarValue);
    const antenna = screen.getByRole("group", { name: "Antenna" });
    expect(within(antenna).getByRole("textbox", { name: "gain_db" })).toHaveValue("30.5");
    expect(within(antenna).getByRole("textbox", { name: "gain_db" })).toHaveAccessibleDescription(
      /required/i,
    );
    const mount = within(antenna).getByRole("group", { name: "mount" });
    expect(within(mount).getByRole("textbox", { name: "offset_m" })).toHaveAccessibleDescription(
      /Unit: m/,
    );
  });

  test("a string field shows its text", () => {
    form(radarSchema, radarValue);
    expect(screen.getByRole("textbox", { name: "label" })).toHaveValue("north");
  });

  test("an integer is shown without decimals and bounds", () => {
    form(radarSchema, radarValue);
    const c = screen.getByRole("textbox", { name: "channels" });
    expect(c).toHaveValue("4");
    expect(c).toHaveAccessibleDescription(/min 1, max 16/);
  });
});

describe("SchemaForm fallbacks are visible", () => {
  const schema = {
    type: "object",
    properties: {
      choice: { oneOf: [{ type: "number" }, { type: "string" }] },
      n: { type: "number" },
    },
  };

  test("oneOf renders a group with the note 'Shown as JSON' and the pretty value", () => {
    // Catches: dropping the field, rendering it as an input, or omitting the note.
    form(schema, { choice: { a: [1, 2] }, n: 1 });
    const g = screen.getByRole("group", { name: "choice" });
    expect(within(g).getByText("Shown as JSON")).toBeInTheDocument();
    expect(g.querySelector("pre")?.textContent).toBe(JSON.stringify({ a: [1, 2] }, null, 2));
  });

  test("a wrong-typed value is shown as JSON, not as an empty input", () => {
    form(schema, { n: "not a number" });
    const g = screen.getByRole("group", { name: "n" });
    expect(g.querySelector("pre")?.textContent).toBe('"not a number"');
  });

  test("a non-object root renders the whole thing as JSON", () => {
    form({ type: "array", items: { type: "number" } }, [1, 2]);
    const g = screen.getByRole("group", { name: "Parameters" });
    expect(within(g).getByText("Shown as JSON")).toBeInTheDocument();
    expect(g.querySelector("pre")?.textContent).toContain("1");
  });

  test("values not in the schema are listed under 'Other parameters' with their JSON", () => {
    // Catches: unknown keys silently dropped (ADR 0070).
    form(schema, { n: 1, extra: { k: true }, tag: "x" });
    const other = screen.getByRole("group", { name: "Other parameters" });
    expect(
      within(other).getByRole("group", { name: "extra" }).querySelector("pre")?.textContent,
    ).toBe(JSON.stringify({ k: true }, null, 2));
    expect(
      within(other).getByRole("group", { name: "tag" }).querySelector("pre")?.textContent,
    ).toBe('"x"');
  });

  test("no 'Other parameters' group when nothing is extra", () => {
    form(schema, { n: 1 });
    expect(screen.queryByRole("group", { name: "Other parameters" })).toBeNull();
  });

  test("a recursive schema renders the recursion point as JSON and does not hang", () => {
    const s = {
      type: "object",
      properties: { next: { $ref: "#/$defs/n" } },
      $defs: {
        n: { type: "object", properties: { next: { $ref: "#/$defs/n" }, v: { type: "number" } } },
      },
    };
    form(s, { next: { v: 1, next: { v: 2 } } });
    expect(screen.getAllByText("Shown as JSON").length).toBe(1);
  });
});

describe("SchemaForm robustness and accessibility", () => {
  test("hostile props render without throwing", () => {
    // Catches: a throw in render for malformed schemas/values (an error boundary must never be needed).
    const cyc: Record<string, unknown> = {};
    cyc.self = cyc;
    for (const [s, v] of [
      [null, null],
      [undefined, undefined],
      [cyc, cyc],
      [radarSchema, cyc],
      [true, 5],
      [[], {}],
      [10n, 10n],
    ] as const) {
      expect(() => form(s, v).unmount()).not.toThrow();
    }
  });

  test("a huge flat schema renders at most the node budget of fields", () => {
    // Catches: 10,000 DOM fields.
    const properties: Record<string, unknown> = {};
    for (let i = 0; i < 10_000; i++) properties[`p${i}`] = { type: "number" };
    const { container } = form({ type: "object", properties }, {});
    expect(container.querySelectorAll("input").length).toBeLessThanOrEqual(1000);
  });

  test("ids are prefixed, unique within a form, and two forms with different prefixes do not collide", () => {
    // Catches: ids from the key alone (duplicate ids make labels point at the wrong input).
    const { container } = render(
      <>
        <SchemaForm schema={radarSchema} value={radarValue} label="A" idPrefix="a" readOnly />
        <SchemaForm schema={radarSchema} value={radarValue} label="B" idPrefix="b" readOnly />
      </>,
    );
    const ids = [...container.querySelectorAll("[id]")].map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.every((id) => id.startsWith("a-") || id.startsWith("b-"))).toBe(true);
    expect(ids.some((id) => id.startsWith("a-"))).toBe(true);
    expect(ids.some((id) => id.startsWith("b-"))).toBe(true);
  });

  test("equal key names at different depths get different ids", () => {
    // Catches: ids built from the leaf key only.
    const s = {
      type: "object",
      properties: {
        x: { type: "number" },
        in: { type: "object", properties: { x: { type: "number" } } },
      },
    };
    const { container } = form(s, { x: 1, in: { x: 2 } });
    const ids = [...container.querySelectorAll("input")].map((e) => e.id);
    expect(ids).toHaveLength(2);
    expect(new Set(ids).size).toBe(2);
  });

  test("every control has an accessible name and no interactive element is enabled", () => {
    // Catches: unnamed controls and enabled controls in read-only mode.
    const { container } = form(radarSchema, radarValue);
    for (const el of container.querySelectorAll("input, select, button, [role=switch]")) {
      expect(el).toBeDisabled();
      expect(
        el.getAttribute("aria-label") ||
          el.getAttribute("aria-labelledby") ||
          (el as HTMLInputElement).labels?.length ||
          el.textContent,
      ).toBeTruthy();
    }
  });

  test("it uses ui-kit controls (vt- classes), not bare inputs", () => {
    // Catches: raw <input> without the design-system class (A-003).
    const { container } = form(radarSchema, radarValue);
    expect(container.querySelector(".vt-field")).not.toBeNull();
    expect(container.querySelector(".vt-toggle")).not.toBeNull();
    expect(container.querySelector(".vt-chipgroup")).not.toBeNull();
  });

  test("rerendering with a new value updates the displayed value", () => {
    // Catches: state initialised once from props (the template browser changes selection).
    const { rerender } = form(builtinSchema("builtin.visual"), { range_m: 1, fov_deg: 1 });
    rerender(
      <SchemaForm
        schema={builtinSchema("builtin.visual")}
        value={{ range_m: 2, fov_deg: 1 }}
        label="Parameters"
        idPrefix="f"
        readOnly
      />,
    );
    expect(screen.getByRole("textbox", { name: "range_m" })).toHaveValue("2");
  });

  test("every value in the data appears in the DOM (nothing is silently dropped)", () => {
    // Catches: any path that drops a value; a pretty printed JSON view counts as showing it.
    const value = { ...radarValue, unknownKey: "needle-123" };
    const { container } = form(radarSchema, value);
    expect(container.textContent).toContain("needle-123");
    for (const v of ["5000", "Ku", "30.5", "12", "1.5", "north"]) {
      const inInputs = [...container.querySelectorAll("input, select")].some(
        (e) => (e as HTMLInputElement).value === v,
      );
      expect(inInputs || container.textContent?.includes(v)).toBeTruthy();
    }
  });
});
