// WI-12 SpawnForm (private to the package, rendered by TemplateDetail), written before the implementation.
// Contracts (ASM-947, ASM-820):
// - props `{ templateId: string }`; a `form` named "Spawn entity" with these ui-kit TextFields (role textbox):
//   "Template" (read-only, preset to the id), "Latitude", "Longitude", "Altitude", "Heading", "Speed", "Name",
//   "Force", "Group", and a button "Spawn" (type submit; while a request is in flight it reads "Spawning..." and is
//   disabled). The numeric fields are text inputs with `inputMode="decimal"`, so a non-number can be typed and
//   rejected with a message;
// - the button is enabled only when the run state is ready, running or paused (idle, stopped and an unknown state
//   disable it; the fields stay editable);
// - validation runs on submit and sends nothing when it fails: Latitude and Longitude are required, numbers, and in
//   -90..90 and -180..180 inclusive; Altitude, Heading and Speed are optional numbers (the engine judges their
//   range: `invalid_spawn`). Each problem is the field's help text, so `aria-invalid="true"` and the accessible
//   description say "<Field> is required", "<Field> must be a number" or "<Field> must be between <min> and <max>";
//   all problems show at once and the first invalid field gets focus;
// - the request is `POST /api/v1/entities` with `{ template, lat_deg, lon_deg }` plus `alt_m`, `heading_deg`,
//   `speed_mps` (numbers), `name`, `force`, `group` (trimmed strings) only for the fields that are not blank;
// - a problem answer is a danger Alert (role alert) with the problem's title and detail; `duplicate_name` also marks
//   the Name field invalid; success is a polite `status` "Spawned entity #<id>" (the id of `summary.id`) and the
//   Alert is gone; a new submit clears both before it sends; typed values are kept after success;
// - a different `templateId` changes the preset, clears Name, the result and the problem, and keeps the rest.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import {
  engineWith,
  type FakeEngine,
  jsonReply,
  mount,
  problemReply,
  runFrame,
  settle,
  spawned,
} from "../../browserTesting.tsx";
import { SpawnForm } from "./SpawnForm.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

type Script = Parameters<typeof engineWith>[0];
async function mountForm(templateId = "tank", script: Script = {}, state = "ready") {
  engine = engineWith(script);
  const m = mount(<SpawnForm templateId={templateId} />, { state });
  await settle();
  return m;
}

const field = (name: string) => screen.getByRole("textbox", { name });
const fill = (name: string, value: string) =>
  act(() => void fireEvent.change(field(name), { target: { value } }));
const spawnButton = () => screen.getByRole("button", { name: /^Spawn/ });
const submit = async () => {
  await act(async () => void fireEvent.click(spawnButton()));
  await settle();
};
const posts = () => (engine as FakeEngine).to("POST", "/api/v1/entities");
const validPosition = () => {
  fill("Latitude", "52.5");
  fill("Longitude", "13.4");
};

describe("SpawnForm fields", () => {
  test("is a form named Spawn entity", async () => {
    await mountForm();
    expect(screen.getByRole("form", { name: "Spawn entity" })).toBeInTheDocument();
  });

  test("the template is preset and read-only", async () => {
    await mountForm("tank");
    expect(field("Template")).toHaveValue("tank");
    expect(field("Template")).toHaveAttribute("readonly");
  });

  test.each(["Latitude", "Longitude", "Altitude", "Heading", "Speed", "Name", "Force", "Group"])(
    "has a labelled %s field",
    async (name) => {
      await mountForm();
      expect(field(name)).toBeInTheDocument();
      expect(field(name)).not.toHaveAttribute("readonly");
    },
  );

  test("the numeric fields take a decimal keypad", async () => {
    await mountForm();
    for (const name of ["Latitude", "Longitude", "Altitude", "Heading", "Speed"])
      expect(field(name)).toHaveAttribute("inputmode", "decimal");
  });

  test("a different template changes the preset, clears the name and keeps the position", async () => {
    const m = await mountForm("tank");
    validPosition();
    fill("Name", "Bravo");
    m.rerender(<SpawnForm templateId="scout" />);
    await settle();
    expect(field("Template")).toHaveValue("scout");
    expect(field("Name")).toHaveValue("");
    expect(field("Latitude")).toHaveValue("52.5");
  });
});

describe("SpawnForm run state", () => {
  test.each(["ready", "running", "paused"])("the button is enabled when %s", async (state) => {
    await mountForm("tank", {}, state);
    expect(spawnButton()).toBeEnabled();
  });

  test.each(["idle", "stopped"])("the button is disabled when %s", async (state) => {
    await mountForm("tank", {}, state);
    expect(spawnButton()).toBeDisabled();
  });

  test("the button is disabled while the run state is unknown", async () => {
    engine = engineWith();
    mount(<SpawnForm templateId="tank" />);
    await settle();
    expect(spawnButton()).toBeDisabled();
  });

  test("follows the run state of the stream", async () => {
    const m = await mountForm("tank", {}, "ready");
    expect(spawnButton()).toBeEnabled();
    act(() => m.push(runFrame(2, "stopped")));
    expect(spawnButton()).toBeDisabled();
    act(() => m.push(runFrame(3, "paused")));
    expect(spawnButton()).toBeEnabled();
  });

  test("a disabled button sends nothing", async () => {
    await mountForm("tank", {}, "idle");
    validPosition();
    await submit();
    expect(posts()).toHaveLength(0);
  });

  test("the fields stay editable while disabled", async () => {
    await mountForm("tank", {}, "idle");
    fill("Name", "Bravo");
    expect(field("Name")).toHaveValue("Bravo");
  });
});

