// Whether this page is the user's: the one-time code from `workriver open`, and the link
// opened while signed out, kept for the sign-in.

/**
 * Whether this page is the user's. A one-time code in the address (`#k=…`, from
 * `workriver open`) is exchanged for the session cookie first and taken out of the address,
 * so it is not kept in history or shared with a copied link.
 */
export async function startSession(): Promise<boolean> {
  const code = new URLSearchParams(location.hash.slice(1)).get('k')
  if (code) {
    // `workriver open` opens the root: a link the user opened while signed out lands there.
    const wanted = recallWanted()
    history.replaceState(null, '', location.pathname === '/' && wanted ? wanted : location.pathname + location.search)
    await fetch('/api/session', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ code }) }).catch(() => undefined)
  }
  const res = await fetch('/api/session')
  if (!res.ok) throw new Error(await res.text())
  return (await res.json()).user === true
}

const WANTED = 'workriver.signin-to'

/** A link opened while signed out (`/u1/3`), kept for the sign-in that follows in a new tab. */
export function rememberWanted() {
  if (location.pathname === '/') return
  try { localStorage.setItem(WANTED, JSON.stringify({ path: location.pathname + location.search, at: Date.now() })) } catch { /* lands on the root */ }
}

/** That link, once, if it is from the last ten minutes. */
function recallWanted(): string | null {
  try {
    const w = JSON.parse(localStorage.getItem(WANTED) ?? 'null')
    localStorage.removeItem(WANTED)
    return w && Date.now() - w.at < 600_000 && typeof w.path === 'string' && /^\/(?!\/)/.test(w.path) ? w.path : null
  } catch { return null }
}
