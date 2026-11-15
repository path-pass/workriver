// The colour theme: follow the system unless the user picked one. A pick is remembered
// and applied as data-theme on <html>, which the tokens in index.css listen to.
export const THEMES = ['system', 'light', 'dark'] as const
export type Theme = (typeof THEMES)[number]
const STORE = 'workriver.theme'

export function readTheme(): Theme {
  try {
    const saved = localStorage.getItem(STORE)
    if (saved === 'light' || saved === 'dark') return saved
  } catch { /* storage unavailable: follow the system */ }
  return 'system'
}

export function applyTheme(theme: Theme) {
  if (theme === 'system') delete document.documentElement.dataset.theme
  else document.documentElement.dataset.theme = theme
}

export function setTheme(theme: Theme) {
  applyTheme(theme)
  try {
    if (theme === 'system') localStorage.removeItem(STORE)
    else localStorage.setItem(STORE, theme)
  } catch { /* not remembered, still applied */ }
}
