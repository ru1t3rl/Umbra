import { t } from "./messages.ts";
import type { HelpMessages } from "./model/helpText.ts";

export const helpMessages: HelpMessages = {
  required: t("required"),
  unit: (unit) => t("unit", { unit }),
  default: (value) => t("default", { value }),
  usingDefault: (value) => t("usingDefault", { value }),
};
