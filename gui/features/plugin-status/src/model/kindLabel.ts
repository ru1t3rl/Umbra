import { ComponentKind } from "@umbra/data";
import { t } from "../messages.ts";

/** The display label of a component kind; a kind this version does not know comes back verbatim (ASM-822). */
export function kindLabel(kind: string): string {
  return Object.hasOwn(ComponentKind, kind) ? t(`kind.${kind as ComponentKind}`) : kind;
}
