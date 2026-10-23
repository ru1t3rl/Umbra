import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "../components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream } from "../connection.ts";
import { entity, frame, hello, rig, run } from "../testing.ts";
import { useRunState } from "./useRunState.ts";

afterEach(cleanup);

test("useRunState is null before the first frame and re-renders only when the state word changes", () => {
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  let renders = 0;
  function State(): ReactElement {
    renders += 1;
    return <p data-testid="s">{String(useRunState())}</p>;
  }
  render(
    <EngineStreamProvider connection={connection}>
      <State />
    </EngineStreamProvider>,
  );
  const flush = (...messages: unknown[]) =>
    act(() => {
      for (const m of messages) r.socket().receive(m);
      r.clock.paint();
    });
  expect(screen.getByTestId("s").textContent).toBe("null");
  flush(frame(0, 1, [entity(1)]));
  expect(screen.getByTestId("s").textContent).toBe("running");
  const before = renders;
  // The tick advances on every frame; the state word does not: no re-render.
  flush(frame(1, 2, [entity(1)], { run: run(2) }), frame(2, 3, [entity(1)], { run: run(3) }));
  expect(renders).toBe(before);
  flush(frame(3, 4, [], { run: { ...run(4), state: "paused" } }));
  expect(screen.getByTestId("s").textContent).toBe("paused");
  expect(renders).toBe(before + 1);
});
