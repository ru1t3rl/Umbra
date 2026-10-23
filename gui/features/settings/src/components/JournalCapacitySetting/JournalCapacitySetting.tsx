import { TextField } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { useSettings } from "../../hooks/useSettings.ts";
import { useUpdateSettings } from "../../hooks/useUpdateSettings.ts";
import { t } from "../../messages.ts";
import { parseCapacity } from "../../model/capacity.ts";

/**
 * Edits `settings.journal.capacity`. The text is judged and saved on blur or Enter, never while typing; a value
 * outside 1,000..50,000 is flagged, kept in the field and not saved (ASM-995 e).
 */
export function JournalCapacitySetting(): ReactElement {
  const capacity = useSettings().journal.capacity;
  const update = useUpdateSettings();
  const [text, setText] = useState(String(capacity));
  const [invalid, setInvalid] = useState(false);

  const commit = (): void => {
    const value = parseCapacity(text);
    setInvalid(value === undefined);
    if (value !== undefined) update({ journal: { capacity: value } });
  };

  return (
    <TextField
      type="number"
      label={t("capacity.label")}
      help={invalid ? t("capacity.invalid") : t("capacity.help")}
      invalid={invalid}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
      }}
    />
  );
}
