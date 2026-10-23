import { type PluginReport, useGetPlugins } from "@umbra/data";
import { useRunStatus } from "@umbra/engine-stream";
import type { QueryView } from "../types.ts";
import { useRunRefetch } from "./useRunRefetch.ts";
import { useScenarioIdle } from "./useScenarioIdle.ts";

/** `GET /plugins`: nothing while idle, read again on a run-state or session change, never polled (ASM-957). */
export function usePluginsQuery(): {
  idle: boolean;
  query: QueryView<PluginReport>;
} {
  const idle = useScenarioIdle();
  const run = useRunStatus();
  const query = useGetPlugins({ query: { enabled: !idle } });
  useRunRefetch(query.refetch, !idle, run ? `${run.state}:${run.session}` : undefined);
  return { idle, query };
}
