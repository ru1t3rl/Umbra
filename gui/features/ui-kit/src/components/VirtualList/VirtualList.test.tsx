import { afterEach, expect, mock, test } from "bun:test";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ReactElement, useState } from "react";
import { ROW_HEIGHT_PX } from "../../model/rowHeights.ts";
import { VirtualList, type VirtualListProps } from "./VirtualList.tsx";

afterEach(cleanup);

const RH = ROW_HEIGHT_PX["row-md"];
const VIEWPORT = RH * 10; // ten rows visible

/** Fake size observer: reports a fixed viewport height at once (happy-dom has no layout, ASM-873). */
function sized(height: number) {
  return (_el: Element, onSize: (h: number) => void) => {
    onSize(height);
    return () => {};
  };
}

type ListboxProps = Extract<VirtualListProps, { role: "listbox" }>;

function Harness(
  props: Partial<ListboxProps> & { count?: number; onActivate?: (i: number) => void },
): ReactElement {
  const [active, setActive] = useState<number | null>(props.activeIndex ?? null);
  return (
    <VirtualList
      role="listbox"
      label="Entities"
      count={props.count ?? 10_000}
      rowHeight="row-md"
      renderRow={(i) => <span>Row {i}</span>}
      getRowId={(i) => `ent-${i}`}
      observeSize={sized(VIEWPORT)}
      activeIndex={active}
      onActiveIndexChange={setActive}
      onActivate={props.onActivate ?? (() => {})}
      {...(props.overscan === undefined ? {} : { overscan: props.overscan })}
    />
  );
}

function list(): HTMLElement {
  return screen.getByRole("listbox", { name: "Entities" });
}

// --- windowing -----------------------------------------------------------------------------

test("10,000 items render at most visible rows plus twice the default overscan", () => {
  render(<Harness />);
  const rows = screen.getAllByRole("option");
  expect(rows.length).toBeLessThanOrEqual(Math.ceil(VIEWPORT / RH) + 1 + 2 * 8);
  expect(rows.length).toBeGreaterThan(0);
});

test("the first rows are rendered from the top", () => {
  render(<Harness />);
  expect(screen.getByText("Row 0")).toBeTruthy();
  expect(screen.queryByText("Row 9999")).toBeNull();
});

test("the overscan prop widens the rendered window", () => {
  render(<Harness overscan={2} count={1000} />);
  const n = screen.getAllByRole("option").length;
  expect(n).toBeLessThanOrEqual(Math.ceil(VIEWPORT / RH) + 1 + 2 * 2);
});

test("scrolling renders the rows at the new offset and drops the old ones", () => {
  render(<Harness />);
  const el = list();
  el.scrollTop = RH * 5000;
  fireEvent.scroll(el);
  expect(screen.getByText("Row 5000")).toBeTruthy();
  expect(screen.queryByText("Row 0")).toBeNull();
  expect(screen.getAllByRole("option").length).toBeLessThanOrEqual(
    Math.ceil(VIEWPORT / RH) + 1 + 16,
  );
});

test("scrolling re-renders the list only, not the parent render function of other rows", () => {
  const renderRow = mock((i: number) => <span>Row {i}</span>);
  render(
    <VirtualList
      role="listbox"
      label="Entities"
      count={10_000}
      rowHeight="row-md"
      renderRow={renderRow}
      getRowId={(i) => `ent-${i}`}
      observeSize={sized(VIEWPORT)}
      activeIndex={null}
      onActiveIndexChange={() => {}}
      onActivate={() => {}}
    />,
  );
  renderRow.mockClear();
  const el = list();
  el.scrollTop = RH * 3000;
  fireEvent.scroll(el);
  const indexes = new Set(renderRow.mock.calls.map((c) => c[0] as number));
  expect(indexes.size).toBeLessThanOrEqual(Math.ceil(VIEWPORT / RH) + 1 + 16);
  expect(Math.min(...indexes)).toBeGreaterThanOrEqual(3000 - 8 - 1);
});

