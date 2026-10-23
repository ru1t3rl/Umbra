// WI-10 EntityInspector, written before the implementation. Contracts (ASM-937):
// - `source` is the connection's `EntitySource`; the run state and session come from the same stream
//   (`useRunStatus`), the detail from `@umbra/data` (`GET /api/v1/entities/{id}`), all through a faked `fetch`;
// - a heading with the name or `#id`; Readouts (dt label, dd value + unit): Latitude, Longitude, Altitude,
//   Heading, Speed, Route, Status, Template, Motion model; "Source" only for a remote entity;
// - contacts: a list named "Contacts" of buttons, named from the latest frame (`#id` when unknown), click
//   selects them (ASM-807); "No contacts" when empty;
// - the detail is refetched every 1 s while the run state is running and at once whenever engine-stream's
//   `controlSeq` (`useControlSeq()`, counts `cause: "control"` frames before coalescing, ASM-938) changes,
//   never per tick frame (ASM-806); there is no run-state-change refetch: every command sends a control frame;
// - an entity missing from the frame: "No longer in the simulation", last values kept, `data-stale="true"` on
//   the inspector root, Remove disabled; a selected id never seen: the message and no readouts;
// - a session change calls onSelect(null) once; Remove only for owned entities (danger variant), with an
//   in-flight guard and the problem shown inline; "Center on map" only with `onCenter`, called with the id;
//   Close calls onClose.
import { afterEach, describe, expect, jest, test } from "bun:test";
import { readFileSync } from "node:fs";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import { Profiler, type ReactElement } from "react";
import {
  detailBody,
  type FakeEngine,
  fakeEngine,
  hello,
  jsonReply,
  mount,
  noContent,
  problemReply,
  streamFrame,
  wireEntity,
} from "../../inspectorTesting.tsx";
import { EntityInspector, type EntityInspectorProps } from "./EntityInspector.tsx";

let engine: FakeEngine | undefined;
afterEach(() => {
  cleanup();
  jest.useRealTimers();
  fake = false;
  engine?.restore();
  engine = undefined;
});

const friend = () => "friend" as const;
type Props = Partial<Omit<EntityInspectorProps, "source">>;
const inspectorFor =
  (props: Props = {}) =>
  (source: EntityInspectorProps["source"]): ReactElement => (
    <EntityInspector
      source={source}
      selectedId={12}
      onSelect={() => {}}
      onClose={() => {}}
      affiliationOf={friend}
      {...props}
    />
  );

const ALPHA = {
  name: "Alpha",
  template: "tank",
  force: "blue",
  status: "moving",
  route: { len: 5, next: 2 },
};
const frame = (seq: number, entities: unknown[], opts: Parameters<typeof streamFrame>[1] = {}) =>
  streamFrame(seq, { ...opts, entities });
const GET_12 = (e: FakeEngine) =>
  e.calls.filter((c) => c.method === "GET" && c.path === "/api/v1/entities/12");

/** Fake engine answering the detail of entity 12; other paths 404. */
function engineWith(detail: ReturnType<typeof detailBody> | Response = detailBody(12)) {
  engine = fakeEngine((call) => {
    if (call.method === "GET" && call.path === "/api/v1/entities/12")
      return "ok" in detail && "text" in detail ? detail : jsonReply(detail);
    if (call.method === "DELETE") return noContent();
    return problemReply(404, "entity_not_found", "Not found", "no such entity");
  });
  return engine;
}

let fake = false;
const fakeTimers = () => {
  jest.useFakeTimers();
  fake = true;
};
/**
 * Let pending promises settle: the replies resolve in microtasks, and TanStack Query hands results to React
 * through `setTimeout(0)`, so each round also turns that timer (advanced by hand under fake timers, one
 * macrotask yield otherwise; neither waits for real time).
 */
