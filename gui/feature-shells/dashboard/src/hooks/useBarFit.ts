import { type RefObject, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  contentShrank,
  type FitLevel,
  type FitLevels,
  type FitMeasure,
  type FitState,
  initialFit,
  nextFit,
} from "../model/fitLevel.ts";

/** The bar is the first child of the wrapper (as `useBarInset` reads it). */
const barOf = (wrapper: HTMLElement | null): HTMLElement | null => {
  const bar = wrapper?.firstElementChild;
  return bar instanceof HTMLElement ? bar : null;
};

/**
 * The width of an element's own content: the sum of its children's widths, or its own width when it has none. A bar
 * child that takes the spare width has the width of the bar whatever its content needs, so only its children tell
 * how wide the readouts really are (ASM-997).
 */
const naturalWidth = (el: HTMLElement): number =>
  el.children.length === 0
    ? el.offsetWidth
    : Array.from(el.children).reduce(
        (n, c) => n + (c instanceof HTMLElement ? c.offsetWidth : 0),
        0,
      );

/** What the bar's content wants, whether or not it overflows: the natural widths of the bar's children, summed. */
const contentWidth = (bar: HTMLElement): number =>
  Array.from(bar.children).reduce(
    (n, c) => n + (c instanceof HTMLElement ? naturalWidth(c) : 0),
    0,
  );

const measure = (bar: HTMLElement): FitMeasure => ({
  client: bar.clientWidth,
  overflow: Math.max(0, bar.scrollWidth - bar.clientWidth),
});

/**
 * How compact the command bar must be to hold its content in one row, in `levels` steps (default 3: 0 full, 1
 * compact, 2 icon-only lifecycle buttons; the dashboard passes 4, ASM-992) (ASM-988). It measures the rendered bar, so it follows the real widths of the readouts and the viewport.
 * It re-measures at mount, after every level change (a step may not be enough) and when a ResizeObserver reports a
 * size change of the bar or of its content, at most once per animation frame (the stream's frame rate cannot cause
 * a storm: a readout changes size only when its digit count changes). Stepping back down needs a margin
 * (`FIT_HYSTERESIS_PX`). The fit restarts from level 0 when the fonts finish loading (the first fit may have been
 * measured with the icon ligatures still showing as words) and when the content shrinks by more than the margin
 * (counters reset by an unload): the widths remembered per level are stale then (ASM-990). The content width counts a bar child by its own children (a child that takes the spare width has the bar's width whatever its readouts need, so its size hides a counter reset; four links, ASM-997), and the observer watches those children too.
 * The default `levels = 3` exists for the pinned tests of the M5b/WI-14 ladder, which call `useBarFit(ref)`
 * (ASM-992); the dashboard passes `BAR_FIT_LEVELS`. A changed `levels` starts the fit again.
 */
export function useBarFit(
  wrapperRef: RefObject<HTMLElement | null>,
  levels: FitLevels = 3,
): FitLevel {
  const [level, setLevel] = useState<FitLevel>(0);
  // One holder for the whole life of the component, created once (lazily) and mutated, never re-created.
  const holder = useRef<{ fit: FitState } | null>(null);
  if (holder.current === null) holder.current = { fit: initialFit(levels) };
  const state = holder.current;
  const shown = useRef<FitLevel>(0);
  const lastContent = useRef<number | null>(null);

  /** Forget everything measured: the ladder starts again at level 0. */
  const reset = useCallback((): void => {
    state.fit = initialFit(levels);
    lastContent.current = null;
  }, [state, levels]);

  const evaluate = useCallback((): void => {
    const bar = barOf(wrapperRef.current);
    if (!bar) return;
    const content = contentWidth(bar);
    if (contentShrank(lastContent.current, content) && state.fit.level !== 0) {
      // Restart: render level 0, whose layout effect measures again.
      reset();
      shown.current = 0;
      setLevel(0);
      return;
    }
    const next = nextFit(state.fit, measure(bar));
    lastContent.current = next.level === state.fit.level ? content : null;
    state.fit = next;
    shown.current = next.level;
    setLevel(next.level);
  }, [wrapperRef, state, reset]);

  const restart = useCallback((): void => {
    reset();
    if (shown.current === 0) evaluate();
    else {
      shown.current = 0;
      setLevel(0);
    }
  }, [evaluate, reset]);

  useEffect(() => {
    const fonts = typeof document === "undefined" ? undefined : document.fonts;
    if (!fonts) return;
    let live = true;
    fonts.addEventListener("loadingdone", restart);
    void fonts.ready.then(() => {
      if (live) restart();
    });
    return () => {
      live = false;
      fonts.removeEventListener("loadingdone", restart);
    };
  }, [restart]);

  // A different ladder length: start again (runs before the measuring effect below).
  // biome-ignore lint/correctness/useExhaustiveDependencies: `levels` is the trigger, `reset` reads it.
  useLayoutEffect(() => {
    reset();
    shown.current = 0;
    setLevel(0);
  }, [levels]);

  // After every render that changed the level: the new level may still overflow.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `level` is the trigger, `evaluate` reads the DOM.
  useLayoutEffect(evaluate, [level, evaluate]);

  // What the observer watches differs per level (labels appear and go): watch the current elements again after every
  // level change (observing an element twice is a no-op).
  const watchAgain = useRef<(() => void) | null>(null);
  useEffect(() => {
    const bar = barOf(wrapperRef.current);
    if (!bar || typeof ResizeObserver === "undefined") return;
    let frame = 0;
    const schedule = (): void => {
      if (frame !== 0) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        evaluate();
      });
    };
    const observer = new ResizeObserver(schedule);
    const watch = (): void => {
      observer.observe(bar);
      for (const child of bar.children) {
        observer.observe(child);
        // A child that takes the spare width keeps its size when a readout inside it shrinks: watch those too.
        for (const inner of child.children) observer.observe(inner);
      }
    };
    watch();
    watchAgain.current = watch;
    return () => {
      watchAgain.current = null;
      observer.disconnect();
      if (frame !== 0) cancelAnimationFrame(frame);
    };
  }, [wrapperRef, evaluate]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `level` is the trigger: the elements differ per level.
  useEffect(() => watchAgain.current?.(), [level]);

  return level;
}
