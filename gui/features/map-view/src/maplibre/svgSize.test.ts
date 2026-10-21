import { describe, expect, test } from "bun:test";
import { withSize } from "./svgSize.ts";

describe("withSize", () => {
  test("sets the root width and height to the raster size and nothing else", () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 40 40"><rect width="24" height="16"/></svg>';
    expect(withSize(svg, 80)).toBe(
      '<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80" viewBox="0 0 40 40"><rect width="24" height="16"/></svg>',
    );
  });
});
