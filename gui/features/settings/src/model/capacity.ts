export const MIN_CAPACITY = 1000;
export const MAX_CAPACITY = 50_000;

/** The journal capacity a typed text means: a whole number from 1,000 to 50,000, else `undefined`. */
export function parseCapacity(text: string): number | undefined {
  if (text.trim() === "") return undefined;
  const value = Number(text);
  return Number.isInteger(value) && value >= MIN_CAPACITY && value <= MAX_CAPACITY
    ? value
    : undefined;
}
