// WI-7 public API pin. Catches: a WI-7 export missing or renamed, or internals leaking from index.ts.
import { expect, test } from "bun:test";
import * as api from "./index.ts";

test("the package exports exactly the M5b slice plus the WI-7 controls", () => {
  expect(Object.keys(api).sort()).toEqual([
    "ClockControl",
    "ConnectionAlert",
    "RunControls",
    "RunFaultAlert",
    "RunStatusBar",
  ]);
});
