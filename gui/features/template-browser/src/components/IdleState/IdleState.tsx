import { AppLink } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import "./IdleState.css";

/** The empty state of both exports while no scenario is loaded (ASM-948). */
export function IdleState(): ReactElement {
  return (
    <div className="umbra-template-idle">
      <p className="type-body">{t("idle.title")}</p>
      <AppLink to="/" className="type-body">
        {t("idle.link")}
      </AppLink>
    </div>
  );
}
