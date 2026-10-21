import { afterEach, expect, test } from "bun:test";
import { act, cleanup, renderHook } from "@testing-library/react";
import { EngineStreamProvider } from "@umbra/engine-stream";
import type { ReactNode } from "react";
import { entity, frame, hello, rig } from "../testing.ts";
import { useEntityCount } from "./useEntityCount.ts";

afterEach(cleanup);

test("undefined before a frame, then the count; renders only when it changes", () => {
  const r = rig();
  let renders = 0;
  const wrapper = ({ children }: { children: ReactNode }) => (
    <EngineStreamProvider connection={r.connection}>{children}</EngineStreamProvider>
  );
  const { result } = renderHook(
    () => {
      renders += 1;
      return useEntityCount();
    },
    { wrapper },
  );
  expect(result.current).toBeUndefined();
  act(() => r.push(hello(), frame(0, 1, [entity(1), entity(2)])));
  expect(result.current).toBe(2);
  const before = renders;
  for (let i = 1; i <= 50; i += 1) act(() => r.push(frame(i, 1 + i, [entity(1), entity(2)])));
  expect(renders).toBe(before);
  act(() => r.push(frame(60, 60, [entity(1)])));
  expect(result.current).toBe(1);
  expect(renders).toBe(before + 1);
});
