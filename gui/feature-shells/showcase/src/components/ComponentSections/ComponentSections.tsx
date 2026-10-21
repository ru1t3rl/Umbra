import { AffiliationTag, Alert, Badge, Button, Card, Panel, TextField } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import { ButtonsDemo } from "../ButtonsDemo/ButtonsDemo.tsx";
import { CommandBarDemo } from "../CommandBarDemo/CommandBarDemo.tsx";
import { DataDemo } from "../DataDemo/DataDemo.tsx";
import { InputsDemo } from "../InputsDemo/InputsDemo.tsx";
import { Section } from "../Section/Section.tsx";

export function ComponentSections(): ReactElement {
  return (
    <div id="showcase-components" className="showcase-group">
      <Section id="buttons" heading={t("buttons.heading")}>
        <ButtonsDemo />
      </Section>
      <Section id="inputs" heading={t("inputs.heading")}>
        <InputsDemo />
      </Section>
      <Section id="panel" heading={t("panel.heading")}>
        <div className="showcase-row">
          <Panel
            title={t("sample.movement-orders")}
            actions={
              <>
                <Button
                  size="sm"
                  variant="ghost"
                  icon="open_in_full"
                  aria-label={t("sample.expand")}
                />
                <Button size="sm" variant="ghost" icon="close" aria-label={t("sample.close")} />
              </>
            }
          >
            <div className="showcase-col showcase-flush">
              <p className="showcase-muted">Alpha Company, 2nd Mechanised Brigade</p>
              <TextField
                label={t("sample.destination")}
                mono
                defaultValue={t("sample.31u-ft-12345-67890")}
              />
              <div className="showcase-row showcase-end">
                <Button variant="ghost" size="sm">
                  {t("sample.discard")}
                </Button>
                <Button variant="filled" size="sm">
                  {t("sample.issue-order")}
                </Button>
              </div>
            </div>
          </Panel>
          <Card
            title={t("sample.2nd-mechanised-brigade")}
            subtitle={t("sample.48-entities-3-battalions")}
          >
            <div className="showcase-row showcase-flush">
              <AffiliationTag affiliation="friend">{t("sample.bde")}</AffiliationTag>
              <Badge tone="success">{t("sample.running")}</Badge>
            </div>
          </Card>
          <Card
            level={3}
            title={t("sample.red-force-reserve")}
            subtitle={t("sample.12-entities-held")}
          >
            <div className="showcase-row showcase-flush">
              <AffiliationTag affiliation="hostile">{t("sample.bn")}</AffiliationTag>
              <Badge tone="info">{t("sample.paused")}</Badge>
            </div>
          </Card>
          <Card
            variant="outline"
            title={t("sample.add-formation")}
            subtitle={t("sample.from-template-or-empty")}
          />
        </div>
      </Section>
      <Section id="commandbar" heading={t("commandbar.heading")}>
        <CommandBarDemo />
      </Section>
      <Section id="data" heading={t("data.heading")}>
        <DataDemo />
      </Section>
      <Section id="alerts" heading={t("alerts.heading")}>
        <div className="showcase-col">
          <Alert
            tone="danger"
            title={t("sample.route-rejected")}
            action={
              <Button size="sm" variant="ghost">
                {t("sample.edit-route")}
              </Button>
            }
          >
            {t("sample.the-destination-lies-outside")}
          </Alert>
          <Alert
            tone="warning"
            title={t("sample.federate-latency-480-ms")}
            action={
              <Button size="sm" variant="ghost">
                {t("sample.details")}
              </Button>
            }
          >
            {t("sample.updates-from-the-flight-simu")}
          </Alert>
          <Alert tone="success" title={t("sample.recording-started")}>
            {t("sample.session-is-being-written-to-")}
          </Alert>
          <Alert tone="info" title={t("sample.12-entities-paused")}>
            {t("sample.resume-the-run-to-continue-t")}
          </Alert>
        </div>
      </Section>
    </div>
  );
}
