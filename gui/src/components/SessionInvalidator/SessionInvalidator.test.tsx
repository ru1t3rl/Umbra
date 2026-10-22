import { afterEach, beforeEach, expect, test } from "bun:test";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { configureEngineUrl, DataProvider, useGetStatus, useListEntities } from "@umbra/data";
import {
  connectEngineStream,
  type EngineStreamConnection,
  EngineStreamProvider,
  type WebSocketLike,
} from "@umbra/engine-stream";
import type { ReactElement } from "react";
import { SessionInvalidator } from "./SessionInvalidator.tsx";

const realFetch = globalThis.fetch;
let calls: string[] = [];
let statusSession = 1;
beforeEach(() => {
  configureEngineUrl("http://engine:7878");
  calls = [];
  statusSession = 1;
  globalThis.fetch = ((url: string) => {
    calls.push(String(url));
    return Promise.resolve(
      new Response(
        JSON.stringify({
          api_version: "1.0.0",
          entities: [],
          total: 0,
          run: { session: statusSession },
        }),
        {
          status: 200,
          headers: { "content-type": "application/json" },
        },
      ),
    );
  }) as unknown as typeof fetch;
});
afterEach(() => {
  cleanup();
  globalThis.fetch = realFetch;
  configureEngineUrl("");
});

const count = (needle: string) => calls.filter((u) => u.includes(needle)).length;

/** Mounts two real REST queries: the entity list (engine data) and the status (excluded). */
function Queries(): ReactElement {
  const entities = useListEntities();
  const status = useGetStatus();
  return <p>{entities.isSuccess && status.isSuccess ? "loaded" : "loading"}</p>;
}

/** The signal under test is `useRunStatus()?.session` (plan WI-3, ASM-851), fed by stream frames. */
function setup() {
  let socket: WebSocketLike | undefined;
  let timers: (() => void)[] = [];
  let pending: (() => void)[] = [];
  const connection: EngineStreamConnection = connectEngineStream({
    url: "ws://engine/api/v1/stream?encoding=json",
    createSocket: () => {
      socket = {
        send: () => {},
        close: () => {},
        onopen: null,
        onmessage: null,
        onclose: null,
        onerror: null,
      };
      return socket;
    },
    scheduleFrame: (cb) => {
      pending.push(cb);
      return pending.length;
    },
    cancelFrame: () => {
      pending = [];
    },
    setTimer: (cb) => {
      timers.push(cb);
      return timers.length;
    },
    clearTimer: () => {
      timers = [];
    },
  });
  const send = (message: unknown) => socket?.onmessage?.({ data: JSON.stringify(message) });
  const hello = () =>
    send({
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
    });
  hello();
  let seq = 0;
  /** One frame of `session` at `tick`, flushed to the store. */
  const frame = (session: number, tick: number) =>
    act(() => {
      send({
        type: "frame",
        kind: "snapshot",
        cause: "tick",
        session,
        seq: seq++,
        tick,
        time_s: tick / 30,
        dropped: 0,
        entities: [],
        events: [],
        events_truncated: false,
        run: {
          state: "running",
          session,
          clock: { mode: "realtime" },
          step_pending: false,
          tick,
          time_s: tick / 30,
        },
      });
      const run = pending;
      pending = [];
      for (const f of run) f();
    });
  render(
    <DataProvider>
      <EngineStreamProvider connection={connection}>
        <SessionInvalidator>
          <Queries />
        </SessionInvalidator>
      </EngineStreamProvider>
    </DataProvider>,
  );
  /** Drops the socket, fires the retry timer and completes a new handshake. */
  const reconnect = () => {
    act(() => {
      socket?.onclose?.({ code: 1006 } as never);
    });
    act(() => {
      const run = timers;
      timers = [];
      for (const t of run) t();
    });
    hello();
  };
  return { frame, connection, reconnect };
}

const loaded = () => screen.findByText("loaded");

test("a new session refetches the engine queries once", async () => {
  const s = setup();
  await loaded();
  s.frame(1, 1);
  const before = count("/entities");
  s.frame(2, 1);
  await waitFor(() => expect(count("/entities")).toBe(before + 1));
  s.connection.close();
});

test("the status query is not refetched on a session change", async () => {
  const s = setup();
  await loaded();
  s.frame(1, 1);
  const before = count("/status");
  s.frame(2, 1);
  await waitFor(() => expect(count("/entities")).toBeGreaterThan(1));
  expect(count("/status")).toBe(before);
  s.connection.close();
});

test("further frames of the same session do not refetch", async () => {
  const s = setup();
  await loaded();
  s.frame(1, 1);
  const before = calls.length;
  s.frame(1, 2);
  s.frame(1, 3);
  await act(async () => {
    await Promise.resolve();
  });
  expect(calls.length).toBe(before);
  s.connection.close();
});

test("the first session seen after mount does not refetch (nothing is stale yet)", async () => {
  const s = setup();
  await loaded();
  const before = calls.length;
  s.frame(1, 1);
  await act(async () => {
    await Promise.resolve();
  });
  expect(calls.length).toBe(before);
  s.connection.close();
});

test("a reconnect that resumes the same session does not refetch", async () => {
  const s = setup();
  await loaded();
  s.frame(1, 1);
  const before = calls.length;
  s.reconnect();
  s.frame(1, 2);
  await act(async () => {
    await Promise.resolve();
  });
  expect(calls.length).toBe(before);
  s.connection.close();
});

test("a first session equal to the cached status session does not refetch", async () => {
  statusSession = 3;
  const s = setup();
  await loaded();
  const before = calls.length;
  s.frame(3, 1);
  await act(async () => {
    await Promise.resolve();
  });
  expect(calls.length).toBe(before);
  s.connection.close();
});

test("a first session different from the cached status session refetches once", async () => {
  statusSession = 3;
  const s = setup();
  await loaded();
  const before = count("/entities");
  s.frame(4, 1);
  await waitFor(() => expect(count("/entities")).toBe(before + 1));
  s.frame(4, 2);
  await act(async () => {
    await Promise.resolve();
  });
  expect(count("/entities")).toBe(before + 1);
  s.connection.close();
});
