// WI-17 (docs/plans/M8.md), written before the component. Contract:
// - `AffiliationOverridesEditor()` takes no props; it edits `settings.map.affiliationOverrides` (force -> friend |
//   hostile | neutral | unknown, at most 256, keys trimmed and lower case; `useAffiliationOf` applies them first);
// - a ui-kit `Table` named "Affiliation overrides" with the columns Force, Affiliation and Actions; one row per
//   row of the editor: a `TextField` "Force", a `Select` "Affiliation" (Friend, Hostile, Neutral, Unknown) and a
//   Button named "Remove override <force>" ("Remove new override" while the force is empty); no rows: the table
//   says "No overrides"; a tonal Button "Add override" adds a blank row (affiliation Unknown) and moves focus to
//   its Force field; at 256 rows the Add button is disabled;
// - THE MODEL: the stored map is exactly the valid rows. A row is valid when its force, trimmed and lower-cased, is
//   not empty, is not on `Object.prototype`'s reserved list (`__proto__`, `constructor`, `prototype`, which
//   `parseSettings` would drop silently) and no other row has it. Nothing else is stored: a flagged row
//   contributes nothing, and a row renamed into a flagged state stops contributing its old key;
// - when rows are judged: the Force field on blur or Enter (a half-typed name is never judged; on commit it shows
//   the normalized key), the Affiliation select on change, Remove at once. A change that does not alter the
//   stored map writes nothing;
// - an invalid row is flagged on its Force field: `aria-invalid="true"` and a description: "Enter a force name",
//   "Already listed" or "Reserved name"; fixing it clears the flag; the typed text stays visible;
// - Remove deletes the row and its key at once and moves focus to the "Add override" Button; a blur that the removed
//   (focused) Force field sends while it goes away does not bring the row back.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import { AffiliationProbe, mountSettings } from "../../testing.tsx";
import { AffiliationOverridesEditor } from "./AffiliationOverridesEditor.tsx";

afterEach(cleanup);

const table = (): HTMLElement => screen.getByRole("table", { name: "Affiliation overrides" });
const rows = (): HTMLElement[] =>
  within(table())
    .getAllByRole("row")
    .filter((r) => within(r).queryByRole("textbox") !== null);
const force = (row: HTMLElement): HTMLInputElement =>
  within(row).getByRole("textbox", { name: "Force" }) as HTMLInputElement;
const affiliation = (row: HTMLElement): HTMLSelectElement =>
  within(row).getByRole("combobox", { name: "Affiliation" }) as HTMLSelectElement;
const add = (): HTMLElement => screen.getByRole("button", { name: "Add override" });
const lastRow = (): HTMLElement => {
  const all = rows();
  const row = all.at(-1);
  if (!row) throw new Error("no rows");
  return row;
};

/** Types into a row's Force field and commits with blur. */
function enter(row: HTMLElement, text: string): void {
  fireEvent.change(force(row), { target: { value: text } });
  fireEvent.blur(force(row));
}
/** Adds a row and returns it. */
function addRow(): HTMLElement {
  fireEvent.click(add());
  return lastRow();
}
const map = (m: { stored: () => { map: { affiliationOverrides: unknown } } | undefined }) =>
  m.stored()?.map.affiliationOverrides;

describe("AffiliationOverridesEditor table", () => {
  test("with no overrides the table says so and offers Add override", () => {
    mountSettings(<AffiliationOverridesEditor />);
    expect(table()).toBeInTheDocument();
    expect(screen.getByText("No overrides")).toBeInTheDocument();
    expect(rows()).toHaveLength(0);
    expect(add()).toBeEnabled();
    expect(add()).not.toHaveClass("vt-btn-filled");
  });

  test("has the columns Force, Affiliation and Actions", () => {
    mountSettings(<AffiliationOverridesEditor />);
    expect(
      within(table())
        .getAllByRole("columnheader")
        .map((h) => h.textContent),
    ).toEqual(["Force", "Affiliation", "Actions"]);
  });

  test("lists the stored overrides, one row each, with the affiliation selected", () => {
    mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "hostile", purple: "friend" } },
    });
    expect(screen.queryByText("No overrides")).toBeNull();
    const shown = rows().map((r) => [force(r).value, affiliation(r).value]);
    expect(shown).toEqual([
      ["blue", "hostile"],
      ["purple", "friend"],
    ]);
  });

  test("the affiliation select offers Friend, Hostile, Neutral and Unknown", () => {
    mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const [row] = rows();
    if (!row) throw new Error("no row");
    expect(
      within(affiliation(row))
        .getAllByRole("option")
        .map((o) => [o.textContent, (o as HTMLOptionElement).value]),
    ).toEqual([
      ["Friend", "friend"],
      ["Hostile", "hostile"],
      ["Neutral", "neutral"],
      ["Unknown", "unknown"],
    ]);
  });
});

