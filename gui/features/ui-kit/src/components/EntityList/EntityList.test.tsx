import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { EntityRow, type EntityRowProps } from "../EntityRow/EntityRow.tsx";
import { EntityList } from "./EntityList.tsx";

afterEach(cleanup);

const base: EntityRowProps = {
  name: "Alpha 1-1",
  id: "1:3:1021",
  affiliation: "friend",
  echelon: "PLT",
  meta: "31U FT 12345 67890 · 047° · 32 km/h",
  status: { label: "Running", tone: "success" },
};

test("EntityList is a labelled listbox and rows are options in it", () => {
  render(
    <EntityList label="Entities">
      <EntityRow {...base} />
    </EntityList>,
  );
  const list = screen.getByRole("listbox", { name: "Entities" });
  expect(list).toHaveClass("vt-list");
  const option = screen.getByRole("option");
  expect(list).toContainElement(option);
  expect(option).toHaveClass("vt-entity");
});

function Harness() {
  const [sel, setSel] = useState("a");
  return (
    <EntityList label="Entities">
      {["a", "b", "c"].map((k) => (
        <EntityRow
          key={k}
          {...base}
          name={`Unit ${k}`}
          selected={sel === k}
          onSelect={() => setSel(k)}
        />
      ))}
    </EntityList>
  );
}

test("arrow keys, Home and End move focus between rows without selecting", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  const names = (n: string) => screen.getByRole("option", { name: new RegExp(`Unit ${n}`) });
  names("a").focus();
  await user.keyboard("{ArrowDown}");
  expect(names("b")).toHaveFocus();
  expect(names("a")).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{End}");
  expect(names("c")).toHaveFocus();
  await user.keyboard("{ArrowDown}");
  expect(names("c")).toHaveFocus();
  await user.keyboard("{ArrowUp}");
  expect(names("b")).toHaveFocus();
  await user.keyboard("{Home}");
  expect(names("a")).toHaveFocus();
  await user.keyboard("{Enter}");
  expect(names("a")).toHaveAttribute("aria-selected", "true");
});

test("EntityList className is appended", () => {
  render(
    <EntityList label="E" className="extra">
      <EntityRow {...base} />
    </EntityList>,
  );
  expect(screen.getByRole("listbox")).toHaveClass("vt-list", "extra");
});
