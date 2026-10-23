import type { EntityFrame, EntitySource } from "@umbra/engine-stream";
import { defaultAffiliationOf } from "@umbra/settings";
import { Alert, useTheme } from "@umbra/ui-kit";
import {
  type ReactElement,
  type Ref,
  useContext,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { AnnounceTimersContext } from "../../context/AnnounceTimersContext.ts";
import { registerE2eHook } from "../../e2eHook.ts";
import type { SelectableMapView } from "../../MapView.ts";
import { BASEMAP_ASSETS } from "../../maplibre/basemapAssets.ts";
import { createMapLibreView } from "../../maplibre/createMapLibreView.ts";
import { t } from "../../messages.ts";
import { createAnnouncer } from "../../model/announce.ts";
import {
  bannerPosition,
  fitOptionsFrom,
  type MapInsets,
  needsFit,
  withInsets,
} from "../../model/camera.ts";
import { resolveMapColors } from "../../model/colors.ts";
import { type AffiliationOf, frameBounds } from "../../model/features.ts";

export interface LiveMapProps {
  /** Where frames come from: `useEntitySource()` of `@umbra/engine-stream`, passed by the shell. */
  source: EntitySource;
  /** Accessible name of the map region. Default "Live map". */
  label?: string;
  /** A MapLibre style URL that replaces the offline basemap (`/config.json` `basemap.styleUrl`). */
  basemapStyleUrl?: string;
  /**
   * Screen space (px) a shell covers with floating UI. Added to the fit padding (`space-16` per side) and
   * used to place the error banner, so entities and errors never sit under the overlay. Read when the
   * camera fits, so changing it never remounts the map.
   */
  insets?: MapInsets;
  /** The selected entity id, owned by the shell (ADR 0078). Drives the ring and the label; never remounts the map. */
  selectedId?: number | null;
  /** A click on the map gives the nearest entity id, a click on empty map or Escape gives null. */
  onSelect?: ((id: number | null) => void) | undefined;
  /** Force to affiliation for the symbols and the ring. Default: the settings default. A new function applies from the next flush. */
  affiliationOf?: AffiliationOf | undefined;
  /** React 19 `ref` prop: the imperative handle. */
  ref?: Ref<LiveMapHandle>;
}

/** What a shell can ask of the map besides props. */
export interface LiveMapHandle {
  /** Moves the camera to the entity in the newest frame. False (camera untouched) when it is not there. */
  centerOn(id: number): boolean;
}

/** What is wrong, which decides the message and whether a theme change retries (ASM-369). */
type MapError = "colors" | "webgl" | "lost";

/**
 * The live map. One MapLibre view per mount; frames go from `source` straight to the view (imperative), so
 * React re-renders only when the error state or the announced count changes, never per frame or per entity.
 * The selection (`selectedId`, `onSelect`) is controlled by the shell; the ring and the label are updated in
 * the view's animation-frame flush, not by React (ADR 0078).
 */
export function LiveMap({
  source,
  label,
  basemapStyleUrl,
  insets,
  selectedId,
  onSelect,
  affiliationOf,
  ref,
}: LiveMapProps): ReactElement {
  const { resolved } = useTheme();
  const timers = useContext(AnnounceTimersContext);
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<SelectableMapView | null>(null);
  const appliedThemeRef = useRef(resolved);
  const themeRef = useRef(resolved);
  themeRef.current = resolved;
  const insetsRef = useRef(insets);
  insetsRef.current = insets;
  const selectedIdRef = useRef(selectedId ?? null);
  selectedIdRef.current = selectedId ?? null;
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;
  const affiliationOfRef = useRef(affiliationOf);
  affiliationOfRef.current = affiliationOf;
  useImperativeHandle(
    ref,
    () => ({ centerOn: (id) => viewRef.current?.centerOn(id) ?? false }),
    [],
  );
  const [announced, setAnnounced] = useState(0);
  const [error, setError] = useState<MapError | null>(null);
  const errorRef = useRef<MapError | null>(null);
  const errorThemeRef = useRef(resolved);
  // Bumped by a theme change while the colours were unreadable: the mount effect runs again.
  const [attempt, setAttempt] = useState(0);

  const fail = (kind: MapError | null): void => {
    errorRef.current = kind;
    setError(kind);
  };

  // biome-ignore lint/correctness/useExhaustiveDependencies: `attempt` only re-triggers the mount after a colour error
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const style = getComputedStyle(document.documentElement);
    let colors: ReturnType<typeof resolveMapColors>;
    let fit: ReturnType<typeof fitOptionsFrom>;
    try {
      fit = fitOptionsFrom(style);
      colors = resolveMapColors(style);
    } catch (e) {
      console.error("map:", e);
      errorThemeRef.current = themeRef.current;
      fail("colors");
      return;
    }
    let view: SelectableMapView;
    try {
      view = createMapLibreView(container, {
        colors,
        theme: themeRef.current,
        basemap: { styleUrl: basemapStyleUrl, data: BASEMAP_ASSETS },
        pixelRatio: window.devicePixelRatio,
        selectedId: selectedIdRef.current,
        onSelect: (id) => onSelectRef.current?.(id),
        affiliationOf: (force) => (affiliationOfRef.current ?? defaultAffiliationOf)(force),
        onError: (e) => console.error("map:", e),
        // A lost context shows the error state until MapLibre has it back; it does not clobber a worse error.
        onContextChange: (lost) => {
          if (lost) fail("lost");
          else if (errorRef.current === "lost") fail(null);
        },
      });
    } catch (e) {
      // MapLibre throws when it cannot get a WebGL context.
      console.error("map:", e);
      fail("webgl");
      return;
    }
    fail(null);
    appliedThemeRef.current = themeRef.current;
    viewRef.current = view;
    const unregisterHook =
      import.meta.env.VITE_UMBRA_E2E === "1" ? registerE2eHook(view) : undefined;

    // The count is announced at most every ~2 s (trailing), from a ref: a changing count costs no render.
    const announcer = createAnnouncer(0, setAnnounced, timers);
    setAnnounced(0);
    let fittedSession: number | undefined;
    const apply = (frame: EntityFrame): void => {
      view.setEntities(frame);
      announcer.report(frame.entities.length);
      if (needsFit(fittedSession, frame)) {
        const bounds = frameBounds(frame);
        // Fitted only when the camera really moved: a zero-size container is retried on the next frame.
        if (bounds && view.fitTo(bounds, withInsets(fit, insetsRef.current)))
          fittedSession = frame.session;
      }
    };
    const current = source.getFrame();
    if (current) apply(current);
    const unsubscribe = source.subscribe(apply);

    return () => {
      unsubscribe();
      announcer.dispose();
      unregisterHook?.();
      view.destroy();
      viewRef.current = null;
    };
  }, [source, basemapStyleUrl, attempt, timers]);

  // Theme switch: re-read the tokens (ThemeProvider has already set data-theme) and re-colour in place.
  // After a colour error at mount there is no view yet: the change retries the mount instead.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `resolved` is the trigger; the rest is read through refs
  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      if (errorRef.current === "colors" && resolved !== errorThemeRef.current)
        setAttempt((n) => n + 1);
      return;
    }
    // The mount already used this theme: nothing to apply (unless a failed change left the error state up).
    if (resolved === appliedThemeRef.current && errorRef.current !== "colors") return;
    try {
      view.setTheme(resolveMapColors(getComputedStyle(document.documentElement)), resolved);
      appliedThemeRef.current = resolved;
      if (errorRef.current === "colors") fail(null);
    } catch (e) {
      console.error("map:", e);
      fail("colors");
    }
  }, [resolved]);

  useEffect(() => {
    viewRef.current?.setSelection(selectedId ?? null);
  }, [selectedId]);

  const name = label ?? t("map.label");
  return (
    <section className="umbra-live-map" aria-label={name}>
      {error === null ? null : (
        <div className="umbra-live-map-error" style={bannerPosition(insets)}>
          <Alert
            tone="danger"
            title={error === "lost" ? t("map.error.lostTitle") : t("map.error.title")}
          >
            {error === "colors"
              ? t("map.error.colors")
              : error === "webgl"
                ? t("map.error.webgl")
                : t("map.error.lostBody")}
          </Alert>
        </div>
      )}
      <div ref={containerRef} className="umbra-live-map-canvas" />
      <p className="vt-visually-hidden" aria-live="polite">
        {t(announced === 1 ? "map.count.one" : "map.count.other", { n: announced })}
      </p>
    </section>
  );
}
