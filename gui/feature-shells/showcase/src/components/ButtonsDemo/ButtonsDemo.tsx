import { Button, Chip, ChipGroup, Toggle } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";
import { RATES } from "../../rates.ts";

export function ButtonsDemo(): ReactElement {
  const [rate, setRate] = useState(t("sample.1-4"));
  const [layers, setLayers] = useState({ tracks: true, grid: false });
  const [history, setHistory] = useState(true);
  const [record, setRecord] = useState(false);
  return (
    <>
      <div className="showcase-row">
        <Button variant="filled" icon="play_arrow">
          {t("sample.start-exercise")}
        </Button>
        <Button>{t("sample.assign-route")}</Button>
        <Button variant="outline">{t("sample.export-log")}</Button>
        <Button variant="ghost">{t("sample.cancel")}</Button>
        <Button variant="danger" icon="delete">
          {t("sample.remove-entity")}
        </Button>
        <Button variant="outline" icon="settings" aria-label={t("sample.settings")} />
        <Button size="sm">{t("sample.small")}</Button>
        <Button disabled>{t("sample.disabled")}</Button>
      </div>
      <div className="showcase-row">
        <ChipGroup label={t("sample.time-rate")}>
          {RATES.slice(0, 4).map((r) => (
            <Chip key={r} selected={rate === r} onClick={() => setRate(r)}>
              {r}
            </Chip>
          ))}
        </ChipGroup>
        <ChipGroup label={t("sample.map-layers")}>
          <Chip
            glass
            icon="timeline"
            selected={layers.tracks}
            onClick={() => setLayers({ ...layers, tracks: !layers.tracks })}
          >
            {t("sample.tracks")}
          </Chip>
          <Chip
            glass
            icon="grid_on"
            selected={layers.grid}
            onClick={() => setLayers({ ...layers, grid: !layers.grid })}
          >
            {t("sample.mgrs-grid")}
          </Chip>
        </ChipGroup>
      </div>
      <div className="showcase-row">
        <Toggle checked={history} onChange={setHistory} label={t("sample.show-historic-tracks")} />
        <Toggle checked={record} onChange={setRecord} label={t("sample.record-session")} />
        <Toggle checked disabled label={t("sample.terrain-loaded")} />
      </div>
    </>
  );
}