describe("AffiliationOverridesEditor adding", () => {
  test("Add override adds a blank row with affiliation Unknown, focuses its Force field and saves nothing", () => {
    const { writes } = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    expect(rows()).toHaveLength(1);
    expect(force(row).value).toBe("");
    expect(affiliation(row).value).toBe("unknown");
    expect(force(row)).toHaveFocus();
    expect(force(row)).not.toHaveAttribute("aria-invalid", "true");
    expect(screen.queryByText("No overrides")).toBeNull();
    expect(writes).toHaveLength(0);
  });

  test("a force name is saved on blur, trimmed and lower case, and the field shows the key", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    enter(row, "  Blue ");
    expect(m.writes).toHaveLength(1);
    expect(map(m)).toEqual({ blue: "unknown" });
    expect(force(row).value).toBe("blue");
  });

  test("Enter in the Force field saves it too", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    fireEvent.change(force(row), { target: { value: "red" } });
    fireEvent.keyDown(force(row), { key: "Enter" });
    expect(map(m)).toEqual({ red: "unknown" });
  });

  // Catches: judging or saving a half-typed name.
  test("typing alone saves nothing and flags nothing", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    fireEvent.change(force(row), { target: { value: "r" } });
    fireEvent.change(force(row), { target: { value: "re" } });
    expect(m.writes).toHaveLength(0);
    expect(force(row)).not.toHaveAttribute("aria-invalid", "true");
  });

  test("changing the affiliation of a saved row saves at once", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const [row] = rows();
    if (!row) throw new Error("no row");
    fireEvent.change(affiliation(row), { target: { value: "hostile" } });
    expect(m.writes).toHaveLength(1);
    expect(map(m)).toEqual({ blue: "hostile" });
  });

  test("an affiliation chosen before the name is kept when the name is committed", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    fireEvent.change(affiliation(row), { target: { value: "hostile" } });
    expect(m.writes).toHaveLength(0);
    enter(row, "red");
    expect(map(m)).toEqual({ red: "hostile" });
  });

  test("committing a saved row unchanged writes nothing", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const [row] = rows();
    if (!row) throw new Error("no row");
    enter(row, "blue");
    expect(m.writes).toHaveLength(0);
  });

  test("renaming a saved row replaces its key", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "hostile" } },
    });
    const [row] = rows();
    if (!row) throw new Error("no row");
    enter(row, "Teal");
    expect(map(m)).toEqual({ teal: "hostile" });
  });

  test("at 256 overrides the Add button is disabled", () => {
    const many = Object.fromEntries(Array.from({ length: 256 }, (_, i) => [`force${i}`, "friend"]));
    mountSettings(<AffiliationOverridesEditor />, { map: { affiliationOverrides: many } });
    expect(rows()).toHaveLength(256);
    expect((add() as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("AffiliationOverridesEditor invalid rows", () => {
  test("an empty name is flagged, described and not saved", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    enter(row, "   ");
    expect(force(row)).toHaveAttribute("aria-invalid", "true");
    expect(force(row)).toHaveAccessibleDescription(/enter a force name/i);
    expect(m.writes).toHaveLength(0);
    expect(force(row).value).toBe("   ");
  });

  test("a name that is already listed is flagged, whatever its case, and not saved", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const row = addRow();
    enter(row, " BLUE");
    expect(force(row)).toHaveAttribute("aria-invalid", "true");
    expect(force(row)).toHaveAccessibleDescription(/already listed/i);
    expect(m.writes).toHaveLength(0);
    // the saved row is not the flagged one
    expect(force(rows()[0] as HTMLElement)).not.toHaveAttribute("aria-invalid", "true");
  });

  for (const reserved of ["__proto__", "constructor", "prototype", "  Constructor "]) {
    test(`the reserved name "${reserved.trim()}" is flagged and not saved`, () => {
      const m = mountSettings(<AffiliationOverridesEditor />);
      const row = addRow();
      enter(row, reserved);
      expect(force(row)).toHaveAttribute("aria-invalid", "true");
      expect(force(row)).toHaveAccessibleDescription(/reserved name/i);
      expect(m.writes).toHaveLength(0);
    });
  }

  // THE MODEL: valid rows are stored, a flagged row adds nothing.
  test("a valid row is saved while an invalid one beside it is flagged and left out", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const first = addRow();
    enter(first, "red");
    const second = addRow();
    enter(second, "");
    expect(force(second)).toHaveAttribute("aria-invalid", "true");
    expect(force(first)).not.toHaveAttribute("aria-invalid", "true");
    expect(m.writes).toHaveLength(1);
    expect(map(m)).toEqual({ red: "unknown" });
  });

  test("renaming a saved row into a duplicate flags it and its old key is no longer stored", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend", red: "hostile" } },
    });
    const second = rows()[1] as HTMLElement;
    enter(second, "blue");
    expect(force(second)).toHaveAttribute("aria-invalid", "true");
    expect(force(second)).toHaveAccessibleDescription(/already listed/i);
    expect(map(m)).toEqual({ blue: "friend" });
  });

  test("a changed affiliation on a flagged row saves nothing", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    enter(row, "");
    fireEvent.change(affiliation(row), { target: { value: "hostile" } });
    expect(m.writes).toHaveLength(0);
  });

  test("fixing a flagged name clears the flag and saves the row", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    enter(row, "");
    expect(force(row)).toHaveAttribute("aria-invalid", "true");
    enter(row, "white");
    expect(force(row)).not.toHaveAttribute("aria-invalid", "true");
    expect(map(m)).toEqual({ white: "unknown" });
  });

  test("a flagged row stops being flagged when the row it duplicated is removed and it is committed again", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const dup = addRow();
    enter(dup, "blue");
    expect(force(dup)).toHaveAttribute("aria-invalid", "true");
    fireEvent.click(
      within(rows()[0] as HTMLElement).getByRole("button", { name: "Remove override blue" }),
    );
    const [only] = rows();
    if (!only) throw new Error("no row");
    enter(only, "blue");
    expect(force(only)).not.toHaveAttribute("aria-invalid", "true");
    expect(map(m)).toEqual({ blue: "unknown" });
  });
});

