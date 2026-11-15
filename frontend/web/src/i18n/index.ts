// i18n through i18next: typed keys (i18next.d.ts), interpolation and plurals from the
// library, the language detected from the browser with a remembered override, and numbers
// and dates through Intl with the active locale.
import i18n, { type TFunction } from 'i18next'
import LanguageDetector from 'i18next-browser-languagedetector'
import { useMemo } from 'react'
import { initReactI18next, useTranslation } from 'react-i18next'
import { en } from './en'
import { zhCN } from './zh-CN'

/** Each language is named in itself, so the switch reads the same in either. */
export const LANGS = [
  { id: 'zh-CN', label: '中文' },
  { id: 'en', label: 'English' },
] as const
export type Lang = (typeof LANGS)[number]['id']
export type T = TFunction
const STORE = 'workriver.lang'

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: { en: { translation: en }, 'zh-CN': { translation: zhCN } },
    supportedLngs: LANGS.map(l => l.id),
    fallbackLng: 'en',
    interpolation: { escapeValue: false }, // React escapes
    detection: {
      order: ['localStorage', 'navigator'],
      lookupLocalStorage: STORE,
      // Only an explicit choice is remembered (setLang below); otherwise keep following the browser.
      caches: [],
      convertDetectedLanguage: lng => (lng.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en'),
    },
  })
const syncHtml = (lng: string) => { document.documentElement.lang = lng }
i18n.on('languageChanged', syncHtml)
syncHtml(i18n.resolvedLanguage ?? 'en')

function setLang(lang: Lang) {
  i18n.changeLanguage(lang)
  try { localStorage.setItem(STORE, lang) } catch { /* not remembered, still switched */ }
}

function formatters(lang: string) {
  const nf = new Intl.NumberFormat(lang)
  const day = new Intl.DateTimeFormat(lang, { dateStyle: 'medium' })
  const minute = new Intl.DateTimeFormat(lang, { dateStyle: 'medium', timeStyle: 'short' })
  const span = (unit: string) => new Intl.NumberFormat(lang, { style: 'unit', unit, unitDisplay: 'narrow', maximumFractionDigits: 0 })
  const spans = { minute: span('minute'), hour: span('hour'), day: span('day') }
  return {
    num: (n: number) => nf.format(n),
    /** Formats the data's `YYYY-MM-DD HH:mm` strings; anything unparseable is shown as it is. */
    date: (s: string, time = true) => {
      const d = new Date(s.replace(' ', 'T'))
      return Number.isNaN(d.getTime()) ? s : (time ? minute : day).format(d)
    },
    /** How long since `since` (milliseconds), in its largest whole unit: 5m, 3h, 2d. */
    waited: (since: number) => {
      const m = Math.max(0, (Date.now() - since) / 60000)
      return m < 60 ? spans.minute.format(Math.floor(m)) : m < 1440 ? spans.hour.format(Math.floor(m / 60)) : spans.day.format(Math.floor(m / 1440))
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
