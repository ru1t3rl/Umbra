/** The longest ladder: the dashboard bar's (ASM-992). `FitLevel` is derived from it. */
export const MAX_FIT_LEVELS = 4;

type Range<N extends number, Acc extends number[] = []> = Acc["length"] extends N
  ? Acc[number]
  : Range<N, [...Acc, Acc["length"]]>;

/** A level of a fit ladder: 0 to `MAX_FIT_LEVELS - 1`. */
export type FitLevel = Range<typeof MAX_FIT_LEVELS>;

/** The length of a ladder: the default has three levels (0 full, 1 compact, 2 icon-only lifecycle buttons), the dashboard's four. */
export type FitLevels = 3 | 4;

const LEVELS: readonly FitLevel[] = [0, 1, 2, 3];
/** The level for an index, clamped into the valid range (no cast that could lie). */
const levelAt = (index: number): FitLevel =>
  LEVELS[Math.min(Math.max(index, 0), MAX_FIT_LEVELS - 1)] ?? 0;

/** The bar must be this much wider than a level needed before it steps back to it (no flip-flop at the boundary). */
export const FIT_HYSTERESIS_PX = 24;

export interface FitState {
  level: FitLevel;
  /** The width each level last needed when it overflowed (0 = never measured). */
  need: readonly number[];
}

export interface FitMeasure {
  /** The bar's own width. 0 when nothing is laid out. */
  client: number;
  /** How far its content runs past the bar (0 = it fits). */
  overflow: number;
}

/** The dashboard bar's ladder: full, icon-only navigation, compact, icon-only lifecycle buttons (ASM-992). */
export const BAR_FIT_LEVELS: FitLevels = 4;

export const INITIAL_FIT: FitState = { level: 0, need: [0, 0, 0] };

/** The start of a ladder with `levels` steps. */
export function initialFit(levels: FitLevels): FitState {
  return { level: 0, need: Array.from({ length: levels }, () => 0) };
}

/** The next level for a measurement: up on overflow, down only with room for the lower level plus the hysteresis. */
export function nextFit(state: FitState, { client, overflow }: FitMeasure): FitState {
  if (client <= 0) return state;
  if (overflow > 0) {
    const need = [...state.need];
    need[state.level] = client + overflow;
    const last = state.need.length - 1;
    return { level: state.level >= last ? state.level : levelAt(state.level + 1), need };
  }
  if (state.level > 0) {
    const lower = levelAt(state.level - 1);
    const needed = state.need[lower] ?? 0;
    if (needed > 0 && client >= needed + FIT_HYSTERESIS_PX) return { ...state, level: lower };
  }
  return state;
}

/** True when the content got narrower by more than the hysteresis since the last measurement (null = none yet). */
export function contentShrank(previous: number | null, current: number): boolean {
  return previous !== null && previous - current > FIT_HYSTERESIS_PX;
}
