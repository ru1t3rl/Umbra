// The `render` column cell of the ui-kit Table (M8 WI-13, ASM-961) in the showcase: normal and dense, in a
// section of its own so the existing Table screenshot is unchanged.
import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import { ThemeProvider } from "@umbra/ui-kit";
import ShowcasePage from "../ShowcasePage/ShowcasePage.tsx";
import { TableCellsDemo } from "./TableCellsDemo.tsx";

afterEach(cleanup);

test("two tables show a danger Badge in a render column, one of them dense", () => {
  render(<TableCellsDemo />);
  const normal = screen.getByRole("table", { name: "Capability status" });
  const dense = screen.getByRole("table", { name: "Capability status, dense" });
  expect(normal).not.toHaveClass("vt-table-dense");
  expect(dense).toHaveClass("vt-table-dense");
  for (const table of [normal, dense]) {
    expect(within(table).getByText("Faulted")).toHaveClass("vt-badge-danger");
    expect(within(table).getAllByText("OK").length).toBeGreaterThan(0);
  }
});

test("the page has them in their own section inside the M8 group, outside the Table, Select and AppNav demo", () => {
  const { container } = render(
    <ThemeProvider>
      <ShowcasePage />
    </ThemeProvider>,
  );
  const group = container.querySelector("#showcase-m8") as HTMLElement;
  const demo = container.querySelector("#showcase-table-cells");
  expect(demo).toBeTruthy();
  expect(group.contains(demo)).toBe(true);
  expect(container.querySelector("#showcase-table")?.contains(demo)).toBe(false);
  expect(screen.getByRole("heading", { name: "Table cells", level: 3 })).toBeTruthy();
});
