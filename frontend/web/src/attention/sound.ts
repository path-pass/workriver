// A short, quiet sound when a new row comes into Needs you, if the user turned it on in
// Settings. Made in the page with Web Audio: no file to ship.
import { useEffect, useRef } from 'react'
import type { Inbox } from './select'

const SOUND = 'workriver.sound'
/** Whether the user asked for the sound (off until they turn it on in Settings). */
export function soundWanted() { try { return localStorage.getItem(SOUND) === '1' } catch { return false } }
export function wantSound(on: boolean) { try { localStorage.setItem(SOUND, on ? '1' : '0') } catch { /* not remembered; still applies now */ } }

/** A row of Needs you that sounds: `key` names the row and stays while the row only changes;
 *  `once` names this coming of it (an agent asking again, a gate waiting again is another). */
export interface Sounding { key: string; once: string }

/** The groups whose new rows make the sound: what waits on the user. Gaps and Not urgent
 *  block nobody, so they stay quiet. A decision on a gate holds only what the user wrote
 *  there: their own act, not news. A conversation's row stays while it has unread words, so
 *  it sounds as each of them: a new word is a new row, reading one is not. An agent's row is
 *  its wait (since when it waits): the same agent waiting again is a new row, even when no
 *  read saw it working between. */
const SOUNDING = ['blocking', 'machines', 'decisions', 'messages'] as const
export const soundingRows = (box: Inbox): Sounding[] => SOUNDING.flatMap(g => box[g]
  .filter(r => !(r.kind === 'decision' && r.node?.kind === 'gate'))
  .flatMap((r): Sounding[] => {
    if (r.words) return r.words.map(w => ({ key: `${r.key}:${w}`, once: `${r.key}:${w}` }))
    const once = `${r.key}@${r.since}`
    return [{ key: r.kind === 'agent' ? once : r.key, once }]
  }))

/** Rows that come within this long of a sound make no second one. */
export const QUIET_MS = 5000
/** How long the tabs remember what was played for: a tab that sees a row later than this
 *  plays for it again. */
export const REMEMBER_MS = 10 * 60_000

/**
 * Says, for each list of rows in turn, which rows came that were not in the list before (by
 * their coming). The first list is what was there when the page loaded: not news. A row that
 * only changes keeps its key, so it is not new. Rows seen while the sound is off are still
 * seen, so turning it on later does not play for them.
 */
export function rowSound() {
  let seen: Set<string> | null = null
  return (rows: Sounding[], wanted: boolean): string[] => {
    const before = seen
    seen = new Set(rows.map(r => r.key))
    return wanted && before ? rows.filter(r => !before.has(r.key)).map(r => r.once) : []
  }
}

// What was played for, shared by all open tabs: each reads the same change, one plays.
const PLAYED = 'workriver.sound.played'
interface Played { at: number; rows: Record<string, number> }
let here: Played = { at: -Infinity, rows: {} }
const played = (): Played => {
  try {
    const p = JSON.parse(localStorage.getItem(PLAYED) ?? 'null')
    return typeof p?.at === 'number' && p.rows ? p : { at: -Infinity, rows: {} }
  } catch { return here }
}
const remember = (p: Played) => { here = p; try { localStorage.setItem(PLAYED, JSON.stringify(p)) } catch { /* this tab still keeps it */ } }

/**
 * Claims the sound for these comings of rows, for all open tabs, at the moment it would play:
 * false when every one was played for already (in any tab), or a sound played within QUIET_MS.
 * Rows that come within QUIET_MS of a sound count as played for: that sound covered them.
 */
export function claim(once: string[], at: number): boolean {
  const p = played()
  const rows = Object.fromEntries(Object.entries(p.rows).filter(([, t]) => at - t < REMEMBER_MS))
  const fresh = once.filter(o => !(o in rows))
  if (!fresh.length) return false
  for (const o of fresh) rows[o] = at
  const quiet = Math.abs(at - p.at) < QUIET_MS
  remember({ at: quiet ? p.at : at, rows })
  return !quiet
}

// ---- the sound itself ----

let ctx: AudioContext | null = null
// Whether the last sound could not play (the browser plays one only after a click on the
// page); Settings says so.
let missed = false
export const soundMissed = () => missed

function audio(): AudioContext | null {
  if (!ctx && typeof AudioContext !== 'undefined') ctx = new AudioContext()
  return ctx
}

/** Two soft notes, a quarter of a second in all. */
function tones(c: AudioContext) {
  const t0 = c.currentTime
  for (const [i, hz] of [660, 880].entries()) {
    const at = t0 + i * 0.12
    const osc = c.createOscillator()
    const gain = c.createGain()
    osc.type = 'sine'
    osc.frequency.value = hz
    gain.gain.setValueAtTime(0, at)
    gain.gain.linearRampToValueAtTime(0.08, at + 0.01)
    gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.14)
    osc.connect(gain).connect(c.destination)
    osc.start(at)
    osc.stop(at + 0.15)
  }
}

/** Plays the sound now; whether it played. A context the browser holds back is asked to go
 *  on, but a sound that cannot play now is dropped, not played late. For new rows, the tabs
 *  are asked right before it plays (claim): only a tab that can play takes them. */
export async function chime(once?: string[], now: () => number = Date.now): Promise<boolean> {
  const c = audio()
  if (c && c.state !== 'running') await Promise.race([c.resume().catch(() => {}), new Promise(r => setTimeout(r, 200))])
  missed = !c || c.state !== 'running'
  if (missed || (once && !claim(once, now()))) return false
  tones(c!)
  return true
}

/** One tab's sound for new rows: plays for those of each list that no tab played for yet. */
export function rowPlayer() {
  const step = rowSound()
  return (rows: Sounding[], wanted: boolean): Promise<boolean> => {
    const once = step(rows, wanted)
    return once.length ? chime(once) : Promise.resolve(false)
  }
}

/** The sound for new rows of Needs you; a click or a key on the page lets the browser play it
 *  from then on. */
export function useRowSound(box: Inbox | null) {
  const play = useRef(rowPlayer())
  useEffect(() => { if (box) void play.current(soundingRows(box), soundWanted()) }, [box])
  useEffect(() => {
    const allow = () => { if (soundWanted()) audio()?.resume().catch(() => {}) }
    window.addEventListener('pointerdown', allow, true)
    window.addEventListener('keydown', allow, true)
    return () => { window.removeEventListener('pointerdown', allow, true); window.removeEventListener('keydown', allow, true) }
  }, [])
}