test("the scroll content is as tall as every row together so the scrollbar is true", () => {
  render(<Harness count={10_000} />);
  const sizer = list().querySelector("[data-virtual-sizer]") as HTMLElement;
  expect(sizer.style.height).toBe(`${10_000 * RH}px`);
});

test("a changed viewport size re-windows the rows", () => {
  let report: (h: number) => void = () => {};
  render(
    <VirtualList
      role="listbox"
      label="Entities"
      count={1000}
      rowHeight="row-md"
      renderRow={(i) => <span>Row {i}</span>}
      getRowId={(i) => `ent-${i}`}
      observeSize={(_el, onSize) => {
        report = onSize;
        onSize(RH * 2);
        return () => {};
      }}
      activeIndex={null}
      onActiveIndexChange={() => {}}
      onActivate={() => {}}
      overscan={0}
    />,
  );
  const before = screen.getAllByRole("option").length;
  act(() => report(RH * 20));
  expect(screen.getAllByRole("option").length).toBeGreaterThan(before);
});

test("the size observer is cleaned up on unmount", () => {
  const off = mock(() => {});
  const { unmount } = render(
    <VirtualList
      role="listbox"
      label="Entities"
      count={5}
      rowHeight="row-md"
      renderRow={(i) => <span>Row {i}</span>}
      getRowId={(i) => `ent-${i}`}
      observeSize={(_el, onSize) => {
        onSize(VIEWPORT);
        return off;
      }}
      activeIndex={null}
      onActiveIndexChange={() => {}}
      onActivate={() => {}}
    />,
  );
  unmount();
  expect(off).toHaveBeenCalledTimes(1);
});

// --- aria ----------------------------------------------------------------------------------

test("the listbox has the accessible name from label", () => {
  render(<Harness />);
  expect(list()).toBeTruthy();
});

test("every rendered row carries aria-setsize and a one-based aria-posinset", () => {
  render(<Harness count={10_000} />);
  const rows = screen.getAllByRole("option");
  rows.forEach((row, k) => {
    expect(row).toHaveAttribute("aria-setsize", "10000");
    expect(row).toHaveAttribute("aria-posinset", String(k + 1));
  });
});

test("posinset stays absolute after scrolling", () => {
  render(<Harness />);
  const el = list();
  el.scrollTop = RH * 2000;
  fireEvent.scroll(el);
  const row = screen.getByText("Row 2000").closest("[role='option']");
  expect(row).toHaveAttribute("aria-posinset", "2001");
});

test("rows use the ids from getRowId", () => {
  render(<Harness />);
  expect(screen.getByText("Row 3").closest("[role='option']")).toHaveAttribute("id", "ent-3");
});

test("rows are marked data-index with their absolute index", () => {
  render(<Harness />);
  expect(screen.getByText("Row 4").closest("[role='option']")).toHaveAttribute("data-index", "4");
});

// --- one tab stop and roving by aria-activedescendant ----------------------------------------

test("the list is the only tab stop; rows are not tabbable", () => {
  render(<Harness />);
  expect(list()).toHaveAttribute("tabindex", "0");
  for (const row of screen.getAllByRole("option")) {
    expect(row.getAttribute("tabindex")).toBeNull();
  }
});

test("Tab moves focus onto the list once and out of it on the next Tab", async () => {
  const user = userEvent.setup();
  render(
    <>
      <button type="button">before</button>
      <Harness />
      <button type="button">after</button>
    </>,
  );
  await user.tab();
  await user.tab();
  expect(list()).toHaveFocus();
  await user.tab();
  expect(screen.getByRole("button", { name: "after" })).toHaveFocus();
});

test("without an active index there is no aria-activedescendant", () => {
  render(<Harness />);
  expect(list().getAttribute("aria-activedescendant")).toBeNull();
});

test("the active index is named by aria-activedescendant", () => {
  render(<Harness activeIndex={2} />);
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-2");
});

test("ArrowDown from nothing activates the first row", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  list().focus();
  await user.keyboard("{ArrowDown}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-0");
});

