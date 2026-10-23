// WI-10 EntityListPanel, written before the implementation. Markup and behaviour contracts (ASM-937):
// - a `VirtualList role="listbox"` named "Entities"; rows are presentational content (ASM-880): one `option`
//   per row, no button or option nested inside it; the row text carries the affiliation word (sr-only), the
//   name or `#id`, `#id` beside a name, the template, the status word and a "Remote" badge for remote entities;
// - a search field "Search entities" and a group "Filters" of toggle chips (aria-pressed): one chip per force
//   present in the frame (its text is the force), "Owned" and "Remote";
// - a polite status "N of M entities" under the filters;
// - the row model is rebuilt at most every 250 ms (leading edge on mount, trailing edge for later frames);
//   search and chip changes apply at once;
// - viewport height comes from ResizeObserver (faked here, no layout in happy-dom).
import { afterEach, beforeEach, describe, expect, jest, test } from "bun:test";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Profiler } from "react";
import {
  FakeEntitySource,
  frameOf,
  installResizeObserver,
  summary,
} from "../../inspectorTesting.tsx";
import { EntityListPanel, type EntityListPanelProps } from "./EntityListPanel.tsx";

const VIEWPORT = 400;
let restoreObserver: () => void;
beforeEach(() => {
  restoreObserver = installResizeObserver(VIEWPORT);
});
afterEach(() => {
  cleanup();
  jest.useRealTimers();
  restoreObserver();
});

const friend = () => "friend" as const;
const byForce = (force: string | null | undefined) =>
  force === "red" ? ("hostile" as const) : ("friend" as const);

function mountList(source: FakeEntitySource, props: Partial<EntityListPanelProps> = {}) {
  const onSelect = props.onSelect ?? (() => {});
  return render(
    <EntityListPanel
      source={source}
      selectedId={null}
      affiliationOf={friend}
      {...props}
      onSelect={onSelect}
    />,
  );
}
const sourceWith = (...entities: ReturnType<typeof summary>[]) => {
  const s = new FakeEntitySource();
  s.frame = frameOf(entities);
  return s;
};
const listbox = () => screen.getByRole("listbox", { name: "Entities" });
const options = () => within(listbox()).queryAllByRole("option");
const optionNames = () => options().map((o) => o.textContent ?? "");
const chip = (name: string) => screen.getByRole("button", { name });

const SAMPLE = [
  summary(1, { name: "Alpha", template: "tank", status: "moving", force: "blue" }),
  summary(2, { name: "Bravo", template: "truck", force: "red" }),
  summary(3, {
    template: "tank",
    force: "red",
    ownership: "remote",
    remote_source: "peer-a",
    status: "idle",
  }),
];

