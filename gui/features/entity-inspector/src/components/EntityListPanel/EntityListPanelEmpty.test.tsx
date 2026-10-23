// WI-14 review minor 2a: the empty list must not leave a blank box (design-system defect in the glass panel).

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, fireEvent, render } from "@testing-library/react";
import {
  FakeEntitySource,
  frameOf,
  installResizeObserver,
  summary,
} from "../../inspectorTesting.tsx";
import { EntityListPanel } from "./EntityListPanel.tsx";

let restore: () => void;
beforeEach(() => {
  restore = installResizeObserver(400);
});
afterEach(() => {
  cleanup();
  restore();
});

const mount = (source: FakeEntitySource) =>
  render(
    <EntityListPanel
      source={source}
      selectedId={null}
      onSelect={() => {}}
      affiliationOf={() => "friend"}
    />,
  );
const root = (c: HTMLElement) => c.querySelector(".umbra-entity-list") as HTMLElement;

describe("an empty list", () => {
  test("is marked data-empty with no entities, and not with rows", () => {
    const { container } = mount(new FakeEntitySource());
    expect(root(container)).toHaveAttribute("data-empty");
    cleanup();
    const source = new FakeEntitySource();
    source.push(frameOf([summary(1)]));
    const again = mount(source);
    expect(root(again.container)).not.toHaveAttribute("data-empty");
  });

  test("a search that matches nothing is empty too", () => {
    const source = new FakeEntitySource();
    source.push(frameOf([summary(1, { name: "Alpha" })]));
    const { container, getByLabelText } = mount(source);
    expect(root(container)).not.toHaveAttribute("data-empty");
    fireEvent.change(getByLabelText("Search entities"), {
      target: { value: "zzz" },
    });
    expect(root(container)).toHaveAttribute("data-empty");
  });

  test("the stylesheet gives the empty listbox no height (flex: 0 0 0)", () => {
    const css = readFileSync(join(import.meta.dir, "EntityListPanel.css"), "utf8");
    expect(css).toMatch(/\[data-empty\][^{]*\.umbra-entity-list-rows\s*\{[^}]*flex:\s*0 0 0/);
  });
});