test("ArrowDown and ArrowUp move the active row by one and stop at the ends", async () => {
  const user = userEvent.setup();
  render(<Harness count={3} />);
  list().focus();
  await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{ArrowDown}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-2");
  await user.keyboard("{ArrowUp}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-1");
  await user.keyboard("{ArrowUp}{ArrowUp}{ArrowUp}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-0");
});

test("End and Home jump to the last and first row", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  list().focus();
  await user.keyboard("{End}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-9999");
  await user.keyboard("{Home}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-0");
});

test("PageDown and PageUp move by one viewport of rows", async () => {
  const user = userEvent.setup();
  render(<Harness activeIndex={0} />);
  list().focus();
  await user.keyboard("{PageDown}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-10");
  await user.keyboard("{PageUp}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-0");
});

test("PageUp at the top stays on the first row", async () => {
  const user = userEvent.setup();
  render(<Harness activeIndex={3} />);
  list().focus();
  await user.keyboard("{PageUp}");
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-0");
});

test("moving the active row far away scrolls it into the rendered window", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  list().focus();
  await user.keyboard("{End}");
  expect(screen.getByText("Row 9999")).toBeTruthy();
  expect(document.getElementById("ent-9999")).toBeTruthy();
});

test("the active row is marked and only that row", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  list().focus();
  await user.keyboard("{ArrowDown}{ArrowDown}");
  const marked = screen.getAllByRole("option").filter((o) => o.hasAttribute("data-active"));
  expect(marked).toHaveLength(1);
  expect(marked[0]).toHaveAttribute("id", "ent-1");
});

test("arrow keys report the new index through onActiveIndexChange and do not activate", async () => {
  const user = userEvent.setup();
  const onActiveIndexChange = mock((_i: number | null) => {});
  const onActivate = mock((_i: number) => {});
  render(
    <VirtualList
      role="listbox"
      label="Entities"
      count={5}
      rowHeight="row-md"
      renderRow={(i) => <span>Row {i}</span>}
      getRowId={(i) => `ent-${i}`}
      observeSize={sized(VIEWPORT)}
      activeIndex={1}
      onActiveIndexChange={onActiveIndexChange}
      onActivate={onActivate}
    />,
  );
  list().focus();
  await user.keyboard("{ArrowDown}");
  expect(onActiveIndexChange).toHaveBeenCalledWith(2);
  expect(onActivate).not.toHaveBeenCalled(); // selection does not follow focus (ASM-836)
});

test("Enter activates the active row", async () => {
  const user = userEvent.setup();
  const onActivate = mock((_i: number) => {});
  render(<Harness activeIndex={4} onActivate={onActivate} />);
  list().focus();
  await user.keyboard("{Enter}");
  expect(onActivate).toHaveBeenCalledWith(4);
});

test("Enter without an active row does nothing", async () => {
  const user = userEvent.setup();
  const onActivate = mock((_i: number) => {});
  render(<Harness onActivate={onActivate} />);
  list().focus();
  await user.keyboard("{Enter}");
  expect(onActivate).not.toHaveBeenCalled();
});

test("clicking a row makes it active and activates it", async () => {
  const user = userEvent.setup();
  const onActivate = mock((_i: number) => {});
  render(<Harness onActivate={onActivate} />);
  await user.click(screen.getByText("Row 6"));
  expect(onActivate).toHaveBeenCalledWith(6);
  expect(list()).toHaveAttribute("aria-activedescendant", "ent-6");
});

test("an empty list renders no rows and ignores keys", async () => {
  const user = userEvent.setup();
  render(<Harness count={0} />);
  list().focus();
  await user.keyboard("{ArrowDown}{End}{Enter}");
  expect(screen.queryAllByRole("option")).toHaveLength(0);
  expect(list().getAttribute("aria-activedescendant")).toBeNull();
});

