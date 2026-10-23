// `TableColumn.render` (M8 WI-13, ASM-956): an additive custom cell; the default cell is unchanged.
import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import { Badge } from "../Badge/Badge.tsx";
import { Table, type TableColumn } from "./Table.tsx";

afterEach(cleanup);

interface Row {
  id: string;
  faulted: boolean;
  note?: string | null;
}

const rows: Row[] = [
  { id: "a", faulted: true },
  { id: "b", faulted: false, note: "x" },
];

const columns: TableColumn<Row>[] = [
  { key: "id", label: "Id" },
  {
    key: "faulted",
    label: "Status",
    render: (r) => (r.faulted ? <Badge tone="danger">Faulted</Badge> : "OK"),
  },
];

const body = (cols: TableColumn<Row>[]) =>
  render(<Table caption="T" columns={cols} rows={rows} rowKey={(r) => r.id} />);

test("a column with render shows the node it returns for each row", () => {
  body(columns);
  const [, first, second] = screen.getAllByRole("row") as HTMLElement[];
  expect(within(first as HTMLElement).getByText("Faulted")).toHaveClass("vt-badge-danger");
  expect(within(second as HTMLElement).getByText("OK")).toBeInTheDocument();
  expect(within(second as HTMLElement).queryByText("Faulted")).toBeNull();
});

test("render receives the whole row", () => {
  const seen: Row[] = [];
  body([
    {
      key: "id",
      label: "Id",
      render: (r) => {
        seen.push(r);
        return r.id.toUpperCase();
      },
    },
  ]);
  expect(seen).toEqual(rows);
  expect(screen.getByRole("cell", { name: "A" })).toBeInTheDocument();
});

test("render wins over the default for its column and leaves the others alone", () => {
  body(columns);
  expect(screen.getByRole("cell", { name: "a" })).toBeInTheDocument();
  expect(screen.queryByText("true")).toBeNull();
  expect(screen.queryByText("false")).toBeNull();
});

test("without render the cell is String(value), empty for null and undefined", () => {
  body([
    { key: "faulted", label: "F" },
    { key: "note", label: "N" },
  ]);
  const cells = screen.getAllByRole("cell");
  expect(cells.map((c) => c.textContent)).toEqual(["true", "", "false", "x"]);
});

test("render keeps the column classes (mono, end)", () => {
  body([{ key: "id", label: "Id", mono: true, align: "end", render: (r) => r.id }]);
  const cell = screen.getAllByRole("cell")[0];
  expect(cell).toHaveClass("vt-table-mono");
  expect(cell).toHaveClass("vt-table-end");
});
