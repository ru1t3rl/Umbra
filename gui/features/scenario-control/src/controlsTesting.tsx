// Test doubles for the WI-7 controls (not exported from index.ts): a fake engine behind `fetch` and a render
// helper with the same providers the app mounts (EngineStreamProvider above DataProvider, ADR 0074).
import { spyOn } from "bun:test";
import { act, render } from "@testing-library/react";
import { DataProvider } from "@umbra/data";
import { EngineStreamProvider } from "@umbra/engine-stream";
import type { ReactElement } from "react";
import { frame, hello, type Rig, rig } from "./testing.ts";

export interface RecordedCall {
  method: string;
  /** Path and query, without the origin (the base URL is "" in tests). */
  path: string;
  /** The parsed JSON request body, `undefined` when there was none. */
  body: unknown;
}

export interface FakeEngine {
  calls: RecordedCall[];
  restore(): void;
}

/** Replace `fetch`. The handler answers each call; the default answer is an empty 200 run status. */
export function fakeEngine(
  handler: (call: RecordedCall) => Response | Promise<Response> = () => runReply(),
): FakeEngine {
  const calls: RecordedCall[] = [];
  const spy = spyOn(globalThis, "fetch").mockImplementation(((
    url: string,
    init: RequestInit = {},
  ) => {
    const call: RecordedCall = {
      method: init.method ?? "GET",
      path: String(url),
      body: typeof init.body === "string" ? (JSON.parse(init.body) as unknown) : undefined,
    };
    calls.push(call);
    return Promise.resolve(handler(call));
  }) as unknown as typeof fetch);
  return {
    calls,
    restore() {
      spy.mockRestore();
    },
  };
}

export const runReply = (extra: Record<string, unknown> = {}): Response =>
  new Response(
    JSON.stringify({
      state: "running",
      session: 1,
      clock: { mode: "realtime" },
      step_pending: false,
      ...extra,
    }),
    { status: 200, headers: { "content-type": "application/json" } },
  );

/** An `application/problem+json` answer (docs/api.md "Errors"). */
export const problemReply = (
  status: number,
  code: string,
  title: string,
  detail: string,
): Response =>
  new Response(JSON.stringify({ type: `about:blank#${code}`, code, title, status, detail }), {
    status,
    headers: { "content-type": "application/problem+json" },
  });

/** A promise whose resolution the test controls (a request that is still in flight). */
export function deferred<T>(): { promise: Promise<T>; resolve(value: T): void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

export interface Mounted {
  r: Rig;
  /** Push one frame whose `run` block is `{ ...running defaults, ...runExtra }`. */
  setRun(runExtra: Record<string, unknown>): void;
}

/** Mount `ui` under the stream and data providers with an open connection (hello delivered). */
export function mount(ui: ReactElement, initialRun?: Record<string, unknown>): Mounted {
  const r = rig();
  let seq = 0;
  const setRun = (runExtra: Record<string, unknown>): void => {
    seq += 1;
    act(() => r.push(frame(seq, seq, [], runExtra)));
  };
  render(
    <EngineStreamProvider connection={r.connection}>
      <DataProvider>{ui}</DataProvider>
    </EngineStreamProvider>,
  );
  act(() => r.push(hello()));
  if (initialRun) setRun(initialRun);
  return { r, setRun };
}
