import { Slider } from "@umbra/ui-kit";
import { type ReactElement, useEffect, useRef, useState } from "react";
import { useSettings } from "../../hooks/useSettings.ts";
import { useUpdateSettings } from "../../hooks/useUpdateSettings.ts";
import { t } from "../../messages.ts";

/**
 * Edits `settings.stream.maxRateHz` (1..60 Hz). A drag shows the value as it moves (React's `onChange` is the
 * `input` event) and commits it on the native `change` event, which a range input fires when the gesture ends (and
 * for each keyboard step): one write, one subscribe, however long the drag. The subscribe that applies the rate is
 * sent by the app, not here (ASM-995 d, ASM-997).
 */
export function StreamRateSetting(): ReactElement {
  const stored = useSettings().stream.maxRateHz;
  const update = useUpdateSettings();
  const [draft, setDraft] = useState<number | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const commit = useRef(update);
  commit.current = update;

  // The stored rate is the truth again once it changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `stored` is the trigger.
  useEffect(() => setDraft(null), [stored]);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const onChange = (e: Event): void => {
      if (e.target instanceof HTMLInputElement) {
        commit.current({ stream: { maxRateHz: Number(e.target.value) } });
      }
    };
    el.addEventListener("change", onChange);
    return () => el.removeEventListener("change", onChange);
  }, []);

  return (
    <div ref={wrap}>
      <Slider
        label={t("rate.label")}
        min={1}
        max={60}
        step={1}
        value={draft ?? stored}
        format={(n) => t("rate.value", { rate: n })}
        onChange={(e) => setDraft(Number(e.target.value))}
      />
    </div>
  );
}
