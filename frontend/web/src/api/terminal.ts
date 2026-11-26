// An agent's terminal, from the page: its screen as text, the keys the user presses to answer
// a dialog on it, and its terminal view as a stream of frames. All three are the user's alone;
// the page's session cookie goes with them (same origin). Nothing of a screen or a frame is
// kept in browser storage or logged.
import type { Pressed, Press, Refusal, ScreenView } from '@/agents/types'
import { eventStream, request, SIGNED_OUT } from './http'

/** Where an agent is, as the routes name it: its session and machine (empty: this one). */
export interface Place { session: string; machine?: string }

const query = (p: Place) => `agent=${encodeURIComponent(p.session)}&machine=${encodeURIComponent(p.machine ?? '')}`

/** A non-2xx answer as a refusal: the service's code and words, or, if it said none, "failed". */
async function refusal(res: Response): Promise<Refusal> {
  const text = await res.text()
  try {
    const r = JSON.parse(text)
    if (r && typeof r.code === 'string') return { code: r.code, reason: String(r.reason ?? ''), screen: r.screen ?? undefined }
  } catch { /* not JSON: its words as they are */ }
  return { code: 'failed', reason: `${res.status} ${text}`.trim() }
}

/** A request that did not get an answer at all (the service cannot be reached). */
const lost = (e: unknown): Refusal => ({ code: 'failed', reason: String(e) })

/** What the agent's screen shows now; a refusal is thrown as a `Refusal`. */
export async function readScreen(p: Place): Promise<ScreenView> {
  let res: Response
  try { res = await request(`/api/agent/screen?${query(p)}`, { cache: 'no-store' }) } catch (e) { throw lost(e) }
  if (!res.ok) throw await refusal(res)
  return res.json()
}

/**
 * Presses an option or a key, bound to the screen the page showed (`shown`, its digest): the
 * service sends nothing when its screen differs. A refusal is thrown as a `Refusal`. Never
 * sent again by itself: a lost answer may mean a key went.
 */
export async function pressKeys(p: Place, shown: string, press: Press): Promise<Pressed> {
  let res: Response
  try {
    res = await request('/api/agent/keys', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ agent: p.session, ...(p.machine ? { machine: p.machine } : {}), shown, press }),
    })
  } catch (e) { throw { ...lost(e), code: 'unsure' } satisfies Refusal }
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) throw await refusal(res)
  return res.json()
}

/** One frame of a terminal view: what the terminal draws (ANSI included), base64. */
export interface Frame { full: boolean; cols: number; rows: number; seq: number; bytes: string }

/** The agent's terminal as frames, the first one whole; closing it ends the view. */
export const viewTerminal = (p: Place) => eventStream(`/api/agent/view?${query(p)}`)
