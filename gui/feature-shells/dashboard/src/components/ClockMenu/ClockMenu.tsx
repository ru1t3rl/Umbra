import { ClockControl, StepControls } from "@umbra/scenario-control";
import { Button, Panel } from "@umbra/ui-kit";
import { type ReactElement, useId, useRef } from "react";
import { usePopover } from "../../hooks/usePopover.ts";
import { t } from "../../messages.ts";
import "./ClockMenu.css";

export interface ClockMenuProps {
  open: boolean;
  onToggle: () => void;
  onClose: () => void;
  /** Icon-only button (a compact bar). */
  compact: boolean;
}

/**
 * The clock in the bar: a button that opens ClockControl and the step controls in a small Panel under it. The
 * panel stays mounted while closed (hidden), so a pending request and its error survive closing and the status
 * "Clock: ..." stays in the DOM (ASM-977). Inside the floating bar a Panel is a surface, not glass.
 */
export function ClockMenu({ open, onToggle, onClose, compact }: ClockMenuProps): ReactElement {
  const id = useId();
  const triggerId = `${id}-button`;
  const panelId = `${id}-panel`;
  const panelRef = useRef<HTMLDivElement>(null);
  usePopover({ open, onClose, triggerId, panelRef });
  return (
    <div className="dashboard-clock">
      {compact ? (
        <Button
          id={triggerId}
          variant="tonal"
          icon="schedule"
          aria-label={t("clock.title")}
          aria-expanded={open}
          aria-controls={panelId}
          onClick={onToggle}
        />
      ) : (
        <Button
          id={triggerId}
          variant="tonal"
          icon="schedule"
          aria-expanded={open}
          aria-controls={panelId}
          onClick={onToggle}
        >
          {t("clock.title")}
        </Button>
      )}
      <div
        className="dashboard-clock-panel"
        id={panelId}
        ref={panelRef}
        tabIndex={-1}
        hidden={!open}
      >
        <Panel title={t("clock.title")} width="100%">
          <div className="dashboard-clock-body">
            <ClockControl />
            <StepControls />
          </div>
        </Panel>
      </div>
    </div>
  );
}
