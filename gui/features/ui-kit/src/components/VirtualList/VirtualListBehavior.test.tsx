import { afterEach, expect, test } from "bun:test";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { ROW_HEIGHT_PX } from "../../model/rowHeights.ts";
import { VirtualList } from "./VirtualList.tsx";

afterEach(cleanup);

// Review tests for WI-2: selection semantics, follow-end robustness, window edge cases.

const RH = ROW_HEIGHT_PX["row-md"];
const LH = ROW_HEIGHT_PX["row-sm"];
const VIEWPORT = RH * 10;
const sized = (h: number) => (_el: Element, onSize: (h: number) => void) => {
  onSize(h);
  return () => {};
};

function box(
  active: number | null,
  selected: number | null | undefined,
  count = 100,
): ReactElement {
  return (
    <VirtualList
      role="listbox"
      label="Entities"
      count={count}
      rowHeight="row-md"
      renderRow={(i) => <span>Row {i}</span>}
      getRowId={(i) => `ent-${i}`}
      observeSize={sized(VIEWPORT)}
      activeIndex={active}
      {...(selected === undefined ? {} : { selectedIndex: selected })}
      onActiveIndexChange={() => {}}
      onActivate={() => {}}
    />
  );
}

test("every option states aria-selected; only the selected one is true", () => {
  render(box(2, 5));
  const options = screen.getAllByRole("option");
  for (const o of options) expect(o.getAttribute("aria-selected")).toMatch(/^(true|false)$/);
  const selected = options.filter((o) => o.getAttribute("aria-selected") === "true");
  expect(selected).toHaveLength(1);
  expect(selected[0]).toHaveAttribute("id", "ent-5");
});

test("active and selected are independent", () => {
  render(box(2, 5));
  expect(document.getElementById("ent-2")).toHaveAttribute("data-active");
  expect(document.getElementById("ent-2")).toHaveAttribute("aria-selected", "false");
  expect(document.getElementById("ent-5")).not.toHaveAttribute("data-active");
});

test("without selectedIndex no option is selected", () => {
  render(box(null, undefined));
  expect(
    screen.getAllByRole("option").every((o) => o.getAttribute("aria-selected") === "false"),
  ).toBe(true);
});

test("the default overscan is 8 rows on each side", () => {
  render(box(null, null, 1000));
  expect(screen.getAllByRole("option")).toHaveLength(10 + 8);
});

test("the active row stays mounted when it is scrolled out of the window", () => {
  render(box(3, null, 10_000));
  const el = screen.getByRole("listbox");
  el.scrollTop = RH * 5000;
  fireEvent.scroll(el);
  expect(document.getElementById("ent-3")).toBeTruthy();
  expect(el).toHaveAttribute("aria-activedescendant", "ent-3");
  expect(document.getElementById("ent-5000")).toBeTruthy();
});

// --- log -----------------------------------------------------------------------------------

function log(count: number, observe = sized(VIEWPORT)): ReactElement {
  return (
    <VirtualList
      role="log"
      label="Event log"
      count={count}
      rowHeight="row-sm"
      renderRow={(i) => <span>Line {i}</span>}
      getRowId={(i) => `log-${i}`}
      observeSize={observe}
      followEnd
    />
  );
}
const endOf = (count: number) => count * LH - VIEWPORT;

test("a log is not a live region and exposes list semantics", () => {
  render(log(20));
  const el = screen.getByRole("log");
  expect(el).toHaveAttribute("aria-live", "off");
  expect(screen.getByRole("list")).toBeTruthy();
  const items = screen.getAllByRole("listitem");
  expect(items[0]).toHaveAttribute("aria-setsize", "20");
  expect(items[0]).toHaveAttribute("aria-posinset", "1");
});

test("followEnd survives a size that arrives late and a scroll event before it", () => {
  let report: (h: number) => void = () => {};
  const late = (_el: Element, onSize: (h: number) => void) => {
    report = onSize;
    return () => {};
  };
  const { rerender } = render(log(500, late));
  const el = screen.getByRole("log");
  // A real browser clamps the mount scrollTo(count * rh) to scrollHeight - clientHeight and fires
  // scroll before ResizeObserver reports; happy-dom does not clamp, so set that value by hand.
  el.scrollTop = endOf(500);
  fireEvent.scroll(el);
  act(() => report(VIEWPORT));
  expect(el.scrollTop).toBe(endOf(500));
  rerender(log(520, late));
  expect(el.scrollTop).toBe(endOf(520));
});

test("two rows from the end stops following, one row keeps following", () => {
  const { rerender } = render(log(500));
  const el = screen.getByRole("log");
  el.scrollTop = endOf(500) - LH;
  fireEvent.scroll(el);
  rerender(log(510));
  expect(el.scrollTop).toBe(endOf(510));
  el.scrollTop = endOf(510) - 2 * LH;
  fireEvent.scroll(el);
  rerender(log(520));
  expect(el.scrollTop).toBe(endOf(510) - 2 * LH);
});