describe("EntityListPanel rows", () => {
  test("is a listbox named Entities with one option per entity, in frame order", () => {
    mountList(sourceWith(...SAMPLE));
    expect(options()).toHaveLength(3);
    expect(optionNames()[0]).toContain("Alpha");
    expect(optionNames()[1]).toContain("Bravo");
    expect(optionNames()[2]).toContain("#3");
  });

  test("a named entity shows its name and #id; an unnamed one is named #id", () => {
    mountList(sourceWith(...SAMPLE));
    const [alpha, , unnamed] = options();
    expect(alpha).toHaveTextContent("Alpha");
    expect(alpha).toHaveTextContent("#1");
    expect(unnamed?.textContent?.match(/#3/g)).toHaveLength(1); // not repeated
  });

  test("shows the template as meta and the status as a badge", () => {
    mountList(sourceWith(...SAMPLE));
    const [alpha] = options();
    expect(alpha).toHaveTextContent("tank");
    expect(alpha).toHaveTextContent("Moving"); // the status word is capitalised for display (WI-10 review m10)
  });

  test("a remote entity has a Remote badge; an owned one does not", () => {
    mountList(sourceWith(...SAMPLE));
    const rows = options();
    expect(rows[2]).toHaveTextContent("Remote");
    expect(rows[0]).not.toHaveTextContent("Remote");
  });

  test("the affiliation word comes from affiliationOf(force) and is in the option's text", () => {
    mountList(sourceWith(...SAMPLE), { affiliationOf: byForce });
    const rows = options();
    expect(rows[0]).toHaveTextContent("Friend");
    expect(rows[1]).toHaveTextContent("Hostile");
  });

  test("rows are presentational: no button or nested option inside the listbox (ASM-880)", () => {
    mountList(sourceWith(...SAMPLE));
    expect(within(listbox()).queryAllByRole("button")).toHaveLength(0);
    expect(listbox().querySelectorAll('[role="option"] [role="option"]')).toHaveLength(0);
  });

  test("the selected entity's option is aria-selected, the others are not", () => {
    mountList(sourceWith(...SAMPLE), { selectedId: 2 });
    expect(options().map((o) => o.getAttribute("aria-selected"))).toEqual([
      "false",
      "true",
      "false",
    ]);
  });

  test("an empty frame shows the empty message instead of rows", () => {
    mountList(new FakeEntitySource());
    expect(options()).toHaveLength(0);
    expect(screen.getByText(/no entities/i)).toBeInTheDocument();
  });
});

describe("EntityListPanel selection", () => {
  test("clicking a row calls onSelect with its id", () => {
    const picked: (number | null)[] = [];
    mountList(sourceWith(...SAMPLE), { onSelect: (id) => picked.push(id) });
    fireEvent.click(options()[1] as HTMLElement);
    expect(picked).toEqual([2]);
  });

  test("the keyboard moves the active row and Enter selects it", () => {
    const picked: (number | null)[] = [];
    mountList(sourceWith(...SAMPLE), { onSelect: (id) => picked.push(id) });
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    fireEvent.keyDown(listbox(), { key: "Enter" });
    expect(picked).toEqual([2]);
  });

  test("a new selectedId prop moves aria-selected without calling onSelect", () => {
    const picked: (number | null)[] = [];
    const source = sourceWith(...SAMPLE);
    const view = mountList(source, { onSelect: (id) => picked.push(id) });
    view.rerender(
      <EntityListPanel
        source={source}
        selectedId={3}
        affiliationOf={friend}
        onSelect={(id) => picked.push(id)}
      />,
    );
    expect(options()[2]).toHaveAttribute("aria-selected", "true");
    expect(picked).toEqual([]);
  });
});

describe("EntityListPanel filters (ASM-805)", () => {
  test("has a search field and a group of filter chips: one per force present, Owned and Remote", () => {
    mountList(sourceWith(...SAMPLE));
    expect(screen.getByRole("textbox", { name: "Search entities" })).toBeInTheDocument();
    const group = screen.getByRole("group", { name: "Filters" });
    const names = within(group)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(names).toEqual(expect.arrayContaining(["blue", "red", "Owned", "Remote"]));
    for (const b of within(group).getAllByRole("button"))
      expect(b).toHaveAttribute("aria-pressed", "false");
  });

  test("typing filters by name, template and #id, case-insensitively, at once", () => {
    jest.useFakeTimers();
    mountList(sourceWith(...SAMPLE));
    const search = screen.getByRole("textbox", { name: "Search entities" });
    fireEvent.change(search, { target: { value: "BRA" } });
    expect(optionNames()).toHaveLength(1);
    expect(optionNames()[0]).toContain("Bravo");
    fireEvent.change(search, { target: { value: "tank" } });
    expect(optionNames()).toHaveLength(2);
    fireEvent.change(search, { target: { value: "#3" } });
    expect(optionNames()).toHaveLength(1);
  });

  test("a force chip keeps that force; two force chips are OR'd; the chip is pressed", () => {
    mountList(sourceWith(...SAMPLE));
    fireEvent.click(chip("red"));
    expect(chip("red")).toHaveAttribute("aria-pressed", "true");
    expect(optionNames()).toHaveLength(2);
    fireEvent.click(chip("blue"));
    expect(optionNames()).toHaveLength(3);
    fireEvent.click(chip("blue"));
    fireEvent.click(chip("red"));
    expect(optionNames()).toHaveLength(3);
  });

  test("Owned and Remote chips filter by ownership", () => {
    mountList(sourceWith(...SAMPLE));
    fireEvent.click(chip("Remote"));
    expect(optionNames()).toHaveLength(1);
    expect(optionNames()[0]).toContain("#3");
    fireEvent.click(chip("Remote"));
    fireEvent.click(chip("Owned"));
    expect(optionNames()).toHaveLength(2);
  });

  test("text and chips are AND'd", () => {
    mountList(sourceWith(...SAMPLE));
    fireEvent.click(chip("red"));
    fireEvent.change(screen.getByRole("textbox", { name: "Search entities" }), {
      target: { value: "tank" },
    });
    expect(optionNames()).toHaveLength(1);
    expect(optionNames()[0]).toContain("#3");
  });

  test("a status line says how many of how many entities are shown, politely", () => {
    mountList(sourceWith(...SAMPLE));
    const status = screen.getByText(/3 of 3 entities/);
    expect(status.closest("[aria-live]")).toHaveAttribute("aria-live", "polite");
    fireEvent.click(chip("red"));
    expect(screen.getByText(/2 of 3 entities/)).toBeInTheDocument();
  });

  test("a filter with no match shows a no-match message", () => {
    mountList(sourceWith(...SAMPLE));
    fireEvent.change(screen.getByRole("textbox", { name: "Search entities" }), {
      target: { value: "zzz" },
    });
    expect(options()).toHaveLength(0);
    expect(screen.getByText(/no entities match/i)).toBeInTheDocument();
  });
});

describe("EntityListPanel throttle (ASM-804, performance plan 2)", () => {
  // The status badge shows the capitalised word (review m10): "first" is displayed as "First".
  const frame = (status: string, extra = {}) =>
    frameOf([summary(1, { name: "Alpha", status }), summary(2, { name: "Bravo" })], extra);

  test("shows the frame already in the source at once", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = frame("first");
    mountList(source);
    expect(optionNames()[0]).toContain("First");
  });

  test("a frame that arrives within 250 ms of the last rebuild shows only after the interval", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = frame("first");
    mountList(source);
    act(() => {
      jest.advanceTimersByTime(100);
      source.push(frame("second"));
    });
    expect(optionNames()[0]).toContain("First");
    act(() => {
      jest.advanceTimersByTime(149);
    });
    expect(optionNames()[0]).toContain("First");
    act(() => {
      jest.advanceTimersByTime(1);
    });
    expect(optionNames()[0]).toContain("Second");
  });

  test("of many frames inside one interval the newest wins", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = frame("first");
    mountList(source);
    act(() => {
      for (let i = 0; i < 5; i++) source.push(frame(`s${i}`));
      jest.advanceTimersByTime(250);
    });
    expect(optionNames()[0]).toContain("S4");
    expect(optionNames()[0]).not.toContain("S3");
  });

  test("100 frames in one second cost at most 5 commits", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = frame("first");
    let commits = 0;
    render(
      <Profiler id="list" onRender={() => commits++}>
        <EntityListPanel
          source={source}
          selectedId={null}
          affiliationOf={friend}
          onSelect={() => {}}
        />
      </Profiler>,
    );
    const mounted = commits;
    for (let i = 0; i < 100; i++) {
      act(() => {
        source.push(frame(`s${i}`));
        jest.advanceTimersByTime(10);
      });
    }
    expect(commits - mounted).toBeLessThanOrEqual(5);
    expect(commits - mounted).toBeGreaterThanOrEqual(1); // the list does follow the stream
  });

  test("unmounting unsubscribes from the source and cancels the pending rebuild", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = frame("first");
    const view = mountList(source);
    act(() => {
      source.push(frame("second"));
    });
    view.unmount();
    expect(source.listeners.size).toBe(0);
    expect(() => act(() => void jest.advanceTimersByTime(500))).not.toThrow();
  });
});

