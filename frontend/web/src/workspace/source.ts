// Loading the workspace (from the service, or the sample with VITE_MOCK=1), and again
// whenever the service says it changed. What the views derive from it is in each domain's
// select.ts.
import type { Workspace } from './types'
import { withLines } from '@/code/load'
import type { FileChange } from '@/code/types'
import { fileTree, shownDelta } from '@/code/files'
import { repoOf } from '@/code/select'
import { unitsOf, noCode } from '@/planning/select'
import { selectionOf } from '@/work/select'
import { changes, read } from '@/api/http'
import { setRuntime } from '@/i18n'
import { HIDDEN_READ, VISIBLE_READ_PACING } from '@/config/defaults'
import { recordWorkspaceReady, WORKSPACE_PATH } from './first-load'

/** Sample data instead of the service, with `VITE_MOCK=1`. */
export const MOCK = import.meta.env.VITE_MOCK === '1'

/**
 * With the sample, what the page changed (an intent edited), as a function of the workspace
 * shown: the sample is shown again with it, as the service would after the change.
 */
export const MOCK_CHANGED = 'workriver:mock-changed'

/**
 * The service's snapshot names each file with its line counts and leaves its texts out (a
 * unit's come with `loadCode`); the sample carries them inline. The runtime's name it gives
 * goes to the texts that name the runtime.
 */
function prepare(ws: Workspace): Workspace {
  setRuntime(ws.runtime)
  const textless = (f: FileChange) => (f.before === undefined && f.after === undefined ? { ...f, before: null, after: null, lines: [] } : withLines(f))
  for (const unit of unitsOf(ws)) {
    for (const c of unit.commits) c.files = c.files.map(textless)
    if (unit.net) unit.net.files = unit.net.files.map(textless)
  }
  if (import.meta.env.DEV && MOCK) checkCounts(ws)
  return ws
}

async function loadWorkspace(): Promise<Workspace> {
  if (!MOCK) {
    const started = performance.now()
    const ws = prepare(await read<Workspace>(WORKSPACE_PATH))
    recordWorkspaceReady(ws, started)
    return ws
  }
  const [{ default: demo }, { default: lockscreen }] = await Promise.all([import('@/mock/demo.json'), import('@/mock/lockscreen.svg')])
  const ws = demo as unknown as Workspace
  // The sample names no git entries: each file is a regular one, its text standing for its id.
  const entry = (text?: string | null) => (text == null ? ['000000', '0'.repeat(40)] : ['100644', text])
  for (const c of unitsOf(ws).flatMap(u => [...u.commits, ...(u.net ? [u.net] : [])])) for (const f of c.files) if (f.beforeMode === undefined) [[f.beforeMode, f.beforeId], [f.afterMode, f.afterId]] = [entry(f.before), entry(f.after)]
  // Sample pictures are files next to the data.
  const pictures: Record<string, string> = { 'lockscreen.svg': lockscreen }
  for (const n of unitsOf(ws).flatMap(u => u.tasks)) for (const a of [...(n.output.attachments ?? []), ...(n.output.findings ?? []).flatMap(f => f.attachments ?? [])]) a.url = pictures[a.url] ?? a.url
  ws.alerts = (await import('@/mock/metrics')).mockAlerts()
  return prepare(ws)
}

/**
 * The one place data comes from: the workspace now, and again whenever the service says it
 * changed. `onDown` is called while the service cannot be reached. `awake` says whether a
 * hidden tab still reads (the user asked to hear or be told of what comes). Returns a stop
 * function.
 */
export function watchWorkspace(onData: (ws: Workspace) => void, onDown: () => void, awake: () => boolean): () => void {
  let stopped = false
  let opened = false
  // Catch-up takes priority over changes waiting for the visible read gap.
  let reading = false, pending: 'changed' | 'catch-up' | undefined
  let lastReadStarted = -Infinity, nextVisibleRead = -Infinity
  let later: ReturnType<typeof setTimeout> | undefined
  let shown: Workspace | null = null
  const get = (reason?: typeof pending) => {
    if (stopped) return
    if (reason && pending !== 'catch-up') pending = reason
    if (!pending || reading) return
    const hidden = document.hidden
    if (hidden && !awake()) return
    const started = performance.now()
    const due = hidden ? lastReadStarted + HIDDEN_READ : pending === 'changed' ? nextVisibleRead : started
    if (due > started) {
      later ??= setTimeout(() => { later = undefined; get() }, Math.ceil(due - started))
      return
    }
    clearTimeout(later)
    later = undefined
    reading = true
    pending = undefined
    lastReadStarted = started
    const beforeOpen = !opened
    loadWorkspace()
      .then(ws => {
        if (stopped) return
        onData((shown = ws))
        // A pending read begun before the stream opened may have missed Git becoming ready.
        if (ws.gitPending && beforeOpen && opened) pending = 'catch-up'
      }, () => { if (!stopped) onDown() })
      .finally(() => {
        const ended = performance.now()
        nextVisibleRead = ended + Math.max(VISIBLE_READ_PACING.minGap, VISIBLE_READ_PACING.durationMultiplier * (ended - started))
        reading = false
        get()
      })
  }
  const visibilityChanged = () => {
    if (!pending) return
    // A waiting change moves to the read policy of the tab's new visibility.
    clearTimeout(later)
    later = undefined
    get(document.hidden ? undefined : 'catch-up')
  }
  document.addEventListener('visibilitychange', visibilityChanged)
  get('catch-up')
  if (MOCK) {
    const changed = (e: Event) => { if (shown && !stopped) onData((shown = (e as CustomEvent<(ws: Workspace) => Workspace>).detail(shown))) }
    window.addEventListener(MOCK_CHANGED, changed)
    return () => { stopped = true; clearTimeout(later); document.removeEventListener('visibilitychange', visibilityChanged); window.removeEventListener(MOCK_CHANGED, changed) }
  }
  const events = changes()
  // Git warm-up uses the same change event to tell us the full snapshot is ready.
  events.addEventListener('changed', () => get(shown?.gitPending ? 'catch-up' : 'changed'))
  // Reopening catches up after a disconnect; the first open does so if Git was pending.
  events.onopen = () => {
    const refresh = opened || shown?.gitPending
    opened = true
    if (refresh) get('catch-up')
  }
  events.onerror = () => { if (!stopped) onDown() }
  return () => { stopped = true; clearTimeout(later); events.close(); document.removeEventListener('visibilitychange', visibilityChanged) }
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
