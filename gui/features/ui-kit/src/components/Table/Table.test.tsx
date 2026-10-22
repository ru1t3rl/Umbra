import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import { Table, type TableColumn } from "./Table.tsx";

afterEach(cleanup);

interface Row {
  id: string;
  kind: string;
  count: number;
}

const columns: TableColumn<Row>[] = [
  { key: "kind", label: "Kind" },
  { key: "id", label: "Identifier", mono: true },
  { key: "count", label: "Count", align: "end" },
];
const rows: Row[] = [
  { id: "a-1", kind: "motion", count: 3 },
  { id: "b-2", kind: "behavior", count: 12 },
];

test("the caption names the table", () => {
  render(<Table caption="Components by kind" columns={columns} rows={rows} rowKey={(r) => r.id} />);
  const table = screen.getByRole("table", { name: "Components by kind" });
  expect(table).toHaveClass("vt-table");
});

test("each column label is a columnheader", () => {
  render(<Table caption="C" columns={columns} rows={rows} rowKey={(r) => r.id} />);
  const headers = screen.getAllByRole("columnheader").map((h) => h.textContent);
  expect(headers).toEqual(["Kind", "Identifier", "Count"]);
});

test("rows render one cell per column in column order", () => {
  render(<Table caption="C" columns={columns} rows={rows} rowKey={(r) => r.id} />);
  const body = screen.getAllByRole("row").slice(1); // first row is the header
  expect(body).toHaveLength(2);
  expect(
    within(body[1] as HTMLElement)
      .getAllByRole("cell")
      .map((c) => c.textContent),
  ).toEqual(["behavior", "b-2", "12"]);
});

test("mono columns get the mono class and others do not", () => {
  render(<Table caption="C" columns={columns} rows={rows} rowKey={(r) => r.id} />);
  const cells = within(screen.getAllByRole("row")[1] as HTMLElement).getAllByRole("cell");
  expect(cells[1]).toHaveClass("vt-table-mono");
  expect(cells[0]).not.toHaveClass("vt-table-mono");
});

test("end alignment is applied to the header and its cells", () => {
  render(<Table caption="C" columns={columns} rows={rows} rowKey={(r) => r.id} />);
  expect(screen.getByRole("columnheader", { name: "Count" })).toHaveClass("vt-table-end");
  const cells = within(screen.getAllByRole("row")[1] as HTMLElement).getAllByRole("cell");
  expect(cells[2]).toHaveClass("vt-table-end");
});

test("no rows shows the empty text and keeps the headers", () => {
  render(
    <Table
      caption="C"
      columns={columns}
      rows={[]}
      rowKey={(r) => r.id}
      empty="No adapters loaded."
    />,
  );
  expect(screen.getByText("No adapters loaded.")).toBeInTheDocument();
  expect(screen.getAllByRole("columnheader")).toHaveLength(3);
});

test("the empty text is absent while there are rows", () => {
  render(
    <Table caption="C" columns={columns} rows={rows} rowKey={(r) => r.id} empty="Nothing here." />,
  );
  expect(screen.queryByText("Nothing here.")).toBeNull();
});

test("dense adds vt-table-dense; className passes through", () => {
  render(
    <Table
      caption="C"
      columns={columns}
      rows={rows}
      rowKey={(r) => r.id}
      dense
      className="extra"
    />,
  );
  expect(screen.getByRole("table")).toHaveClass("vt-table", "vt-table-dense", "extra");
});

test("rows with distinct keys render without duplicate-key warnings", () => {
  const errors: unknown[] = [];
  const original = console.error;
  console.error = (...a: unknown[]) => errors.push(a);
  try {
    render(<Table caption="C" columns={columns} rows={rows} rowKey={(r) => r.id} />);
  } finally {
    console.error = original;
  }
  expect(errors).toEqual([]);
});

test("null and undefined cells render empty, not the words", () => {
  interface R {
    id: string;
    note: string | null | undefined;
  }
  render(
    <Table<R>
      caption="C"
      columns={[
        { key: "id", label: "Id" },
        { key: "note", label: "Note" },
      ]}
      rows={[
        { id: "x", note: null },
        { id: "y", note: undefined },
      ]}
      rowKey={(r) => r.id}
    />,
  );
  const cells = screen.getAllByRole("cell").map((c) => c.textContent);
  expect(cells).toEqual(["x", "", "y", ""]);
});

test("captionHidden keeps the name but hides the caption visually", () => {
  render(
    <Table caption="Hidden" columns={columns} rows={rows} rowKey={(r) => r.id} captionHidden />,
  );
  expect(screen.getByRole("table", { name: "Hidden" })).toBeInTheDocument();
  expect(screen.getByText("Hidden")).toHaveClass("vt-visually-hidden");
});

test("rowKey is called with each row", () => {
  const seen: Row[] = [];
  render(
    <Table
      caption="C"
      columns={columns}
      rows={rows}
      rowKey={(r) => {
        seen.push(r);
        return r.id;
      }}
    />,
  );
  for (const r of rows) expect(seen).toContain(r);
});

test("column headers use the label style", () => {
  render(<Table caption="C" columns={columns} rows={rows} rowKey={(r) => r.id} />);
  for (const h of screen.getAllByRole("columnheader")) expect(h).toHaveClass("vt-label");
});
