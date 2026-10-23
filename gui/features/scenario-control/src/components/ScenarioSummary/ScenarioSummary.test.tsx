// WI-8 failing tests. Assumed API:
//   <ScenarioSummary />  reads GET /api/v1/scenario (useGetScenario). Markup: a Readout per field with the labels
//   "Name", "Path", "Seed", "Tick rate", "Duration", "Entities at load"; tick rate "30 Hz" plus its source ("scenario" or
//   "default"); duration as formatSimTime of duration_ticks / tick_rate_hz (T+00:04:00) and "7,200 ticks".
//   While loading a "Loading scenario" status; a 409 no_scenario problem shows an empty state "No scenario is loaded.".
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import { type FakeEngine, fakeEngine, mount, problemReply } from "../../controlsTesting.tsx";
import { ScenarioSummary } from "./ScenarioSummary.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  engine?.restore();
  engine = undefined;
});

const scenario = (extra: Record<string, unknown> = {}): Response =>
  new Response(
    JSON.stringify({
      session: 1,
      name: "walking_skeleton",
      path: "scenarios/walking_skeleton/scenario.toml",
      seed: "18446744073709551615",
      tick_rate_hz: 30,
      tick_rate_source: "scenario",
      duration_ticks: 7200,
      entity_count: 3,
      ...extra,
    }),
    { status: 200, headers: { "content-type": "application/json" } },
  );

/** The definition of a Readout term. */
const field = (label: string): HTMLElement => {
  const term = screen.getAllByRole("term").find((el) => el.textContent === label);
  if (!term) throw new Error(`no field ${label}`);
  return within(term.closest("dl") as HTMLElement).getByRole("definition");
};

describe("fields from getScenario", () => {
  // Catches: a field not wired, or the request going to the wrong URL.
  test("shows name, path, seed, tick rate and source, duration and entity count", async () => {
    engine = fakeEngine(() => scenario());
    mount(<ScenarioSummary />);
    await screen.findByText("walking_skeleton");
    expect(engine.calls.some((c) => c.method === "GET" && c.path === "/api/v1/scenario")).toBe(
      true,
    );
    expect(screen.getByText("scenarios/walking_skeleton/scenario.toml")).toBeTruthy();
    expect(screen.getByText("18446744073709551615")).toBeTruthy();
    expect(screen.getByText(/30 Hz/)).toBeTruthy();
    expect(within(field("Tick rate")).getByText(/scenario/)).toBeTruthy();
    expect(screen.getByText(/T\+00:04:00/)).toBeTruthy();
    expect(screen.getByText(/7,200 ticks/)).toBeTruthy();
    expect(field("Entities at load")).toHaveTextContent(/^3$/);
  });

  // Catches: the source dropped when the rate is the server default (ADR 0026).
  test("a defaulted tick rate says so", async () => {
    engine = fakeEngine(() =>
      scenario({ tick_rate_hz: 60, tick_rate_source: "default", duration_ticks: 600 }),
    );
    mount(<ScenarioSummary />);
    await screen.findByText("walking_skeleton");
    expect(screen.getByText(/60 Hz/)).toBeTruthy();
    expect(within(field("Tick rate")).getByText(/default/)).toBeTruthy();
    expect(screen.getByText(/T\+00:00:10/)).toBeTruthy();
  });

  // Catches: NaN/Infinity from a zero tick rate.
  test("a zero tick rate does not produce a made-up duration", async () => {
    engine = fakeEngine(() => scenario({ tick_rate_hz: 0 }));
    mount(<ScenarioSummary />);
    await screen.findByText("walking_skeleton");
    expect(field("Duration")).toHaveTextContent(/^7,200 ticks$/);
  });
});

describe("states", () => {
  // Catches: a blank panel while the request is in flight.
  test("shows a loading status before the answer", () => {
    engine = fakeEngine(() => new Promise<Response>(() => {}) as unknown as Response);
    mount(<ScenarioSummary />);
    expect(screen.getByRole("status")).toHaveTextContent("Loading scenario");
  });

  // Catches: an error thrown or an empty panel when nothing is loaded (idle).
  test("no scenario loaded shows the empty state", async () => {
    engine = fakeEngine(() =>
      problemReply(409, "no_scenario", "No scenario", "no scenario is loaded"),
    );
    mount(<ScenarioSummary />);
    await waitFor(() => expect(screen.getByText("No scenario is loaded.")).toBeTruthy());
  });
});
