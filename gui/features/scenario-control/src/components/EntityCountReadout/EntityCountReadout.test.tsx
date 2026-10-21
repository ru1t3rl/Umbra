import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render } from "@testing-library/react";
import { EngineStreamProvider } from "@umbra/engine-stream";
import { Profiler } from "react";
import { entity, frame, hello, rig } from "../../testing.ts";
import { EntityCountReadout } from "./EntityCountReadout.tsx";

afterEach(cleanup);

test("renders on a count change only, not on a tick-only frame", () => {
  const r = rig();
  let renders = 0;
  render(
    <EngineStreamProvider connection={r.connection}>
      <Profiler
        id="count"
        onRender={() => {
          renders += 1;
        }}
      >
        <EntityCountReadout />
      </Profiler>
    </EngineStreamProvider>,
  );
  act(() => r.push(hello(), frame(0, 1, [entity(1)])));
  const before = renders;
  for (let i = 1; i <= 20; i += 1) act(() => r.push(frame(i, 1 + i, [entity(1)])));
  expect(renders).toBe(before);
  act(() => r.push(frame(30, 30, [entity(1), entity(2)])));
  expect(renders).toBe(before + 1);
});
