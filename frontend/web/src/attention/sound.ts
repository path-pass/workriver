// A short, quiet sound when a new row comes into Needs you, if the user turned it on in
// Settings. Made in the page with Web Audio: no file to ship.
import { useEffect, useRef } from 'react'
import type { Inbox } from './select'

const SOUND = 'workriver.sound'
/** Whether the user asked for the sound (off until they turn it on in Settings). */
export function soundWanted() { try { return localStorage.getItem(SOUND) === '1' } catch { return false } }
export function wantSound(on: boolean) { try { localStorage.setItem(SOUND, on ? '1' : '0') } catch { /* not remembered; still applies now */ } }

/** The groups whose new rows make the sound: what waits on the user. Gaps and Not urgent
 *  block nobody, so they stay quiet. */
const SOUNDING = ['blocking', 'machines', 'decisions', 'messages'] as const
export const soundingRows = (box: Inbox) => SOUNDING.flatMap(g => box[g].map(r => r.key))

/** Rows that come within this long of a sound make no second one. */
export const QUIET_MS = 5000

/**
 * Says, for each list of rows in turn, whether it plays: only when a row is there that was not
 * in the list before. The first list is what was there when the page loaded: not news. A row
 * that only changes keeps its key, so it is not new. Rows seen while the sound is off are
 * still seen, so turning it on later does not play for them.
 */
export function rowSound(now: () => number = Date.now) {
  let seen: Set<string> | null = null
  let last = -Infinity
  return (keys: string[], wanted: boolean): boolean => {
    const before = seen
    seen = new Set(keys)
    if (!wanted || !before || keys.every(k => before.has(k))) return false
    const at = now()
    if (at - last < QUIET_MS) return false
    last = at
    return true
  }
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

/** Plays the sound now; whether it could. A context the browser holds back is asked to go on,
 *  but a sound that cannot play now is dropped, not played late. */
export async function chime(): Promise<boolean> {
  const c = audio()
  if (c && c.state !== 'running') await Promise.race([c.resume().catch(() => {}), new Promise(r => setTimeout(r, 200))])
  missed = !c || c.state !== 'running'
  if (!missed) tones(c!)
  return !missed
}

/** The sound for new rows of Needs you; a click or a key on the page lets the browser play it
 *  from then on. */
export function useRowSound(box: Inbox | null) {
  const step = useRef(rowSound())
  useEffect(() => { if (box && step.current(soundingRows(box), soundWanted())) void chime() }, [box])
  useEffect(() => {
    const allow = () => { if (soundWanted()) audio()?.resume().catch(() => {}) }
    window.addEventListener('pointerdown', allow, true)
    window.addEventListener('keydown', allow, true)
    return () => { window.removeEventListener('pointerdown', allow, true); window.removeEventListener('keydown', allow, true) }
  }, [])
}
