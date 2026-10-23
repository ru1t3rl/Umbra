// WI-14 review round 2 nit 2 (ASM-988): the README says a failed command's Alert stays "until the next command", so a
// new lifecycle or step command clears the error of the other kind too. A NEW file.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import {
  type FakeEngine,
  fakeEngine,
  mount,
  problemReply,
  runReply,
} from "../../controlsTesting.tsx";
import { CommandProblemProvider } from "../CommandProblemProvider/CommandProblemProvider.tsx";
import { StepControls } from "../StepControls/StepControls.tsx";
import { RunControls } from "./RunControls.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const stepped = { state: "running", clock: { mode: "stepped" } };
/** Fails the first call with `first`, then answers every call with a run status. */
const failOnce = (title: string, detail: string) => {
  let n = 0;
  return fakeEngine(() =>
    n++ === 0 ? problemReply(409, "invalid_state", title, detail) : runReply(),
  );
};
const click = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

describe("RunControls panel layout (the step group inside)", () => {
  // Catches: a stale step error left under a new lifecycle command.
  test("a lifecycle command clears a step error", async () => {
    engine = failOnce("Step failed", "the step failed");
    mount(<RunControls />, stepped);
    click("Step 1 tick");
    expect(await screen.findByRole("alert")).toHaveTextContent("the step failed");
    click("Pause");
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  // Catches: a stale lifecycle error left under a new step command.
  test("a step command clears a lifecycle error", async () => {
    engine = failOnce("Pause failed", "the pause failed");
    mount(<RunControls />, stepped);
    click("Pause");
    expect(await screen.findByRole("alert")).toHaveTextContent("the pause failed");
    click("Step 1 tick");
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  // Catches: two Alerts for one failure.
  test("one failure shows one Alert", async () => {
    engine = failOnce("Step failed", "the step failed");
    mount(<RunControls />, stepped);
    click("Step 10 ticks");
    await screen.findByRole("alert");
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });
});

describe("a shell with a bar and a separate step group", () => {
  const shell = () => (
    <CommandProblemProvider>
      <RunControls layout="bar" />
      <StepControls />
    </CommandProblemProvider>
  );

  // Catches: the two components not sharing one error (the shell case of the dashboard).
  test("a step error is cleared by a lifecycle command and shown once", async () => {
    engine = failOnce("Step failed", "the step failed");
    mount(shell(), stepped);
    click("Step 1 tick");
    expect(await screen.findByRole("alert")).toHaveTextContent("the step failed");
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    click("Pause");
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  test("a lifecycle error is cleared by a step command", async () => {
    engine = failOnce("Pause failed", "the pause failed");
    mount(shell(), stepped);
    click("Pause");
    expect(await screen.findByRole("alert")).toHaveTextContent("the pause failed");
    click("Step 100 ticks");
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });
});

describe("StepControls alone", () => {
  // Catches: StepControls losing its own Alert when no provider wraps it.
  test("shows its own failure when used without a provider", async () => {
    engine = failOnce("Step failed", "the step failed");
    mount(<StepControls />, stepped);
    click("Step 1 tick");
    expect(await screen.findByRole("alert")).toHaveTextContent("the step failed");
    engine.restore();
    engine = fakeEngine(() => runReply());
    click("Step 10 ticks");
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });
});
