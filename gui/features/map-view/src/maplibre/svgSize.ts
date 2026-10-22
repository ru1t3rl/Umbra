/** Sets the root `<svg>` width and height so the image has its intrinsic size at the raster size (no reliance on `drawImage` up-scaling). */
export function withSize(svg: string, px: number): string {
  return svg.replace(/^<svg\b[^>]*>/, (root) =>
    root
      .replace(/\swidth="[^"]*"/, ` width="${px}"`)
      .replace(/\sheight="[^"]*"/, ` height="${px}"`),
  );
}
