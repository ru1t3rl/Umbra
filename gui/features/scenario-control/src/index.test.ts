// WI-7 public API pin. Catches: a WI-7 export missing or renamed, or internals leaking from index.ts.
import { expect, test } from "bun:test";
import * as api from "./index.ts";

test("the package exports exactly the M5b slice plus the WI-7 controls", () => {
  expect(Object.keys(api).sort()).toEqual([
    "ClockControl",
    "CommandProblemAlert", // WI-14 round 3 (append-only, standing instruction 2026-10-23, ASM-990)
    "CommandProblemProvider", // WI-14 review round 2 (append-only, standing instruction 2026-10-23, ASM-988)
    "ConnectionAlert",
    "RunControls",
    "RunFaultAlert",
    "RunStatusBar",
    // WI-8 (append-only, human standing approval 2026-10-23)
    "ScenarioLoader",
    "ScenarioSummary",
    // WI-14 review (append-only, standing instruction 2026-10-23, ASM-986)
    "StepControls",
  ]);
});
