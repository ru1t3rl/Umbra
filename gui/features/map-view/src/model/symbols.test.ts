import { describe, expect, test } from "bun:test";
import {
  AFFILIATIONS,
  directionSvg,
  RING_GAP_PX,
  RING_STROKE_PX,
  SYMBOL_CANVAS_PX,
  SYMBOL_STROKE_PX,
  symbolImageId,
  symbolImages,
  symbolSvg,
} from "./symbols.ts";
import { COLORS } from "./testing.ts";

const attrs = (svg: string, name: string): string[] =>
  [...svg.matchAll(new RegExp(`\\s${name}="([^"]*)"`, "g"))].map((m) => m[1] ?? "");
const num = (svg: string, name: string): number => Number(attrs(svg, name)[0]);

/** The frame element only (the part after the optional ring). */
const frameOf = (svg: string): string => svg.slice(svg.search(/<(rect|polygon|path)/));

/** Farthest frame vertex from the canvas centre; path control points bound the curve (conservative). */
function farthestVertex(svg: string): number {
  const c = SYMBOL_CANVAS_PX / 2;
  const frame = frameOf(svg);
  let points: [number, number][] = [];
  if (frame.startsWith("<rect")) {
    const [x, y, w, h] = [
      num(frame, "x"),
      num(frame, "y"),
      num(frame, "width"),
      num(frame, "height"),
    ];
    points = [
      [x, y],
      [x + w, y],
      [x, y + h],
      [x + w, y + h],
    ];
  } else {
    const nums =
      (attrs(frame, frame.startsWith("<polygon") ? "points" : "d")[0] ?? "")
        .match(/-?\d+(\.\d+)?/g)
        ?.map(Number) ?? [];
    for (let i = 0; i + 1 < nums.length; i += 2)
      points.push([nums[i] as number, nums[i + 1] as number]);
  }
  expect(points.length).toBeGreaterThanOrEqual(4);
  return Math.max(...points.map(([x, y]) => Math.hypot(x - c, y - c)));
}

describe("symbolSvg", () => {
  test("four distinct frame geometries", () => {
    const shape = (a: (typeof AFFILIATIONS)[number]) => symbolSvg(a, "owned", COLORS);
    expect(shape("friend")).toContain("<rect");
    expect(shape("friend")).toContain('width="24" height="16"');
    expect(shape("hostile")).toContain("<polygon");
    expect(shape("neutral")).toContain("<rect");
    expect(shape("neutral")).toContain('width="18" height="18"');
    expect(shape("unknown")).toContain("<path");
    expect(new Set(AFFILIATIONS.map(shape)).size).toBe(4);
  });

  test("fill is the affiliation colour and stroke is symbol-frame, from the input only", () => {
    for (const a of AFFILIATIONS) {
      const svg = symbolSvg(a, "owned", COLORS);
      expect(attrs(svg, "fill")).toEqual([COLORS.affiliation[a]]);
      expect(attrs(svg, "stroke")).toEqual([COLORS.symbolFrame]);
    }
  });

  test("the SVG holds no paint value other than the passed-in colours or none", () => {
    const allowed = new Set([
      ...Object.values(COLORS.affiliation),
      COLORS.symbolFrame,
      COLORS.inkMuted,
      "none",
    ]);
    const all = [
      ...AFFILIATIONS.flatMap((a) => [
        symbolSvg(a, "owned", COLORS),
        symbolSvg(a, "remote", COLORS),
        symbolSvg(a, "unknown", COLORS),
      ]),
      directionSvg(COLORS),
    ];
    for (const svg of all) {
      for (const v of [...attrs(svg, "fill"), ...attrs(svg, "stroke")])
        expect(allowed.has(v)).toBe(true);
      expect(svg).not.toMatch(/#[0-9a-f]{3,8}\b|rgb|hsl/i);
    }
  });

  test("frame stroke width is 1.5", () => {
    for (const a of AFFILIATIONS) {
      expect(attrs(symbolSvg(a, "owned", COLORS), "stroke-width")).toEqual(["1.5"]);
    }
    expect(attrs(directionSvg(COLORS), "stroke-width")).toEqual(["1.5"]);
  });

  test("the ring is drawn for remote and unknown, never for owned, in ink-muted at 1px", () => {
    for (const a of AFFILIATIONS) {
      expect(symbolSvg(a, "owned", COLORS)).not.toContain("<circle");
      for (const o of ["remote", "unknown"] as const) {
        const svg = symbolSvg(a, o, COLORS);
        expect(svg.match(/<circle/g)?.length).toBe(1);
        expect(attrs(svg, "stroke")).toContain(COLORS.inkMuted);
        expect(attrs(svg, "stroke-width")).toEqual(["1", "1.5"]);
        expect(svg.indexOf("<circle")).toBeLessThan(svg.search(/<(rect|polygon|path)/));
      }
    }
  });

  test("the ring clears every frame: inner edge >= farthest vertex + stroke/2 + 3, and fits the canvas", () => {
    for (const a of AFFILIATIONS) {
      const svg = symbolSvg(a, "remote", COLORS);
      const r = num(svg, "r");
      const inner = r - RING_STROKE_PX / 2;
      expect(inner).toBeGreaterThanOrEqual(
        farthestVertex(svg) + SYMBOL_STROKE_PX / 2 + RING_GAP_PX,
      );
      expect(r + RING_STROKE_PX / 2).toBeLessThanOrEqual(SYMBOL_CANVAS_PX / 2);
    }
  });
});

describe("images", () => {
  test("only owned has the plain image; remote and unknown share the ring image", () => {
    expect(symbolImageId("friend", "owned")).toBe("symbol-friend-plain");
    expect(symbolImageId("friend", "remote")).toBe("symbol-friend-ring");
    expect(symbolImageId("friend", "unknown")).toBe(symbolImageId("friend", "remote"));
  });
  test("symbolImages: 8 symbols plus the direction line, unique ids", () => {
    const images = symbolImages(COLORS);
    expect(images).toHaveLength(9);
    expect(new Set(images.map((i) => i.id)).size).toBe(9);
  });
  test("the direction line is 1.5x the frame (36px) long, from the centre to the canvas edge", () => {
    const svg = directionSvg(COLORS);
    expect(attrs(svg, "x1")).toEqual(["36"]);
    expect(attrs(svg, "x2")).toEqual(["36"]);
    expect(attrs(svg, "y1")).toEqual(["36"]);
    expect(attrs(svg, "y2")).toEqual(["0"]);
    expect(num(svg, "y1") - num(svg, "y2")).toBe(1.5 * 24);
  });
});
