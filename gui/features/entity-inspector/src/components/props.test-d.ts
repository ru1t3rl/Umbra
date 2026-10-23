// WI-10 prop contracts (compile-time; `tsc -b` checks them, ASM-937 d).
import type { EntitySource } from "@umbra/engine-stream";
import type { EntityInspectorProps, EntityListPanelProps } from "../index.ts";

declare const source: EntitySource;
const friend = () => "friend" as const;

export const list: EntityListPanelProps = {
  source,
  selectedId: null,
  onSelect: () => {},
  affiliationOf: friend,
};
export const listSelected: EntityListPanelProps = { ...list, selectedId: 12 };
// @ts-expect-error source and affiliationOf are required
export const listMissing: EntityListPanelProps = { selectedId: null, onSelect: () => {} };

export const inspector: EntityInspectorProps = {
  source,
  selectedId: 12,
  onSelect: () => {},
  onClose: () => {},
  affiliationOf: friend,
};
export const inspectorCenter: EntityInspectorProps = {
  ...inspector,
  onCenter: (_id: number) => {},
};
// @ts-expect-error onClose is required: the shell decides what closing means
export const inspectorNoClose: EntityInspectorProps = {
  source,
  selectedId: 12,
  onSelect: () => {},
  affiliationOf: friend,
};
