import { afterEach, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { AffiliationTag } from "./AffiliationTag.tsx";

afterEach(cleanup);

test.each([
  ["friend", "Friend"],
  ["hostile", "Hostile"],
  ["neutral", "Neutral"],
  ["unknown", "Unknown"],
] as const)("%s shows the default word %s", (affiliation, word) => {
  render(<AffiliationTag affiliation={affiliation} />);
  const el = screen.getByText(word);
  expect(el).toHaveClass("vt-affil", `vt-affil-${affiliation}`);
  expect(el).toHaveAttribute("title", word);
});

test("children replace the default word", () => {
  render(<AffiliationTag affiliation="friend">2 MECH BDE</AffiliationTag>);
  expect(screen.getByText("2 MECH BDE")).toHaveClass("vt-affil-friend");
  expect(screen.queryByText("Friend")).toBeNull();
});

test("the frame symbol is decorative, with fill and 1.5px frame shapes", () => {
  const { container } = render(<AffiliationTag affiliation="hostile" />);
  const svg = container.querySelector("svg");
  expect(svg).toHaveAttribute("aria-hidden", "true");
  expect(svg?.querySelector(".vt-affil-fill")).not.toBeNull();
  expect(svg?.querySelector(".vt-affil-frame")).not.toBeNull();
});

test("the friend fill keeps the reference class vt-affil-frame-shape", () => {
  const { container } = render(<AffiliationTag affiliation="friend" />);
  expect(container.querySelector(".vt-affil-fill")).toHaveClass("vt-affil-frame-shape");
  expect(container.querySelector(".vt-affil-frame")).not.toHaveClass("vt-affil-frame-shape");
});
