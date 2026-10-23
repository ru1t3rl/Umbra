// WI-10 review m9: `badges` is data, not markup: `readonly { label: string; tone?: Tone }[]`, each rendered as a
// Badge after the status, in order. Works in both variants of the row. Written before the prop type changes.
import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { EntityList } from "../EntityList/EntityList.tsx";
import { EntityRow, type EntityRowProps } from "./EntityRow.tsx";

afterEach(cleanup);

const base: EntityRowProps = {
  name: "Alpha 1-1",
  affiliation: "friend",
  status: { label: "Running", tone: "success" },
  presentational: true,
};
const badges = [{ label: "Remote", tone: "info" as const }, { label: "Ghost" }];

test("each badge is rendered as a Badge with its label, after the status, in order", () => {
  const { container } = render(<EntityRow {...base} badges={badges} />);
  const all = [...container.querySelectorAll(".vt-badge")].map((b) => b.textContent);
  expect(all).toEqual(["Running", "Remote", "Ghost"]);
});

test("a badge tone selects the Badge tone class; no tone is the neutral badge", () => {
  const { container } = render(<EntityRow {...base} badges={badges} />);
  const [, remote, ghost] = container.querySelectorAll(".vt-badge");
  expect(remote?.className).toContain("vt-badge-info");
  expect(ghost?.className).not.toMatch(/vt-badge-(info|success|warning|danger)/);
});

test("an empty list renders no extra badge", () => {
  const { container } = render(<EntityRow {...base} badges={[]} />);
  expect(container.querySelectorAll(".vt-badge")).toHaveLength(1);
});

test("badges are non-interactive in the presentational row", () => {
  render(<EntityRow {...base} badges={badges} />);
  expect(screen.queryByRole("button")).toBeNull();
});

test("the EntityList option shows the badges too", () => {
  render(
    <EntityList label="Entities">
      <EntityRow name="Alpha" affiliation="friend" badges={badges} />
    </EntityList>,
  );
  expect(screen.getByRole("option")).toHaveTextContent("Remote");
  expect(screen.getByRole("option")).toHaveTextContent("Ghost");
});