async function settle(rounds = 6) {
  for (let i = 0; i < rounds; i++) {
    await act(async () => {
      for (let k = 0; k < 10; k++) await Promise.resolve();
      if (fake) jest.advanceTimersByTime(0);
      else await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
}
const readout = (label: string): HTMLElement => {
  const dt = screen.getByText(label, { selector: "dt" });
  const dd = dt.parentElement?.querySelector("dd");
  if (!dd) throw new Error(`no value for ${label}`);
  return dd;
};

describe("EntityInspector readouts", () => {
  test("renders nothing without a selection", () => {
    engineWith();
    mount(inspectorFor({ selectedId: null }));
    expect(document.body.firstElementChild).toBeEmptyDOMElement();
    expect(screen.queryByRole("heading")).toBeNull();
    expect(GET_12(engine as FakeEngine)).toHaveLength(0);
  });

  test("shows the name as the heading and every stream readout", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(screen.getByRole("heading", { name: "Alpha" })).toBeInTheDocument();
    expect(readout("Latitude")).toHaveTextContent("10.012000");
    expect(readout("Longitude")).toHaveTextContent("20.012000");
    expect(readout("Altitude")).toHaveTextContent("12.5");
    expect(readout("Heading")).toHaveTextContent("90.0");
    expect(readout("Speed")).toHaveTextContent("5.0");
    expect(readout("Route")).toHaveTextContent("2 / 5");
    expect(readout("Status")).toHaveTextContent("moving");
    expect(readout("Template")).toHaveTextContent("tank");
  });

  test("an unnamed entity is headed #id", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12)])));
    expect(screen.getByRole("heading", { name: "#12" })).toBeInTheDocument();
  });

  test("the units are shown with the values", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(readout("Altitude")).toHaveTextContent(/12\.5\s*m\b/);
    expect(readout("Speed")).toHaveTextContent(/5\.0\s*m\/s/);
    expect(readout("Heading")).toHaveTextContent(/90\.0\s*°/);
  });

  test("the affiliation word comes from affiliationOf(force)", () => {
    engineWith();
    const seen: (string | null | undefined)[] = [];
    const r = mount(
      inspectorFor({
        affiliationOf: (force) => {
          seen.push(force);
          return "hostile";
        },
      }),
    );
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(document.body).toHaveTextContent("Hostile");
    expect(seen).toContain("blue");
  });

  test("readouts follow the stream: a new frame changes the position in the same flush", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    act(() => r.push(frame(2, [wireEntity(12, { ...ALPHA, lat_deg: 11.5, speed_mps: 9 })])));
    expect(readout("Latitude")).toHaveTextContent("11.500000");
    expect(readout("Speed")).toHaveTextContent("9.0");
  });

  test("an entity without a route shows no route progress value", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, { name: "Alpha" })])));
    expect(readout("Route")).toHaveTextContent("—");
  });
});

describe("EntityInspector detail from REST", () => {
  test("fetches the entity once and shows the motion model", async () => {
    engineWith(detailBody(12, { motion_model: "waypoint_follower" }));
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(1);
    expect(readout("Motion model")).toHaveTextContent("waypoint_follower");
  });

  test("makes no request without a selection", async () => {
    engineWith();
    const r = mount(inspectorFor({ selectedId: null }));
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    await settle();
    expect(engine?.calls.filter((c) => c.method === "GET")).toHaveLength(0);
  });

  test("lists the contacts as buttons named from the frame, #id when unknown; a click selects it", async () => {
    engineWith(detailBody(12, { contacts: [3, 40] }));
    const picked: (number | null)[] = [];
    const r = mount(inspectorFor({ onSelect: (id) => picked.push(id) }));
    act(() => r.push(frame(1, [wireEntity(3, { name: "Bravo" }), wireEntity(12, ALPHA)])));
    await settle();
    const list = screen.getByRole("list", { name: "Contacts" });
    const buttons = within(list).getAllByRole("button");
    expect(buttons.map((b) => b.textContent)).toEqual(["Bravo", "#40"]);
    fireEvent.click(buttons[0] as HTMLElement);
    fireEvent.click(buttons[1] as HTMLElement);
    expect(picked).toEqual([3, 40]);
  });

  test("with no contacts it says so", async () => {
    engineWith(detailBody(12, { contacts: [] }));
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    await settle();
    expect(screen.getByText("No contacts")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Contacts" })).toBeNull();
  });

  test("a failed detail request does not hide the stream readouts and says the detail is unavailable", async () => {
    engineWith(problemReply(404, "entity_not_found", "Not found", "no such entity"));
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    await settle();
    expect(readout("Latitude")).toHaveTextContent("10.012000");
    expect(screen.getByText(/details? (is |are )?unavailable/i)).toBeInTheDocument();
  });

  test("refetches every second while the run is running", async () => {
    fakeTimers();
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(1);
    await act(async () => void jest.advanceTimersByTime(1000));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(2);
    await act(async () => void jest.advanceTimersByTime(1000));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(3);
  });

  test("tick frames do not cause requests: 30 frames inside one second still make one request", async () => {
    fakeTimers();
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    await settle();
    for (let i = 2; i < 32; i++) {
      act(() => r.push(frame(i, [wireEntity(12, ALPHA)])));
      await act(async () => void jest.advanceTimersByTime(30));
    }
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(1);
  });

  test("does not poll while paused", async () => {
    fakeTimers();
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)], { state: "paused" })));
    await settle();
    const before = GET_12(engine as FakeEngine).length;
    expect(before).toBe(1);
    await act(async () => void jest.advanceTimersByTime(5000));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(before);
  });

  test("refetches once per control frame (engine-stream controlSeq), never on tick frames", async () => {
    fakeTimers();
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)], { state: "paused" })));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(1);
    for (let i = 2; i < 6; i++) {
      act(() => r.push(frame(i, [wireEntity(12, ALPHA)], { state: "paused", cause: "tick" })));
    }
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(1);
    act(() => r.push(frame(6, [wireEntity(12, ALPHA)], { state: "paused", cause: "control" })));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(2);
    act(() => r.push(frame(7, [wireEntity(12, ALPHA)], { state: "paused", cause: "control" })));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(3);
    act(() => r.push(frame(8, [wireEntity(12, ALPHA)], { state: "paused", cause: "tick" })));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(3);
  });

  test("a control frame while running refetches at once and the 1 s polling carries on", async () => {
    fakeTimers();
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    await settle();
    act(() => r.push(frame(2, [wireEntity(12, ALPHA)], { cause: "control" })));
    await settle();
    expect(GET_12(engine as FakeEngine)).toHaveLength(2);
    await act(async () => void jest.advanceTimersByTime(1000));
    await settle();
    expect(GET_12(engine as FakeEngine).length).toBeGreaterThanOrEqual(3);
  });
});

