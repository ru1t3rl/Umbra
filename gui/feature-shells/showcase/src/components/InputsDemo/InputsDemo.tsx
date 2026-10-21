import { Slider, Tabs, TextField } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";
import { RATES } from "../../rates.ts";

export function InputsDemo(): ReactElement {
  const [rate, setRate] = useState(2);
  const [hz, setHz] = useState(60);
  const [tab, setTab] = useState("orbat");
  return (
    <>
      <div className="showcase-col">
        <TextField
          label={t("sample.callsign")}
          placeholder={t("sample.e-g-alpha-1-1")}
          help={t("sample.shown-on-the-map-label")}
        />
        <TextField
          label={t("sample.position-mgrs")}
          mono
          defaultValue={t("sample.31u-ft-12345-67890")}
        />
        <TextField
          label={t("sample.march-speed")}
          mono
          defaultValue={t("sample.140-km-h")}
          invalid
          help={t("sample.exceeds-the-platform-maximum")}
        />
      </div>
      <div className="showcase-col">
        <Slider
          label={t("sample.time-rate")}
          min={0}
          max={4}
          step={1}
          value={rate}
          onChange={(e) => setRate(Number(e.target.value))}
          format={(v) => RATES[v] ?? String(v)}
        />
        <Slider
          label={t("sample.update-rate")}
          min={1}
          max={120}
          value={hz}
          onChange={(e) => setHz(Number(e.target.value))}
          format={(v) => `${v} Hz`}
        />
      </div>
      <Tabs
        label={t("sample.sidebar")}
        value={tab}
        onChange={setTab}
        items={[
          { value: "orbat", label: t("sample.order-of-battle"), count: 48 },
          { value: "orders", label: t("sample.orders"), count: 6 },
          { value: "events", label: t("sample.event-log"), count: 1203 },
          { value: "settings", label: t("sample.settings") },
        ]}
      />
    </>
  );
}
