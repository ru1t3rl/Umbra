/** "moving" to "Moving": the status words of the behavior are lower case on the wire. */
export function capitalise(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1);
}
