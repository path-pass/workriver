export const recall = (key: string) => { try { return localStorage.getItem(key) } catch { return null } }
export const remember = (key: string, value: string) => { try { localStorage.setItem(key, value) } catch { /* not remembered, still shown */ } }
