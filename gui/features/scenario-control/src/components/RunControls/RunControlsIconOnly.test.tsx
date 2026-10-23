// WI-14 review round 2 (ASM-988): `iconOnly` is the second compact step of the bar layout: every lifecycle button
// becomes an icon with an accessible name and a tooltip. A NEW file; the WI-7 tests are untouched.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { type FakeEngine, fakeEngine, mount } from "../../controlsTesting.tsx";
import { RunControls } from "./RunControls.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const NAMES = ["Load", "Start", "Pause", "Resume", "Stop", "Unload"] as const;
const group = () => screen.getByRole("group", { name: "Run controls" });
const ligature = (el: Element) => el.textContent?.trim() ?? "";

describe("iconOnly in the bar layout", () => {
  // Catches: icon buttons without a name (unusable with a screen reader) or without a tooltip.
  test("every button is named, has a title and shows only an icon", () => {
    engine = fakeEngine();
    mount(<RunControls layout="bar" iconOnly />, { state: "ready" });
    const buttons = within(group()).getAllByRole("button");
    expect(buttons.map((b) => b.getAttribute("aria-label"))).toEqual([...NAMES]);
    for (const b of buttons) {
      expect(b).toHaveAttribute("title", b.getAttribute("aria-label") as string);
      expect(b).toHaveClass("vt-btn-icon");
      expect(ligature(b)).toMatch(/^[a-z_]+$/); // the icon ligature, no label text
    }
  });

  // Catches: two commands sharing a glyph (Start and Resume would look alike).
  test("the six icons are different", () => {
    engine = fakeEngine();
    mount(<RunControls layout="bar" iconOnly />, { state: "ready" });
    const icons = within(group())
      .getAllByRole("button")
      .map((b) => ligature(b));
    expect(new Set(icons).size).toBe(6);
  });

  // Catches: the compact form changing what is enabled, the azure command, or the disabled reasons.
  test("enabled state, the single filled command and the disabled reasons are unchanged", () => {
    engine = fakeEngine();
    mount(<RunControls layout="bar" iconOnly />, { state: "ready" });
    const enabled = within(group())
      .getAllByRole("button")
      .filter((b) => !(b as HTMLButtonElement).disabled)
      .map((b) => b.getAttribute("aria-label"));
    expect(enabled.sort()).toEqual(["Load", "Start", "Stop", "Unload"]);
    expect(
      [...document.querySelectorAll("button.vt-btn-filled")].map((b) =>
        b.getAttribute("aria-label"),
      ),
    ).toEqual(["Start"]);
    const pause = screen.getByRole("button", { name: "Pause" });
    const why = document.getElementById(pause.getAttribute("aria-describedby") ?? "");
    expect(why?.textContent).toMatch(/\w/);
  });

  // Catches: the icon buttons not sending.
  test("a click still sends the command", async () => {
    engine = fakeEngine();
    mount(<RunControls layout="bar" iconOnly />, { state: "ready" });
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await Promise.resolve();
    expect(engine.calls.map((c) => `${c.method} ${c.path}`)).toEqual(["POST /api/v1/run/start"]);
  });

  // Catches: the option leaking into the panel layout (a column has room for labels).
  test("the panel layout ignores it", () => {
    engine = fakeEngine();
    mount(<RunControls iconOnly />, { state: "ready" });
    expect(screen.getByRole("button", { name: "Start" })).toHaveTextContent("Start");
    expect(screen.getByRole("button", { name: "Start" })).not.toHaveAttribute("title");
  });
});
