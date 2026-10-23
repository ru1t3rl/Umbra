import { type EntitySource, useRunStatus } from "@umbra/engine-stream";
import { AffiliationTag, Alert, Badge, Button, Panel } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { useEntityDetail } from "../../hooks/useEntityDetail.ts";
import { useSelectedEntity } from "../../hooks/useSelectedEntity.ts";
import { useSessionGuard } from "../../hooks/useSessionGuard.ts";
import { t } from "../../messages.ts";
import { entityName } from "../../model/entityName.ts";
import type { AffiliationOf } from "../../model/entityRowModel.ts";
import { ContactList } from "../ContactList/ContactList.tsx";
import { InspectorReadouts } from "../InspectorReadouts/InspectorReadouts.tsx";
import { RemoveEntityControl } from "../RemoveEntityControl/RemoveEntityControl.tsx";
import "./EntityInspector.css";

export interface EntityInspectorProps {
  /** The connection's entity source; the run state and the session come from the same stream. */
  source: EntitySource;
  selectedId: number | null;
  onSelect: (id: number | null) => void;
  /** Adds a "Center on map" button. */
  onCenter?: ((id: number) => void) | undefined;
  /** The shell decides what closing means. */
  onClose: () => void;
  affiliationOf: AffiliationOf;
}

/** The selected entity: live readouts from the stream, motion model and contacts from REST (ASM-806, ASM-807). */
export function EntityInspector({
  source,
  selectedId,
  onSelect,
  onCenter,
  onClose,
  affiliationOf,
}: EntityInspectorProps): ReactElement | null {
  const { frame, entity, present } = useSelectedEntity(source, selectedId);
  const running = useRunStatus()?.state === "running";
  const { detail, failed } = useEntityDetail(selectedId, running && present);
  useSessionGuard(source, selectedId !== null, () => onSelect(null));
  if (selectedId === null) return null;

  const title = entityName({ id: selectedId, name: entity?.name });
  const owned = entity?.ownership === "owned";
  return (
    <div className="umbra-entity-inspector" data-stale={present ? undefined : "true"}>
      <Panel
        title={title}
        width="100%"
        actions={
          <Button
            variant="ghost"
            size="sm"
            icon="close"
            aria-label={t("inspector.close")}
            onClick={onClose}
          />
        }
      >
        {entity ? (
          <div className="umbra-entity-inspector-badges">
            <AffiliationTag affiliation={affiliationOf(entity.force)} />
            {entity.ownership === "remote" ? <Badge tone="info">{t("list.remote")}</Badge> : null}
          </div>
        ) : null}
        {present ? null : (
          <Alert
            tone="warning"
            title={t("inspector.gone")}
            className="umbra-entity-inspector-gone"
          />
        )}
        {entity ? (
          <InspectorReadouts
            entity={entity}
            motionModel={detail?.motion_model}
            live={running && present}
          />
        ) : null}
        {failed ? <p className="type-caption">{t("inspector.detailUnavailable")}</p> : null}
        {detail ? (
          <ContactList contacts={detail.contacts} frame={frame} onSelect={onSelect} />
        ) : null}
        <div className="umbra-entity-inspector-actions">
          {onCenter ? (
            <Button variant="outline" size="sm" onClick={() => onCenter(selectedId)}>
              {t("inspector.center")}
            </Button>
          ) : null}
          {owned ? (
            <RemoveEntityControl key={selectedId} id={selectedId} disabled={!present} />
          ) : null}
        </div>
      </Panel>
    </div>
  );
}
