import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Badge, type Tone } from "./Badge.tsx";

afterEach(cleanup);

test("neutral is the default and adds no tone class", () => {
  render(<Badge>Paused</Badge>);
  const el = screen.getByText("Paused");
  expect(el).toHaveClass("vt-badge");
  expect(el.className).toBe("vt-badge");
});

test("explicit neutral adds no tone class", () => {
  render(<Badge tone="neutral">Stale</Badge>);
  expect(screen.getByText("Stale").className).toBe("vt-badge");
});

test.each(["success", "info", "warning", "danger"] as const satisfies readonly Tone[])(
  "tone %s maps to vt-badge-%s",
  (tone) => {
    render(<Badge tone={tone}>Word</Badge>);
    expect(screen.getByText("Word")).toHaveClass("vt-badge", `vt-badge-${tone}`);
  },
);

test("className is appended", () => {
  render(<Badge className="extra">Running</Badge>);
  expect(screen.getByText("Running")).toHaveClass("vt-badge", "extra");
});