describe("AffiliationOverridesEditor removing", () => {
  test("Remove deletes the row and its key at once", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "hostile", purple: "friend" } },
    });
    fireEvent.click(screen.getByRole("button", { name: "Remove override blue" }));
    expect(rows().map((r) => force(r).value)).toEqual(["purple"]);
    expect(map(m)).toEqual({ purple: "friend" });
  });

  test("removing the last row shows the empty state", () => {
    mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    fireEvent.click(screen.getByRole("button", { name: "Remove override blue" }));
    expect(screen.getByText("No overrides")).toBeInTheDocument();
  });

  test("a blank row is removed with 'Remove new override' and nothing is written", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    addRow();
    fireEvent.click(screen.getByRole("button", { name: "Remove new override" }));
    expect(rows()).toHaveLength(0);
    expect(m.writes).toHaveLength(0);
  });

  test("removing a flagged row leaves the stored map as it was", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const dup = addRow();
    enter(dup, "BLUE");
    fireEvent.click(within(dup).getByRole("button", { name: /^Remove override/ }));
    expect(rows()).toHaveLength(1);
    expect(m.writes).toHaveLength(0);
  });

  // Pins the end state of removing the row whose Force field has focus: the row and its key stay gone and nothing
  // more is written. The stale-closure bug the throwaway implementation had (a blur handler re-adding the row)
  // needs the focus to move to Add while the field is still attached; this implementation moves it in an effect
  // after the row is gone, and happy-dom sends no blur then, so no test can kill that mutant (ASM-997 b). The rows
  // are kept in a ref as well, so a handler that runs late still reads the current rows.
  test("the blur of a focused Force field whose row is being removed does not bring the row back", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const [row] = rows();
    if (!row) throw new Error("no row");
    force(row).focus();
    fireEvent.click(screen.getByRole("button", { name: "Remove override blue" }));
    expect(rows()).toHaveLength(0);
    expect(map(m)).toEqual({});
    expect(m.writes).toHaveLength(1);
  });

  test("focus moves to Add override after a removal", () => {
    mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    fireEvent.click(screen.getByRole("button", { name: "Remove override blue" }));
    expect(add()).toHaveFocus();
  });
});

