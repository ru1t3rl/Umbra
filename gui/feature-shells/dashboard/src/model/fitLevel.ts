/** 0 = full, 1 = compact (no title, icon-only Clock, Scenario and theme), 2 = compact with icon-only lifecycle buttons. */
export type FitLevel = 0 | 1 | 2;

/** The bar must be this much wider than a level needed before it steps back to it (no flip-flop at the boundary). */
export const FIT_HYSTERESIS_PX = 24;

export interface FitState {
  level: FitLevel;
  /** The width each level last needed when it overflowed (0 = never measured). */
  need: readonly [number, number, number];
}

export interface FitMeasure {
  /** The bar's own width. 0 when nothing is laid out. */
  client: number;
  /** How far its content runs past the bar (0 = it fits). */
  overflow: number;
}

export const INITIAL_FIT: FitState = { level: 0, need: [0, 0, 0] };

/** The next level for a measurement: up on overflow, down only with room for the lower level plus the hysteresis. */
export function nextFit(state: FitState, { client, overflow }: FitMeasure): FitState {
  if (client <= 0) return state;
  if (overflow > 0) {
    const need = [...state.need] as [number, number, number];
    need[state.level] = client + overflow;
    return { level: state.level === 2 ? 2 : ((state.level + 1) as FitLevel), need };
  }
  if (state.level > 0) {
    const lower = (state.level - 1) as FitLevel;
    const needed = state.need[lower];
    if (needed > 0 && client >= needed + FIT_HYSTERESIS_PX) return { ...state, level: lower };
  }
  return state;
}

/** True when the content got narrower by more than the hysteresis since the last measurement (null = none yet). */
export function contentShrank(previous: number | null, current: number): boolean {
  return previous !== null && previous - current > FIT_HYSTERESIS_PX;
}
