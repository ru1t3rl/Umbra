// WI-5 performance (plan "Performance plan" 1, 3, 5): 10,000-entity frames with a high event rate must not
// re-render unrelated subscribers and must notify the journal once per flush. Render and notification counts,
// never wall time (deterministic).
import { afterEach, expect, test } from "bun:test";
import { act, cleanup, render } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "../components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream } from "../connection.ts";
import { entity, ev, frame, hello, rig } from "../testing.ts";
import { useEntity } from "./useEntity.ts";
import { useJournalSource } from "./useJournalSource.ts";
import { useRunStatus } from "./useRunStatus.ts";
import { useStreamStatus } from "./useStreamStatus.ts";

afterEach(cleanup);

test("10,000 entities x 100 events per frame x 100 flushes: one journal notification per flush, no unrelated re-render", () => {
  // Mutations caught: notify per frame/event (count > 100); journal feeding React state (journal consumer
  // renders); useEntity selecting the frame (watched entity renders); the journal storing unbounded records.
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  const renders = { status: 0, run: 0, watched: 0, absent: 0, journal: 0 };
  function Status(): ReactElement {
    renders.status += 1;
    return <p>{useStreamStatus()}</p>;
  }
  function Run(): ReactElement {
    renders.run += 1;
    return <p>{useRunStatus()?.state}</p>;
  }
  function Watched(): ReactElement {
    renders.watched += 1;
    return <p>{useEntity(5_000)?.id}</p>;
  }
  function Absent(): ReactElement {
    renders.absent += 1;
    return <p>{useEntity(20_000)?.id ?? "none"}</p>;
  }
  function Journal(): ReactElement {
    renders.journal += 1;
    useJournalSource();
    return <p>journal</p>;
  }
  render(
    <EngineStreamProvider connection={connection}>
      <Status />
      <Run />
      <Watched />
      <Absent />
      <Journal />
    </EngineStreamProvider>,
  );
  const entities = Array.from({ length: 10_000 }, (_, i) => entity(i + 1));
  const events = Array.from({ length: 100 }, (_, i) => ev("fire", 1, { entity_id: (i % 10) + 1 }));
  const flush = (m: unknown) =>
    act(() => {
      r.socket().receive(m);
      r.clock.paint();
    });
  flush(frame(0, 1, entities, { events }));
  const before = { ...renders };
  let journalNotifications = 0;
  connection.journal.subscribe(() => {
    journalNotifications += 1;
  });
  for (let i = 1; i <= 100; i++) flush(frame(i, 1, entities, { cause: "control", events }));
  expect(journalNotifications).toBe(100);
  expect(renders).toEqual(before);
  expect(connection.journal.snapshot()).toHaveLength(connection.journal.capacity); // 10,100 events into 5,000
  expect(connection.journal.evicted()).toBeGreaterThan(5_000);
});

test("many frames inside one animation frame still notify the journal once", () => {
  // Mutation caught: a notification per decoded frame under a 1,000 events/s style burst.
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  r.clock.paint();
  let n = 0;
  connection.journal.subscribe(() => {
    n += 1;
  });
  for (let i = 0; i < 100; i++) {
    r.socket().receive(
      frame(i, i + 1, [], { events: Array.from({ length: 10 }, () => ev("custom", i + 1)) }),
    );
  }
  r.clock.paint();
  expect(n).toBe(1);
  expect(connection.journal.snapshot().filter((x) => x.kind === "custom")).toHaveLength(1000);
});