describe("EntityInspector remote entities", () => {
  test("shows the remote source and a Remote badge, and offers no Remove", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() =>
      r.push(
        frame(1, [wireEntity(12, { ...ALPHA, ownership: "remote", remote_source: "peer-a" })]),
      ),
    );
    expect(readout("Source")).toHaveTextContent("peer-a");
    expect(screen.getByText("Remote")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove entity" })).toBeNull();
  });

  test("an owned entity has no Source readout and no Remote badge", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(screen.queryByText("Source", { selector: "dt" })).toBeNull();
    expect(screen.queryByText("Remote")).toBeNull();
  });

  test("an entity of unknown ownership gets no Remove either (control gating tests ownership owned)", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, { ...ALPHA, ownership: "mystery" })])));
    expect(screen.queryByRole("button", { name: "Remove entity" })).toBeNull();
  });
});

describe("EntityInspector entity gone", () => {
  test("keeps the last values, says it is no longer in the simulation and marks the root stale", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(document.querySelector('[data-stale="true"]')).toBeNull();
    expect(screen.queryByText("No longer in the simulation")).toBeNull();
    act(() => r.push(frame(2, [wireEntity(3)])));
    expect(screen.getByText("No longer in the simulation")).toBeInTheDocument();
    expect(document.querySelector('[data-stale="true"]')).not.toBeNull();
    expect(screen.getByRole("heading", { name: "Alpha" })).toBeInTheDocument();
    expect(readout("Latitude")).toHaveTextContent("10.012000");
  });

  test("comes back to normal if the entity returns", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    act(() => r.push(frame(2, [])));
    act(() => r.push(frame(3, [wireEntity(12, { ...ALPHA, lat_deg: 12 })])));
    expect(screen.queryByText("No longer in the simulation")).toBeNull();
    expect(document.querySelector('[data-stale="true"]')).toBeNull();
    expect(readout("Latitude")).toHaveTextContent("12.000000");
  });

  test("Remove is disabled for an entity that is gone", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    act(() => r.push(frame(2, [])));
    const remove = screen.queryByRole("button", { name: "Remove entity" });
    if (remove) expect(remove).toBeDisabled();
  });

  test("a selected id the stream never had shows the message and no readouts", () => {
    engineWith();
    const r = mount(inspectorFor({ selectedId: 99 }));
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(screen.getByText("No longer in the simulation")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "#99" })).toBeInTheDocument();
    expect(screen.queryByText("Latitude", { selector: "dt" })).toBeNull();
  });
});

