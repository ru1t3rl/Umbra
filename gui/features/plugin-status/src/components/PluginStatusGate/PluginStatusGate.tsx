import type { ReactNode } from "react";
import { useScenarioIdle } from "../../hooks/useScenarioIdle.ts";
import { IdleState } from "../IdleState/IdleState.tsx";

/**
 * Shows the plugin-status empty state once while no scenario is loaded, and `children` otherwise. A page that
 * composes several exports (the plugins page) wraps them in it so the state is not repeated per export (ASM-994).
 */
export function PluginStatusGate({ children }: { children: ReactNode }): ReactNode {
  return useScenarioIdle() ? <IdleState /> : children;
}
