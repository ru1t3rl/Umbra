// WI-17 (docs/plans/M8.md), written before the component. Contract:
// - `JournalCapacitySetting()` takes no props; it reads and writes `settings.journal.capacity` (whole number,
//   1,000..50,000, default 5,000, ADR 0074);
// - one ui-kit `TextField` of type number, a `spinbutton` named "Journal capacity", with help text that says the
//   change applies after a reload (the stream connection reads the capacity once, when the app starts, `main.tsx`);
// - the value is committed on blur or Enter, not while typing (so "5" on the way to "5000" is never judged);
// - a committed value that is not a whole number from 1,000 to 50,000 (empty, 999, 50001, 1500.5) is flagged:
//   `aria-invalid="true"`, and the field's description names the range ("1,000" and "50,000"); it is not saved,
//   the stored capacity stays as it was and the typed text stays visible; fixing it clears the flag.
import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { mountSettings } from "../../testing.tsx";
import { JournalCapacitySetting } from "./JournalCapacitySetting.tsx";

afterEach(cleanup);

const field = (): HTMLInputElement =>
  screen.getByRole("spinbutton", { name: "Journal capacity" }) as HTMLInputElement;
const type = (text: string): void => {
  fireEvent.change(field(), { target: { value: text } });
};
const commit = (): void => {
  fireEvent.blur(field());
};

describe("JournalCapacitySetting", () => {
  test("shows the default capacity and says the change applies after a reload", () => {
    mountSettings(<JournalCapacitySetting />);
    expect(field()).toHaveValue(5000);
    expect(field()).toHaveAccessibleDescription(/reload/i);
    expect(field()).not.toHaveAttribute("aria-invalid", "true");
  });

  test("shows the stored capacity", () => {
    mountSettings(<JournalCapacitySetting />, { journal: { capacity: 12_000 } });
    expect(field()).toHaveValue(12_000);
  });

  test("a valid value is persisted on blur", () => {
    const { writes, stored } = mountSettings(<JournalCapacitySetting />);
    type("20000");
    commit();
    expect(writes).toHaveLength(1);
    expect(stored()?.journal.capacity).toBe(20_000);
    expect(field()).not.toHaveAttribute("aria-invalid", "true");
  });

  test("a valid value is persisted on Enter", () => {
    const { stored } = mountSettings(<JournalCapacitySetting />);
    type("30000");
    fireEvent.keyDown(field(), { key: "Enter" });
    expect(stored()?.journal.capacity).toBe(30_000);
  });

  // Catches: judging or saving a half-typed value.
  test("typing alone saves nothing", () => {
    const { writes } = mountSettings(<JournalCapacitySetting />);
    type("2");
    type("20");
    type("2000");
    expect(writes).toHaveLength(0);
    expect(field()).not.toHaveAttribute("aria-invalid", "true");
  });

  test("the limits 1,000 and 50,000 are valid", () => {
    const { stored } = mountSettings(<JournalCapacitySetting />);
    type("1000");
    commit();
    expect(stored()?.journal.capacity).toBe(1000);
    type("50000");
    commit();
    expect(stored()?.journal.capacity).toBe(50_000);
  });

  for (const bad of ["999", "50001", "0", "-5000", "1500.5", ""]) {
    test(`"${bad}" is flagged, described with the range and not saved`, () => {
      const { writes } = mountSettings(<JournalCapacitySetting />, {
        journal: { capacity: 7000 },
      });
      type(bad);
      commit();
      expect(field()).toHaveAttribute("aria-invalid", "true");
      expect(field()).toHaveAccessibleDescription(/1,000/);
      expect(field()).toHaveAccessibleDescription(/50,000/);
      expect(writes).toHaveLength(0);
      expect(field().value).toBe(bad);
    });
  }

  test("Enter flags an invalid value too", () => {
    const { writes } = mountSettings(<JournalCapacitySetting />);
    type("5");
    fireEvent.keyDown(field(), { key: "Enter" });
    expect(field()).toHaveAttribute("aria-invalid", "true");
    expect(writes).toHaveLength(0);
  });

  test("fixing a flagged value clears the flag and saves it", () => {
    const { stored } = mountSettings(<JournalCapacitySetting />);
    type("5");
    commit();
    expect(field()).toHaveAttribute("aria-invalid", "true");
    type("6000");
    commit();
    expect(field()).not.toHaveAttribute("aria-invalid", "true");
    expect(stored()?.journal.capacity).toBe(6000);
  });

  test("committing the value already stored writes nothing", () => {
    const { writes } = mountSettings(<JournalCapacitySetting />);
    commit();
    type("5000");
    commit();
    expect(writes).toHaveLength(0);
  });
});
