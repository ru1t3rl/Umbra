import { withSize } from "./svgSize.ts";

/** A rasterised symbol image in the shape `Map.addImage` accepts (no DOM `ImageData` needed). */
export interface RasterImage {
  readonly width: number;
  readonly height: number;
  readonly data: Uint8ClampedArray;
}

/** Draws an SVG string at `size * pixelRatio` px. Injected so tests need no canvas. */
export type Rasterizer = (svg: string, size: number, pixelRatio: number) => Promise<RasterImage>;

/** The browser rasteriser: SVG data URL -> `Image` -> canvas. Exercised by the e2e and measure runs. */
export const rasterizeSvg: Rasterizer = async (svg, size, pixelRatio) => {
  const px = Math.round(size * pixelRatio);
  const image = new Image(px, px);
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(withSize(svg, px))}`;
  await image.decode();
  const canvas = document.createElement("canvas");
  canvas.width = px;
  canvas.height = px;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("map symbols: no 2D canvas context");
  context.drawImage(image, 0, 0, px, px);
  const { width, height, data } = context.getImageData(0, 0, px, px);
  return { width, height, data };
};
