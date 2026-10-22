export type MessageParams = Record<string, string | number>;

/**
 * English-only message catalogue (ASM-333). `{name}` placeholders are replaced in one pass; an unknown
 * placeholder stays visible so a missing param is noticed. Swap the body for a real i18n library later
 * without touching call sites.
 */
export function defineMessages<const M extends Record<string, string>>(
  en: M,
): (key: keyof M, params?: MessageParams) => string {
  return (key, params) =>
    (Object.hasOwn(en, key) ? en[key as string] : undefined)?.replace(
      /\{(\w+)\}/g,
      (whole, name: string) => {
        const value = params?.[name];
        return value === undefined ? whole : String(value);
      },
    ) ?? String(key);
}
