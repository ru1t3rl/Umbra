// WI-5 / ADR 0071. Assumed: useJournalSource(): JournalSource === connection.journal (stable, no re-render).
import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "../components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream } from "../connection.ts";
import { ev, frame, hello, rig } from "../testing.ts";
import { useJournalSource } from "./useJournalSource.ts";

afterEach(cleanup);

test("useJournalSource is the connection's journal and delivers records without rendering", () => {
  // Mutation caught: a hook that subscribes via React state (re-renders per record) or returns a copy.
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  let renders = 0;
  let source: unknown;
  function Probe(): ReactElement {
    renders += 1;
    source = useJournalSource();
    return <p>probe</p>;
  }
  render(
    <EngineStreamProvider connection={connection}>
      <Probe />
    </EngineStreamProvider>,
  );
  const seen: number[] = [];
  const off = connection.journal.subscribe(() => seen.push(connection.journal.snapshot().length));
  act(() => {
    r.socket().receive(frame(0, 1, [], { events: [ev("fire", 1)] }));
    r.clock.paint();
  });
  off();
  expect(screen.getByText("probe")).toBeTruthy();
  expect(source).toBe(connection.journal);
  expect(seen).toHaveLength(1);
  expect(renders).toBe(1);
});

test("useJournalSource throws a clear error outside the provider", () => {
  // Mutation caught: returning undefined silently.
  function Probe(): ReactElement {
    useJournalSource();
    return <p>x</p>;
  }
  const spy = console.error;
  console.error = () => {};
  try {
    expect(() => render(<Probe />)).toThrow(/EngineStreamProvider/);
  } finally {
    console.error = spy;
  }
});
