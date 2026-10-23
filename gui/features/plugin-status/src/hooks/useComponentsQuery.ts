import { type ComponentEntry, useListComponents } from "@umbra/data";
import { useRunStatus } from "@umbra/engine-stream";
import type { QueryView } from "../types.ts";
import { FRESH_MS } from "./useAdaptersQuery.ts";
import { useRunRefetch } from "./useRunRefetch.ts";
import { useScenarioIdle } from "./useScenarioIdle.ts";

/** `GET /components`: nothing while idle, read again on a session change only, never polled (ASM-957). Fresh for the polling period (ASM-994 c). */
export function useComponentsQuery(): {
  idle: boolean;
  query: QueryView<ComponentEntry[]>;
} {
  const idle = useScenarioIdle();
  const run = useRunStatus();
  const query = useListComponents(undefined, {
    query: { enabled: !idle, staleTime: FRESH_MS },
  });
  useRunRefetch(query.refetch, !idle, run ? String(run.session) : undefined);
  return { idle, query };
}
