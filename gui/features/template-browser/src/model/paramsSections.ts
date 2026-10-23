import type { ComponentEntry, ComponentRef, TemplateDetail } from "@umbra/data";

export type SectionKind = "motion" | "sensor" | "behavior";

export interface ParamsSection {
  /** Unique within one result: the React key and the SchemaForm id part. */
  readonly key: string;
  readonly kind: SectionKind;
  readonly id: string;
  readonly params: unknown;
  /** The joined `params_schema`, or undefined when the component is unknown (ASM-816). */
  readonly schema: unknown;
}

/** One section per ComponentRef of a template: motion model, sensors in template order, then the behavior. */
export function paramsSections(
  template: TemplateDetail,
  index: ReadonlyMap<string, ComponentEntry>,
): readonly ParamsSection[] {
  const section = (kind: SectionKind, position: number, ref: ComponentRef): ParamsSection => ({
    key: `${kind}-${position}`,
    kind,
    id: ref.id,
    params: ref.params,
    schema: index.get(ref.id)?.params_schema,
  });
  return [
    section("motion", 0, template.motion),
    ...template.sensors.map((ref, i) => section("sensor", i, ref)),
    ...(template.behavior ? [section("behavior", 0, template.behavior)] : []),
  ];
}
