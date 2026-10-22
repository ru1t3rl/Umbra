// WI-7 failing tests. Assumed API:
//   <RunFaultAlert />  reads useRunStatus(); renders nothing unless state === "stopped" with a stop_reason that is not
//   command/duration. Danger Alert (role alert), never dismissible:
//     plugin_fault   title "Plugin fault", term/definition rows Plugin, Capability, Call, Message from run.fault
//     network_fault  title "Network fault", rows Adapter, Reason from run.network_fault
//     error          title "Run stopped with an error"
//     anything else  title "Run stopped: <word>" (the decoder maps an unknown reason to "unknown", ASM-359, so the word is
//                    "unknown" until the decoder keeps the raw text; see ASM-908)
//   A fault reason without its details still shows the title.
//   Needs engine-stream to decode run.network_fault (today only run.fault is decoded: model/decode.ts runOf).
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen, within } from "@testing-library/react";
import { mount } from "../../controlsTesting.tsx";
import { RunFaultAlert } from "./RunFaultAlert.tsx";

afterEach(cleanup);

const stopped = (extra: Record<string, unknown>) => ({ state: "stopped", ...extra });
const FAULT = {
  plugin: "example-wander",
  capability: "example.behavior.wander",
  call: "on_tick",
  message: "index out of bounds",
};

const rowOf = (alert: HTMLElement, term: string): string => {
  const dt = within(alert).getByText(term, { selector: "dt" });
  return dt.nextElementSibling?.textContent ?? "";
};

describe("plugin fault", () => {
  // Catches: a field of the fault dropped or mislabelled.
  test("shows plugin, capability, call and message in a danger alert", () => {
    mount(<RunFaultAlert />, stopped({ stop_reason: "plugin_fault", fault: FAULT }));
    const alert = screen.getByRole("alert", { name: "Plugin fault" });
    expect(alert).toHaveClass("vt-alert-danger");
    expect(rowOf(alert, "Plugin")).toBe("example-wander");
    expect(rowOf(alert, "Capability")).toBe("example.behavior.wander");
    expect(rowOf(alert, "Call")).toBe("on_tick");
    expect(rowOf(alert, "Message")).toBe("index out of bounds");
  });

  // Catches: a crash or silence when the details are missing.
  test("without details it still shows the title", () => {
    mount(<RunFaultAlert />, stopped({ stop_reason: "plugin_fault" }));
    expect(screen.getByRole("alert", { name: "Plugin fault" })).toBeInTheDocument();
  });

  // Catches: markup injection from a plugin-supplied message (rendered as text).
  test("a message with markup is text", () => {
    mount(
      <RunFaultAlert />,
      stopped({
        stop_reason: "plugin_fault",
        fault: { ...FAULT, message: "<img src=x onerror=1>" },
      }),
    );
    expect(document.querySelector("img")).toBeNull();
    expect(screen.getByText("<img src=x onerror=1>")).toBeInTheDocument();
  });
});

describe("network fault", () => {
  // Catches: network_fault decoded away (engine-stream) or shown as a plugin fault.
  test("shows adapter and reason in a danger alert", () => {
    mount(
      <RunFaultAlert />,
      stopped({
        stop_reason: "network_fault",
        network_fault: { adapter: "dis-main", reason: "socket closed" },
      }),
    );
    const alert = screen.getByRole("alert", { name: "Network fault" });
    expect(alert).toHaveClass("vt-alert-danger");
    expect(rowOf(alert, "Adapter")).toBe("dis-main");
    expect(rowOf(alert, "Reason")).toBe("socket closed");
    expect(within(alert).queryByText("Plugin", { selector: "dt" })).toBeNull();
  });

  test("without details it still shows the title", () => {
    mount(<RunFaultAlert />, stopped({ stop_reason: "network_fault" }));
    expect(screen.getByRole("alert", { name: "Network fault" })).toBeInTheDocument();
  });
});

describe("other stop reasons", () => {
  // Catches: error treated as a normal stop.
  test("error is a danger alert", () => {
    mount(<RunFaultAlert />, stopped({ stop_reason: "error" }));
    expect(screen.getByRole("alert", { name: "Run stopped with an error" })).toHaveClass(
      "vt-alert-danger",
    );
  });

  // Catches: an unknown reason hidden (plan: danger with the raw word).
  test("an unknown reason is danger and shows the word the client holds", () => {
    mount(<RunFaultAlert />, stopped({ stop_reason: "meteor" }));
    const alert = screen.getByRole("alert", { name: "Run stopped: meteor" });
    expect(alert).toHaveClass("vt-alert-danger");
  });

  // Catches: a normal end shown as an alarm.
  test.each(["command", "duration"])("%s shows nothing", (reason) => {
    mount(<RunFaultAlert />, stopped({ stop_reason: reason }));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  // Catches: a stale fault alert after the run was reloaded.
  test("clears when the run is no longer stopped", () => {
    const m = mount(<RunFaultAlert />, stopped({ stop_reason: "plugin_fault", fault: FAULT }));
    expect(screen.getByRole("alert")).toBeInTheDocument();
    m.setRun({ state: "ready", stop_reason: null, fault: null, session: 2 });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  // Catches: an alert before any status.
  test("renders nothing without a status or for a running run", () => {
    const m = mount(<RunFaultAlert />);
    expect(screen.queryByRole("alert")).toBeNull();
    m.setRun({ state: "running" });
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
