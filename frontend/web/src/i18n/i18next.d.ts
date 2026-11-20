// Makes t() accept only keys that exist in en.ts.
import 'i18next'
import type { en } from './en'

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation'
    resources: { translation: typeof en }
    /** Filled in for every text: the agent runtime's name (setRuntime). */
    defaultVariables: { runtime: string }
  }
}
