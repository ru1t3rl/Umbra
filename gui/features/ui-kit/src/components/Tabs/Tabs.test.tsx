import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { type TabItem, Tabs } from "./Tabs.tsx";

afterEach(cleanup);

const items: TabItem[] = [
  { value: "entities", label: "Entities", count: 12 },
  { value: "events", label: "Events" },
  { value: "orders", label: "Orders", count: 0 },
];

function Harness({ initial = "entities" }: { initial?: string }) {
  const [v, setV] = useState(initial);
  return <Tabs items={items} value={v} onChange={setV} />;
}

test("tablist with tabs; the value is aria-selected", () => {
  render(<Harness />);
  expect(screen.getByRole("tablist")).toHaveClass("vt-tabs");
  const tabs = screen.getAllByRole("tab");
  expect(tabs).toHaveLength(3);
  expect(tabs.every((t) => t.classList.contains("vt-tab"))).toBe(true);
  expect(screen.getByRole("tab", { name: /Entities/ })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("tab", { name: "Events" })).toHaveAttribute("aria-selected", "false");
});

test("count renders in mono, including zero; absent when not given", () => {
  const { container } = render(<Harness />);
  const counts = [...container.querySelectorAll(".vt-tab-count")].map((c) => c.textContent);
  expect(counts).toEqual(["12", "0"]);
});

test("a count above 9999 is shown as 9999+", () => {
  const { container } = render(
    <Tabs items={[{ value: "a", label: "A", count: 12000 }]} value="a" />,
  );
  expect(container.querySelector(".vt-tab-count")?.textContent).toBe("9999+");
});

test("click selects through onChange", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click(screen.getByRole("tab", { name: "Events" }));
  expect(screen.getByRole("tab", { name: "Events" })).toHaveAttribute("aria-selected", "true");
});

test("roving tabindex: only the selected tab is in the tab order", () => {
  render(<Harness initial="events" />);
  expect(screen.getByRole("tab", { name: "Events" })).toHaveAttribute("tabindex", "0");
  expect(screen.getByRole("tab", { name: /Entities/ })).toHaveAttribute("tabindex", "-1");
});

test("if the value matches no item the first tab stays reachable", () => {
  render(<Tabs items={items} value="missing" />);
  expect(screen.getAllByRole("tab")[0]).toHaveAttribute("tabindex", "0");
});

test("arrow keys, Home and End move focus and selection, wrapping around", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await user.tab();
  expect(screen.getByRole("tab", { name: /Entities/ })).toHaveFocus();
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("tab", { name: "Events" })).toHaveFocus();
  expect(screen.getByRole("tab", { name: "Events" })).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{End}");
  expect(screen.getByRole("tab", { name: /Orders/ })).toHaveFocus();
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("tab", { name: /Entities/ })).toHaveFocus();
  await user.keyboard("{ArrowLeft}");
  expect(screen.getByRole("tab", { name: /Orders/ })).toHaveFocus();
  await user.keyboard("{Home}");
  expect(screen.getByRole("tab", { name: /Entities/ })).toHaveFocus();
  expect(screen.getByRole("tab", { name: /Entities/ })).toHaveAttribute("aria-selected", "true");
});

test("the tab name includes the count, separated by a space", () => {
  render(<Harness />);
  expect(screen.getByRole("tab", { name: "Entities 12" })).toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "Events" })).toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "Orders 0" })).toBeInTheDocument();
});

test("label names the tablist", () => {
  render(<Tabs items={items} value="events" label="Sidebar sections" />);
  expect(screen.getByRole("tablist", { name: "Sidebar sections" })).toBeInTheDocument();
});

test("id derives tab ids and aria-controls; without id neither is set", () => {
  const { rerender } = render(<Tabs items={items} value="events" id="side" />);
  const tab = screen.getByRole("tab", { name: "Events" });
  expect(tab.id).toBe("side-tab-events");
  expect(tab).toHaveAttribute("aria-controls", "side-panel-events");
  rerender(<Tabs items={items} value="events" />);
  const bare = screen.getByRole("tab", { name: "Events" });
  expect(bare).not.toHaveAttribute("id");
  expect(bare).not.toHaveAttribute("aria-controls");
});

test("className is appended", () => {
  render(<Tabs items={items} value="events" className="extra" />);
  expect(screen.getByRole("tablist")).toHaveClass("vt-tabs", "extra");
});
