import type { Language } from "./types";

/** Every language text can be translated between, in the order menus list them. */
export const languages: Language[] = ["chinese", "english", "japanese", "korean", "french", "german", "spanish", "russian"];

/** Each language's own name, for choosing it. */
export const languageNames: Record<Language, string> = {
  chinese: "中文",
  english: "English",
  japanese: "日本語",
  korean: "한국어",
  french: "Français",
  german: "Deutsch",
  spanish: "Español",
  russian: "Русский",
};

/** Compact labels for a direction such as `EN → 中文`. */
export const languageShort: Record<Language, string> = {
  chinese: "中文",
  english: "EN",
  japanese: "日文",
  korean: "韩文",
  french: "FR",
  german: "DE",
  spanish: "ES",
  russian: "RU",
};

export function directionLabel(source: Language, target: Language): string {
  return `${languageShort[source] ?? source} → ${languageShort[target] ?? target}`;
}

/**
 * Reads glossary text, one `term = translation` per line, into entries. Lines without an
 * equals sign are kept as a term with no translation so that saving can point them out.
 */
export function parseGlossary(text: string): Array<{ source: string; target: string }> {
  return text
    .split("\n")
    .map(line => line.trim())
    .filter(Boolean)
    .map(line => {
      const split = line.search(/[=＝]/);
      return split < 0
        ? { source: line, target: "" }
        : { source: line.slice(0, split).trim(), target: line.slice(split + 1).trim() };
    });
}

export function formatGlossary(entries: Array<{ source: string; target: string }> | undefined): string {
  return (entries ?? []).map(entry => `${entry.source} = ${entry.target}`).join("\n");
}
