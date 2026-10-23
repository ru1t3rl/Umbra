// WI-17 (docs/plans/M8.md), written before the component. Contract:
// - `RecentScenariosSetting()` takes no props; it reads `settings.scenarios.recent` (WI-14: newest first, at most
//   10) and clears it;
// - a status line: "No recent scenarios", "1 recent scenario" or "<n> recent scenarios";
// - a tonal Button "Clear recent scenarios", disabled while the list is empty; a click empties the list and
//   persists it, every other setting stays as it was.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { mountSettings } from "../../testing.tsx";
import { RecentScenariosSetting } from "./RecentScenariosSetting.tsx";

afterEach(cleanup);

const clear = (): HTMLElement => screen.getByRole("button", { name: "Clear recent scenarios" });

describe("RecentScenariosSetting", () => {
  test("with nothing recorded it says so and the button is disabled", () => {
    mountSettings(<RecentScenariosSetting />);
    expect(screen.getByText("No recent scenarios")).toBeInTheDocument();
    expect(clear()).toBeDisabled();
  });

  test("counts the recent scenarios", () => {
    mountSettings(<RecentScenariosSetting />, { scenarios: { recent: ["a.yaml"] } });
    expect(screen.getByText("1 recent scenario")).toBeInTheDocument();
    expect(clear()).toBeEnabled();
  });

  test("uses the plural for several", () => {
    mountSettings(<RecentScenariosSetting />, { scenarios: { recent: ["a.yaml", "b.yaml", "c"] } });
    expect(screen.getByText("3 recent scenarios")).toBeInTheDocument();
  });

  test("Clear empties the list, persists it and disables the button", () => {
    const { writes, stored } = mountSettings(<RecentScenariosSetting />, {
      scenarios: { recent: ["a.yaml", "b.yaml"] },
      stream: { maxRateHz: 12 },
    });
    fireEvent.click(clear());
    expect(writes).toHaveLength(1);
    expect(stored()?.scenarios.recent).toEqual([]);
    expect(stored()?.stream.maxRateHz).toBe(12);
    expect(screen.getByText("No recent scenarios")).toBeInTheDocument();
    expect(clear()).toBeDisabled();
  });

  test("the button is tonal, not the view's one filled command", () => {
    mountSettings(<RecentScenariosSetting />, { scenarios: { recent: ["a.yaml"] } });
    expect(clear()).not.toHaveClass("vt-btn-filled");
  });
});
