// WI-15 review m4 (ASM-993): the selection belongs to the scenario. It is cleared when the session changes or the
// scenario unloads, and an id that the (refreshed) list no longer holds is dropped.
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { useListTemplates } from "@umbra/data";
import type { ReactElement } from "react";
import {
  engineWith,
  type FakeEngine,
  hello,
  installResizeObserver,
  mount,
  runFrame,
  settle,
  TEMPLATES,
} from "../browserTesting.tsx";
import { useTemplateSelection } from "./useTemplateSelection.ts";

let engine: FakeEngine | undefined;
let restoreObserver: () => void;
beforeEach(() => {
  restoreObserver = installResizeObserver(400);
});
afterEach(() => {
  cleanup();
  restoreObserver();
  engine?.restore();
  engine = undefined;
});

function Probe(): ReactElement {
  const { selectedId, select } = useTemplateSelection();
  const list = useListTemplates({ query: { enabled: true } });
  return (
    <div>
      <output aria-label="selected">{selectedId ?? "none"}</output>
      {["tank", "scout", "drone"].map((id) => (
        <button key={id} type="button" onClick={() => select(id)}>
          {`select ${id}`}
        </button>
      ))}
      <button type="button" onClick={() => void list.refetch()}>
        refetch
      </button>
    </div>
  );
}

const selected = () => screen.getByLabelText("selected").textContent;
const pick = async (id: string) => {
  fireEvent.click(screen.getByRole("button", { name: `select ${id}` }));
  await settle();
};

async function open(state = "ready") {
  engine = engineWith();
  const m = mount(<Probe />, { state });
  await settle();
  return m;
}

describe("useTemplateSelection", () => {
  test("starts with nothing and holds what is selected", async () => {
    await open();
    expect(selected()).toBe("none");
    await pick("scout");
    expect(selected()).toBe("scout");
    await pick("tank");
    expect(selected()).toBe("tank");
  });

  // Catches: the selection surviving an unload and coming back for the next scenario.
  test("an unload clears it, and it stays cleared when a scenario is loaded again", async () => {
    const m = await open();
    await pick("tank");
    act(() => m.push(runFrame(2, "idle")));
    await settle();
    expect(selected()).toBe("none");
    act(() => m.push(runFrame(3, "ready")));
    await settle();
    expect(selected()).toBe("none");
  });

  // Catches: a new session (another scenario, no idle frame seen) keeping the old selection.
  test("a new session clears it", async () => {
    const m = await open();
    await pick("tank");
    act(() => m.push(runFrame(2, "ready", 2)));
    await settle();
    expect(selected()).toBe("none");
  });

  // Catches: a selected id that the scenario no longer has (the detail would show a 404).
  test("an id that the refreshed list no longer holds is dropped, one that it holds is kept", async () => {
    await open();
    await pick("tank");
    engine?.restore();
    engine = engineWith({ templates: TEMPLATES.filter((tpl) => tpl.id !== "tank") });
    fireEvent.click(screen.getByRole("button", { name: "refetch" }));
    await settle();
    expect(selected()).toBe("none");
    await pick("scout");
    expect(selected()).toBe("scout");
  });

  test("while the list is not loaded yet a selection is kept", async () => {
    engine = engineWith();
    mount(<Probe />, { state: "ready" });
    fireEvent.click(screen.getByRole("button", { name: "select tank" }));
    expect(selected()).toBe("tank");
    await settle();
    expect(selected()).toBe("tank");
  });

  // Catches: a pick made before the first frame (session unknown) surviving the first session (ASM-993).
  test("a pick made before the first run frame is cleared when the first session arrives", async () => {
    engine = engineWith();
    const m = mount(<Probe />); // no frame yet: the run status is undefined
    await settle();
    await pick("tank");
    expect(selected()).toBe("tank");
    act(() => m.push(runFrame(1, "ready", 1)));
    await settle();
    expect(selected()).toBe("none");
  });

  // Catches: the strict rule dropping a pick on every frame of its own session.
  test("a pick made in a session survives further frames of that session", async () => {
    const m = await open();
    await pick("tank");
    act(() => m.push(runFrame(2, "running", 1)));
    await settle();
    expect(selected()).toBe("tank");
  });

  // Catches: a dropped connection clearing the choice (a reconnect brings the same session and run back).
  test("a reconnect within the same session keeps the selection", async () => {
    const m = await open();
    await pick("tank");
    act(() => m.drop());
    await settle();
    expect(selected()).toBe("tank"); // still there while the connection is down
    act(() => m.reconnect());
    act(() => m.push(hello(), runFrame(5, "ready", 1)));
    await settle();
    expect(selected()).toBe("tank");
  });
});
