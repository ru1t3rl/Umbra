import { afterEach, expect, test } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { Icon } from "./Icon.tsx";

afterEach(cleanup);

test("renders an aria-hidden glyph by ligature", () => {
  const { container } = render(<Icon name="layers" />);
  const el = container.querySelector(".vt-icon");
  expect(el).not.toBeNull();
  expect(el?.textContent).toBe("layers");
  expect(el?.getAttribute("aria-hidden")).toBe("true");
});