describe("EntityListPanel with 10,000 entities (NFR-1)", () => {
  const big = () =>
    frameOf(Array.from({ length: 10_000 }, (_, i) => summary(i + 1, { status: "moving" })));

  test("renders at most the visible rows plus twice the overscan", () => {
    const source = new FakeEntitySource();
    source.frame = big();
    mountList(source);
    const rows = options().length;
    expect(rows).toBeGreaterThan(0);
    expect(rows).toBeLessThanOrEqual(Math.ceil(VIEWPORT / 28) + 1 + 2 * 8);
    expect(options()[0]).toHaveAttribute("aria-setsize", "10000");
  });

  test("the commit count is constant while 100 identical-content frames of 10,000 entities arrive", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = big();
    let commits = 0;
    render(
      <Profiler id="list" onRender={() => commits++}>
        <EntityListPanel
          source={source}
          selectedId={5_000}
          affiliationOf={friend}
          onSelect={() => {}}
        />
      </Profiler>,
    );
    const mounted = commits;
    for (let i = 0; i < 100; i++) {
      act(() => {
        source.push({ ...big(), seq: i + 1 });
        jest.advanceTimersByTime(10);
      });
    }
    expect(commits - mounted).toBeLessThanOrEqual(5);
  }, 30_000);
});

describe("EntityListPanel review follow-ups (WI-10 code review)", () => {
  const frames = {
    base: () =>
      frameOf([
        summary(1, { name: "Alpha", force: "blue" }),
        summary(2, { name: "Bravo", force: "red" }),
        summary(3, { name: "Charlie", force: "blue" }),
      ]),
  };
  const activeId = () => listbox().getAttribute("aria-activedescendant");

  test("M2: rows are two-line rows of height row-lg (56 px), not row-md", () => {
    mountList(sourceWith(...SAMPLE));
    const [first] = options();
    expect((first as HTMLElement).style.height).toBe("56px"); // ROW_HEIGHT_PX["row-lg"]
    const sizer = listbox().querySelector<HTMLElement>("[data-virtual-sizer]");
    expect(sizer?.style.height).toBe(`${SAMPLE.length * 56}px`);
  });

  test("m10: the status Badge word is capitalised", () => {
    mountList(sourceWith(summary(1, { status: "moving" }), summary(2, { status: "idle" })));
    expect(options()[0]).toHaveTextContent("Moving");
    expect(options()[1]).toHaveTextContent("Idle");
    expect(options()[0]?.textContent).not.toContain("moving");
  });

  test("m1: an empty source at mount shows the first frame at once, even 50 ms after mount", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    mountList(source);
    expect(options()).toHaveLength(0);
    act(() => {
      jest.advanceTimersByTime(50);
      source.push(frameOf([summary(1, { name: "Alpha" })]));
    });
    expect(optionNames()[0]).toContain("Alpha"); // not after 250 ms: nothing had been shown yet
  });

  test("m1: after the first frame the 250 ms interval applies as before", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    mountList(source);
    act(() => {
      source.push(frameOf([summary(1, { name: "Alpha" })]));
    });
    act(() => {
      jest.advanceTimersByTime(100);
      source.push(frameOf([summary(1, { name: "Alpha" }), summary(2, { name: "Bravo" })]));
    });
    expect(options()).toHaveLength(1);
    act(() => void jest.advanceTimersByTime(150));
    expect(options()).toHaveLength(2);
  });

  test("m7: a pressed force chip whose force left the frame stays visible and can be released", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = frames.base();
    mountList(source);
    fireEvent.click(chip("red"));
    expect(optionNames()).toHaveLength(1);
    act(() => {
      source.push(
        frameOf([
          summary(1, { name: "Alpha", force: "blue" }),
          summary(3, { name: "Charlie", force: "blue" }),
        ]),
      );
      jest.advanceTimersByTime(250);
    });
    expect(chip("red")).toHaveAttribute("aria-pressed", "true"); // not gone: the user can still see why the list is empty
    expect(options()).toHaveLength(0);
    expect(screen.getByText(/no entities match/i)).toBeInTheDocument();
    fireEvent.click(chip("red"));
    expect(options()).toHaveLength(2);
  });

  test("m7: an unpressed chip of a force that left the frame is gone", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = frames.base();
    mountList(source);
    expect(screen.queryByRole("button", { name: "red" })).not.toBeNull();
    act(() => {
      source.push(frameOf([summary(1, { name: "Alpha", force: "blue" })]));
      jest.advanceTimersByTime(250);
    });
    expect(screen.queryByRole("button", { name: "red" })).toBeNull();
  });

  test("m8: the active row follows its entity when a rebuild inserts rows before it", () => {
    jest.useFakeTimers();
    const source = new FakeEntitySource();
    source.frame = frames.base();
    mountList(source);
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    expect(activeId()).toMatch(/-2$/);
    act(() => {
      source.push(frameOf([summary(0, { name: "Zero" }), ...frames.base().entities]));
      jest.advanceTimersByTime(250);
    });
    expect(activeId()).toMatch(/-2$/);
    const row = activeId() ? document.getElementById(activeId() as string) : null;
    expect(row).toHaveTextContent("Bravo");
  });

  test("m8: the active row follows its entity when a filter changes the rows before it", () => {
    const source = new FakeEntitySource();
    source.frame = frames.base();
    mountList(source);
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    expect(activeId()).toMatch(/-2$/);
    fireEvent.click(chip("red")); // only Bravo remains, at index 0
    expect(activeId()).toMatch(/-2$/);
    fireEvent.click(chip("red"));
    expect(activeId()).toMatch(/-2$/);
  });

  test("m8: when the active entity is filtered out there is no active descendant", () => {
    const source = new FakeEntitySource();
    source.frame = frames.base();
    mountList(source);
    fireEvent.keyDown(listbox(), { key: "ArrowDown" });
    expect(activeId()).toMatch(/-1$/);
    fireEvent.click(chip("red")); // Alpha (1) is blue
    expect(activeId()).toBeNull();
  });
});
