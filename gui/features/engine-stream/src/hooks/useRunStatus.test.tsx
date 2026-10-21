import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "../components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream } from "../connection.ts";
import { entity, frame, hello, rig, run } from "../testing.ts";
import { useRunStatus } from "./useRunStatus.ts";

afterEach(cleanup);

test("useRunStatus re-renders only on run changes", () => {
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  let runRenders = 0;
  function Run(): ReactElement {
    runRenders += 1;
    const s = useRunStatus();
    return <p data-testid="run">{s ? `${s.state}@${s.tick}` : "none"}</p>;
  }
  render(
    <EngineStreamProvider connection={connection}>
      <Run />
    </EngineStreamProvider>,
  );
  const flush = (...messages: unknown[]) =>
    act(() => {
      for (const m of messages) r.socket().receive(m);
      r.clock.paint();
    });
  flush(frame(0, 1, [entity(1)]));
  expect(screen.getByTestId("run").textContent).toBe("running@1");
  const before = runRenders;
  // Same run, different entities: no re-render of the run consumer.
  flush(frame(1, 1, [entity(1), entity(2)], { cause: "control" }));
  expect(runRenders).toBe(before);
  // Errors and ignored messages do not touch it either.
  flush({ type: "error", code: "invalid_message", message: "m" }, { type: "mystery" });
  expect(runRenders).toBe(before);
  expect(connection.getState().errors).toBe(1);
  // A real run change does.
  flush(frame(2, 2, [], { run: run(2) }));
  expect(screen.getByTestId("run").textContent).toBe("running@2");
  expect(runRenders).toBe(before + 1);
});
