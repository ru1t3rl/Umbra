import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import "./LoadingState.css";

/** The line shown until the first answer of a read arrives (ASM-967). */
export function LoadingState(): ReactElement {
  return (
    <p className="umbra-status-loading type-caption" role="status">
      {t("loading")}
    </p>
  );
}
