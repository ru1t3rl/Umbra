/** Save text as a `.jsonl` file through an object URL on an anchor; the URL is revoked after the click. */
export function saveJsonLines(content: string, now: Date = new Date()): void {
  const url = URL.createObjectURL(new Blob([content], { type: "application/x-ndjson" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `umbra-log-${now.toISOString().replace(/[:.]/g, "-")}.jsonl`;
  anchor.hidden = true;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}
