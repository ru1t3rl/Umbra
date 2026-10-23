// Public API pin of @umbra/engine-stream (runtime exports). WI-10 appends `useControlSeq` (standing approval:
// append-only export names). Types have no runtime key.
import { expect, test } from "bun:test";
import * as api from "./index.ts";

test("the package exports exactly the stream API", () => {
  expect(Object.keys(api).sort()).toEqual([
    "DEFAULT_JOURNAL_CAPACITY",
    "EngineStreamProvider",
    "connectEngineStream",
    "streamUrl",
    "useApplySubscription",
    "useControlSeq", // WI-10 (append-only, human standing approval 2026-10-23)
    "useEntity",
    "useEntitySource",
    "useJournalSource",
    "useRunStatus",
    "useStreamStatus",
  ]);
});
