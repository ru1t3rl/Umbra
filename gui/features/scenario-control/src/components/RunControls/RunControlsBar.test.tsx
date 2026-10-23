// WI-14 review blocker 2: `layout="bar"` renders only the lifecycle row (the step group moves to StepControls), and
// the command problem goes to a container the shell provides. A NEW file; the WI-7 tests are untouched.

import { afterEach, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { type FakeEngine, fakeEngine, mount, problemReply } from "../../controlsTesting.tsx";
import { RunControls } from "./RunControls.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const stepped = { state: "running", clock: { mode: "stepped" } };

describe("layout", () => {
  // Catches: a changed default (the panel layout keeps the step group).
  test("panel is the default and still shows the step group", () => {
    engine = fakeEngine();
    mount(<RunControls />, stepped);
    expect(screen.getByRole("group", { name: "Step" })).toBeInTheDocument();
  });

  // Catches: the bar growing a second row.
  test("bar renders only the lifecycle row, even for a running stepped clock", () => {
    engine = fakeEngine();
    mount(<RunControls layout="bar" />, stepped);
    const group = screen.getByRole("group", { name: "Run controls" });
    expect(within(group).getAllByRole("button")).toHaveLength(6);
    expect(screen.queryByRole("group", { name: "Step" })).toBeNull();
    expect(screen.queryByRole("textbox", { name: "Tick count" })).toBeNull();
    expect(screen.getAllByRole("button")).toHaveLength(6);
  });

  // Catches: the bar layout changing which command is the azure one.
  test("bar keeps the single filled primary command", () => {
    engine = fakeEngine();
    mount(<RunControls layout="bar" />, { state: "ready" });
    const filled = [...document.querySelectorAll("button.vt-btn-filled")];
    expect(filled.map((b) => b.textContent)).toEqual(["Start"]);
  });
});

describe("the command problem in bar layout", () => {
  const failing = () =>
    fakeEngine(() => problemReply(409, "invalid_state", "Invalid state", "no scenario is loaded"));

  // Catches: a failed command that vanishes when a shell gives no container (ASM-988 reverses the WI-14 rule
  // "nothing without it": the inline Alert is the fallback).
  test("falls back to the inline Alert when there is no container", async () => {
    engine = failing();
    mount(<RunControls layout="bar" />, { state: "ready" });
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("no scenario is loaded");
  });

  // Catches: the problem lost (a failed command must stay visible somewhere).
  test("is rendered into the container the shell provides", async () => {
    engine = failing();
    const holder = document.createElement("div");
    holder.setAttribute("data-testid", "holder");
    document.body.append(holder);
    mount(<RunControls layout="bar" problemContainer={holder} />, { state: "ready" });
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("no scenario is loaded");
    expect(holder).toContainElement(alert);
    holder.remove();
  });
});

describe("the bar row never stacks (ASM-988)", () => {
  const css = readFileSync(join(import.meta.dir, "RunControls.css"), "utf8");

  // Catches: the lifecycle row wrapping into a second line inside the 48 px bar.
  test("the bar layout marks the container and its stylesheet sets nowrap", () => {
    engine = fakeEngine();
    mount(<RunControls layout="bar" />, { state: "ready" });
    const root = screen.getByRole("group", { name: "Run controls" }).parentElement as HTMLElement;
    expect(root).toHaveClass("umbra-run-controls-bar");
    expect(css).toMatch(/\.umbra-run-controls-bar[^{]*\{[^}]*flex-wrap:\s*nowrap/);
  });

  test("the panel layout is not marked (it may wrap)", () => {
    engine = fakeEngine();
    mount(<RunControls />, { state: "ready" });
    const root = screen.getByRole("group", { name: "Run controls" }).parentElement as HTMLElement;
    expect(root).not.toHaveClass("umbra-run-controls-bar");
  });
});
