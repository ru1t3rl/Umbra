import type { EntitySummary } from "@umbra/engine-stream";
import { Readout } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import {
  formatHeading,
  formatPosition,
  formatRoute,
  formatSpeed,
  NO_VALUE,
} from "../../model/format.ts";

export interface InspectorReadoutsProps {
  entity: EntitySummary;
  /** The REST motion model; undefined until the detail arrived. */
  motionModel: string | undefined;
  /** Values change at runtime: shown in the live (cyan) style. */
  live: boolean;
}

/** The stream values of one entity as Readouts, and the REST motion model. */
export function InspectorReadouts({
  entity,
  motionModel,
  live,
}: InspectorReadoutsProps): ReactElement {
  const position = formatPosition(entity);
  return (
    <div className="umbra-entity-inspector-readouts">
      <Readout
        label={t("field.latitude")}
        value={position.lat}
        unit={t("unit.degrees")}
        live={live}
      />
      <Readout
        label={t("field.longitude")}
        value={position.lon}
        unit={t("unit.degrees")}
        live={live}
      />
      <Readout
        label={t("field.altitude")}
        value={position.alt}
        unit={t("unit.metres")}
        live={live}
      />
      <Readout
        label={t("field.heading")}
        value={formatHeading(entity.heading_deg)}
        unit={t("unit.degrees")}
        live={live}
      />
      <Readout
        label={t("field.speed")}
        value={formatSpeed(entity.speed_mps)}
        unit={t("unit.speed")}
        live={live}
      />
      <Readout label={t("field.route")} value={formatRoute(entity.route)} live={live} />
      <Readout label={t("field.status")} value={entity.status || NO_VALUE} />
      <Readout label={t("field.template")} value={entity.template || NO_VALUE} />
      <Readout label={t("field.motionModel")} value={motionModel ?? NO_VALUE} />
      {entity.ownership === "remote" ? (
        <Readout label={t("field.source")} value={entity.remote_source || NO_VALUE} />
      ) : null}
    </div>
  );
}
