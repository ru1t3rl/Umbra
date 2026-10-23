// WI-12 review: switching to a template that is not cached must not unmount the spawn form (ASM-951).
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { engineWith, type FakeEngine, mount, settle, TANK } from "../../browserTesting.tsx";
import { TemplateDetail } from "./TemplateDetail.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const field = (name: string) => screen.getByRole("textbox", { name });
const fill = (name: string, value: string) =>
  act(() => void fireEvent.change(field(name), { target: { value } }));

describe("TemplateDetail switching to an uncached template", () => {
  async function switching() {
    let release: () => void = () => {};
    const hold = new Promise<void>((resolve) => {
      release = resolve;
    });
    engine = engineWith({ hold: { tank: hold } });
    const m = mount(<TemplateDetail templateId="scout" />, { state: "ready" });
    await settle();
    fill("Latitude", "52.5");
    fill("Longitude", "13.4");
    fill("Name", "Bravo");
    m.rerender(<TemplateDetail templateId="tank" />);
    await settle();
    return { m, release };
  }

  test("keeps the typed position and clears the name while the template loads", async () => {
    await switching();
    expect(field("Template")).toHaveValue("tank");
    expect(field("Latitude")).toHaveValue("52.5");
    expect(field("Longitude")).toHaveValue("13.4");
    expect(field("Name")).toHaveValue("");
  });

  test("shows the new id and a loading caption, and no sections of the old template", async () => {
    await switching();
    expect(screen.getByRole("heading", { level: 2, name: "tank" })).toBeInTheDocument();
    expect(screen.getByText("Loading template")).toBeInTheDocument();
    expect(screen.queryByRole("region")).toBeNull();
  });

  test("the loaded template fills in and the typed position is still there", async () => {
    const { release } = await switching();
    await act(async () => release());
    await settle();
    expect(screen.queryByText("Loading template")).toBeNull();
    expect(screen.getByRole("region", { name: "Behavior: builtin.patrol" })).toBeInTheDocument();
    expect(field("Latitude")).toHaveValue("52.5");
    expect(TANK.id).toBe("tank");
  });
});
