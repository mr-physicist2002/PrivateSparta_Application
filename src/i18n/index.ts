import { en } from "./en";
import { fa } from "./fa";
import { zh } from "./zh";
import { ru } from "./ru";
import { useAppStore } from "../state/store";

export type MessageKey = keyof typeof en;
export type Language = "en" | "fa" | "zh" | "ru";

export const LANGUAGES: Array<{ value: Language; label: string }> = [
  { value: "en", label: "English" },
  { value: "fa", label: "فارسی" },
  { value: "zh", label: "中文" },
  { value: "ru", label: "Русский" },
];

const DICTS: Record<Language, Record<MessageKey, string>> = { en, fa, zh, ru };

export function isRtl(lang: string): boolean {
  return lang === "fa";
}

function normalize(lang: string): Language {
  return (["en", "fa", "zh", "ru"] as const).includes(lang as Language)
    ? (lang as Language)
    : "en";
}

/** Reactive translator — re-renders subscribers when the language changes. */
export function useT(): (key: MessageKey) => string {
  const lang = useAppStore((s) => normalize(s.settings.language));
  return (key) => DICTS[lang][key] ?? en[key] ?? key;
}

/** Non-reactive lookup for code outside React components. */
export function t(key: MessageKey): string {
  const lang = normalize(useAppStore.getState().settings.language);
  return DICTS[lang][key] ?? en[key] ?? key;
}

/** "{n}" interpolation for count strings. */
export function interpolate(template: string, n: number): string {
  return template.replace("{n}", String(n));
}

/** Keeps <html dir/lang> in sync; call once from App. */
export function applyDocumentLanguage(langRaw: string): void {
  const lang = normalize(langRaw);
  document.documentElement.lang = lang;
  document.documentElement.dir = isRtl(lang) ? "rtl" : "ltr";
}
