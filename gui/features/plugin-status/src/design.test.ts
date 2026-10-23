// WI-13 design and layout pins, by source text (A-003 `apply-design-system`, ADR 0046). Catches: a raw HTML control
// or table instead of the ui-kit component, a design value outside the tokens, a component or hook outside its
// own file, a direct `fetch` or a generated request function (features read data only through `@umbra/data` hooks),
// and a dependency on another domain feature.
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL(".", import.meta.url).pathname;
const files = (dir: string, ext: RegExp): string[] =>
  readdirSync(join(root, dir), { recursive: true, encoding: "utf8" })
    .filter((f) => ext.test(f) && !/\.test\./.test(f))
    .map((f) => join(dir, f));
const read = (f: string) => readFileSync(join(root, f), "utf8");

const components = files("components", /\.tsx$/);
const styles = files("components", /\.css$/);
const all = (): string[] => files(".", /\.tsx?$/).filter((f) => !/Testing\./.test(f));

describe("plugin-status design", () => {
  test("has the components the plan names, each in its own folder and file", () => {
    for (const name of ["PluginList", "AdapterList", "FaultSummary", "ComponentList"])
      expect(components).toContain(`components/${name}/${name}.tsx`);
  });

  test("a component uses no raw interactive HTML control and no raw table: ui-kit components only", () => {
    expect(components.length).toBeGreaterThan(0);
    for (const f of components) {
      expect(read(f), f).not.toMatch(/<(button|input|select|textarea)\b/);
      expect(read(f), f).not.toMatch(/<(table|thead|tbody|tr|th|td)\b/);
    }
  });

  test("components import ui-kit by package name and never deep", () => {
    const text = components.map(read).join("\n");
    expect(text).toMatch(/from\s*["']@umbra\/ui-kit["']/);
    expect(text).not.toMatch(/@umbra\/[a-z-]+\/src/);
  });

  test("plugins and adapters use Card and Table, adapters use Toggle, faults and statuses use Badge", () => {
    expect(read("components/PluginList/PluginList.tsx")).toMatch(/\bCard\b/);
    expect(read("components/PluginList/PluginList.tsx")).toMatch(/\bTable\b/);
    expect(read("components/AdapterList/AdapterList.tsx")).toMatch(/\bCard\b/);
    expect(read("components/AdapterList/AdapterList.tsx")).toMatch(/\bToggle\b/);
    expect(read("components/AdapterList/AdapterList.tsx")).toMatch(/\bTable\b/);
    expect(read("components/FaultSummary/FaultSummary.tsx")).toMatch(/\bBadge\b/);
    expect(read("components/ComponentList/ComponentList.tsx")).toMatch(/\bCard\b/);
    expect(read("components/ComponentList/ComponentList.tsx")).toMatch(/\bBadge\b/);
  });

  test("the empty state is one private component using the ui-kit AppLink, used by every export", () => {
    const idle = components.find((f) => /IdleState/.test(f));
    expect(idle).toBeDefined();
    expect(read(idle as string)).toMatch(/\bAppLink\b/);
    for (const name of ["PluginList", "AdapterList", "FaultSummary", "ComponentList"])
      expect(read(`components/${name}/${name}.tsx`), name).toMatch(/\bIdleState\b/);
  });

  test("no colour, font or pixel literal outside the tokens", () => {
    expect(components.length).toBeGreaterThan(0);
    for (const f of styles) {
      const css = read(f);
      expect(css, f).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
      expect(css, f).not.toMatch(/\b(rgb|rgba|hsl|hsla|oklch)\(/);
      expect(css, f).not.toMatch(/font-(size|family)\s*:\s*[^;]*\b\d/);
    }
    for (const f of components)
      expect(read(f), f).not.toMatch(/style=\{\{[^}]*(color|fontSize|fontFamily|background)/);
  });

  test("no direct fetch and no plain generated request function in the package", () => {
    expect(all().length).toBeGreaterThan(0);
    for (const f of all()) {
      const text = read(f);
      expect(text, f).not.toMatch(/\bfetch\(/);
      expect(text, f).not.toMatch(
        /\b(getPlugins|listNetworkAdapters|enableNetworkAdapter|disableNetworkAdapter|listComponents)\(/,
      );
    }
  });

  test("hooks live in src/hooks as use<Name>.ts(x) and models in src/model", () => {
    for (const f of files("hooks", /\.tsx?$/))
      expect(f).toMatch(/^hooks\/use[A-Z][A-Za-z]*\.tsx?$/);
    expect(files("model", /\.ts$/)).toEqual(
      expect.arrayContaining(["model/kindLabel.ts", "model/faultRows.ts", "model/counterRows.ts"]),
    );
  });

  test("the data is read through the generated query and mutation hooks of @umbra/data", () => {
    const text = all().map(read).join("\n");
    for (const hook of [
      "useGetPlugins",
      "useListNetworkAdapters",
      "useListComponents",
      "useEnableNetworkAdapter",
      "useDisableNetworkAdapter",
    ])
      expect(text).toMatch(new RegExp(`\\b${hook}\\b`));
  });

  test("the run state and the session come from useRunStatus of @umbra/engine-stream", () => {
    expect(all().map(read).join("\n")).toMatch(/\buseRunStatus\b/);
  });

  test("the package imports no other domain feature", () => {
    const text = all().map(read).join("\n");
    for (const other of [
      "entity-inspector",
      "scenario-control",
      "template-browser",
      "log-view",
      "map-view",
      "settings",
      "schema-form",
    ])
      expect(text).not.toContain(`@umbra/${other}`);
  });

  test("all user-visible text lives in messages.ts", () => {
    expect(files(".", /^messages\.ts$/)).toEqual(["messages.ts"]);
    expect(read("messages.ts")).toMatch(/\bdefineMessages\b/);
    for (const f of components) expect(read(f), f).toMatch(/from\s*["'][./]+\/messages\.ts["']/);
  });
});
