// WI-11 LogView, written before the implementation. Contracts (ASM-944):
// - a `VirtualList role="log"` named by `label` (rows are `listitem`s of height row-sm, 28 px, ASM-835) that
//   follows the end until the user scrolls up; the text of a row is `formatRecord(record)`; warning and error
//   rows carry a Badge "Warning" / "Error" (loss records are warnings);
// - not compact: a textbox "Search log", a group "Level" of aria-pressed Chips "All" (default), "Warnings",
//   "Errors" (a minimum severity, one pressed at a time) and a group "Sources" of toggle Chips "Events", "Run",
//   "Connection", "GUI" and, only when the desktop app supplies an engine-output source, "Engine process";
//   nothing pressed in "Sources" means every source; search and chips apply at once;
// - a polite line "N of M records" ("No log records" / "No records match" when empty);
// - "Download" (always visible, disabled with nothing to save): a Blob of the filtered records as JSON Lines
//   (type application/x-ndjson) behind an object URL on an anchor with a `.jsonl` download name, URL revoked;
// - compact: the search field and both chip groups are hidden behind a button "Filters" (aria-expanded); the
//   filters stay in force while hidden;
// - the engine-output source comes from `useEngineOutputSource()` of `@umbra/desktop` (mocked here: undefined
//   in the web build); its records are merged with the journal's by arrival (model/mergeSources).
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import { Profiler } from "react";
import {
  engineOutput,
  eventRec,
  FakeJournal,
  installResizeObserver,
  mount,
  rec,
} from "../../logTesting.tsx";

mock.module("@umbra/desktop", () => ({ useEngineOutputSource: () => engineOutput.current }));
const { LogView } = await import("./LogView.tsx");

const VIEWPORT = 280; // ten rows of 28 px
let restoreObserver: () => void;
beforeEach(() => {
  restoreObserver = installResizeObserver(VIEWPORT);
  engineOutput.current = undefined;
});
afterEach(() => {
  cleanup();
  restoreObserver();
  engineOutput.current = undefined;
});

const log = () => screen.getByRole("log", { name: "Log" });
const rows = () => within(log()).queryAllByRole("listitem");
const rowTexts = () => rows().map((r) => r.textContent ?? "");
const chip = (name: string) => screen.getByRole("button", { name });
const search = () => screen.getByRole("textbox", { name: "Search log" });
const numbered = (n: number) => Array.from({ length: n }, (_, i) => rec(i + 1));

describe("LogView rows", () => {
  test("is a log named by its label, one listitem per visible record", () => {
    mount(<LogView label="Log" />, new FakeJournal(numbered(3)));
    expect(rows()).toHaveLength(3);
  });

  test("rows show the formatted text: events by kind, other sources by message", () => {
    const journal = new FakeJournal([
      eventRec(1, { kind: "entity_created", entity_id: 12, template: "tank" }),
      eventRec(2, { kind: "fire", entity_id: 3, target_id: 7, weapon: "rifle" }),
      eventRec(3, { kind: "custom", detail: { kind: "wander.leader" } }),
      eventRec(4, { kind: "weather_changed" }),
      rec(5, { source: "connection", kind: "connected", message: "Connected to the engine" }),
    ]);
    mount(<LogView label="Log" />, journal);
    const text = rowTexts().join("\n");
    expect(text).toContain("Created #12 (tank)");
    expect(text).toContain("#3 fired at #7 (rifle)");
    expect(text).toContain("custom: wander.leader");
    expect(text).toContain("weather_changed");
    expect(text).toContain("Connected to the engine");
  });

  test("rows are row-sm (28 px) high", () => {
    mount(<LogView label="Log" />, new FakeJournal(numbered(3)));
    for (const row of rows()) expect(row.style.height).toBe("28px");
  });

  test("a new record arriving in the journal is shown", () => {
    const journal = new FakeJournal(numbered(2));
    mount(<LogView label="Log" />, journal);
    act(() => journal.push(rec(3, { message: "fresh" })));
    expect(rowTexts().join("\n")).toContain("fresh");
  });

  test("subscribes to the journal and unsubscribes on unmount", () => {
    const journal = new FakeJournal(numbered(2));
    const view = mount(<LogView label="Log" />, journal);
    expect(journal.listenerCount).toBe(1);
    view.unmount();
    expect(journal.listenerCount).toBe(0);
  });
});

