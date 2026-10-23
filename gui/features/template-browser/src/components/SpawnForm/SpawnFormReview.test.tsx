// WI-12 review tests for the spawn form: focus order, late answers, the status region, duplicate_name, idle submit.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import {
  engineWith,
  type FakeEngine,
  jsonReply,
  mount,
  problemReply,
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
const submit = async () => {
  await act(async () => void fireEvent.click(screen.getByRole("button", { name: /^Spawn/ })));
  await settle();
};
const posts = () => (engine as FakeEngine).to("POST", "/api/v1/entities");
const position = () => {
  fill("Latitude", "52.5");
  fill("Longitude", "13.4");
};

describe("SpawnForm focus", () => {
  test("the first invalid field is already marked and described when focus arrives", async () => {
    await mountForm();
    const seen: { invalid: string | null; description: string }[] = [];
    field("Latitude").addEventListener("focus", () => {
      seen.push({
        invalid: field("Latitude").getAttribute("aria-invalid"),
        description: field("Latitude").getAttribute("aria-describedby") ?? "",
      });
    });
    await submit();
    expect(seen).toHaveLength(1);
    expect(seen[0]?.invalid).toBe("true");
    expect(seen[0]?.description).not.toBe("");
    expect(document.getElementById(seen[0]?.description ?? "")?.textContent).toBe(
      "Latitude is required",
    );
  });

  test("a second failed submit focuses the first invalid field again", async () => {
    await mountForm();
    await submit();
    field("Longitude").focus();
    await submit();
    expect(field("Latitude")).toHaveFocus();
  });
});

describe("SpawnForm late answers", () => {
  test("a success for the previous template is dropped", async () => {
    let answer: (r: Response) => void = () => {};
    const pending = new Promise<Response>((resolve) => {
      answer = resolve;
    });
    const m = await mountForm("tank", { spawn: () => pending });
    position();
    await submit();
    m.rerender(<SpawnForm templateId="scout" />);
    await settle();
    await act(async () => answer(jsonReply(spawned(9), 201)));
    await settle();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("an error for the previous template is dropped", async () => {
    let answer: (r: Response) => void = () => {};
    const pending = new Promise<Response>((resolve) => {
      answer = resolve;
    });
    const m = await mountForm("tank", { spawn: () => pending });
    position();
    await submit();
    m.rerender(<SpawnForm templateId="scout" />);
    await settle();
    await act(async () => answer(problemReply(422, "invalid_spawn", "Invalid spawn", "bad")));
    await settle();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("the answer for the current template is still shown", async () => {
    let answer: (r: Response) => void = () => {};
    const pending = new Promise<Response>((resolve) => {
      answer = resolve;
    });
    await mountForm("tank", { spawn: () => pending });
    position();
    await submit();
    await act(async () => answer(jsonReply(spawned(9), 201)));
    await settle();
    expect(screen.getByRole("status")).toHaveTextContent("Spawned entity #9");
  });
});

describe("SpawnForm status region", () => {
  test("is mounted empty from the start and only its text changes", async () => {
    await mountForm();
    const region = screen.getByRole("status");
    expect(region).toBeEmptyDOMElement();
    expect(region).toHaveAttribute("aria-live", "polite");
    position();
    await submit();
    expect(screen.getByRole("status")).toBe(region);
    expect(region).toHaveTextContent("Spawned entity #42");
  });
});

describe("SpawnForm duplicate_name", () => {
  test("explains the mark with a description and focuses the name", async () => {
    await mountForm("tank", {
      spawn: () => problemReply(409, "duplicate_name", "Duplicate name", "exists"),
    });
    position();
    fill("Name", "Bravo");
    await submit();
    expect(field("Name")).toHaveAttribute("aria-invalid", "true");
    expect(field("Name")).toHaveAccessibleDescription("Name is already in use");
    expect(field("Name")).toHaveFocus();
  });

  test("other problems do not move the focus", async () => {
    await mountForm("tank", {
      spawn: () => problemReply(422, "invalid_spawn", "Invalid spawn", "bad"),
    });
    position();
    field("Latitude").focus();
    await submit();
    expect(field("Name")).not.toHaveFocus();
  });
});

describe("SpawnForm direct submit", () => {
  test("submitting the form while idle sends nothing", async () => {
    await mountForm("tank", {}, "idle");
    position();
    await act(
      async () => void fireEvent.submit(screen.getByRole("form", { name: "Spawn entity" })),
    );
    await settle();
    expect(posts()).toHaveLength(0);
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });
});
