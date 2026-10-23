// WI-9 review (m4, m8): the selection ring as pure data. Written before the ring image exists.
// Contracts: `selectionSvg(colors)` / `selectionImage(colors)` / `SELECTION_IMAGE_ID` in symbols.ts, kept OUT of
// `symbolImages` (the frozen tests count 8 symbols + direction = 9 images at load); `selectionOverlay()` in
// style.ts takes no colour (the colour is in the image, so a theme switch updates one image).
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { SELECTION_LAYER, SELECTION_SOURCE, selectionOverlay } from "./style.ts";
import {
  IMAGE_PREFIX,
  SELECTION_IMAGE_ID,
  SYMBOL_FRAME_PX,
  selectionImage,
  selectionSvg,
  symbolImages,
} from "./symbols.ts";
import { COLORS } from "./testing.ts";

describe("selection ring image (tactical-display.md: 2px azure rounded rect at radius-xs outside the frame)", () => {
  test("the id is namespaced and is not one of the nine load-time images", () => {
    expect(SELECTION_IMAGE_ID.startsWith(IMAGE_PREFIX)).toBe(true);
    expect(symbolImages(COLORS)).toHaveLength(9);
    expect(symbolImages(COLORS).some((i) => i.id === SELECTION_IMAGE_ID)).toBe(false);
  });
  test("is a 2px stroked rounded rect with radius-xs (4px), unfilled, in the selection colour", () => {
    const svg = selectionSvg(COLORS);
    expect(svg).toContain(`stroke="${COLORS.selection}"`);
    expect(svg).toContain('stroke-width="2"');
    expect(svg).toContain('rx="4"');
    expect(svg).toContain('fill="none"');
    expect(svg).not.toContain(COLORS.symbolFrame);
  });
  test("is keyed by colour: another selection colour gives another image, same id and size", () => {
    const a = selectionImage(COLORS);
    const b = selectionImage({ ...COLORS, selection: "OTHER-AZURE" });
    expect(a.id).toBe(SELECTION_IMAGE_ID);
    expect(b.id).toBe(SELECTION_IMAGE_ID);
    expect(b.size).toBe(a.size);
    expect(b.svg).toContain("OTHER-AZURE");
    expect(b.svg).not.toBe(a.svg);
  });
  test("clears the symbol frame: the canvas is larger than the frame and the svg is that size", () => {
    const image = selectionImage(COLORS);
    expect(image.size).toBeGreaterThan(SYMBOL_FRAME_PX);
    expect(image.svg).toContain(`width="${image.size}"`);
    expect(image.svg).toContain(`height="${image.size}"`);
  });
});

describe("selectionOverlay", () => {
  test("has an empty GeoJSON source and one symbol layer, both with the selection ids", () => {
    const overlay = selectionOverlay();
    expect(Object.keys(overlay.sources)).toEqual([SELECTION_SOURCE]);
    expect(SELECTION_SOURCE).toBe("selection");
    expect(SELECTION_LAYER).toBe("selection");
    expect(overlay.sources[SELECTION_SOURCE]).toMatchObject({
      type: "geojson",
      data: { type: "FeatureCollection", features: [] },
    });
    expect(overlay.layers).toHaveLength(1);
    expect(overlay.layers[0]).toMatchObject({
      id: "selection",
      type: "symbol",
      source: "selection",
      layout: { "icon-image": SELECTION_IMAGE_ID },
    });
  });
  test("the ring is never hidden by label collision and draws no text (ASM-314)", () => {
    const [layer] = selectionOverlay().layers;
    expect(layer).toMatchObject({
      layout: { "icon-allow-overlap": true, "icon-ignore-placement": true },
    });
    expect(JSON.stringify(selectionOverlay())).not.toContain("text-");
  });
});

describe("selection label style (glass pill, tactical-display.md)", () => {
  const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
  const rule = /\.umbra-live-map-label\s*\{([^}]*)\}/.exec(css)?.[1] ?? "";
  test("the label class has the pill radius token", () => {
    expect(rule).toContain("border-radius: var(--radius-full)");
  });
  test("uses glass tokens only, no colour literal", () => {
    expect(rule).toContain("var(--glass)");
    expect(rule).not.toMatch(/#[0-9a-f]{3,8}\b|rgba?\(/i);
  });
});