describe("LogView follows the end (ADR 0072)", () => {
  test("with more records than fit it shows the newest at the bottom, not the oldest", () => {
    mount(<LogView label="Log" />, new FakeJournal(numbered(100)));
    expect(screen.queryByText("message 100")).not.toBeNull();
    expect(screen.queryByText("message 1")).toBeNull();
  });

  test("keeps following while records arrive", () => {
    const journal = new FakeJournal(numbered(100));
    mount(<LogView label="Log" />, journal);
    act(() => journal.push(rec(101), rec(102)));
    expect(screen.queryByText("message 102")).not.toBeNull();
    expect(screen.queryByText("message 1")).toBeNull();
  });

  test("stops following after the user scrolls up, and records arriving do not pull the view down", () => {
    const journal = new FakeJournal(numbered(100));
    mount(<LogView label="Log" />, journal);
    act(() => {
      log().scrollTop = 0;
      fireEvent.scroll(log());
    });
    expect(screen.queryByText("message 1")).not.toBeNull();
    act(() => journal.push(rec(101)));
    expect(screen.queryByText("message 1")).not.toBeNull();
    expect(screen.queryByText("message 101")).toBeNull();
  });
});

describe("LogView levels and loss records", () => {
  const mixed = () =>
    new FakeJournal([
      rec(1, { message: "plain info" }),
      rec(2, { level: "warning", kind: "paused", message: "Run paused" }),
      rec(3, { level: "error", kind: "stopped", message: "Run stopped (plugin_fault)" }),
      rec(4, {
        source: "event",
        level: "warning",
        kind: "events_truncated",
        message: "Events were lost beyond the per-frame cap",
      }),
      rec(5, {
        source: "event",
        level: "warning",
        kind: "frames_dropped",
        message: "The server dropped 2 frame(s) for this client",
      }),
    ]);

  test("warning and error rows carry a Warning or Error badge; info rows carry none", () => {
    mount(<LogView label="Log" />, mixed());
    const [info, paused, stopped, truncated, dropped] = rows();
    expect(info).not.toHaveTextContent(/Warning|Error/);
    expect(paused).toHaveTextContent("Warning");
    expect(stopped).toHaveTextContent("Error");
    expect(truncated).toHaveTextContent("Warning");
    expect(dropped).toHaveTextContent("Warning");
  });

  test("loss records show their message", () => {
    mount(<LogView label="Log" />, mixed());
    expect(screen.queryByText(/Events were lost beyond the per-frame cap/)).not.toBeNull();
    expect(screen.queryByText(/The server dropped 2 frame/)).not.toBeNull();
  });

  test("the Warnings chip keeps warnings and errors, Errors keeps errors, All keeps everything", () => {
    mount(<LogView label="Log" />, mixed());
    expect(chip("All")).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(chip("Warnings"));
    expect(rows()).toHaveLength(4);
    expect(chip("Warnings")).toHaveAttribute("aria-pressed", "true");
    expect(chip("All")).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(chip("Errors"));
    expect(rows()).toHaveLength(1);
    expect(chip("Warnings")).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(chip("All"));
    expect(rows()).toHaveLength(5);
  });
});

