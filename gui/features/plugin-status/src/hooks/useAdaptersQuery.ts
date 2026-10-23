import { type AdapterDto, isApiProblem, useListNetworkAdapters } from "@umbra/data";
import { useRunStatus } from "@umbra/engine-stream";
import type { QueryView } from "../types.ts";
import { useRunRefetch } from "./useRunRefetch.ts";
import { useScenarioIdle } from "./useScenarioIdle.ts";

/** Polling period of the counters while the run is `running` (ASM-957). */
export const ADAPTER_POLL_MS = 2000;

/**
 * How long plugin-status data counts as fresh, so a tab switch reuses it (ASM-994 c2). Equal to the polling
 * period: data is never older than one poll while running, and explicit refetches ignore it.
 */
export const FRESH_MS = ADAPTER_POLL_MS;

/**
 * `GET /network/adapters`: nothing while idle, read again on a run-state or session change and every 2 s while
 * running. `noScenario` is the engine's `no_scenario` answer, which counts as an empty list (ASM-958).
 */
export function useAdaptersQuery(): {
  idle: boolean;
  query: QueryView<AdapterDto[]>;
  noScenario: boolean;
} {
  const idle = useScenarioIdle();
  const run = useRunStatus();
  const query = useListNetworkAdapters({
    query: {
      enabled: !idle,
      staleTime: FRESH_MS,
      refetchInterval: !idle && run?.state === "running" ? ADAPTER_POLL_MS : false,
    },
  });
  useRunRefetch(query.refetch, !idle, run ? `${run.state}:${run.session}` : undefined);
  const noScenario =
    query.isError && isApiProblem(query.error) && query.error.code === "no_scenario";
  return { idle, query, noScenario };
}
