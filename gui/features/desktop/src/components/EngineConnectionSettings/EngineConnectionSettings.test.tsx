// WI-17 (docs/plans/M8.md), written before the component. In this WI `EngineConnectionSettings` is a stub; WI-22
// (ADRs 0075, 0076) replaces it on the desktop. Contract of the stub:
// - `EngineConnectionSettings()` takes no props;
// - one read-only ui-kit `TextField` (a `textbox` named "Engine URL") showing `useRuntimeConfig().engineUrl`; with
//   an empty URL (same origin as the page, the default) it shows the page origin;
// - its help text says where the value comes from (`config.json`); the field cannot be edited and nothing is
//   written anywhere (no settings dependency).
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { RuntimeConfigProvider } from "@umbra/runtime-config";
import { EngineConnectionSettings } from "./EngineConnectionSettings.tsx";

afterEach(cleanup);

const field = (): HTMLInputElement =>
  screen.getByRole("textbox", { name: "Engine URL" }) as HTMLInputElement;

describe("EngineConnectionSettings (stub, WI-17)", () => {
  test("shows the engine URL from the runtime config, read-only", () => {
    render(
      <RuntimeConfigProvider config={{ engineUrl: "http://engine.test:8080" }}>
        <EngineConnectionSettings />
      </RuntimeConfigProvider>,
    );
    expect(field()).toHaveValue("http://engine.test:8080");
    expect(field()).toHaveAttribute("readonly");
  });

  test("with the default same-origin URL it shows the page origin", () => {
    render(
      <RuntimeConfigProvider config={{ engineUrl: "" }}>
        <EngineConnectionSettings />
      </RuntimeConfigProvider>,
    );
    expect(field()).toHaveValue(window.location.origin);
  });

  test("without a provider it shows the page origin (the default config)", () => {
    render(<EngineConnectionSettings />);
    expect(field()).toHaveValue(window.location.origin);
  });

  test("says the value comes from config.json", () => {
    render(
      <RuntimeConfigProvider config={{ engineUrl: "http://engine.test:8080" }}>
        <EngineConnectionSettings />
      </RuntimeConfigProvider>,
    );
    expect(field()).toHaveAccessibleDescription(/config\.json/);
  });

  test("typing does not change the shown value", () => {
    render(
      <RuntimeConfigProvider config={{ engineUrl: "http://engine.test:8080" }}>
        <EngineConnectionSettings />
      </RuntimeConfigProvider>,
    );
    fireEvent.change(field(), { target: { value: "http://elsewhere" } });
    expect(field()).toHaveValue("http://engine.test:8080");
  });
});