describe("LogView filters", () => {
  const sources = () =>
    new FakeJournal([
      rec(1, { source: "run", message: "run one" }),
      rec(2, { source: "connection", kind: "connected", message: "Connected to the engine" }),
      eventRec(3, { kind: "fire", entity_id: 3, target_id: 7, weapon: "rifle" }),
      rec(4, { source: "gui", kind: "note", message: "gui note" }),
      rec(5, { source: "run", message: "run two" }),
    ]);

  test("has a search field and the Level and Sources groups with their chips", () => {
    mount(<LogView label="Log" />, sources());
    expect(search()).toBeInTheDocument();
    const level = screen.getByRole("group", { name: "Level" });
    expect(
      within(level)
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual(["All", "Warnings", "Errors"]);
    const group = screen.getByRole("group", { name: "Sources" });
    expect(
      within(group)
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual(["Events", "Run", "Connection", "GUI"]);
    for (const b of within(group).getAllByRole("button"))
      expect(b).toHaveAttribute("aria-pressed", "false");
  });

  test("a source chip keeps that source; two chips are OR'd; releasing shows all again", () => {
    mount(<LogView label="Log" />, sources());
    fireEvent.click(chip("Run"));
    expect(rows()).toHaveLength(2);
    expect(chip("Run")).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(chip("Events"));
    expect(rows()).toHaveLength(3);
    fireEvent.click(chip("Run"));
    fireEvent.click(chip("Events"));
    expect(rows()).toHaveLength(5);
  });

  test("typing filters at once, case-insensitively, on the text the user reads", () => {
    mount(<LogView label="Log" />, sources());
    fireEvent.change(search(), { target: { value: "FIRED AT" } });
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toHaveTextContent("#3 fired at #7 (rifle)");
    fireEvent.change(search(), { target: { value: "" } });
    expect(rows()).toHaveLength(5);
  });

  test("level, source and text are AND'd", () => {
    const journal = new FakeJournal([
      rec(1, { source: "run", level: "warning", message: "alpha" }),
      rec(2, { source: "run", level: "info", message: "alpha" }),
      rec(3, { source: "gui", level: "warning", message: "alpha" }),
      rec(4, { source: "run", level: "warning", message: "beta" }),
    ]);
    mount(<LogView label="Log" />, journal);
    fireEvent.click(chip("Warnings"));
    fireEvent.click(chip("Run"));
    fireEvent.change(search(), { target: { value: "alpha" } });
    expect(rows()).toHaveLength(1);
  });

  test("a polite status says how many of how many records are shown", () => {
    mount(<LogView label="Log" />, sources());
    const status = screen.getByText(/5 of 5 records/);
    expect(status.closest("[aria-live]")).toHaveAttribute("aria-live", "polite");
    fireEvent.click(chip("Run"));
    expect(screen.getByText(/2 of 5 records/)).toBeInTheDocument();
  });

  test("an empty journal says so; a filter with no match says so", () => {
    const empty = mount(<LogView label="Log" />, new FakeJournal([]));
    expect(screen.getByText("No log records")).toBeInTheDocument();
    expect(rows()).toHaveLength(0);
    empty.unmount();
    mount(<LogView label="Log" />, sources());
    fireEvent.change(search(), { target: { value: "zzz" } });
    expect(screen.getByText("No records match")).toBeInTheDocument();
  });
});

describe("LogView download", () => {
  const realCreate = URL.createObjectURL;
  const realRevoke = URL.revokeObjectURL;
  const realClick = HTMLAnchorElement.prototype.click;
  let urlCounter = 0; // never reset: a revoke timer of an earlier test must not look like this test's
  let blobs: Blob[];
  let revoked: string[];
  let clicks: { href: string; download: string }[];
  beforeEach(() => {
    blobs = [];
    revoked = [];
    clicks = [];
    URL.createObjectURL = (blob: Blob | MediaSource) => {
      blobs.push(blob as Blob);
      urlCounter += 1;
      return `blob:umbra-log-${urlCounter}`;
    };
    URL.revokeObjectURL = (url: string) => {
      revoked.push(url);
    };
    HTMLAnchorElement.prototype.click = function (this: HTMLAnchorElement) {
      clicks.push({ href: this.href, download: this.download });
    };
  });
  afterEach(() => {
    URL.createObjectURL = realCreate;
    URL.revokeObjectURL = realRevoke;
    HTMLAnchorElement.prototype.click = realClick;
  });

  const records = () => [
    rec(1, { source: "run", message: "one" }),
    rec(2, { source: "connection", level: "warning", kind: "x", message: "two" }),
    eventRec(3, { kind: "fire", entity_id: 3, target_id: 7 }),
  ];

  test("saves all records as JSON Lines through an object URL on a .jsonl download", async () => {
    const list = records();
    mount(<LogView label="Log" />, new FakeJournal(list));
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    expect(blobs).toHaveLength(1);
    expect(blobs[0]?.type).toBe("application/x-ndjson");
    expect(await blobs[0]?.text()).toBe(list.map((r) => `${JSON.stringify(r)}\n`).join(""));
    expect(clicks).toHaveLength(1);
    expect(clicks[0]?.href).toBe(`blob:umbra-log-${urlCounter}`);
    expect(clicks[0]?.download).toMatch(/\.jsonl$/);
  });

  test("saves only the filtered records, in order", async () => {
    const list = records();
    mount(<LogView label="Log" />, new FakeJournal(list));
    fireEvent.click(chip("Warnings"));
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    expect(await blobs[0]?.text()).toBe(`${JSON.stringify(list[1])}\n`);
  });

  test("revokes the object URL after the click", async () => {
    mount(<LogView label="Log" />, new FakeJournal(records()));
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(revoked).toContain(`blob:umbra-log-${urlCounter}`);
  });

  test("with nothing to save the button is disabled and saves nothing", () => {
    mount(<LogView label="Log" />, new FakeJournal([]));
    const button = screen.getByRole("button", { name: "Download" });
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(blobs).toHaveLength(0);
  });

  test("is available in compact mode too", () => {
    mount(<LogView label="Log" compact />, new FakeJournal(records()));
    expect(screen.getByRole("button", { name: "Download" })).toBeEnabled();
  });
});

describe("LogView compact", () => {
  const list = () =>
    new FakeJournal([
      rec(1, { message: "info" }),
      rec(2, { level: "warning", kind: "paused", message: "warn" }),
    ]);

  test("hides the search field and the chip groups behind a Filters button", () => {
    mount(<LogView label="Log" compact />, list());
    expect(screen.queryByRole("textbox", { name: "Search log" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Level" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Sources" })).toBeNull();
    const toggle = screen.getByRole("button", { name: "Filters" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(rows()).toHaveLength(2);
  });

  test("the button shows and hides them", () => {
    mount(<LogView label="Log" compact />, list());
    const toggle = screen.getByRole("button", { name: "Filters" });
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(search()).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Level" })).toBeInTheDocument();
    fireEvent.click(toggle);
    expect(screen.queryByRole("textbox", { name: "Search log" })).toBeNull();
  });

  test("a filter chosen while open stays in force after the filters are hidden", () => {
    mount(<LogView label="Log" compact />, list());
    const toggle = screen.getByRole("button", { name: "Filters" });
    fireEvent.click(toggle);
    fireEvent.click(chip("Warnings"));
    fireEvent.click(toggle);
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toHaveTextContent("warn");
  });

  test("a full (not compact) view has no Filters button", () => {
    mount(<LogView label="Log" />, list());
    expect(screen.queryByRole("button", { name: "Filters" })).toBeNull();
    mount(<LogView label="Log" compact={false} />, list());
    expect(screen.queryByRole("button", { name: "Filters" })).toBeNull();
  });
});

describe("LogView with and without the desktop engine-output source", () => {
  test("web build: no Engine process chip", () => {
    mount(<LogView label="Log" />, new FakeJournal(numbered(2)));
    const group = screen.getByRole("group", { name: "Sources" });
    expect(within(group).queryByRole("button", { name: "Engine process" })).toBeNull();
  });

  test("desktop: the chip appears and engine lines are merged with the journal by arrival", () => {
    const journal = new FakeJournal([
      rec(1, { received_at: 100, message: "journal a" }),
      rec(2, { received_at: 300, message: "journal b" }),
    ]);
    engineOutput.current = new FakeJournal([
      rec(1, { source: "engine-process", kind: "line", received_at: 200, message: "engine line" }),
    ]);
    mount(<LogView label="Log" />, journal);
    expect(chip("Engine process")).toBeInTheDocument();
    expect(rowTexts().map((t) => t.replace(/Warning|Error/g, ""))).toEqual([
      expect.stringContaining("journal a"),
      expect.stringContaining("engine line"),
      expect.stringContaining("journal b"),
    ]);
  });

  test("desktop: the Engine process chip keeps only engine lines", () => {
    engineOutput.current = new FakeJournal([
      rec(1, { source: "engine-process", kind: "line", message: "engine line" }),
    ]);
    mount(<LogView label="Log" />, new FakeJournal(numbered(3)));
    fireEvent.click(chip("Engine process"));
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toHaveTextContent("engine line");
  });

  test("desktop: a new engine line is shown, and both sources are unsubscribed on unmount", () => {
    const engine = new FakeJournal([]);
    engineOutput.current = engine;
    const journal = new FakeJournal(numbered(1));
    const view = mount(<LogView label="Log" />, journal);
    act(() =>
      engine.push(rec(1, { source: "engine-process", kind: "line", message: "late line" })),
    );
    expect(rowTexts().join("\n")).toContain("late line");
    expect(engine.listenerCount).toBe(1);
    expect(journal.listenerCount).toBe(1);
    view.unmount();
    expect(engine.listenerCount).toBe(0);
    expect(journal.listenerCount).toBe(0);
  });

  test("the source appearing later (the desktop engine starts) adds the chip", () => {
    const journal = new FakeJournal(numbered(1));
    const view = mount(<LogView label="Log" />, journal);
    expect(screen.queryByRole("button", { name: "Engine process" })).toBeNull();
    engineOutput.current = new FakeJournal([]);
    view.rerender(<LogView label="Log" />);
    expect(screen.queryByRole("button", { name: "Engine process" })).not.toBeNull();
  });
});

describe("LogView with 50,000 records (NFR-1, performance plan 5)", () => {
  const big = (n: number) => Array.from({ length: n }, (_, i) => rec(i + 1, { received_at: i }));

  function arrivals(preloaded: number) {
    const journal = new FakeJournal(big(preloaded));
    let commits = 0;
    mount(
      <Profiler id="log" onRender={() => commits++}>
        <LogView label="Log" />
      </Profiler>,
      journal,
    );
    const mounted = commits;
    for (let i = 0; i < 100; i++) {
      act(() => {
        journal.push(
          ...Array.from({ length: 10 }, (_, k) =>
            rec(preloaded + i * 10 + k + 1, { received_at: preloaded + i * 10 + k }),
          ),
        );
      });
    }
    cleanup();
    return commits - mounted;
  }

  test("renders only the visible rows plus the overscan", () => {
    mount(<LogView label="Log" />, new FakeJournal(big(50_000)));
    expect(rows().length).toBeGreaterThan(0);
    expect(rows().length).toBeLessThanOrEqual(Math.ceil(VIEWPORT / 28) + 1 + 2 * 8);
    expect(screen.getByText(/50000 of 50000 records|50,000 of 50,000 records/)).toBeInTheDocument();
  });

  test("at most two commits per journal notification (the list's own window update is the second), and no more with 50,000 records than with 100", () => {
    const small = arrivals(100);
    const large = arrivals(50_000);
    expect(small).toBeLessThanOrEqual(200);
    expect(large).toBeLessThanOrEqual(200);
    expect(large).toBeLessThanOrEqual(small);
  }, 60_000);
});