describe("EntityInspector session change", () => {
  test("a frame of a new session calls onSelect(null) once", () => {
    engineWith();
    const picked: (number | null)[] = [];
    const r = mount(inspectorFor({ onSelect: (id) => picked.push(id) }));
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)], { session: 1 })));
    act(() => r.push(frame(2, [wireEntity(12, ALPHA)], { session: 1 })));
    expect(picked).toEqual([]);
    act(() => r.push(frame(3, [wireEntity(12, ALPHA)], { session: 2 })));
    act(() => r.push(frame(4, [wireEntity(12, ALPHA)], { session: 2 })));
    expect(picked).toEqual([null]);
  });

  test("does not call onSelect on mount or for the first frame", () => {
    engineWith();
    const picked: (number | null)[] = [];
    const r = mount(inspectorFor({ onSelect: (id) => picked.push(id) }));
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)], { session: 7 })));
    expect(picked).toEqual([]);
  });
});

describe("EntityInspector actions", () => {
  const removeButton = () => screen.getByRole("button", { name: "Remove entity" });
  const deletes = () => (engine?.calls ?? []).filter((c) => c.method === "DELETE");

  test("Remove (owned) sends DELETE for the entity", async () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    fireEvent.click(removeButton());
    await settle();
    expect(deletes().map((c) => c.path)).toEqual(["/api/v1/entities/12"]);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("Remove uses the danger variant", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(removeButton().className).toContain("danger");
  });

  test("a second click while the request is in flight sends nothing and the button is disabled", async () => {
    let release: (() => void) | undefined;
    engine = fakeEngine((call) => {
      if (call.method === "DELETE")
        return new Promise<Response>((resolve) => {
          release = () => resolve(noContent());
        });
      return jsonReply(detailBody(12));
    });
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    fireEvent.click(removeButton());
    await settle();
    expect(removeButton()).toBeDisabled();
    fireEvent.click(removeButton());
    await settle();
    expect(deletes()).toHaveLength(1);
    release?.();
    await settle();
    expect(removeButton()).not.toBeDisabled();
  });

  test("a 409 entity_remote problem is shown inline with its title and detail", async () => {
    engine = fakeEngine((call) =>
      call.method === "DELETE"
        ? problemReply(409, "entity_remote", "Entity is remote", "Only the owner can remove it")
        : jsonReply(detailBody(12)),
    );
    const picked: (number | null)[] = [];
    const r = mount(inspectorFor({ onSelect: (id) => picked.push(id) }));
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    fireEvent.click(removeButton());
    await settle();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Entity is remote");
    expect(alert).toHaveTextContent("Only the owner can remove it");
    expect(picked).toEqual([]);
  });

  test("the problem clears when the next removal starts", async () => {
    let first = true;
    engine = fakeEngine((call) => {
      if (call.method !== "DELETE") return jsonReply(detailBody(12));
      if (first) {
        first = false;
        return problemReply(409, "entity_remote", "Entity is remote", "Only the owner");
      }
      return noContent();
    });
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    fireEvent.click(removeButton());
    await settle();
    expect(screen.getByRole("alert")).toBeInTheDocument();
    fireEvent.click(removeButton());
    await settle();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("Center on map calls onCenter with the id; without onCenter there is no such button", () => {
    engineWith();
    const centered: number[] = [];
    const r = mount(inspectorFor({ onCenter: (id) => centered.push(id) }));
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    fireEvent.click(screen.getByRole("button", { name: "Center on map" }));
    expect(centered).toEqual([12]);
  });

  test("no Center on map button without onCenter", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(screen.queryByRole("button", { name: "Center on map" })).toBeNull();
  });

  test("Close calls onClose", () => {
    engineWith();
    let closed = 0;
    const r = mount(inspectorFor({ onClose: () => closed++ }));
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    fireEvent.click(screen.getByRole("button", { name: /close/i }));
    expect(closed).toBe(1);
  });
});

test("the hello helper is part of the double", () => {
  expect(hello().type).toBe("hello");
});

// ---- WI-10 code review follow-ups ------------------------------------------------------------

const BRAVO = { name: "Bravo", template: "truck", force: "red", status: "idle" };
const pair = (seq: number, opts: Parameters<typeof streamFrame>[1] = {}) =>
  frame(seq, [wireEntity(12, ALPHA), wireEntity(13, BRAVO)], opts);
const GET = (id: number) =>
  (engine?.calls ?? []).filter((c) => c.method === "GET" && c.path === `/api/v1/entities/${id}`);
const removeBtn = () => screen.getByRole("button", { name: "Remove entity" });

describe("EntityInspector: state does not leak between entities (M1, m2)", () => {
  test("M1: a failed removal of 12 shows no alert after selecting 13, and Remove is enabled", async () => {
    engine = fakeEngine((call) =>
      call.method === "DELETE"
        ? problemReply(409, "entity_remote", "Entity is remote", "Only the owner can remove it")
        : call.path.endsWith("/13")
          ? jsonReply(detailBody(13))
          : jsonReply(detailBody(12)),
    );
    const m = mount(inspectorFor({ selectedId: 12 }));
    act(() => m.push(pair(1)));
    fireEvent.click(removeBtn());
    await settle();
    expect(screen.getByRole("alert")).toHaveTextContent("Entity is remote");
    m.rerender(inspectorFor({ selectedId: 13 }));
    await settle();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(removeBtn()).not.toBeDisabled();
  });

  test("M1: a removal of 12 in flight does not disable Remove for the owned entity 13", async () => {
    engine = fakeEngine((call) =>
      call.method === "DELETE"
        ? new Promise<Response>(() => {}) // never answers
        : jsonReply(detailBody(call.path.endsWith("/13") ? 13 : 12)),
    );
    const m = mount(inspectorFor({ selectedId: 12 }));
    act(() => m.push(pair(1)));
    fireEvent.click(removeBtn());
    await settle();
    expect(removeBtn()).toBeDisabled();
    m.rerender(inspectorFor({ selectedId: 13 }));
    await settle();
    expect(removeBtn()).not.toBeDisabled();
    fireEvent.click(removeBtn());
    await settle();
    expect((engine?.calls ?? []).filter((c) => c.method === "DELETE").map((c) => c.path)).toEqual([
      "/api/v1/entities/12",
      "/api/v1/entities/13",
    ]);
  });

  test("m2: after a selectedId change no commit shows anything of the previous entity", async () => {
    engineWith();
    const seen: string[] = [];
    const probed = (props: Props) => (source: EntityInspectorProps["source"]) => (
      <Profiler id="inspector" onRender={() => seen.push(document.body.textContent ?? "")}>
        {inspectorFor(props)(source)}
      </Profiler>
    );
    const m = mount(probed({ selectedId: 12 }));
    act(() => m.push(pair(1)));
    await settle();
    expect(seen.at(-1)).toContain("Alpha");
    seen.length = 0;
    m.rerender(probed({ selectedId: 13 }));
    await settle();
    expect(seen.length).toBeGreaterThan(0);
    for (const text of seen) {
      expect(text).not.toContain("Alpha");
      expect(text).not.toContain("10.012000"); // entity 12's latitude
    }
    expect(seen.at(-1)).toContain("Bravo");
  });
});

/** What each commit looked like, recorded at commit time (the DOM is already updated in `onRender`). */
interface CommitView {
  text: string;
  alerts: number;
  stale: number;
}
function probe() {
  const commits: CommitView[] = [];
  const wrap = (props: Props) => (source: EntityInspectorProps["source"]) => (
    <Profiler
      id="inspector"
      onRender={() =>
        commits.push({
          text: document.body.textContent ?? "",
          alerts: document.querySelectorAll('[role="alert"]').length,
          stale: document.querySelectorAll('[data-stale="true"]').length,
        })
      }
    >
      {inspectorFor(props)(source)}
    </Profiler>
  );
  return { commits, wrap };
}

describe("EntityInspector: no transient gone state (N1) and the never-seen id (gap)", () => {
  test("N1: switching between two entities that are both in the frame never commits a gone alert or a stale root", async () => {
    engineWith();
    const { commits, wrap } = probe();
    const m = mount(wrap({ selectedId: 12 }));
    act(() => m.push(pair(1)));
    await settle();
    commits.length = 0;
    m.rerender(wrap({ selectedId: 13 }));
    await settle();
    expect(commits.length).toBeGreaterThan(0);
    for (const c of commits) {
      expect(c.alerts).toBe(0);
      expect(c.stale).toBe(0);
    }
    m.rerender(wrap({ selectedId: 12 }));
    await settle();
    for (const c of commits) {
      expect(c.alerts).toBe(0);
      expect(c.stale).toBe(0);
    }
  });

  test("N1: mounting with a selected entity that is already in the stream never commits a gone alert", async () => {
    engineWith();
    const { commits, wrap } = probe();
    mount(wrap({ selectedId: 12 }), (r) => r.push(pair(1)));
    await settle();
    expect(commits.length).toBeGreaterThan(0);
    for (const c of commits) {
      expect(c.alerts).toBe(0);
      expect(c.stale).toBe(0);
    }
    expect(commits.at(-1)?.text).toContain("Alpha");
  });

  test("gap: selecting an id that is not in the frame after a present one shows only the heading and the gone message", async () => {
    engineWith();
    const { commits, wrap } = probe();
    const m = mount(wrap({ selectedId: 12 }));
    act(() => m.push(pair(1)));
    await settle();
    expect(screen.getByRole("heading", { name: "Alpha" })).toBeInTheDocument();
    commits.length = 0;
    m.rerender(wrap({ selectedId: 99 }));
    await settle();
    expect(screen.getByRole("heading", { name: "#99" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("No longer in the simulation");
    expect(screen.queryByText("Latitude", { selector: "dt" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Remove entity" })).toBeNull();
    for (const c of commits) {
      expect(c.text).not.toContain("Alpha");
      expect(c.text).not.toContain("10.012000"); // entity 12's latitude
      expect(c.text).not.toContain("12.5"); // nor its altitude
    }
  });
});

describe("EntityInspector: requests follow the situation (m4, m5, m6)", () => {
  test("m4: an entity that left the simulation is not polled while the run is running", async () => {
    fakeTimers();
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    await settle();
    act(() => r.push(frame(2, [wireEntity(3)])));
    await settle();
    const before = GET(12).length;
    await act(async () => void jest.advanceTimersByTime(5000));
    await settle();
    expect(GET(12)).toHaveLength(before);
  });

  test("m4: it polls again when the entity returns", async () => {
    fakeTimers();
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    act(() => r.push(frame(2, [])));
    await settle();
    const before = GET(12).length;
    act(() => r.push(frame(3, [wireEntity(12, ALPHA)])));
    await act(async () => void jest.advanceTimersByTime(1000));
    await settle();
    expect(GET(12).length).toBeGreaterThan(before);
  });

  test("m5: when the stream goes reconnecting and then open again the detail is read once more", async () => {
    fakeTimers();
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)], { state: "paused" })));
    await settle();
    expect(GET(12)).toHaveLength(1);
    act(() => r.drop());
    await settle();
    expect(GET(12)).toHaveLength(1); // nothing while the connection is down
    act(() => r.reconnect());
    act(() => r.push(hello()));
    act(() => r.push(frame(2, [wireEntity(12, ALPHA)], { state: "paused" })));
    await settle();
    expect(GET(12)).toHaveLength(2);
    act(() => r.push(frame(3, [wireEntity(12, ALPHA)], { state: "paused" })));
    await settle();
    expect(GET(12)).toHaveLength(2); // once, not per frame
  });

  test("m6: two clicks on Remove in one act send one DELETE", async () => {
    engine = fakeEngine((call) =>
      call.method === "DELETE" ? new Promise<Response>(() => {}) : jsonReply(detailBody(12)),
    );
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    const button = removeBtn();
    act(() => {
      fireEvent.click(button);
      fireEvent.click(button);
    });
    await settle();
    expect((engine?.calls ?? []).filter((c) => c.method === "DELETE")).toHaveLength(1);
  });
});

describe("EntityInspector: the gone state and the session guard (m10, nit)", () => {
  test("m10: the gone state is a warning Alert with the message as its title", () => {
    engineWith();
    const r = mount(inspectorFor());
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)])));
    expect(screen.queryByRole("alert")).toBeNull();
    act(() => r.push(frame(2, [])));
    expect(screen.getByRole("alert")).toHaveTextContent("No longer in the simulation");
  });

  test("m10: the stylesheet does not colour text with --warning (contrast: it is a surface colour)", () => {
    const css = readFileSync(new URL("./EntityInspector.css", import.meta.url), "utf8");
    expect(css).not.toMatch(/(^|[;{\s])color:\s*var\(--warning/);
  });

  test("nit: a session change with nothing selected does not call onSelect", () => {
    engineWith();
    const picked: (number | null)[] = [];
    const r = mount(inspectorFor({ selectedId: null, onSelect: (id) => picked.push(id) }));
    act(() => r.push(frame(1, [wireEntity(12, ALPHA)], { session: 1 })));
    act(() => r.push(frame(2, [wireEntity(12, ALPHA)], { session: 2 })));
    expect(picked).toEqual([]);
  });
});
