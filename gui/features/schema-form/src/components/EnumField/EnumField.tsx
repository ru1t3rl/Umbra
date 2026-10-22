import { Chip, ChipGroup, Select } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { helpMessages } from "../../helpMessages.ts";
import { t } from "../../messages.ts";
import { fieldId } from "../../model/fieldId.ts";
import type { FieldNode } from "../../model/fieldNode.ts";
import { helpText } from "../../model/helpText.ts";

export interface EnumFieldProps {
  node: Extract<FieldNode, { kind: "enum" }>;
  idPrefix: string;
}

const noop = (): void => {};

/** A string enum: up to five values as disabled chips, more as a disabled select. */
export function EnumField({ node, idPrefix }: EnumFieldProps): ReactElement {
  const help = helpText(node, helpMessages);
  const id = fieldId(idPrefix, node.path);
  const helpId = `${id}-help`;
  if (node.control === "select") {
    const options = node.options.map((value) => ({ value, label: value }));
    return (
      <Select
        id={id}
        label={node.label}
        {...(help ? { help } : {})}
        options={
          node.selected === undefined ? [{ value: "", label: t("notSet") }, ...options] : options
        }
        value={node.selected ?? ""}
        onChange={noop}
        disabled
      />
    );
  }
  return (
    <div className="umbra-schema-form-field">
      <ChipGroup label={node.label} {...(help ? { "aria-describedby": helpId } : {})}>
        {node.options.map((option) => (
          <Chip key={option} selected={option === node.selected} disabled>
            {option}
          </Chip>
        ))}
      </ChipGroup>
      {help ? (
        <p className="umbra-schema-form-help type-caption" id={helpId}>
          {help}
        </p>
      ) : null}
    </div>
  );
}
