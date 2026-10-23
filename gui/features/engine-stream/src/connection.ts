// The engine stream connection (ADR 0045): one WebSocket, latest-frame-wins, one store update per
// animation frame, reconnect with capped backoff. Socket factory, frame scheduler and timers are
// injectable so tests run without a browser, a network or real time.
import { backoffDelay } from "./model/backoff.ts";
import { DECODERS, type Decoded, STREAM_PROTOCOL } from "./model/decode.ts";
import { INITIAL_RECORDER_STATE, recordFrame } from "./model/frameRecords.ts";
import { createJournal, type JournalDraft } from "./model/journal.ts";
import {
  createStreamStore,
  flushPending,
  type PendingFrame,
  type StreamStore,
} from "./model/store.ts";
import type {
  EntityFrame,
  EntitySource,
  JournalSource,
  StreamState,
  Subscription,
  WebSocketLike,
} from "./types.ts";

/** Application close code (browsers only allow 1000 and 3000-4999): the server speaks another protocol. */
export const CLOSE_UNSUPPORTED_PROTOCOL = 4002;

export const DEFAULT_JOURNAL_CAPACITY = 5000;

export const DEFAULT_SUBSCRIPTION: Subscription = { max_rate_hz: 30 };

export interface EngineStreamOptions {
  /** Full `ws://` or `wss://` URL including `?encoding=json` (see `streamUrl`). */
  url: string;
  /** Only `"json"` exists in M5b; anything else throws at construction. Default `"json"`. */
  encoding?: string;
  /** Sent after every `hello`. Default `{ max_rate_hz: 30 }` (ASM-315). */
  subscription?: Subscription;
  /** Default: the browser `WebSocket`. */
  createSocket?: (url: string) => WebSocketLike;
  /** Default: `requestAnimationFrame`. Returns an opaque handle for `cancelFrame`. */
  scheduleFrame?: (callback: () => void) => unknown;
  cancelFrame?: (handle: unknown) => void;
  /** Records the journal keeps (ADR 0071). Default 5,000; the app passes `settings.journal.capacity`. */
  journalCapacity?: number;
  /** Wall clock for `JournalRecord.received_at`, display only. Default `Date.now`. */
  now?: () => number;
  /** Default: `setTimeout`/`clearTimeout`. */
  setTimer?: (callback: () => void, ms: number) => unknown;
  clearTimer?: (handle: unknown) => void;
}

export interface EngineStreamConnection {
  readonly store: StreamStore;
  readonly source: EntitySource;
  /** Lossless event journal (ADR 0071, amends ADR 0045 for events); spans reconnects and sessions. */
  readonly journal: JournalSource;
  getState(): StreamState;
  /** Replace the subscription; sent at once when open and again after every reconnect. */
  setSubscription(subscription: Subscription): void;
  /** Stop for good: closes the socket, cancels the pending flush and any reconnect. */
  close(): void;
}

export function browserSocket(url: string): WebSocketLike {
  const ws = new WebSocket(url); // no subprotocol: the server does not negotiate one (ASM-356)
  const like: WebSocketLike = {
    send: (data) => ws.send(data),
    close: (code, reason) => ws.close(code, reason),
    onopen: null,
    onmessage: null,
    onclose: null,
    onerror: null,
  };
  ws.onopen = () => like.onopen?.();
  ws.onmessage = (e) => like.onmessage?.({ data: e.data });
  ws.onclose = (e) => like.onclose?.({ code: e.code, reason: e.reason });
  ws.onerror = () => like.onerror?.();
  return like;
}

type FrameHandle = { raf: number } | { timeout: ReturnType<typeof setTimeout> };

/** requestAnimationFrame when it exists, else a 16 ms timeout; the handle says which one to cancel. */
function defaultFrameScheduler(): {
  schedule: (cb: () => void) => unknown;
  cancel: (handle: unknown) => void;
} {
  const hasRaf = typeof requestAnimationFrame === "function";
  const isHandle = (h: unknown): h is FrameHandle => typeof h === "object" && h !== null;
  return {
    schedule: (cb) =>
      hasRaf ? { raf: requestAnimationFrame(cb) } : { timeout: setTimeout(cb, 16) },
    cancel: (h) => {
      if (!isHandle(h)) return;
      if ("raf" in h) cancelAnimationFrame(h.raf);
      else clearTimeout(h.timeout);
    },
  };
}

