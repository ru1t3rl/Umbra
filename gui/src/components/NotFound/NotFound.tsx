import type { ReactElement } from "react";
import { t } from "../../messages.ts";

export function NotFound(): ReactElement {
  return (
    <main>
      <h1>{t("app.notFound")}</h1>
    </main>
  );
}