describe("SpawnForm validation", () => {
  test("empty latitude and longitude are required and nothing is sent", async () => {
    await mountForm();
    await submit();
    expect(posts()).toHaveLength(0);
    for (const name of ["Latitude", "Longitude"]) {
      expect(field(name)).toHaveAttribute("aria-invalid", "true");
      expect(field(name)).toHaveAccessibleDescription(`${name} is required`);
    }
  });

  test("a blank (spaces only) latitude is required", async () => {
    await mountForm();
    fill("Latitude", "   ");
    fill("Longitude", "13");
    await submit();
    expect(field("Latitude")).toHaveAccessibleDescription(/required/);
    expect(posts()).toHaveLength(0);
  });

  test.each([
    ["Latitude", "90.001"],
    ["Latitude", "-90.001"],
    ["Latitude", "91"],
    ["Longitude", "180.5"],
    ["Longitude", "-181"],
  ])("%s %s is out of range", async (name, value) => {
    await mountForm();
    validPosition();
    fill(name, value);
    await submit();
    expect(posts()).toHaveLength(0);
    expect(field(name)).toHaveAttribute("aria-invalid", "true");
    const [min, max] = name === "Latitude" ? ["-90", "90"] : ["-180", "180"];
    expect(field(name)).toHaveAccessibleDescription(`${name} must be between ${min} and ${max}`);
  });

  test.each([
    ["Latitude", "90"],
    ["Latitude", "-90"],
    ["Longitude", "180"],
    ["Longitude", "-180"],
    ["Latitude", "0"],
  ])("%s %s is accepted (the range is inclusive)", async (name, value) => {
    await mountForm();
    validPosition();
    fill(name, value);
    await submit();
    expect(posts()).toHaveLength(1);
  });

  test.each(["abc", "1,5", "12abc", "NaN", "Infinity"])("%p is not a number", async (value) => {
    await mountForm();
    validPosition();
    fill("Latitude", value);
    await submit();
    expect(posts()).toHaveLength(0);
    expect(field("Latitude")).toHaveAccessibleDescription("Latitude must be a number");
  });

  test.each(["Altitude", "Heading", "Speed"])("%s must be a number when given", async (name) => {
    await mountForm();
    validPosition();
    fill(name, "fast");
    await submit();
    expect(posts()).toHaveLength(0);
    expect(field(name)).toHaveAttribute("aria-invalid", "true");
    expect(field(name)).toHaveAccessibleDescription(`${name} must be a number`);
  });

  test("shows every problem at once and focuses the first invalid field", async () => {
    await mountForm();
    fill("Longitude", "500");
    fill("Speed", "x");
    await submit();
    expect(field("Latitude")).toHaveAttribute("aria-invalid", "true");
    expect(field("Longitude")).toHaveAttribute("aria-invalid", "true");
    expect(field("Speed")).toHaveAttribute("aria-invalid", "true");
    expect(field("Latitude")).toHaveFocus();
  });

  test("valid fields are not marked invalid", async () => {
    await mountForm();
    fill("Longitude", "13");
    await submit();
    expect(field("Longitude")).not.toHaveAttribute("aria-invalid", "true");
  });

  test("a corrected form sends and clears the marks", async () => {
    await mountForm();
    await submit();
    validPosition();
    await submit();
    expect(posts()).toHaveLength(1);
    expect(field("Latitude")).not.toHaveAttribute("aria-invalid", "true");
    expect(field("Latitude")).not.toHaveAccessibleDescription(/required/);
  });

  test("nothing is validated before the first submit", async () => {
    await mountForm();
    fill("Latitude", "abc");
    expect(field("Latitude")).not.toHaveAttribute("aria-invalid", "true");
  });
});

