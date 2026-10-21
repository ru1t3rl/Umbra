import type { EntityFrame, EntitySource } from "@umbra/engine-stream";
import { entity, frameOf } from "../model/testing.ts";
import type { FrameScheduler } from "./createMapLibreView.ts";
import type { RasterImage } from "./rasterize.ts";

/** Test doubles for the module boundary: `mock.module("maplibre-gl", () => ({ Map: FakeMap }))`. */

/** Simulates MapLibre's lazy hand-off: `setData` keeps the reference, the "worker" reads it later. */
export class FakeSource {
  setDataCalls = 0;
  lastRef: unknown;
  setData(data: unknown): this {
    this.setDataCalls += 1;
    this.lastRef = data;
    return this;
  }
  /** What the worker would receive if it serialised the pending reference now. */
  serialise(): { features: { id: number }[] } {
    return JSON.parse(JSON.stringify(this.lastRef));
  }
}

type Listener = (event: { error?: unknown }) => void;

export class FakeMap {
  static instances: FakeMap[] = [];
  /** When set, the next constructor call throws it (no WebGL on the device). */
  static failNext: Error | undefined;
  static reset(): void {
    FakeMap.instances = [];
    FakeMap.failNext = undefined;
  }
  readonly listeners = new Map<string, Listener[]>();
  readonly sources = new Map<string, FakeSource>();
  readonly layers: string[] = [];
  readonly images = new Map<string, unknown>();
  readonly imageOps: { op: "add" | "update"; id: string; image: unknown; pixelRatio?: number }[] =
    [];
  readonly paint: { layer: string; property: string; value: unknown }[] = [];
  readonly fits: { bounds: unknown; options: unknown }[] = [];
  rendered: { id: number; geometry?: { type: string; coordinates: number[] } }[] = [];
  removed = false;
  styleLoaded = false;
  /** What `cameraForBounds` returns; undefined simulates a zero-size container. */
  camera: { center: [number, number]; zoom: number } | undefined = { center: [0, 0], zoom: 5 };
  size = { width: 800, height: 600 };
  constructor(
    readonly options: { container: HTMLElement; style: unknown; attributionControl?: unknown },
  ) {
    if (FakeMap.failNext) {
      const error = FakeMap.failNext;
      FakeMap.failNext = undefined;
      throw error;
    }
    FakeMap.instances.push(this);
    const style = options.style;
    if (typeof style === "object" && style !== null && "sources" in style) {
      for (const id of Object.keys(style.sources as object)) this.sources.set(id, new FakeSource());
    }
  }
  on(type: string, listener: Listener): this {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
    return this;
  }
  emit(type: string, event: { error?: unknown } = {}): void {
    for (const l of this.listeners.get(type) ?? []) l(event);
  }
  getSource<T>(id: string): T | undefined {
    return this.sources.get(id) as T | undefined;
  }
  addSource(id: string): void {
    this.sources.set(id, new FakeSource());
  }
  addLayer(layer: { id: string }): void {
    this.layers.push(layer.id);
  }
  hasImage(id: string): boolean {
    return this.images.has(id);
  }
  addImage(id: string, image: unknown, options?: { pixelRatio?: number }): void {
    this.images.set(id, image);
    this.imageOps.push({
      op: "add",
      id,
      image,
      ...(options?.pixelRatio ? { pixelRatio: options.pixelRatio } : {}),
    });
  }
  updateImage(id: string, image: unknown): void {
    this.images.set(id, image);
    this.imageOps.push({ op: "update", id, image });
  }
  setPaintProperty(layer: string, property: string, value: unknown): void {
    this.paint.push({ layer, property, value });
  }
  fitBounds(bounds: unknown, options: unknown): void {
    this.fits.push({ bounds, options });
  }
  queryRenderedFeatures(): { id: number; geometry?: { type: string; coordinates: number[] } }[] {
    return this.rendered;
  }
  zoom = 12;
  getZoom(): number {
    return this.zoom;
  }
  project([lon, lat]: [number, number]): { x: number; y: number } {
    return { x: lon * 10, y: lat * 10 };
  }
  getCanvas(): { clientWidth: number; clientHeight: number } {
    return { clientWidth: this.size.width, clientHeight: this.size.height };
  }
  remove(): void {
    this.removed = true;
  }
  isStyleLoaded(): boolean {
    return this.styleLoaded;
  }
  cameraForBounds(): { center: [number, number]; zoom: number } | undefined {
    return this.camera;
  }
  /** Finishes loading the style (MapLibre signals it with `styledata`). */
  load(): void {
    this.styleLoaded = true;
    this.emit("styledata");
  }
  get entities(): FakeSource {
    const s = this.sources.get("entities");
    if (!s) throw new Error("no entity source");
    return s;
  }
}

/** A scheduler the test drives by hand: one `run()` is one animation frame. */
export class ManualScheduler implements FrameScheduler {
  private next = 1;
  private readonly queue = new Map<number, () => void>();
  cancelled: number[] = [];
  request(callback: () => void): number {
    const handle = this.next++;
    this.queue.set(handle, callback);
    return handle;
  }
  cancel(handle: number): void {
    this.cancelled.push(handle);
    this.queue.delete(handle);
  }
  get pending(): number {
    return this.queue.size;
  }
  run(): void {
    const callbacks = [...this.queue.values()];
    this.queue.clear();
    for (const c of callbacks) c();
  }
}

/** Rasteriser stub: records the SVG each image came from, resolves immediately or on demand. */
export function fakeRasterizer(): {
  rasterize: (svg: string, size: number, ratio: number) => Promise<RasterImage>;
  svgOf: (image: unknown) => string | undefined;
} {
  const origin = new WeakMap<object, string>();
  return {
    rasterize: async (svg, size, ratio) => {
      const image = { width: size * ratio, height: size * ratio, data: new Uint8ClampedArray(4) };
      origin.set(image, svg);
      return image;
    },
    svgOf: (image) => (typeof image === "object" && image !== null ? origin.get(image) : undefined),
  };
}

/** A hand-written `EntitySource`; `push` delivers a frame to every listener, like the store flush. */
export class FakeEntitySource implements EntitySource {
  readonly listeners = new Set<(frame: EntityFrame) => void>();
  frame: EntityFrame | undefined;
  getFrame(): EntityFrame | undefined {
    return this.frame;
  }
  subscribe(listener: (frame: EntityFrame) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  push(frame: EntityFrame): void {
    this.frame = frame;
    for (const l of [...this.listeners]) l(frame);
  }
}

export const frameWith = (count: number, session = 1, seq = 0): EntityFrame => ({
  ...frameOf(Array.from({ length: count }, (_, i) => entity(i + 1))),
  session,
  seq,
});