test("an active index past a shrunken count is dropped", () => {
  const onActiveIndexChange = mock((_i: number | null) => {});
  const props = {
    role: "listbox" as const,
    label: "Entities",
    rowHeight: "row-md" as const,
    renderRow: (i: number) => <span>Row {i}</span>,
    getRowId: (i: number) => `ent-${i}`,
    observeSize: sized(VIEWPORT),
    activeIndex: 8,
    onActiveIndexChange,
    onActivate: () => {},
  };
  const { rerender } = render(<VirtualList {...props} count={10} />);
  rerender(<VirtualList {...props} count={3} />);
  expect(list().getAttribute("aria-activedescendant")).toBeNull();
  expect(onActiveIndexChange).toHaveBeenCalledWith(null);
});

// --- log -----------------------------------------------------------------------------------

type LogCase = { count: number };

function logList({ count }: LogCase): ReactElement {
  return (
    <VirtualList
      role="log"
      label="Event log"
      count={count}
      rowHeight="row-sm"
      renderRow={(i) => <span>Line {i}</span>}
      getRowId={(i) => `log-${i}`}
      observeSize={sized(VIEWPORT)}
      followEnd
    />
  );
}

const LH = ROW_HEIGHT_PX["row-sm"];
const endOf = (count: number) => count * LH - VIEWPORT;

test("a log has role log, a name and one tab stop", () => {
  render(logList({ count: 50 }));
  const log = screen.getByRole("log", { name: "Event log" });
  expect(log).toHaveAttribute("tabindex", "0");
});

test("a log does not use option rows or aria-activedescendant", () => {
  render(logList({ count: 50 }));
  const log = screen.getByRole("log", { name: "Event log" });
  expect(within(log).queryAllByRole("option")).toHaveLength(0);
  expect(log.getAttribute("aria-activedescendant")).toBeNull();
});

test("log rows carry aria-setsize and aria-posinset", () => {
  render(logList({ count: 50 }));
  const row = screen.getByText("Line 49").closest("[data-index]");
  expect(row).toHaveAttribute("aria-setsize", "50");
  expect(row).toHaveAttribute("aria-posinset", "50");
});

test("followEnd starts at the newest row", () => {
  render(logList({ count: 500 }));
  expect(screen.getByRole("log").scrollTop).toBe(endOf(500));
  expect(screen.getByText("Line 499")).toBeTruthy();
});

test("followEnd stays at the end while new rows arrive", () => {
  const { rerender } = render(logList({ count: 500 }));
  rerender(logList({ count: 501 }));
  rerender(logList({ count: 520 }));
  expect(screen.getByRole("log").scrollTop).toBe(endOf(520));
  expect(screen.getByText("Line 519")).toBeTruthy();
});

test("scrolling up stops following; new rows leave the offset alone", () => {
  const { rerender } = render(logList({ count: 500 }));
  const log = screen.getByRole("log");
  log.scrollTop = LH * 100;
  fireEvent.scroll(log);
  rerender(logList({ count: 600 }));
  expect(log.scrollTop).toBe(LH * 100);
  expect(screen.getByText("Line 100")).toBeTruthy();
});

test("scrolling back to the end resumes following", () => {
  const { rerender } = render(logList({ count: 500 }));
  const log = screen.getByRole("log");
  log.scrollTop = LH * 100;
  fireEvent.scroll(log);
  rerender(logList({ count: 600 }));
  log.scrollTop = endOf(600);
  fireEvent.scroll(log);
  rerender(logList({ count: 650 }));
  expect(log.scrollTop).toBe(endOf(650));
});

test("a log with 50,000 rows stays within the rendered-row budget", () => {
  render(logList({ count: 50_000 }));
  expect(document.querySelectorAll("[data-index]").length).toBeLessThanOrEqual(
    Math.ceil(VIEWPORT / LH) + 1 + 16,
  );
});

test("without followEnd a log keeps its offset when rows arrive", () => {
  const base = (count: number) => (
    <VirtualList
      role="log"
      label="Event log"
      count={count}
      rowHeight="row-sm"
      renderRow={(i) => <span>Line {i}</span>}
      getRowId={(i) => `log-${i}`}
      observeSize={sized(VIEWPORT)}
    />
  );
  const { rerender } = render(base(500));
  const log = screen.getByRole("log");
  expect(log.scrollTop).toBe(0);
  rerender(base(600));
  expect(log.scrollTop).toBe(0);
});
