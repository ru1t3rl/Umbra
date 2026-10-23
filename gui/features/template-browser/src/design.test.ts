// WI-12 design and layout pins, by source text (A-003 `apply-design-system`, ADR 0046). Catches: a raw HTML control
// instead of the ui-kit component, a design value outside the tokens, a component or hook outside its own file,
// a direct `fetch` or a generated-client function call (features read data only through `@umbra/data` hooks).
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

describe("template-browser design", () => {
  test("has the components the plan names, each in its own folder and file", () => {
    for (const name of ["TemplateList", "TemplateDetail", "SpawnForm"])
      expect(components).toContain(`components/${name}/${name}.tsx`);
  });

  test("a component uses no raw interactive HTML control: ui-kit components only", () => {
    expect(components.length).toBeGreaterThan(0);
    for (const f of components) expect(read(f), f).not.toMatch(/<(button|input|select|textarea)\b/);
  });

  test("components import ui-kit and schema-form by package name", () => {
    const all = components.map(read).join("\n");
    expect(all).toMatch(/from\s*["']@umbra\/ui-kit["']/);
    expect(all).toMatch(/from\s*["']@umbra\/schema-form["']/);
    expect(all).not.toMatch(/@umbra\/[a-z-]+\/src/);
  });

  test("the list uses the ui-kit VirtualList, the idle state the AppLink, the spawn form the ui-kit TextField and Button", () => {
    expect(read("components/TemplateList/TemplateList.tsx")).toMatch(/\bVirtualList\b/);
    // ASM-948: the empty state with its AppLink is one private component used by both exports.
    expect(read("components/IdleState/IdleState.tsx")).toMatch(/\bAppLink\b/);
    expect(read("components/TemplateList/TemplateList.tsx")).toMatch(/\bIdleState\b/);
    expect(read("components/TemplateDetail/TemplateDetail.tsx")).toMatch(/\bIdleState\b/);
    expect(read("components/SpawnForm/SpawnForm.tsx")).toMatch(/\bTextField\b/);
    expect(read("components/SpawnForm/SpawnForm.tsx")).toMatch(/\bButton\b/);
  });

  test("components render the parameters through SchemaForm", () => {
    expect(read("components/TemplateDetail/TemplateDetail.tsx")).toMatch(/\bSchemaForm\b/);
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
    const sources = [...files(".", /\.tsx?$/)].filter((f) => !/Testing\.tsx?$/.test(f));
    expect(sources.length).toBeGreaterThan(0);
    for (const f of sources) {
      const text = read(f);
      expect(text, f).not.toMatch(/\bfetch\(/);
      expect(text, f).not.toMatch(/\b(listTemplates|getTemplate|listComponents|spawnEntity)\(/);
    }
  });

  test("hooks live in src/hooks as use<Name>.ts(x) and models in src/model", () => {
    for (const f of files("hooks", /\.tsx?$/))
      expect(f).toMatch(/^hooks\/use[A-Z][A-Za-z]*\.tsx?$/);
    expect(files("model", /\.ts$/)).toEqual(
      expect.arrayContaining(["model/componentIndex.ts", "model/paramsSections.ts"]),
    );
  });

  test("the template data is read through the generated query hooks of @umbra/data", () => {
    const all = files(".", /\.tsx?$/)
      .filter((f) => !/Testing\./.test(f))
      .map(read)
      .join("\n");
    expect(all).toMatch(/\buseListTemplates\b/);
    expect(all).toMatch(/\buseGetTemplate\b/);
    expect(all).toMatch(/\buseListComponents\b/);
    expect(all).toMatch(/\buseSpawnEntity\b/);
  });
});
