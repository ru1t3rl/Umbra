// WI-14 round 3 nit 1 (ASM-990): who shows the shared problem. `RunControls` does; a StepControls under a provider with
// no RunControls needs `CommandProblemAlert`. Two RunControls under one provider show the same Alert (documented).
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { type FakeEngine, fakeEngine, mount, problemReply } from "../../controlsTesting.tsx";
import { CommandProblemProvider } from "../CommandProblemProvider/CommandProblemProvider.tsx";
import { RunControls } from "../RunControls/RunControls.tsx";
import { StepControls } from "../StepControls/StepControls.tsx";
import { CommandProblemAlert } from "./CommandProblemAlert.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});
const stepped = { state: "running", clock: { mode: "stepped" } };
const failing = () =>
  fakeEngine(() => problemReply(409, "step_cancelled", "Step cancelled", "cancelled by pause"));

describe("CommandProblemAlert", () => {
  // Catches: a provider with StepControls only (no RunControls) swallowing the step failure.
  test("shows a step failure under a provider that has no RunControls", async () => {
    engine = failing();
    mount(
      <CommandProblemProvider>
        <StepControls />
        <CommandProblemAlert />
      </CommandProblemProvider>,
      stepped,
    );
    fireEvent.click(screen.getByRole("button", { name: "Step 1 tick" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("cancelled by pause");
  });

  test("renders nothing without a problem, and nothing outside a provider", () => {
    engine = fakeEngine();
    mount(
      <>
        <CommandProblemAlert />
        <CommandProblemProvider>
          <CommandProblemAlert />
        </CommandProblemProvider>
      </>,
    );
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("two RunControls under one provider", () => {
  // Documents the behaviour: the slot is shared, so both show the same Alert.
  test("both show the same Alert", async () => {
    engine = failing();
    mount(
      <CommandProblemProvider>
        <RunControls />
        <RunControls />
      </CommandProblemProvider>,
      stepped,
    );
    fireEvent.click(screen.getAllByRole("button", { name: "Step 1 tick" })[0] as HTMLElement);
    await screen.findAllByRole("alert");
    expect(screen.getAllByRole("alert")).toHaveLength(2);
  });
});
