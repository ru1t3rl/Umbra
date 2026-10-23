// Test doubles for the templates shell tests (WI-15; not exported, exempt from check:layout). A shell may import
// features only through their public API, and `@umbra/template-browser` exports no fixtures, so the stream rig
// and the fake engine are stated here, small: a real engine-stream connection over a fake socket (to set the run
// state), a `fetch` double serving contract-shaped fixtures, the providers the app mounts.
import { spyOn } from "bun:test";
import { act, render } from "@testing-library/react";
import {
  type ComponentEntry,
  DataProvider,
  type TemplateDetail,
  type TemplateSummary,
} from "@umbra/data";
import {
  connectEngineStream,
  EngineStreamProvider,
  type WebSocketLike,
} from "@umbra/engine-stream";
import { type NavItem, NavigationProvider, ThemeProvider } from "@umbra/ui-kit";
import type { ComponentType } from "react";

class FakeSocket implements WebSocketLike {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  send(): void {}
  close(): void {}
}

const HELLO = {
  type: "hello",
  protocol: "umbra.stream.v1",
  api_version: "1.0.0",
  encoding: "json",
  settings: {
    default_rate_hz: 30,
    max_rate_hz: 60,
    max_entity_ids: 10000,
    max_events_per_frame: 4096,
    max_message_bytes: 65536,
    queue_frames: 4,
    send_timeout_s: 10,
  },
};

const runFrame = (state: string, session = 1, seq = 1) => ({
  type: "frame",
  kind: "snapshot",
  cause: "control",
  session,
  seq,
  tick: 1,
  time_s: 0,
  dropped: 0,
  entities: [],
  events: [],
  events_truncated: false,
  run: { state, session, clock: { mode: "realtime" }, step_pending: false, tick: 1, time_s: 0 },
});

export const TEMPLATES: readonly TemplateSummary[] = [
  {
    id: "tank",
    description: "Main battle tank",
    motion: "builtin.waypoint",
    behavior: "builtin.patrol",
    source: "templates/tank.yaml",
  },
  { id: "scout", motion: "builtin.waypoint", behavior: null, source: "templates/scout.yaml" },
];

const schema = { type: "object", properties: { speed_mps: { type: "number" } } };
const component = (id: string, kind: ComponentEntry["kind"]): ComponentEntry => ({
  id,
  kind,
  params_schema: schema,
  description: `The ${id} component`,
  provenance: "builtin",
  version: "1.0.0",
});
export const COMPONENTS: readonly ComponentEntry[] = [
  component("builtin.waypoint", "motion_model"),
  component("builtin.patrol", "behavior"),
];

export const DETAILS: Record<string, TemplateDetail> = {
  tank: {
    id: "tank",
    chain: ["tank", "base"],
    source: "templates/tank.yaml",
    description: "Main battle tank",
    motion: { id: "builtin.waypoint", params: { speed_mps: 8 } },
    sensors: [],
    behavior: { id: "builtin.patrol", params: {} },
  },
  scout: {
    id: "scout",
    chain: ["scout", "base"],
    source: "templates/scout.yaml",
    description: "Fast recon vehicle",
    motion: { id: "builtin.waypoint", params: { speed_mps: 12 } },
    sensors: [],
    behavior: null,
  },
};

const reply = (body: string, status: number, contentType: string): Response =>
  ({
    ok: status >= 200 && status < 300,
    status,
    headers: new Headers({ "content-type": contentType }),
    text: () => Promise.resolve(body),
    clone: () => reply(body, status, contentType),
  }) as unknown as Response;
const jsonReply = (body: unknown): Response => reply(JSON.stringify(body), 200, "application/json");

/** Replaces `fetch` with an engine serving the fixtures. Returns the paths asked and the restore function. */
export function fakeEngine() {
  const paths: string[] = [];
  let templates: readonly TemplateSummary[] = TEMPLATES;
  const spy = spyOn(globalThis, "fetch").mockImplementation(((url: string) => {
    const path = String(url);
    paths.push(path);
    if (path === "/api/v1/templates") return Promise.resolve(jsonReply(templates));
    if (path.startsWith("/api/v1/components")) return Promise.resolve(jsonReply(COMPONENTS));
    const id = decodeURIComponent(path.slice("/api/v1/templates/".length));
    const found = DETAILS[id];
    return Promise.resolve(
      found
        ? jsonReply(found)
        : reply(
            JSON.stringify({ type: "about:blank#nf", code: "nf", title: "Not found", status: 404 }),
            404,
            "application/problem+json",
          ),
    );
  }) as unknown as typeof fetch);
  return {
    paths,
    /** The templates `GET /templates` answers from now on (another scenario). */
    setTemplates: (next: readonly TemplateSummary[]) => {
      templates = next;
    },
    restore: () => spy.mockRestore(),
  };
}

export const NAV_ITEMS: NavItem[] = [
  { path: "/", label: "Live map", icon: "map" },
  { path: "/templates", label: "Templates", icon: "category" },
];

/** Mounts the page under the providers the app mounts, with the stream in run state `state`. */
export function mountTemplates(
  Page: ComponentType,
  state = "ready",
): { push: (state: string, session?: number) => void } {
  const sockets: FakeSocket[] = [];
  let pending: (() => void)[] = [];
  const connection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    scheduleFrame: (cb) => {
      pending.push(cb);
      return pending.length;
    },
    cancelFrame: () => {
      pending = [];
    },
    setTimer: () => 1,
    clearTimer: () => {},
  });
  const socket = sockets.at(-1);
  if (!socket) throw new Error("no socket");
  let seq = 0;
  const nextSeq = (): number => {
    seq += 1;
    return seq;
  };
  const deliver = (...messages: unknown[]): void => {
    act(() => {
      for (const m of messages) socket.onmessage?.({ data: JSON.stringify(m) });
      const run = pending;
      pending = [];
      for (const f of run) f();
    });
  };
  deliver(HELLO, runFrame(state, 1, nextSeq()));
  render(
    <ThemeProvider>
      <EngineStreamProvider connection={connection}>
        <DataProvider>
          <NavigationProvider items={NAV_ITEMS} currentPath="/templates" navigate={() => {}}>
            <Page />
          </NavigationProvider>
        </DataProvider>
      </EngineStreamProvider>
    </ThemeProvider>,
  );
  return { push: (next, session = 1) => deliver(runFrame(next, session, nextSeq())) };
}

/** Lets pending promises settle (replies resolve in microtasks; TanStack Query notifies through `setTimeout(0)`). */
export async function settle(rounds = 6): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await act(async () => {
      for (let k = 0; k < 10; k++) await Promise.resolve();
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
}
