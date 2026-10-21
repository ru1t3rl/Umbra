import { expect, test } from "bun:test";
import { runStateLabel } from "./runStateLabel.ts";

test("every state has its word, tone and live flag", () => {
  expect(runStateLabel("idle")).toEqual({ label: "Idle", tone: "neutral", live: false });
  expect(runStateLabel("ready")).toEqual({ label: "Ready", tone: "info", live: false });
  expect(runStateLabel("running")).toEqual({ label: "Running", tone: "success", live: true });
  expect(runStateLabel("paused")).toEqual({ label: "Paused", tone: "warning", live: false });
  expect(runStateLabel("stopped")).toEqual({ label: "Stopped", tone: "neutral", live: false });
  expect(runStateLabel("unknown")).toEqual({ label: "Unknown", tone: "neutral", live: false });
});

test("stopped is danger only for plugin_fault and error", () => {
  expect(runStateLabel("stopped", "plugin_fault").tone).toBe("danger");
  expect(runStateLabel("stopped", "error").tone).toBe("danger");
  expect(runStateLabel("stopped", "unknown").tone).toBe("danger");
  expect(runStateLabel("stopped", "network_fault" as never).tone).toBe("danger");
  expect(runStateLabel("stopped", "command").tone).toBe("neutral");
  expect(runStateLabel("stopped", "duration").tone).toBe("neutral");
  expect(runStateLabel("stopped", null).tone).toBe("neutral");
  expect(runStateLabel("stopped", undefined).tone).toBe("neutral");
});

test("a stop reason does not colour other states", () => {
  expect(runStateLabel("paused", "error").tone).toBe("warning");
  expect(runStateLabel("running", "plugin_fault").tone).toBe("success");
});

test("a fault makes a stopped run danger even after a benign reason", () => {
  const fault = { call: "tick", capability: "c", plugin: "p", message: "boom" };
  expect(runStateLabel("stopped", "command", fault).tone).toBe("danger");
  expect(runStateLabel("stopped", undefined, null).tone).toBe("neutral");
});
