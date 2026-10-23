import { type RefObject, useEffect, useRef } from "react";

const FOCUSABLE =
  'input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

export interface PopoverOptions {
  open: boolean;
  onClose: () => void;
  /** The id of the button that opens it: focus returns there on close. */
  triggerId: string;
  /** The panel's wrapper (give it `tabIndex={-1}`: it takes the focus when nothing inside can). */
  panelRef: RefObject<HTMLElement | null>;
}

/**
 * Keyboard behaviour of a popover panel: focus moves into it on open, Escape closes it, focus returns to the
 * trigger on close (unless the user already moved it elsewhere, for example into another popover).
 */
export function usePopover({ open, onClose, triggerId, panelRef }: PopoverOptions): void {
  const wasOpen = useRef(false);
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    if (!open) return;
    const panel = panelRef.current;
    (panel?.querySelector<HTMLElement>(FOCUSABLE) ?? panel)?.focus();
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      close.current();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open, panelRef]);

  useEffect(() => {
    if (open) {
      wasOpen.current = true;
      return;
    }
    if (!wasOpen.current) return;
    wasOpen.current = false;
    const active = document.activeElement;
    const adrift = active === null || active === document.body || !active.isConnected;
    if (adrift || panelRef.current?.contains(active)) document.getElementById(triggerId)?.focus();
  }, [open, triggerId, panelRef]);
}
