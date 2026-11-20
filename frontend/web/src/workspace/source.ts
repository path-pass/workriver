// Loading the workspace (from the service, or the sample with VITE_MOCK=1), and again
// whenever the service says it changed. What the views derive from it is in each domain's
// select.ts.
import type { Workspace } from './types'
import { withLines } from '@/code/load'
import { fileTree, shownDelta } from '@/code/files'
import { repoOf } from '@/code/select'
import { unitsOf, noCode } from '@/planning/select'
import { selectionOf } from '@/work/select'

/** Sample data instead of the service, with `VITE_MOCK=1`. */
export const MOCK = import.meta.env.VITE_MOCK === '1'

/**
 * The service's snapshot names each file with its line counts and leaves its texts out (a
 * unit's come with `loadCode`); the sample carries them inline.
 */
function prepare(ws: Workspace): Workspace {
  for (const unit of unitsOf(ws)) for (const c of unit.commits) c.files = c.files.map(f => (f.before === undefined && f.after === undefined ? { ...f, before: null, after: null, lines: [] } : withLines(f)))
  if (import.meta.env.DEV && MOCK) checkCounts(ws)
  return ws
}

async function loadWorkspace(): Promise<Workspace> {
  if (!MOCK) {
    const res = await fetch('/api/workspace')
    if (!res.ok) throw new Error(`${res.status} ${await res.text()}`)
    return prepare(await res.json())
  }
  const [{ default: demo }, { default: lockscreen }] = await Promise.all([import('@/mock/demo.json'), import('@/mock/lockscreen.svg')])
  const ws = demo as unknown as Workspace
  // Sample pictures are files next to the data.
  const pictures: Record<string, string> = { 'lockscreen.svg': lockscreen }
  for (const n of unitsOf(ws).flatMap(u => u.tasks)) for (const a of [...(n.output.attachments ?? []), ...(n.output.findings ?? []).flatMap(f => f.attachments ?? [])]) a.url = pictures[a.url] ?? a.url
  ws.alerts = (await import('@/mock/metrics')).mockAlerts()
  return prepare(ws)
}

/**
 * The one place data comes from: the workspace now, and again whenever the service says it
 * changed. `onDown` is called while the service cannot be reached. Returns a stop function.
 */
export function watchWorkspace(onData: (ws: Workspace) => void, onDown: () => void): () => void {
  let stopped = false
  // At most one read at a time: changes told meanwhile make one more read after it. A hidden
  // tab reads nothing; it reads once when shown again, if anything changed meanwhile.
  let reading = false, again = false, missed = true
  const get = () => {
    if (stopped) return
    if (document.hidden) { missed = true; return }
    if (reading) { again = true; return }
    reading = true
    missed = again = false
    loadWorkspace()
      .then(ws => { if (!stopped) onData(ws) }, () => { if (!stopped) onDown() })
      .finally(() => { reading = false; if (again) get() })
  }
  const shown = () => { if (!document.hidden && missed) get() }
  document.addEventListener('visibilitychange', shown)
  get()
  if (MOCK) return () => { stopped = true; document.removeEventListener('visibilitychange', shown) }
  const events = new EventSource('/api/events')
  events.addEventListener('changed', get)
  // The stream reconnects by itself; what changed while it was down is fetched then.
  let opened = false
  events.onopen = () => { if (opened) get(); opened = true }
  events.onerror = () => { if (!stopped) onDown() }
  return () => { stopped = true; events.close(); document.removeEventListener('visibilitychange', shown) }
}

/**
 * Self-check, in development: for every unit and every scope (nothing picked, each node
 * picked), each file's count in the tree equals the change the single-file view shows.
 */
function checkCounts(ws: Workspace) {
  for (const unit of unitsOf(ws).filter(u => !noCode(u)))
    for (const pick of [null, ...unit.tasks.map(n => ({ kind: 'task', id: n.id }) as const)]) {
      const sel = selectionOf(unit, pick)
      for (const r of fileTree(repoOf(ws, unit).modules, unit, sel, true).rows.values()) {
        if (r.folder || !r.status) continue
        const shown = shownDelta(unit, sel, r.id)
        console.assert(!!shown && r.add === shown.add && r.del === shown.del, 'tree and file view disagree', unit.id, pick?.id, r.id)
      }
    }
}

