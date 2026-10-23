// Review follow-ups: status role and throttle (m1), text filter under load (m4), aria-controls (n2).
import { afterEach, beforeEach, expect, jest, test } from "bun:test";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { Profiler } from "react";
import { FakeJournal, installResizeObserver, mount, rec } from "../../logTesting.tsx";
import { LogView } from "./LogView.tsx";

let restore: () => void;
beforeEach(() => {
  restore = installResizeObserver(280);
});
afterEach(() => {
  cleanup();
  restore();
  jest.useRealTimers();
});

test("the count line is a polite status region", () => {
  mount(<LogView label="Log" />, new FakeJournal([rec(1)]));
  expect(screen.getByRole("status")).toHaveTextContent("1 of 1 records");
});

test("arriving records update the count at most once per second", () => {
  jest.useFakeTimers();
  const journal = new FakeJournal([rec(1)]);
  mount(<LogView label="Log" />, journal);
  act(() => journal.push(rec(2), rec(3)));
  expect(screen.getByRole("status")).toHaveTextContent("1 of 1 records");
  act(() => jest.advanceTimersByTime(1000));
  expect(screen.getByRole("status")).toHaveTextContent("3 of 3 records");
});

test("with a text filter and 50,000 records, still at most two commits per notification", () => {
  const big = Array.from({ length: 50_000 }, (_, i) => rec(i + 1, { received_at: i }));
  const journal = new FakeJournal(big);
  let commits = 0;
  mount(
    <Profiler id="log" onRender={() => commits++}>
      <LogView label="Log" />
    </Profiler>,
    journal,
  );
  fireEvent.change(screen.getByRole("textbox", { name: "Search log" }), {
    target: { value: "message 4" },
  });
  const before = commits;
  for (let i = 0; i < 100; i++)
    act(() => journal.push(rec(50_001 + i, { received_at: 50_000 + i })));
  expect(commits - before).toBeLessThanOrEqual(200);
});

test("the compact Filters button controls the filters while they are open", () => {
  mount(<LogView label="Log" compact />, new FakeJournal([rec(1)]));
  const toggle = screen.getByRole("button", { name: "Filters" });
  expect(toggle).not.toHaveAttribute("aria-controls");
  fireEvent.click(toggle);
  const id = toggle.getAttribute("aria-controls");
  expect(id).toBeTruthy();
  const controlled = document.getElementById(id ?? "");
  expect(controlled).toContainElement(screen.getByRole("textbox", { name: "Search log" }));
});
