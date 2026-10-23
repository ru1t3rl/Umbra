import type { EntityFrame } from "@umbra/engine-stream";
import { Button } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import { findById } from "../../model/findById.ts";

export interface ContactListProps {
  contacts: readonly number[];
  /** Names come from the newest frame; an unknown contact is `#id`. */
  frame: EntityFrame | undefined;
  onSelect: (id: number) => void;
}

export function ContactList({ contacts, frame, onSelect }: ContactListProps): ReactElement {
  if (contacts.length === 0) return <p className="type-caption">{t("inspector.noContacts")}</p>;
  return (
    <ul className="umbra-entity-inspector-contacts" aria-label={t("inspector.contacts")}>
      {contacts.map((id) => {
        const name = frame ? findById(frame.entities, id)?.name : undefined;
        return (
          <li key={id}>
            <Button variant="ghost" size="sm" onClick={() => onSelect(id)}>
              {name || `#${id}`}
            </Button>
          </li>
        );
      })}
    </ul>
  );
}
