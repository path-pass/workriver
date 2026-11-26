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
import { HIDDEN_READ } from '@/config/defaults'
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
  // At most one read at a time: changes told meanwhile make one more read after it. A hidden
  // tab reads nothing unless `awake`, and then at most once every HIDDEN_READ; it reads once
  // when shown again, if anything changed meanwhile.
  let reading = false, again = false, missed = true
  let lastRead = -Infinity, later: ReturnType<typeof setTimeout> | undefined
  let shown: Workspace | null = null
  const get = () => {
    if (stopped) return
    if (document.hidden) {
      missed = true
      if (!awake()) return
      const wait = lastRead + HIDDEN_READ - Date.now()
      if (wait > 0) { later ??= setTimeout(() => { later = undefined; if (missed) get() }, wait); return }
    }
    if (reading) { again = true; return }
    reading = true
    lastRead = Date.now()
    missed = again = false
    const beforeOpen = !opened
    loadWorkspace()
      .then(ws => {
        if (stopped) return
        onData((shown = ws))
        // A pending read begun before the stream opened may have missed Git becoming ready.
        if (ws.gitPending && beforeOpen && opened) again = true
      }, () => { if (!stopped) onDown() })
      .finally(() => { reading = false; if (again) get() })
  }
  const visible = () => { if (!document.hidden && missed) get() }
  document.addEventListener('visibilitychange', visible)
  get()
  if (MOCK) {
    const changed = (e: Event) => { if (shown && !stopped) onData((shown = (e as CustomEvent<(ws: Workspace) => Workspace>).detail(shown))) }
    window.addEventListener(MOCK_CHANGED, changed)
    return () => { stopped = true; clearTimeout(later); document.removeEventListener('visibilitychange', visible); window.removeEventListener(MOCK_CHANGED, changed) }
  }
  const events = changes()
  events.addEventListener('changed', get)
  // Reopening catches up after a disconnect; the first open does so if Git was pending.
  events.onopen = () => {
    const refresh = opened || shown?.gitPending
    opened = true
    if (refresh) get()
  }
  events.onerror = () => { if (!stopped) onDown() }
  return () => { stopped = true; clearTimeout(later); events.close(); document.removeEventListener('visibilitychange', visible) }
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
