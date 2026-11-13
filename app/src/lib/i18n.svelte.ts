// Interface language. t() reads the current language from a rune, so text in
// templates updates as soon as the language changes.
import { en, type Key } from "./locales/en";
import { tr } from "./locales/tr";

export type Lang = "en" | "tr";
export type LanguageSetting = "system" | Lang;

const TEXTS: Record<Lang, Record<Key, string>> = { en, tr };

let current = $state<Lang>(systemLanguage());

function systemLanguage(): Lang {
  return navigator.language.toLowerCase().startsWith("tr") ? "tr" : "en";
}

/** The language in use, e.g. for number and date formatting. */
export function lang(): Lang {
  return current;
}

export function setLanguage(setting: LanguageSetting) {
  // Only write the rune here: reading it would make the calling $effect
  // depend on it, and the page's effect would then undo the Settings preview.
  const next = setting === "system" ? systemLanguage() : setting;
  current = next;
  document.documentElement.lang = next;
}

/**
 * The text for `key` with `{name}` placeholders filled in. A text written as
 * "one|other" picks a form by `n` (Turkish uses one form, so it has no "|").
 */
export function t(key: Key, params: Record<string, string | number> = {}): string {
  let text = TEXTS[current][key] ?? en[key];
  if (text.includes("|")) {
    const [one, other] = text.split("|");
    text = params.n === 1 ? one : other;
  }
  return text.replace(/\{(\w+)\}/g, (match, name) => (name in params ? String(params[name]) : match));
}
