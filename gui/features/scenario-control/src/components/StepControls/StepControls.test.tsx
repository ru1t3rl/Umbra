// WI-14 review blocker 2: the step group, extracted from RunControls so a shell can put it in the clock panel.

import { afterEach, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { type FakeEngine, fakeEngine, mount, problemReply } from "../../controlsTesting.tsx";
import { StepControls } from "./StepControls.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const stepped = { state: "running", clock: { mode: "stepped" } };

describe("StepControls", () => {
  // Catches: step controls for the wrong clock or state (the WI-7 rule, ASM-810).
  test.each([
    ["no run status yet", undefined],
    ["realtime running", { state: "running", clock: { mode: "realtime" } }],
    ["stepped paused", { state: "paused", clock: { mode: "stepped" } }],
  ] as const)("renders nothing for %s", (_n, run) => {
    engine = fakeEngine();
    mount(<StepControls />, run as Record<string, unknown> | undefined);
    expect(screen.queryByRole("group", { name: "Step" })).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  // Catches: the extraction losing the presets, the count field or the Step button.
  test("for a running stepped clock: four presets, Tick count and Step", () => {
    engine = fakeEngine();
    mount(<StepControls />, stepped);
    const group = screen.getByRole("group", { name: "Step" });
    expect(
      within(group)
        .getAllByRole("button")
        .map((b) => b.getAttribute("aria-label") ?? b.textContent),
    ).toEqual(["Step 1 tick", "Step 10 ticks", "Step 100 ticks", "Step 1000 ticks", "Step"]);
    expect(screen.getByRole("textbox", { name: "Tick count" })).toBeInTheDocument();
  });

  // Catches: a preset sending the wrong body.
  test("a preset posts its count", async () => {
    engine = fakeEngine();
    mount(<StepControls />, stepped);
    fireEvent.click(screen.getByRole("button", { name: "Step 10 ticks" }));
    await waitFor(() => expect(engine?.calls).toHaveLength(1));
    expect(engine?.calls[0]).toMatchObject({ path: "/api/v1/run/step", body: { count: 10 } });
  });

  // Catches: a failed step swallowed (it shows its own alert beside the group).
  test("a failed step shows the engine's detail", async () => {
    engine = fakeEngine(() =>
      problemReply(409, "step_cancelled", "Step cancelled", "cancelled by pause"),
    );
    mount(<StepControls />, stepped);
    fireEvent.click(screen.getByRole("button", { name: "Step 1 tick" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("cancelled by pause");
  });
});

describe("StepControls stylesheet (ASM-988)", () => {
  const css = readFileSync(join(import.meta.dir, "StepControls.css"), "utf8");
  // Catches: the failure Alert clipped in a narrow panel (a flex item does not shrink below its content by default).
  test("the group and its Alert may shrink and wrap", () => {
    expect(css).toMatch(/\.umbra-step-controls\s*\{[^}]*min-width:\s*0/);
    expect(css).toMatch(/\.umbra-step-controls\s*\{[^}]*max-width:\s*100%/);
    expect(css).toMatch(/\.umbra-step-controls\s+\.vt-alert[^{]*\{[^}]*(overflow-wrap|word-break)/);
  });
});
