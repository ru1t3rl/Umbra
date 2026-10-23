/** At or below this viewport width the left panel starts collapsed (ASM-973, plan risk "crowded at 1280x800"). */
export const COLLAPSE_AT_WIDTH_PX = 1280;

/**
 * The initial state of the left panel: collapsed on a narrow viewport. Read once at mount; the user's toggle then wins.
 * A width of 0 means "not measured" (no layout engine, as in happy-dom): the panel starts open (ASM-977).
 */
export function collapsedAtWidth(viewportWidth: number): boolean {
  return viewportWidth > 0 && viewportWidth <= COLLAPSE_AT_WIDTH_PX;
}
