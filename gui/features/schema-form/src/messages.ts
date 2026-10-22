import { defineMessages } from "@umbra/ui-kit";

/** All user-visible text of the package, including what the pure model embeds (ui-kit helper, ASM-333). */
export const t = defineMessages({
  required: "Required",
  unit: "Unit: {unit}",
  default: "Default: {value}",
  usingDefault: "Using default: {value}",
  shownAsJson: "Shown as JSON",
  otherParameters: "Other parameters",
  moreItems: "...and {count} more",
  notSet: "(not set)",
  boundMin: "min {n}",
  boundMax: "max {n}",
  boundGreaterThan: "greater than {n}",
  boundLessThan: "less than {n}",
  functionValue: "[function]",
  symbolValue: "[symbol]",
  undefinedValue: "undefined",
  invalidDate: "Invalid Date",
  circular: "[Circular]",
  tooDeep: "[Too deep]",
  unreadable: "[Unreadable]",
  truncated: "… (truncated)",
  moreKeys: "{count} more keys not shown",
  moreFields: "{count} more fields not shown (last: {last})",
  jsonNotShown: "JSON text not shown (size limit)",
});