export function connectEngineStream(options: EngineStreamOptions): EngineStreamConnection {
  const encoding = options.encoding ?? "json";
  const decode = DECODERS[encoding];
  if (decode === undefined) {
    throw new RangeError(`unsupported stream encoding "${encoding}"; only "json" is supported`);
  }
  const createSocket = options.createSocket ?? browserSocket;
  const defaults = defaultFrameScheduler();
  const scheduleFrame = options.scheduleFrame ?? defaults.schedule;
  const cancelFrame = options.cancelFrame ?? defaults.cancel;
  const setTimer = options.setTimer ?? ((cb, ms) => setTimeout(cb, ms));
  const clearTimer =
    options.clearTimer ?? ((h) => clearTimeout(h as ReturnType<typeof setTimeout>));

  const store = createStreamStore();
  const journal = createJournal(options.journalCapacity ?? DEFAULT_JOURNAL_CAPACITY, options.now);
  let recorder = INITIAL_RECORDER_STATE;
  let wasOpen = false;
  let socketOpened = false; // the current socket received its hello
  let failureNoted = false; // one `connect_failed` per outage, not one per retry
  let clientClosed = false;
  let subscription: Subscription = options.subscription ?? DEFAULT_SUBSCRIPTION;
  let socket: WebSocketLike | undefined;
  let attempt = 0;
  let closed = false;
  let retryTimer: unknown;
  let pending: PendingFrame | undefined;
  let frameHandle: unknown;

  const send = (): void => socket?.send(JSON.stringify({ type: "subscribe", ...subscription }));

  const flush = (): void => {
    frameHandle = undefined;
    const p = pending;
    pending = undefined;
    if (p) flushPending(store, p);
    journal.publish();
  };

  /** Stage a connection record; it is published by the next flush, which it schedules if none is pending. */
  const note = (draft: JournalDraft): void => {
    journal.append(draft);
    if (frameHandle === undefined) frameHandle = scheduleFrame(flush);
  };

  const queue = (
    frame: EntityFrame,
    run: PendingFrame["run"],
    skipped: number,
    control: boolean,
  ): void => {
    // Latest wins; the run of an older frame stays unless the newer one carries none; drops add up.
    pending = {
      frame,
      run: run ?? pending?.run,
      dropped: (pending?.dropped ?? 0) + frame.dropped,
      skipped: (pending?.skipped ?? 0) + skipped,
      controls: (pending?.controls ?? 0) + (control ? 1 : 0),
    };
    if (frameHandle === undefined) frameHandle = scheduleFrame(flush);
  };

  const handle = (message: Decoded, from: WebSocketLike): void => {
    switch (message.kind) {
      case "hello":
        if (message.hello.protocol !== STREAM_PROTOCOL) {
          // A different protocol version: do not subscribe, do not retry (ASM-356).
          closed = true;
          store.setState({ status: "incompatible", hello: message.hello });
          note({
            source: "connection",
            level: "error",
            kind: "incompatible",
            message: `The server speaks ${message.hello.protocol}, this client ${STREAM_PROTOCOL}`,
          });
          from.close(CLOSE_UNSUPPORTED_PROTOCOL, "unsupported protocol");
          return;
        }
        attempt = 0; // backoff resets on hello
        socketOpened = true;
        failureNoted = false;
        store.setState({ status: "open", hello: message.hello });
        note({
          source: "connection",
          level: "info",
          kind: wasOpen ? "reconnected" : "connected",
          message: wasOpen ? "Reconnected to the engine" : "Connected to the engine",
        });
        wasOpen = true;
        send();
        return;
      case "frame": {
        // Journal every decoded frame before coalescing: the events of a skipped frame are not lost (ADR 0071).
        const recorded = recordFrame(recorder, message);
        recorder = recorded.next;
        for (const draft of recorded.drafts) journal.append(draft);
        queue(message.frame, message.run, message.skipped, message.control);
        return;
      }
      case "error":
        store.setState((s) => ({ errors: s.errors + 1 }));
        note({
          source: "connection",
          level: "warning",
          kind: "server_error",
          message: `${message.code}: ${message.message}`,
          detail: { code: message.code, message: message.message },
        });
        return;
      case "ignored":
        store.setState((s) => ({ ignored: s.ignored + 1 }));
        return;
      default:
        return;
    }
  };

  /** Journals the first failed attempt of an outage (startup, or after an open connection was lost). */
  const noteConnectFailed = (): void => {
    if (failureNoted) return;
    failureNoted = true;
    note({
      source: "connection",
      level: "warning",
      kind: "connect_failed",
      message: "Could not connect to the engine; retrying",
    });
  };

  /** The socket was lost, or could not be created: retry after the next backoff delay. */
  const scheduleRetry = (): void => {
    socket = undefined;
    store.setState({ status: "reconnecting" });
    const delay = backoffDelay(attempt);
    attempt += 1;
    retryTimer = setTimer(() => {
      retryTimer = undefined;
      if (!closed) connect();
    }, delay);
  };

  const connect = (): void => {
    let current: WebSocketLike;
    try {
      current = createSocket(options.url);
    } catch (error) {
      console.error("engine stream: could not create the socket", error);
      noteConnectFailed();
      scheduleRetry();
      return;
    }
    socket = current;
    socketOpened = false;
    store.setState({
      status: attempt === 0 && !store.getState().hello ? "connecting" : "reconnecting",
    });
    current.onmessage = (event) => {
      if (socket !== current || closed) return;
      if (typeof event.data !== "string") {
        store.setState((s) => ({ ignored: s.ignored + 1 }));
        return;
      }
      handle(decode(event.data), current);
    };
    current.onclose = () => {
      if (socket !== current || closed) return;
      if (socketOpened) {
        note({
          source: "connection",
          level: "warning",
          kind: "disconnected",
          message: "The connection to the engine was lost",
        });
      } else {
        noteConnectFailed();
      }
      scheduleRetry();
    };
  };

  const listeners = new Set<(frame: EntityFrame) => void>();
  store.subscribe((state, prev) => {
    const frame = state.frame;
    if (frame === undefined || frame === prev.frame) return;
    for (const listener of [...listeners]) {
      try {
        listener(frame);
      } catch (error) {
        console.error("engine stream: an entity listener threw", error); // the others still run
      }
    }
  });
  const source: EntitySource = {
    getFrame: () => store.getState().frame,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };

  connect();

  return {
    store,
    source,
    journal: journal.source,
    getState: () => store.getState(),
    setSubscription(next) {
      subscription = next;
      if (store.getState().status === "open") send();
    },
    close() {
      if (clientClosed) return;
      clientClosed = true;
      closed = true;
      if (retryTimer !== undefined) clearTimer(retryTimer);
      retryTimer = undefined;
      if (frameHandle !== undefined) cancelFrame(frameHandle);
      frameHandle = undefined;
      pending = undefined;
      const s = socket;
      socket = undefined;
      s?.close(1000, "client closed");
      store.setState({ status: "closed" });
      journal.append({
        source: "connection",
        level: "info",
        kind: "closed",
        message: "The connection to the engine was closed",
      });
      journal.publish();
    },
  };
}
