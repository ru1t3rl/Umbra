// WI-13 public API pin, by source text (importing ./index.ts would pull in every component; never import a
// feature index in a test that can reach real third-party modules, the WI-9 lesson). Catches: a runtime export
// leaking (models, hooks, private components, test doubles), a public name missing. Types have no runtime key.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./index.ts", import.meta.url), "utf8").replace(
  /\/\/.*$/gm,
  "",
);

test("the package exports exactly PluginList, AdapterList, FaultSummary, ComponentList and PluginStatusGate at runtime", () => {
  const withoutTypes = source.replace(/export\s+type\s*\{[^}]*\}\s*from\s*["'][^"']*["'];?/g, "");
  const runtime = [...withoutTypes.matchAll(/export\s*(?:\*|\{([^}]*)\})[^;]*;/g)].flatMap((m) =>
    m[1] === undefined
      ? ["*"]
      : m[1].split(",").map(
          (n) =>
            n
              .trim()
              .split(/\s+as\s+/)
              .pop() ?? "",
        ),
  );
  expect(withoutTypes).not.toMatch(/export\s+(const|function|class|let|var|default)\b/);
  expect(runtime.filter(Boolean).sort()).toEqual([
    "AdapterList",
    "ComponentList",
    "FaultSummary",
    "PluginList",
    "PluginStatusGate",
  ]);
});

test("the components take no props (the gate takes children), so no prop type is exported and no model type leaks", () => {
  expect(source).not.toMatch(/export\s+type\s*\{[^}]*\bFaultRow\b/);
  expect(source).not.toMatch(/export\s+type\s*\{[^}]*Props\b/);
});

test("exports come from the component files, not from deep or foreign paths", () => {
  for (const name of [
    "PluginList",
    "AdapterList",
    "FaultSummary",
    "ComponentList",
    "PluginStatusGate",
  ])
    expect(source).toContain(`./components/${name}/${name}.tsx`);
  expect(source).not.toMatch(/from\s*["']@umbra\//);
  expect(source).not.toMatch(/statusTesting|\/model\/|\/hooks\//);
});
