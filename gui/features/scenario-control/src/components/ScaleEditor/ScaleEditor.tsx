import { Button, Slider, TextField } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";
import {
  formatScale,
  parseScale,
  positionFromScale,
  SLIDER,
  scaleFromPosition,
} from "../../model/clockScale.ts";
import "./ScaleEditor.css";

export interface ScaleEditorProps {
  /** The scale the engine reports; the editor is keyed by it, so a new engine scale replaces the draft. */
  current: number;
  disabled: boolean;
  onApply: (scale: number) => void;
}

/** The scale draft: dragging or typing sends nothing, Apply does (a drag must not fire a request per pixel). */
export function ScaleEditor({ current, disabled, onApply }: ScaleEditorProps): ReactElement {
  const [text, setText] = useState(String(current));
  const draft = parseScale(text);
  const shown = draft ?? current;
  return (
    <div className="umbra-clock-control-scale">
      <Slider
        label={t("clock.scale")}
        min={SLIDER.min}
        max={SLIDER.max}
        step={SLIDER.step}
        value={positionFromScale(shown)}
        format={() => formatScale(shown)}
        disabled={disabled}
        onChange={(e) => setText(String(scaleFromPosition(Number(e.target.value))))}
      />
      <TextField
        label={t("clock.scaleFactor")}
        help={t("clock.scaleHelp")}
        mono
        inputMode="decimal"
        value={text}
        invalid={draft === null}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
      />
      <Button
        size="sm"
        disabled={disabled || draft === null || draft === current}
        onClick={() => draft !== null && onApply(draft)}
      >
        {t("clock.apply")}
      </Button>
    </div>
  );
}