describe("SpawnForm request", () => {
  test("sends the template and the position as numbers when nothing else is given", async () => {
    await mountForm("tank");
    validPosition();
    await submit();
    expect(posts()).toHaveLength(1);
    expect(posts()[0]?.body).toEqual({ template: "tank", lat_deg: 52.5, lon_deg: 13.4 });
  });

  test("sends altitude, heading, speed, name, force and group when given", async () => {
    await mountForm("scout");
    validPosition();
    fill("Altitude", "120.5");
    fill("Heading", "270");
    fill("Speed", "8");
    fill("Name", "Bravo");
    fill("Force", "blue");
    fill("Group", "alpha");
    await submit();
    expect(posts()[0]?.body).toEqual({
      template: "scout",
      lat_deg: 52.5,
      lon_deg: 13.4,
      alt_m: 120.5,
      heading_deg: 270,
      speed_mps: 8,
      name: "Bravo",
      force: "blue",
      group: "alpha",
    });
  });

  test("a zero is sent, a blank is not", async () => {
    await mountForm();
    validPosition();
    fill("Altitude", "0");
    fill("Speed", "");
    await submit();
    expect(posts()[0]?.body).toEqual({ template: "tank", lat_deg: 52.5, lon_deg: 13.4, alt_m: 0 });
  });

  test("trims the text fields and drops a blank name, force and group", async () => {
    await mountForm();
    validPosition();
    fill("Name", "  Bravo  ");
    fill("Force", "   ");
    await submit();
    expect(posts()[0]?.body).toEqual({
      template: "tank",
      lat_deg: 52.5,
      lon_deg: 13.4,
      name: "Bravo",
    });
  });

  test("accepts negative and exponent numbers", async () => {
    await mountForm();
    fill("Latitude", "-33.5");
    fill("Longitude", "1e1");
    await submit();
    expect(posts()[0]?.body).toMatchObject({ lat_deg: -33.5, lon_deg: 10 });
  });

  test("a second click while the request is in flight sends one request", async () => {
    let answer: (r: Response) => void = () => {};
    const pending = new Promise<Response>((resolve) => {
      answer = resolve;
    });
    await mountForm("tank", { spawn: () => pending });
    validPosition();
    await act(async () => {
      fireEvent.click(spawnButton());
      fireEvent.click(spawnButton());
    });
    expect(posts()).toHaveLength(1);
    expect(spawnButton()).toBeDisabled();
    await act(async () => answer(jsonReply(spawned(7), 201)));
    await settle();
    expect(spawnButton()).toBeEnabled();
  });
});

describe("SpawnForm result", () => {
  test("success shows the new entity id in a polite status", async () => {
    await mountForm("tank", { spawn: () => jsonReply(spawned(42), 201) });
    validPosition();
    await submit();
    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Spawned entity #42");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("keeps the typed values after success", async () => {
    await mountForm();
    validPosition();
    fill("Name", "Bravo");
    await submit();
    expect(field("Latitude")).toHaveValue("52.5");
    expect(field("Name")).toHaveValue("Bravo");
  });

  test("no result is shown before anything was sent", async () => {
    await mountForm();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("duplicate_name shows the problem inline and marks the name field", async () => {
    await mountForm("tank", {
      spawn: () =>
        problemReply(409, "duplicate_name", "Duplicate name", "An entity named Bravo exists"),
    });
    validPosition();
    fill("Name", "Bravo");
    await submit();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Duplicate name");
    expect(alert).toHaveTextContent("An entity named Bravo exists");
    expect(field("Name")).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });

  test("invalid_spawn shows the problem inline and leaves the fields unmarked", async () => {
    await mountForm("tank", {
      spawn: () =>
        problemReply(422, "invalid_spawn", "Invalid spawn", "Speed exceeds the template limit"),
    });
    validPosition();
    await submit();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Invalid spawn");
    expect(alert).toHaveTextContent("Speed exceeds the template limit");
    expect(field("Name")).not.toHaveAttribute("aria-invalid", "true");
    expect(field("Latitude")).toHaveValue("52.5");
  });

  test("any other problem is shown the same way", async () => {
    await mountForm("tank", {
      spawn: () => problemReply(409, "no_scenario", "No scenario", "none"),
    });
    validPosition();
    await submit();
    expect(screen.getByRole("alert")).toHaveTextContent("No scenario");
  });

  test("a new submit clears the previous problem, and a later problem replaces a success", async () => {
    let n = 0;
    await mountForm("tank", {
      spawn: () =>
        ++n === 1
          ? problemReply(409, "duplicate_name", "Duplicate name", "taken")
          : n === 2
            ? jsonReply(spawned(5), 201)
            : problemReply(422, "invalid_spawn", "Invalid spawn", "bad"),
    });
    validPosition();
    fill("Name", "Bravo");
    await submit();
    expect(screen.getByRole("alert")).toHaveTextContent("Duplicate name");
    await submit();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(field("Name")).not.toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("status")).toHaveTextContent("#5");
    await submit();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
    expect(screen.getByRole("alert")).toHaveTextContent("Invalid spawn");
  });

  test("a different template clears the result", async () => {
    const m = await mountForm("tank");
    validPosition();
    await submit();
    expect(screen.getByRole("status")).toBeInTheDocument();
    m.rerender(<SpawnForm templateId="scout" />);
    await settle();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });
});

describe("SpawnForm accessibility and design", () => {
  test("every control has an accessible name", async () => {
    await mountForm();
    for (const input of screen.getAllByRole("textbox")) expect(input).toHaveAccessibleName();
    expect(spawnButton()).toHaveAccessibleName();
  });

  test("the submit button is the form's submit", async () => {
    await mountForm();
    expect(spawnButton()).toHaveAttribute("type", "submit");
  });
});
