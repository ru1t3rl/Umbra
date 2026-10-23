// WI-17 (docs/plans/M8.md), written before the component. Contract:
// - `ThemePreferenceSetting()` takes no props; it reads and sets the theme preference through ui-kit's `useTheme`
//   (the ThemeProvider owns where it is stored: localStorage `umbra.theme` on the web, ASM-301, the settings file on
//   the desktop); it writes nothing to the settings backend itself (one writer);
// - one ui-kit `Select` named "Theme preference" with the options Dark, Light and System (values dark, light,
//   system), the current preference selected.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { useTheme } from "@umbra/ui-kit";
import { mountSettings } from "../../testing.tsx";
import { ThemePreferenceSetting } from "./ThemePreferenceSetting.tsx";

afterEach(cleanup);

const select = (): HTMLSelectElement =>
  screen.getByRole("combobox", { name: "Theme preference" }) as HTMLSelectElement;

function Resolved() {
  const { preference, resolved } = useTheme();
  return <output aria-label="Theme">{`${preference}/${resolved}`}</output>;
}

describe("ThemePreferenceSetting", () => {
  test("offers Dark, Light and System, the default dark selected", () => {
    mountSettings(<ThemePreferenceSetting />);
    expect(
      within(select())
        .getAllByRole("option")
        .map((o) => [o.textContent, (o as HTMLOptionElement).value]),
    ).toEqual([
      ["Dark", "dark"],
      ["Light", "light"],
      ["System", "system"],
    ]);
    expect(select()).toHaveValue("dark");
  });

  test("choosing Light sets the preference and the resolved theme", () => {
    mountSettings(
      <>
        <ThemePreferenceSetting />
        <Resolved />
      </>,
    );
    fireEvent.change(select(), { target: { value: "light" } });
    expect(select()).toHaveValue("light");
    expect(screen.getByLabelText("Theme")).toHaveTextContent("light/light");
  });

  test("choosing System sets the preference to system", () => {
    mountSettings(
      <>
        <ThemePreferenceSetting />
        <Resolved />
      </>,
    );
    fireEvent.change(select(), { target: { value: "system" } });
    expect(screen.getByLabelText("Theme").textContent?.startsWith("system/")).toBe(true);
  });

  // Catches: a second writer (the editor writing the settings file next to the ThemeProvider).
  test("writes nothing to the settings backend itself", () => {
    const { writes } = mountSettings(<ThemePreferenceSetting />);
    fireEvent.change(select(), { target: { value: "light" } });
    expect(writes).toHaveLength(0);
  });
});
