import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "../components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream } from "../connection.ts";
import { entity, frame, hello, rig } from "../testing.ts";
import { useEntitySource } from "./useEntitySource.ts";

afterEach(cleanup);

test("useEntitySource is stable and delivers frames without rendering", () => {
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  const seen: unknown[] = [];
  let renders = 0;
  function Probe(): ReactElement {
    renders += 1;
    const source = useEntitySource();
    return <p>{source === connection.source ? "same" : "different"}</p>;
  }
  render(
    <EngineStreamProvider connection={connection}>
      <Probe />
    </EngineStreamProvider>,
  );
  const off = connection.source.subscribe((f) => seen.push(f.tick));
  act(() => {
    r.socket().receive(frame(0, 1, [entity(1)]));
    r.clock.paint();
  });
  off();
  expect(screen.getByText("same")).toBeTruthy();
  expect(seen).toEqual([1]);
  expect(renders).toBe(1);
});
