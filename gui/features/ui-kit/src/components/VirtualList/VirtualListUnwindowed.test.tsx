// `virtualize={false}` (M8 WI-12, ASM-953): the same list with every row mounted, for short lists whose rows
// must all be in the DOM. Same classes, row ids, ARIA and keyboard model as the windowed list.
import { afterEach, expect, mock, test } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { type ReactElement, useState } from "react";
import { ROW_HEIGHT_PX } from "../../model/rowHeights.ts";
import { VirtualList } from "./VirtualList.tsx";

afterEach(cleanup);

const RH = ROW_HEIGHT_PX["row-md"];
const sized = (height: number) => (_el: Element, onSize: (h: number) => void) => {
  onSize(height);
  return () => {};
};

function Harness(props: {
  count: number;
  virtualize?: boolean;
  initialActive?: number;
  onActivate?: (i: number) => void;
}): ReactElement {
  const [active, setActive] = useState<number | null>(props.initialActive ?? null);
  return (
    <VirtualList
      role="listbox"
      label="Things"
      count={props.count}
      rowHeight="row-md"
      renderRow={(i) => <span>Row {i}</span>}
      getRowId={(i) => `thing-${i}`}
      observeSize={sized(RH * 5)}
      activeIndex={active}
      selectedIndex={2}
      onActiveIndexChange={setActive}
      onActivate={props.onActivate ?? (() => {})}
      {...(props.virtualize === undefined ? {} : { virtualize: props.virtualize })}
    />
  );
}

const box = () => screen.getByRole("listbox", { name: "Things" });

test("virtualize false mounts every row of a long list", () => {
  render(<Harness count={300} virtualize={false} />);
  expect(screen.getAllByRole("option")).toHaveLength(300);
  expect(screen.getByText("Row 299")).toBeTruthy();
});

test("virtualize true and the default still window the rows", () => {
  const { unmount } = render(<Harness count={300} virtualize />);
  expect(screen.getAllByRole("option").length).toBeLessThan(300);
  unmount();
  render(<Harness count={300} />);
  expect(screen.getAllByRole("option").length).toBeLessThan(300);
});

test("the rows keep the classes, ids and aria of the windowed list", () => {
  render(<Harness count={20} virtualize={false} />);
  const row = document.getElementById("thing-7") as HTMLElement;
  expect(row).toHaveClass("vt-vlist-row");
  expect(row).toHaveAttribute("role", "option");
  expect(row).toHaveAttribute("aria-setsize", "20");
  expect(row).toHaveAttribute("aria-posinset", "8");
  expect(row).toHaveAttribute("aria-selected", "false");
  expect(document.getElementById("thing-2")).toHaveAttribute("aria-selected", "true");
  expect(box()).toHaveClass("vt-vlist");
  expect(box()).toHaveAttribute("tabindex", "0");
});

test("the active row is mounted once", () => {
  render(<Harness count={20} virtualize={false} />);
  fireEvent.keyDown(box(), { key: "End" });
  expect(box()).toHaveAttribute("aria-activedescendant", "thing-19");
  expect(document.querySelectorAll("#thing-19")).toHaveLength(1);
  expect(screen.getAllByRole("option")).toHaveLength(20);
});

test("an active row far outside the first window is still mounted exactly once", () => {
  render(<Harness count={300} virtualize={false} initialActive={250} />);
  expect(document.querySelectorAll("#thing-250")).toHaveLength(1);
  expect(screen.getAllByRole("option")).toHaveLength(300);
});

test("keys move the active row, PageUp and PageDown by a page, Enter and click activate", () => {
  const onActivate = mock((_i: number) => {});
  render(<Harness count={40} virtualize={false} onActivate={onActivate} />);
  fireEvent.keyDown(box(), { key: "ArrowDown" });
  expect(box()).toHaveAttribute("aria-activedescendant", "thing-0");
  fireEvent.keyDown(box(), { key: "PageDown" });
  expect(box()).toHaveAttribute("aria-activedescendant", "thing-5");
  fireEvent.keyDown(box(), { key: "PageUp" });
  expect(box()).toHaveAttribute("aria-activedescendant", "thing-0");
  fireEvent.keyDown(box(), { key: "Enter" });
  expect(onActivate).toHaveBeenLastCalledWith(0);
  fireEvent.click(screen.getByText("Row 30"));
  expect(onActivate).toHaveBeenLastCalledWith(30);
});

test("an empty listbox is not a tab stop, a listbox with rows is", () => {
  const { unmount } = render(<Harness count={0} virtualize={false} />);
  expect(box()).toHaveAttribute("tabindex", "-1");
  unmount();
  render(<Harness count={1} virtualize={false} />);
  expect(box()).toHaveAttribute("tabindex", "0");
});

test("an empty windowed listbox is not a tab stop either", () => {
  render(<Harness count={0} />);
  expect(box()).toHaveAttribute("tabindex", "-1");
});

test("switching virtualize on a mounted list keeps the same scroller element", () => {
  const { rerender } = render(<Harness count={300} virtualize={false} />);
  const before = box();
  rerender(<Harness count={300} virtualize />);
  expect(box()).toBe(before);
  expect(screen.getAllByRole("option").length).toBeLessThan(300);
});

test("a log with virtualize false mounts every line", () => {
  render(
    <VirtualList
      role="log"
      label="Lines"
      count={120}
      rowHeight="row-sm"
      renderRow={(i) => <span>Line {i}</span>}
      getRowId={(i) => `line-${i}`}
      observeSize={sized(100)}
      virtualize={false}
    />,
  );
  expect(screen.getAllByRole("listitem")).toHaveLength(120);
});
