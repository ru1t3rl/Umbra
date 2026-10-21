import { describe, expect, test } from "bun:test";
import { join, resolve } from "node:path";
import { findLayoutViolations, type LayoutRule } from "../scripts/check-layout.ts";

const GUI = resolve(import.meta.dir, "..");
const FIXTURES = join(import.meta.dir, "arch-fixtures/layout");

/** fixture directory -> [rule it must trigger, file it must name]. */
const CASES: Record<string, [LayoutRule, string]> = {
  "component-outside-components": ["component-location", "features/x/src/Widget.tsx"],
  "component-wrong-directory": ["component-location", "features/x/src/components/Widget.tsx"],
  "two-components-in-a-file": ["one-component-per-file", "features/x/src/components/A/A.tsx"],
  "misnamed-component-file": ["file-name", "features/x/src/components/Foo/Bar.tsx"],
  "hook-outside-hooks": ["hook-location", "features/x/src/useThing.ts"],
  "two-hooks-in-a-file": ["one-hook-per-file", "features/x/src/hooks/useA.ts"],
  "misnamed-hook-file": ["file-name", "features/x/src/hooks/useA.ts"],
  "context-outside-context": ["context-location", "src/FooContext.ts"],
  "default-anonymous-arrow": ["component-location", "features/x/src/components/Foo/Foo.tsx"],
  "default-anonymous-function": ["component-location", "features/x/src/components/Foo/Foo.tsx"],
  "wrapper-memo-in-ts": ["component-location", "features/x/src/Widget.ts"],
  "wrapper-forward-ref": ["component-location", "features/x/src/components/Widget.tsx"],
  "wrapper-lazy": ["component-location", "features/x/src/Page.ts"],
  "wrapper-hoc": ["component-location", "features/x/src/Wrapped.tsx"],
  "create-element-in-ts": ["component-location", "features/x/src/Widget.ts"],
  "nested-component": ["nested-declaration", "features/x/src/components/Outer/Outer.tsx"],
  "object-property-component": ["nested-declaration", "features/x/src/parts.tsx"],
  "nested-hook": ["nested-declaration", "features/x/src/hooks/useOuter.ts"],
  "context-name-without-suffix": ["context-location", "features/x/src/context/Foo.ts"],
  "two-contexts-in-a-file": ["context-location", "features/x/src/context/AContext.ts"],
  "misnamed-context-file": ["file-name", "features/x/src/context/BContext.ts"],
  "jsx-file-scanned": ["component-location", "features/x/src/Widget.jsx"],
  "js-hook-scanned": ["hook-location", "features/x/src/useThing.js"],
};

describe("check:layout (ADR 0046)", () => {
  test("a clean tree passes; tests, testing doubles, generated and reference code are exempt", () => {
    expect(findLayoutViolations(join(FIXTURES, "clean"))).toEqual([]);
  });

  for (const [fixture, [rule, file]] of Object.entries(CASES)) {
    test(`${fixture} is rejected by ${rule}`, () => {
      const found = findLayoutViolations(join(FIXTURES, fixture));
      expect(found.some((v) => v.rule === rule && v.file === file)).toBe(true);
    });
  }

  test("every rule has a failing fixture", () => {
    const rules = new Set(Object.values(CASES).map(([rule]) => rule));
    const all: LayoutRule[] = [
      "component-location",
      "context-location",
      "file-name",
      "hook-location",
      "nested-declaration",
      "one-component-per-file",
      "one-hook-per-file",
    ];
    expect([...rules].sort()).toEqual(all.sort());
  });

  test("the real tree passes", () => {
    expect(findLayoutViolations(GUI)).toEqual([]);
  });
});
