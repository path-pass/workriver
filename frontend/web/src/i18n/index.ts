// i18n through i18next: typed keys (i18next.d.ts), interpolation and plurals from the
// library, the language detected from the browser with a remembered override, and numbers
// and dates through Intl with the active locale.
import i18n, { type TFunction } from 'i18next'
import LanguageDetector from 'i18next-browser-languagedetector'
import { useMemo } from 'react'
import { initReactI18next, useTranslation } from 'react-i18next'
import { en } from './en'
import { ja } from './ja'
import { zhCN } from './zh-CN'

/** Each language is named in itself, so the switch reads the same in any of them. */
export const LANGS = [
  { id: 'zh-CN', label: '中文' },
  { id: 'en', label: 'English' },
  { id: 'ja', label: '日本語' },
] as const
export type Lang = (typeof LANGS)[number]['id']
/**
 * A guess at the language of content (titles, notes, findings), which the UI does not know:
 * kana means Japanese, Han without kana Chinese, anything else unset. Put on the element that
 * shows it, so the browser picks the right CJK glyphs whatever the UI language is.
 */
export const contentLang = (s: string | null | undefined): string | undefined =>
  !s ? undefined : /[\p{Script=Hiragana}\p{Script=Katakana}]/u.test(s) ? 'ja' : /\p{Script=Han}/u.test(s) ? 'zh-CN' : undefined
export type T = TFunction
const STORE = 'workriver.lang'

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: { en: { translation: en }, 'zh-CN': { translation: zhCN }, ja: { translation: ja } },
    supportedLngs: LANGS.map(l => l.id),
    fallbackLng: 'en',
    // React escapes. `runtime`: the agent runtime's name, for the texts that must name it
    // (`{{runtime}}`); the service says it (setRuntime).
    interpolation: { escapeValue: false, defaultVariables: { runtime: '' } },
    detection: {
      order: ['localStorage', 'navigator'],
      lookupLocalStorage: STORE,
      // Only an explicit choice is remembered (setLang below); otherwise keep following the browser.
      caches: [],
      convertDetectedLanguage: lng => {
        const l = lng.toLowerCase()
        return l.startsWith('zh') ? 'zh-CN' : l.startsWith('ja') ? 'ja' : 'en'
      },
    },
  })
const syncHtml = (lng: string) => { document.documentElement.lang = lng }
i18n.on('languageChanged', syncHtml)
syncHtml(i18n.resolvedLanguage ?? 'en')

/** The agent runtime's name as the service says it (`Workspace.runtime`): the texts that name
 *  the runtime take it from here. The page itself names none. */
export function setRuntime(name: string) {
  i18n.options.interpolation = { ...i18n.options.interpolation, defaultVariables: { runtime: name } }
}

function setLang(lang: Lang) {
  i18n.changeLanguage(lang)
  try { localStorage.setItem(STORE, lang) } catch { /* not remembered, still switched */ }
}

function formatters(lang: string) {
  const nf = new Intl.NumberFormat(lang)
  const day = new Intl.DateTimeFormat(lang, { dateStyle: 'medium' })
  const minute = new Intl.DateTimeFormat(lang, { dateStyle: 'medium', timeStyle: 'short' })
  // Japanese's narrow units are Latin (17d); its short ones, without their space, read as Japanese (17日).
  const ja = lang === 'ja'
  const span = (unit: string) => {
    const f = new Intl.NumberFormat(lang, { style: 'unit', unit, unitDisplay: ja ? 'short' : 'narrow', maximumFractionDigits: 0 })
    return ja ? (n: number) => f.format(n).replace(/\s/g, '') : (n: number) => f.format(n)
  }
  const spans = { minute: span('minute'), hour: span('hour'), day: span('day') }
  return {
    num: (n: number) => nf.format(n),
    /** Formats the data's `YYYY-MM-DD HH:mm` strings; anything unparseable is shown as it is. */
    date: (s: string, time = true) => {
      const d = new Date(s.replace(' ', 'T'))
      return Number.isNaN(d.getTime()) ? s : (time ? minute : day).format(d)
    },
    /** How long since `since` (milliseconds), in its largest whole unit: 5m, 3h, 2d (5分, 3時間, 2日). */
    waited: (since: number) => {
      const m = Math.max(0, (Date.now() - since) / 60000)
      return m < 60 ? spans.minute(Math.floor(m)) : m < 1440 ? spans.hour(Math.floor(m / 60)) : spans.day(Math.floor(m / 1440))
    },
    /** A time in milliseconds, written out. */
    when: (t: number) => (Number.isFinite(t) ? minute.format(new Date(t)) : ''),
  }
}

/** The translator plus what goes with a language: the switch and the Intl formatters. */
export function useT() {
  const { t, i18n: active } = useTranslation()
  const lang = (active.resolvedLanguage ?? 'en') as Lang
  return useMemo(() => ({ t, lang, setLang, ...formatters(lang) }), [t, lang])
}
