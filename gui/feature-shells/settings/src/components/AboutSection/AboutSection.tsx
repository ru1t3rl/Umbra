import { useGetStatus } from "@umbra/data";
import { AppLink } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { APP_VERSION } from "../../appVersion.ts";
import { t } from "../../messages.ts";
import "./AboutSection.css";

/** The About list: the engine's contract version (`GET /status`), the app version and the showcase link. */
export function AboutSection(): ReactElement {
  const { data } = useGetStatus();
  return (
    <>
      <dl className="about-list">
        <dt>{t("about.apiVersion")}</dt>
        <dd>{data?.api_version ?? t("about.unavailable")}</dd>
        <dt>{t("about.appVersion")}</dt>
        <dd>{APP_VERSION}</dd>
      </dl>
      <AppLink to="/showcase">{t("about.showcase")}</AppLink>
    </>
  );
}
