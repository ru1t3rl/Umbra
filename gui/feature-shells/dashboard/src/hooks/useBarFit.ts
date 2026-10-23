import { type RefObject, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  contentShrank,
  type FitLevel,
  type FitMeasure,
  INITIAL_FIT,
  nextFit,
} from "../model/fitLevel.ts";

/** The bar is the first child of the wrapper (as `useBarInset` reads it). */
const barOf = (wrapper: HTMLElement | null): HTMLElement | null => {
  const bar = wrapper?.firstElementChild;
  return bar instanceof HTMLElement ? bar : null;
};

/** The sum of the bar's children widths: what the bar's content wants, whether or not it overflows. */
const contentWidth = (bar: HTMLElement): number =>
  Array.from(bar.children).reduce((n, c) => n + (c instanceof HTMLElement ? c.offsetWidth : 0), 0);

const measure = (bar: HTMLElement): FitMeasure => ({
  client: bar.clientWidth,
  overflow: Math.max(0, bar.scrollWidth - bar.clientWidth),
});

/**
 * How compact the command bar must be to hold its content in one row: 0 full, 1 compact, 2 icon-only lifecycle
 * buttons (ASM-988). It measures the rendered bar, so it follows the real widths of the readouts and the viewport.
 * It re-measures at mount, after every level change (a step may not be enough) and when a ResizeObserver reports a
 * size change of the bar or of its content, at most once per animation frame (the stream's frame rate cannot cause
 * a storm: a readout changes size only when its digit count changes). Stepping back down needs a margin
 * (`FIT_HYSTERESIS_PX`). The fit restarts from level 0 when the fonts finish loading (the first fit may have been
 * measured with the icon ligatures still showing as words) and when the content shrinks by more than the margin
 * (counters reset by an unload): the widths remembered per level are stale then (ASM-990).
 */
export function useBarFit(wrapperRef: RefObject<HTMLElement | null>): FitLevel {
  const [level, setLevel] = useState<FitLevel>(0);
  const state = useRef(INITIAL_FIT);
  const shown = useRef<FitLevel>(0);
  const lastContent = useRef<number | null>(null);

  const evaluate = useCallback((): void => {
    const bar = barOf(wrapperRef.current);
    if (!bar) return;
    const content = contentWidth(bar);
    if (contentShrank(lastContent.current, content) && state.current.level !== 0) {
      // Restart: render level 0, whose layout effect measures again.
      state.current = INITIAL_FIT;
      lastContent.current = null;
      shown.current = 0;
      setLevel(0);
      return;
    }
    const next = nextFit(state.current, measure(bar));
    lastContent.current = next.level === state.current.level ? content : null;
    state.current = next;
    shown.current = next.level;
    setLevel(next.level);
  }, [wrapperRef]);

  const restart = useCallback((): void => {
    state.current = INITIAL_FIT;
    lastContent.current = null;
    if (shown.current === 0) evaluate();
    else {
      shown.current = 0;
      setLevel(0);
    }
  }, [evaluate]);

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

  // After every render that changed the level: the new level may still overflow.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `level` is the trigger, `evaluate` reads the DOM.
  useLayoutEffect(evaluate, [level, evaluate]);

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
    observer.observe(bar);
    for (const child of bar.children) observer.observe(child);
    return () => {
      observer.disconnect();
      if (frame !== 0) cancelAnimationFrame(frame);
    };
  }, [wrapperRef, evaluate]);

  return level;
}
