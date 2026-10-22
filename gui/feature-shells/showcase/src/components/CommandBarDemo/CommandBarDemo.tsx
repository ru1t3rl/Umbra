import { Badge, Button, Chip, CommandBar } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";

export function CommandBarDemo(): ReactElement {
  const [paused, setPaused] = useState(false);
  return (
    <>
      <CommandBar
        title={t("sample.exercise-northern-shield")}
        subtitle={t("sample.run-3-48-entities")}
        clock={t("sample.14-32-07z")}
        paused={paused}
      >
        <Chip selected>1:4</Chip>
        {paused ? <Badge tone="info">{t("sample.paused")}</Badge> : null}
        <Button
          variant="outline"
          size="sm"
          icon={paused ? "play_arrow" : "pause"}
          onClick={() => setPaused(!paused)}
        >
          {paused ? t("sample.resume") : t("sample.pause")}
        </Button>
        <Button variant="filled" size="sm" icon="flag">
          {t("sample.end-run")}
        </Button>
      </CommandBar>
      <CommandBar
        floating
        title={t("sample.northern-shield")}
        subtitle={t("sample.review")}
        clock={t("sample.09-12-44z")}
        paused
      >
        <Badge tone="info">{t("sample.paused")}</Badge>
        <Button
          variant="ghost"
          size="sm"
          icon="skip_previous"
          aria-label={t("sample.previous-bookmark")}
        />
        <Button variant="tonal" size="sm" icon="play_arrow">
          {t("sample.play")}
        </Button>
        <Button variant="ghost" size="sm" icon="skip_next" aria-label={t("sample.next-bookmark")} />
      </CommandBar>
    </>
  );
}
