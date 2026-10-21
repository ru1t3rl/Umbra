import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Button } from "../Button/Button.tsx";
import { Panel } from "./Panel.tsx";

afterEach(cleanup);

test("is a region labelled by its title heading", () => {
  render(<Panel title="Layers">body</Panel>);
  const region = screen.getByRole("region", { name: "Layers" });
  expect(region).toHaveClass("vt-panel");
  const heading = screen.getByRole("heading", { name: "Layers", level: 2 });
  expect(heading).toHaveClass("vt-panel-title");
  expect(region.querySelector(".vt-panel-head")).not.toBeNull();
  expect(region.querySelector(".vt-panel-body")?.textContent).toBe("body");
});

test("width is the CSS default (320px) unless given; a number becomes px", () => {
  const { rerender } = render(<Panel title="A">x</Panel>);
  expect(screen.getByRole("region").style.width).toBe("");
  rerender(
    <Panel title="A" width={400}>
      x
    </Panel>,
  );
  expect(screen.getByRole("region").style.width).toBe("400px");
  rerender(
    <Panel title="A" width="30rem">
      x
    </Panel>,
  );
  expect(screen.getByRole("region").style.width).toBe("30rem");
});

test("header actions render beside the title", () => {
  render(
    <Panel title="Layers" actions={<Button icon="close" aria-label="Close" variant="ghost" />}>
      x
    </Panel>,
  );
  expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
});

test("two panels get distinct label ids", () => {
  render(
    <>
      <Panel title="One">x</Panel>
      <Panel title="Two">y</Panel>
    </>,
  );
  expect(screen.getByRole("region", { name: "One" })).not.toBe(
    screen.getByRole("region", { name: "Two" }),
  );
});

test("className is appended", () => {
  render(
    <Panel title="A" className="extra">
      x
    </Panel>,
  );
  expect(screen.getByRole("region")).toHaveClass("vt-panel", "extra");
});
