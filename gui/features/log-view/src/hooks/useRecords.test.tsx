// m2: a source that publishes between render and subscribe is not missed, and unchanged snapshots cause no
// state update (two sources would otherwise get a fresh merged array on every notification).
import { expect, test } from "bun:test";
import { act, render } from "@testing-library/react";
import type { JournalRecord, JournalSource } from "@umbra/engine-stream";
import { type ReactElement, useLayoutEffect } from "react";
import { FakeJournal, rec } from "../logTesting.tsx";
import { useRecords } from "./useRecords.ts";

function Probe({
  sources,
  onRender,
  beforeSubscribe,
}: {
  sources: readonly JournalSource[];
  onRender: (records: readonly JournalRecord[]) => void;
  beforeSubscribe?: () => void;
}): ReactElement {
  const records = useRecords(sources);
  onRender(records);
  // layout effects run before the passive effect that subscribes
  useLayoutEffect(() => beforeSubscribe?.(), [beforeSubscribe]);
  return <p>{records.length}</p>;
}

test("a record published between render and subscribe is shown", () => {
  const journal = new FakeJournal([rec(1)]);
  let latest: readonly JournalRecord[] = [];
  const sources = [journal];
  render(
    <Probe
      sources={sources}
      onRender={(r) => {
        latest = r;
      }}
      beforeSubscribe={() => journal.push(rec(2))}
    />,
  );
  expect(latest).toHaveLength(2);
});

test("with two sources, mounting and an unrelated re-subscribe cause no extra render", () => {
  const a = new FakeJournal([rec(1, { received_at: 1 })]);
  const b = new FakeJournal([rec(1, { received_at: 2, source: "engine-process" })]);
  const sources = [a, b];
  let renders = 0;
  render(<Probe sources={sources} onRender={() => renders++} />);
  expect(renders).toBe(1);
  act(() => a.push(rec(2, { received_at: 3 })));
  expect(renders).toBe(2);
});
