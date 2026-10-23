import { isApiProblem, useGetScenario } from "@umbra/data";
import { Readout } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import { formatSimTime } from "../../model/formatSimTime.ts";
import { problemText } from "../../model/problemText.ts";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./ScenarioSummary.css";

const WHOLE = new Intl.NumberFormat("en-US");

/** Name, path, seed, tick rate and its source, duration and entity count of the loaded scenario (`getScenario`). */
export function ScenarioSummary(): ReactElement {
  const { data, error, isPending } = useGetScenario();
  if (isPending) {
    return (
      <p role="status" className="umbra-scenario-summary-empty">
        {t("summary.loading")}
      </p>
    );
  }
  if (error) {
    if (isApiProblem(error) && error.code === "no_scenario") {
      return <p className="umbra-scenario-summary-empty">{t("summary.none")}</p>;
    }
    return <ProblemAlert problem={problemText(error)} />;
  }
  const ticks = WHOLE.format(data.duration_ticks);
  const duration =
    data.tick_rate_hz > 0
      ? t("summary.durationValue", {
          time: formatSimTime(data.duration_ticks / data.tick_rate_hz),
          ticks,
        })
      : t("summary.durationTicks", { ticks });
  const source =
    data.tick_rate_source === "scenario" || data.tick_rate_source === "default"
      ? t(`summary.source.${data.tick_rate_source}`)
      : String(data.tick_rate_source);
  return (
    <div className="umbra-scenario-summary">
      <Readout label={t("summary.name")} value={data.name} />
      <Readout label={t("summary.path")} value={data.path} />
      <Readout label={t("summary.seed")} value={data.seed} />
      <Readout
        label={t("summary.tickRate")}
        value={t("summary.tickRateValue", { hz: data.tick_rate_hz, source })}
      />
      <Readout label={t("summary.duration")} value={duration} />
      <Readout label={t("summary.entities")} value={WHOLE.format(data.entity_count)} />
    </div>
  );
}
