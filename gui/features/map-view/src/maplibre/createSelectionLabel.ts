/**
 * The one DOM label of the selected entity (ADR 0078): created when there is something to show, moved with
 * `transform`, removed when there is not. Text is written only when it changes.
 */
export interface SelectionLabel {
  /** Shows `text` at the pixel, or removes the label when `text` is null. */
  show(text: string | null, x: number, y: number): void;
  text(): string | null;
  destroy(): void;
}

export function createSelectionLabel(container: HTMLElement): SelectionLabel {
  let element: HTMLElement | undefined;
  return {
    show(text, x, y) {
      if (text === null) {
        element?.remove();
        element = undefined;
        return;
      }
      if (element === undefined) {
        element = container.ownerDocument.createElement("div");
        element.className = "umbra-live-map-label type-data";
        container.append(element);
      }
      if (element.textContent !== text) element.textContent = text;
      element.style.transform = `translate(${x}px, ${y}px)`;
    },
    text: () => element?.textContent ?? null,
    destroy() {
      element?.remove();
      element = undefined;
    },
  };
}
