import type { ReactElement } from "react";
import { t } from "../../messages.ts";

export function Placeholder(): ReactElement {
  return (
    <main>
      <h1>{t("app.placeholder")}</h1>
    </main>
  );
}
