import type { AdapterDto, PluginFaultDto, PluginReport } from "@umbra/data";
import { t } from "../messages.ts";
import { kindLabel } from "./kindLabel.ts";

export type FaultType = "plugin_fault" | "network_fault" | "capability" | "adapter";

export interface FaultRow {
  readonly key: string;
  readonly type: FaultType;
  readonly subject: string;
  readonly detail: string;
}

/** The parts of the run status that carry a fault (the stream's `RunStatus` fits). */
export interface RunFaults {
  readonly fault?: PluginFaultDto | null;
  readonly network_fault?: { readonly adapter: string; readonly reason: string } | null;
}

/**
 * One row per fault, in the order plugin fault, network fault, faulted capabilities, faulted adapters. The
 * report's plugin fault wins over the run status's (the same fault is one row); a capability that carries the
 * plugin fault and an adapter that carries the network fault have no row of their own; any input may be missing.
 */
export function faultRows(
  report: PluginReport | undefined,
  run: RunFaults | undefined,
  adapters: readonly AdapterDto[] | undefined,
): FaultRow[] {
  const rows: FaultRow[] = [];
  const fault = report?.fault ?? run?.fault;
  if (fault)
    rows.push({
      key: "plugin_fault",
      type: "plugin_fault",
      subject: `${fault.plugin} / ${fault.capability}`,
      detail: `${fault.call}: ${fault.message}`,
    });
  const net = run?.network_fault;
  if (net)
    rows.push({
      key: "network_fault",
      type: "network_fault",
      subject: net.adapter,
      detail: net.reason,
    });
  // The host marks the capability faulted and sets the plugin fault for one failure: one row, the richer one.
  for (const plugin of report?.plugins ?? [])
    for (const cap of plugin.capabilities)
      if (cap.faulted && !(fault?.plugin === plugin.id && fault.capability === cap.id))
        rows.push({
          key: `capability:${plugin.id}:${cap.id}`,
          type: "capability",
          subject: `${plugin.id} / ${cap.id}`,
          detail: kindLabel(cap.kind),
        });
  // A network fault always comes with its adapter faulted: one row, the richer one.
  for (const adapter of adapters ?? [])
    if (adapter.faulted && adapter.name !== net?.adapter)
      rows.push({
        key: `adapter:${adapter.name}`,
        type: "adapter",
        subject: adapter.name,
        detail: adapter.fault_reason ?? "",
      });
  return rows;
}

/** The Badge text of a fault type. */
export const faultTypeLabel = (type: FaultType): string => t(`faults.${type}`);
