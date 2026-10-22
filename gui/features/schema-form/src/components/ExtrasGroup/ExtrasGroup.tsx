import { Card } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import type { ExtraEntry } from "../../model/fieldNode.ts";

export interface ExtrasGroupProps {
  extras: readonly ExtraEntry[];
}

/** Values the schema does not describe, one named group each. Renders nothing when there are none. */
export function ExtrasGroup({ extras }: ExtrasGroupProps): ReactElement | null {
  if (extras.length === 0) return null;
  return (
    <fieldset className="umbra-schema-form-group">
      <legend className="type-label">{t("otherParameters")}</legend>
      <Card variant="outline">
        {extras.map((extra, index) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: a static list; a real key may look like the summary
          <fieldset className="umbra-schema-form-group" key={`${index}:${extra.key}`}>
            <legend className="type-label">{extra.key}</legend>
            {/* biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must be keyboard focusable */}
            <pre className="umbra-schema-form-json type-data" tabIndex={0}>
              {extra.json}
            </pre>
          </fieldset>
        ))}
      </Card>
    </fieldset>
  );
}
