import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { EntityList, EntityRow, type EntityRowProps } from "./EntityRow.tsx";

afterEach(cleanup);

const base: EntityRowProps = {
  name: "Alpha 1-1",
  id: "1:3:1021",
  affiliation: "friend",
  echelon: "PLT",
  meta: "31U FT 12345 67890 · 047° · 32 km/h",
  status: { label: "Running", tone: "success" },
};

const { status: _status, ...withoutStatus } = base;

function renderRow(props: Partial<EntityRowProps> = {}, from: EntityRowProps = base) {
  return render(
    <EntityList label="Entities">
      <EntityRow {...from} {...props} />
    </EntityList>,
  );
}

test("EntityList is a labelled listbox and rows are options in it", () => {
  renderRow();
  const list = screen.getByRole("listbox", { name: "Entities" });
  expect(list).toHaveClass("vt-list");
  const option = screen.getByRole("option");
  expect(list).toContainElement(option);
  expect(option).toHaveClass("vt-entity");
});

test("aria-selected reflects selected", () => {
  const { rerender } = renderRow();
  expect(screen.getByRole("option")).toHaveAttribute("aria-selected", "false");
  rerender(
    <EntityList label="Entities">
      <EntityRow {...base} selected />
    </EntityList>,
  );
  expect(screen.getByRole("option")).toHaveAttribute("aria-selected", "true");
});

test("the accessible name reads as words, with the affiliation spelled out", () => {
  renderRow();
  expect(
    screen.getByRole("option", {
      name: "Friend PLT Alpha 1-1 1:3:1021 31U FT 12345 67890 · 047° · 32 km/h Running",
    }),
  ).toBeInTheDocument();
});

test("name, id, meta and the status Badge use the reference classes", () => {
  const { container } = renderRow();
  expect(container.querySelector(".vt-entity-name")?.textContent).toBe("Alpha 1-1");
  expect(container.querySelector(".vt-entity-name")).toHaveAttribute("title", "Alpha 1-1");
  expect(container.querySelector(".vt-entity-id")?.textContent).toBe("1:3:1021");
  expect(container.querySelector(".vt-entity-meta")?.textContent).toContain("32 km/h");
  expect(container.querySelector(".vt-badge")).toHaveClass("vt-badge-success");
  expect(container.querySelector(".vt-affil")).toHaveClass("vt-affil-friend");
});

test("optional parts are left out when absent or empty", () => {
  const { container } = renderRow({ id: "", meta: "", echelon: "" }, withoutStatus);
  expect(container.querySelector(".vt-entity-id")).toBeNull();
  expect(container.querySelector(".vt-entity-meta")).toBeNull();
  expect(container.querySelector(".vt-badge")).toBeNull();
});

test("stale shows the word Stale in the Badge and the stale class", () => {
  const { container } = renderRow({ stale: true });
  expect(container.querySelector(".vt-entity")).toHaveClass("vt-entity", "vt-entity-stale");
  expect(container.querySelector(".vt-badge")?.textContent).toBe("Stale");
  expect(container.querySelector(".vt-badge")).not.toHaveClass("vt-badge-success");
});

test("a stale row without a status still says Stale", () => {
  const { container } = renderRow({ stale: true }, withoutStatus);
  expect(container.querySelector(".vt-badge")?.textContent).toBe("Stale");
});

test("click and Enter and Space call onSelect", async () => {
  const user = userEvent.setup();
  let n = 0;
  renderRow({ onSelect: () => n++ });
  await user.click(screen.getByRole("option"));
  await user.keyboard("{Enter}");
  await user.keyboard(" ");
  expect(n).toBe(3);
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
