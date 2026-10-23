// WI-14 (ASM-972): the optional `submitVariant` prop. A new file; the WI-8 tests of ScenarioLoader are untouched.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen } from "@testing-library/react";
import { mount } from "../../controlsTesting.tsx";
import { ScenarioLoader } from "./ScenarioLoader.tsx";

afterEach(cleanup);

const submit = () => screen.getByRole("button", { name: "Load scenario" });

describe("ScenarioLoader submitVariant", () => {
  // Catches: a changed default (every existing user keeps the azure filled command).
  test("the submit Button is filled by default", () => {
    mount(<ScenarioLoader onLoaded={() => {}} />);
    expect(submit()).toHaveClass("vt-btn-filled");
  });

  // Catches: the prop being ignored (the dashboard needs the only filled Button to be the bar's command).
  test("tonal makes it a tonal Button", () => {
    mount(<ScenarioLoader onLoaded={() => {}} submitVariant="tonal" />);
    expect(submit()).toHaveClass("vt-btn-tonal");
    expect(submit()).not.toHaveClass("vt-btn-filled");
  });
});
