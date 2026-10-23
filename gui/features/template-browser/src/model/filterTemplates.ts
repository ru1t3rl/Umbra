import type { TemplateSummary } from "@umbra/data";

/** Case-insensitive substring match over id, description, motion model and behavior (ASM-947 b). */
export function filterTemplates(
  templates: readonly TemplateSummary[],
  query: string,
): readonly TemplateSummary[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return templates;
  return templates.filter((t) =>
    [t.id, t.description, t.motion, t.behavior].some((v) => v?.toLowerCase().includes(needle)),
  );
}