describe("AffiliationOverridesEditor takes effect through useAffiliationOf", () => {
  test("an override recolours the force, and removing it restores the table's colour", () => {
    mountSettings(
      <>
        <AffiliationOverridesEditor />
        <AffiliationProbe force="Blue" />
      </>,
    );
    const probe = () => screen.getByLabelText("Affiliation of Blue");
    expect(probe()).toHaveTextContent("friend");
    const row = addRow();
    fireEvent.change(affiliation(row), { target: { value: "hostile" } });
    enter(row, "blue");
    expect(probe()).toHaveTextContent("hostile");
    fireEvent.click(screen.getByRole("button", { name: "Remove override blue" }));
    expect(probe()).toHaveTextContent("friend");
  });

  test("an invalid row recolours nothing", () => {
    mountSettings(
      <>
        <AffiliationOverridesEditor />
        <AffiliationProbe force="blue" />
      </>,
    );
    const row = addRow();
    enter(row, "");
    fireEvent.change(affiliation(row), { target: { value: "hostile" } });
    expect(screen.getByLabelText("Affiliation of blue")).toHaveTextContent("friend");
  });

  test("stored overrides are in effect on the first render", () => {
    mountSettings(
      <>
        <AffiliationOverridesEditor />
        <AffiliationProbe force="red" />
      </>,
      { map: { affiliationOverrides: { red: "friend" } } },
    );
    expect(screen.getByLabelText("Affiliation of red")).toHaveTextContent("friend");
  });
});

// WI-17 review (ASM-997): m1 (a flagged duplicate is judged again when its twin goes), m5 (names longer than the
// parser keeps) and n1 (rows told apart by assistive technology).
describe("AffiliationOverridesEditor review round", () => {
  const invalid = (row: HTMLElement): string | null => force(row).getAttribute("aria-invalid");

  test("a duplicate stops being flagged and is stored when the row it duplicated is removed", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const dup = addRow();
    enter(dup, "BLUE");
    expect(invalid(dup)).toBe("true");
    // The flagged field still has focus (a fired blur does not move it): leave it for real, so that moving the focus
    // to Add after the removal does not commit the row again and hide the missing re-judging.
    act(() => (document.activeElement as HTMLElement | null)?.blur());
    fireEvent.click(screen.getByRole("button", { name: "Remove override blue" }));
    const [only] = rows();
    if (!only) throw new Error("no row");
    expect(invalid(only)).toBeNull();
    expect(force(only).value).toBe("blue");
    expect(map(m)).toEqual({ blue: "unknown" });
  });

  test("a duplicate stops being flagged and is stored when the row it duplicated is renamed", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const dup = addRow();
    enter(dup, "blue");
    expect(invalid(dup)).toBe("true");
    enter(rows()[0] as HTMLElement, "teal");
    expect(invalid(dup)).toBeNull();
    expect(map(m)).toEqual({ teal: "friend", blue: "unknown" });
  });

  test("of two duplicates of a removed row the first becomes valid and the second stays flagged", () => {
    const m = mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend" } },
    });
    const second = addRow();
    enter(second, "blue");
    const third = addRow();
    enter(third, "Blue");
    act(() => (document.activeElement as HTMLElement | null)?.blur());
    fireEvent.click(
      within(rows()[0] as HTMLElement).getByRole("button", { name: "Remove override blue" }),
    );
    const [a, b] = rows();
    if (!a || !b) throw new Error("rows");
    expect(invalid(a)).toBeNull();
    expect(invalid(b)).toBe("true");
    expect(map(m)).toEqual({ blue: "unknown" });
  });

  test("a name longer than the parser keeps is flagged and not saved", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    enter(row, "a".repeat(4097));
    expect(invalid(row)).toBe("true");
    expect(force(row).getAttribute("aria-describedby")).toBeTruthy();
    expect(document.body.textContent).toContain("Name is too long");
    expect(m.writes).toHaveLength(0);
  });

  test("a name of exactly 4,096 characters is saved", () => {
    const m = mountSettings(<AffiliationOverridesEditor />);
    const row = addRow();
    enter(row, "a".repeat(4096));
    expect(invalid(row)).toBeNull();
    expect(Object.keys(map(m) ?? {})).toHaveLength(1);
  });

  // Catches: rows that cannot be told apart (the same field names in every row).
  test("every row's fields are described with the row they belong to", () => {
    mountSettings(<AffiliationOverridesEditor />, {
      map: { affiliationOverrides: { blue: "friend", red: "hostile", green: "neutral" } },
    });
    const text = (el: HTMLElement): string =>
      (el.getAttribute("aria-describedby") ?? "")
        .split(" ")
        .map((id) => document.getElementById(id)?.textContent ?? "")
        .join(" ");
    rows().forEach((r, i) => {
      expect(text(force(r))).toContain(`row ${i + 1} of 3`);
      expect(text(affiliation(r))).toContain(`row ${i + 1} of 3`);
    });
  });
});
