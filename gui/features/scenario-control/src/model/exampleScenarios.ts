/** A scenario the loader can suggest; the shape of a future `GET /api/v1/scenarios` row (G-2: path, name). */
export interface ScenarioSuggestion {
  /** Relative to the engine's scenario root. */
  path: string;
  /** Shown instead of the path when given. */
  label?: string;
}

/**
 * Scenarios shipped under `examples/`, relative to a server started with `--scenario-root examples`
 * (ASM-812, ASM-931). Suggestions only: the loader still accepts any path, and a scenario listing
 * (contract gap G-2) replaces this list through the `suggestions` prop.
 */
export const EXAMPLE_SCENARIOS: readonly ScenarioSuggestion[] = [
  { path: "scenarios/walking_skeleton/scenario.toml" },
  { path: "scenarios/patrol_and_engage/scenario.toml" },
  { path: "scenarios/plugin_wander/scenario.toml" },
  { path: "scenarios/plugin_c_circle/scenario.toml" },
];
