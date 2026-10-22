// WI-5 / plan "Subscription". Assumed: useApplySubscription(sub: Subscription): void. When the VALUE of `sub`
// changes (deep equality, key order irrelevant, so an inline object literal is fine) it calls
// connection.setSubscription(sub). The first call applies too: before hello the subscription replaces the
// default one in the single hello subscribe; after reconnects the connection re-sends it (existing behaviour).
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "../components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream } from "../connection.ts";
import { hello, rig } from "../testing.ts";
import type { Subscription } from "../types.ts";
import { useApplySubscription } from "./useApplySubscription.ts";

afterEach(cleanup);

function setup(initial: Subscription, helloFirst: boolean) {
  const r = rig();
  const connection = connectEngineStream(r.options);
  if (helloFirst) r.socket().receive(hello());
  function Probe({ sub }: { sub: Subscription }): ReactElement {
    useApplySubscription(sub);
    return <p>probe</p>;
  }
  const el = (sub: Subscription) => (
    <EngineStreamProvider connection={connection}>
      <Probe sub={sub} />
    </EngineStreamProvider>
  );
  const ui = render(el(initial));
  return { r, connection, update: (s: Subscription) => ui.rerender(el(s)), unmount: ui.unmount };
}
const subscribes = (r: ReturnType<typeof rig>) =>
  (r.socket().sent as ({ type: string } & Subscription)[]).filter((m) => m.type === "subscribe");

describe("useApplySubscription", () => {
  test("applied before hello: one subscribe with the applied rate, no default one", () => {
    // Mutation caught: the hook ignoring the not-yet-open state or sending the 30 Hz default as well.
    const { r } = setup({ max_rate_hz: 10 }, false);
    r.socket().receive(hello());
    expect(subscribes(r)).toEqual([{ type: "subscribe", max_rate_hz: 10 }]);
  });
  test("applied after hello: one more subscribe with the new value", () => {
    // Mutation caught: an effect that never runs on mount.
    const { r } = setup({ max_rate_hz: 10 }, true);
    expect(subscribes(r)).toEqual([
      { type: "subscribe", max_rate_hz: 30 },
      { type: "subscribe", max_rate_hz: 10 },
    ]);
  });
  test("a re-render with an equal value (new object, other key order) sends nothing", () => {
    // Mutation caught: identity dependency (an inline literal would re-send on every render).
    const { r, update } = setup({ max_rate_hz: 10, filter: { event_kinds: ["fire"] } }, true);
    const n = subscribes(r).length;
    update({ filter: { event_kinds: ["fire"] }, max_rate_hz: 10 });
    update({ max_rate_hz: 10, filter: { event_kinds: ["fire"] } });
    expect(subscribes(r)).toHaveLength(n);
  });
  test("a changed rate or filter sends exactly one subscribe per change", () => {
    // Mutation caught: comparing only max_rate_hz; sending twice.
    const { r, update } = setup({ max_rate_hz: 10 }, true);
    const n = subscribes(r).length;
    update({ max_rate_hz: 20 });
    expect(subscribes(r)).toHaveLength(n + 1);
    update({ max_rate_hz: 20, filter: { entity_ids: [1, 2] } });
    expect(subscribes(r)).toHaveLength(n + 2);
    expect(subscribes(r).at(-1)).toEqual({
      type: "subscribe",
      max_rate_hz: 20,
      filter: { entity_ids: [1, 2] },
    });
  });
  test("the applied subscription is sent again after a reconnect", () => {
    // Mutation caught: sending the default after reconnect instead of the applied value.
    const { r } = setup({ max_rate_hz: 12 }, true);
    r.socket().drop();
    act(() => r.clock.fireTimer());
    r.socket().receive(hello());
    expect(subscribes(r)).toEqual([{ type: "subscribe", max_rate_hz: 12 }]);
  });
  test("unmounting sends nothing and does not throw", () => {
    // Mutation caught: a cleanup that resets to the default subscription (flicker on page changes).
    const { r, unmount } = setup({ max_rate_hz: 10 }, true);
    const n = subscribes(r).length;
    unmount();
    expect(subscribes(r)).toHaveLength(n);
  });
});
