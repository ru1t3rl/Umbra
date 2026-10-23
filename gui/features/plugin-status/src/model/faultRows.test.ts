// WI-13 `faultRows(report, run, adapters)`: one row per fault, from the plugin report, the run status and the
// adapter list (ASM-849 contract-shaped fixtures; ASM-955 row shape and order).
import { describe, expect, test } from "bun:test";
import type { AdapterDto, PluginReport } from "@umbra/data";
import { ADAPTERS, NO_PLUGINS, PLUGINS } from "../statusTesting.tsx";
import { faultRows } from "./faultRows.ts";

const healthyAdapters = ADAPTERS.map((a) => ({ ...a, faulted: false, fault_reason: null }));
const healthyPlugins: PluginReport = {
  plugins: PLUGINS.plugins.map((p) => ({
    ...p,
    capabilities: p.capabilities.map((c) => ({ ...c, faulted: false })),
  })),
};
const pluginFault = {
  plugin: "acme.radar",
  capability: "acme.radar.track",
  call: "step",
  message: "panicked at lib.rs:12",
};
const networkFault = { adapter: "dis-main", reason: "socket closed" };

describe("faultRows", () => {
  test("no fault anywhere gives no rows", () => {
    expect(faultRows(healthyPlugins, {}, healthyAdapters)).toEqual([]);
  });

  test("nothing loaded yet (all undefined) gives no rows", () => {
    expect(faultRows(undefined, undefined, undefined)).toEqual([]);
  });

  test("an empty report, run and adapter list give no rows", () => {
    expect(faultRows(NO_PLUGINS, {}, [])).toEqual([]);
  });

  test("a plugin fault in the report is one row with plugin / capability and call: message", () => {
    expect(
      faultRows({ ...healthyPlugins, fault: pluginFault }, undefined, healthyAdapters),
    ).toEqual([
      {
        key: "plugin_fault",
        type: "plugin_fault",
        subject: "acme.radar / acme.radar.track",
        detail: "step: panicked at lib.rs:12",
      },
    ]);
  });

  test("a plugin fault only in the run status is still shown", () => {
    const rows = faultRows(healthyPlugins, { fault: pluginFault }, healthyAdapters);
    expect(rows.map((r) => r.type)).toEqual(["plugin_fault"]);
  });

  test("the same plugin fault in the report and the run status is one row, the report wins", () => {
    const rows = faultRows(
      { ...healthyPlugins, fault: pluginFault },
      { fault: { ...pluginFault, message: "other" } },
      healthyAdapters,
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]?.detail).toBe("step: panicked at lib.rs:12");
  });

  test("a network fault in the run status is one row with the adapter and the reason", () => {
    expect(faultRows(healthyPlugins, { network_fault: networkFault }, healthyAdapters)).toEqual([
      { key: "network_fault", type: "network_fault", subject: "dis-main", detail: "socket closed" },
    ]);
  });

  test("a null fault and a null network fault are no faults", () => {
    expect(
      faultRows(
        { ...healthyPlugins, fault: null },
        { fault: null, network_fault: null },
        healthyAdapters,
      ),
    ).toEqual([]);
  });

  test("a faulted capability is one row with plugin / capability and the kind label", () => {
    expect(faultRows(PLUGINS, undefined, healthyAdapters)).toEqual([
      {
        key: "capability:acme.radar:acme.radar.track",
        type: "capability",
        subject: "acme.radar / acme.radar.track",
        detail: "Behavior",
      },
    ]);
  });

  test("a faulted adapter is one row with its name and fault reason", () => {
    expect(faultRows(healthyPlugins, undefined, ADAPTERS)).toEqual([
      { key: "adapter:hla-lab", type: "adapter", subject: "hla-lab", detail: "socket closed" },
    ]);
  });

  test("a faulted adapter without a reason has an empty detail", () => {
    const adapters: AdapterDto[] = [
      { ...ADAPTERS[1], faulted: true, fault_reason: null } as AdapterDto,
    ];
    expect(faultRows(healthyPlugins, undefined, adapters)[0]?.detail).toBe("");
  });

  test("all four kinds together come in order: plugin fault, network fault, capabilities, adapters", () => {
    // distinct subjects: the plugin fault is not on a faulted capability, the network fault not on a faulted adapter
    // contract-realistic: the faulted capability and adapter behind the two faults are marked faulted too
    const plugins: PluginReport = {
      ...PLUGINS,
      fault: { ...pluginFault, plugin: "acme.dis-plugin", capability: "acme.dis" },
      plugins: PLUGINS.plugins.map((p) => ({
        ...p,
        capabilities: p.capabilities.map((c) =>
          c.id === "acme.dis" ? { ...c, faulted: true } : c,
        ),
      })),
    };
    const adapters = ADAPTERS.map((a) => (a.name === "dis-main" ? { ...a, faulted: true } : a));
    const rows = faultRows(plugins, { network_fault: networkFault }, adapters);
    expect(rows.map((r) => r.type)).toEqual([
      "plugin_fault",
      "network_fault",
      "capability",
      "adapter",
    ]);
  });

  test("row keys are unique, also for two faulted capabilities and two faulted adapters", () => {
    const plugins: PluginReport = {
      plugins: PLUGINS.plugins.map((p) => ({
        ...p,
        capabilities: p.capabilities.map((c) => ({ ...c, faulted: true })),
      })),
    };
    const adapters = ADAPTERS.map((a) => ({ ...a, faulted: true }));
    const rows = faultRows(plugins, undefined, adapters);
    expect(rows).toHaveLength(3 + 2);
    expect(new Set(rows.map((r) => r.key)).size).toBe(rows.length);
  });

  test("an unknown capability kind is shown verbatim in the detail", () => {
    const plugins = {
      plugins: [
        {
          ...PLUGINS.plugins[0],
          capabilities: [{ id: "x.y", kind: "quantum", version: "1", faulted: true }],
        },
      ],
    } as unknown as PluginReport;
    expect(faultRows(plugins, undefined, [])[0]?.detail).toBe("quantum");
  });

  test("a faulted capability that is the plugin fault's capability is not a second row (ASM-966)", () => {
    const rows = faultRows({ ...PLUGINS, fault: pluginFault }, undefined, healthyAdapters);
    expect(rows.map((r) => r.type)).toEqual(["plugin_fault"]);
  });

  test("the plugin fault also hides the capability when it only comes from the run status", () => {
    const rows = faultRows(PLUGINS, { fault: pluginFault }, healthyAdapters);
    expect(rows.map((r) => r.type)).toEqual(["plugin_fault"]);
  });

  test("the same capability id in another plugin, or another capability, still shows", () => {
    const rows = faultRows(
      { ...PLUGINS, fault: { ...pluginFault, plugin: "other.plugin" } },
      undefined,
      healthyAdapters,
    );
    expect(rows.map((r) => r.type)).toEqual(["plugin_fault", "capability"]);
    const other = faultRows(
      { ...PLUGINS, fault: { ...pluginFault, capability: "acme.radar.scan" } },
      undefined,
      healthyAdapters,
    );
    expect(other.map((r) => r.type)).toEqual(["plugin_fault", "capability"]);
  });

  test("a faulted adapter that is the network fault's adapter is not a second row (ASM-966)", () => {
    const adapters = ADAPTERS.map((a) => ({ ...a, faulted: true }));
    const rows = faultRows(
      healthyPlugins,
      { network_fault: { adapter: "dis-main", reason: "socket closed" } },
      adapters,
    );
    expect(rows.map((r) => `${r.type}:${r.subject}`)).toEqual([
      "network_fault:dis-main",
      "adapter:hla-lab",
    ]);
  });
});
