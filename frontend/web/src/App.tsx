// The shell and the state of what is on screen: which unit, what is picked inside it, what
// the user has put in. Everything else is derived.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ActContext, findingKey, gateKey, type Act } from '@/act'
import { Activity } from '@/components/Flow'
import { SideButtons, SidePanel, SideSheet, type CommitFocus, type Section } from '@/components/Side'
import { Notes, Topics } from '@/components/bits'
import { Start } from '@/components/Start'
import { isField, type Talk } from '@/components/Ask'
import { CodeLayer } from '@/components/CodeLayer'
import { Rail } from '@/components/Rail'
import { agents, askeeOf, blame, NO_EDITS, repoOf, selectionOf, shownDelta, unitsOf, watchWorkspace, withEdits, type Edits, type Lines, type Pick } from '@/data'
import { useT } from '@/i18n'
import { ask, choreDone, decideFinding, detach, MANAGER, submitGate, type Delivery } from '@/runtime'
import type { AskContext, Task, Turn, Unit, Workspace } from '@/types'

const SIDE = 'workriver.side'
const RAIL = 'workriver.rail'
const remember = (key: string, value: string) => { try { localStorage.setItem(key, value) } catch { /* not remembered, still shown */ } }
const recall = (key: string) => { try { return localStorage.getItem(key) } catch { return null } }

export default function App() {
  const { t } = useT()
  const [data, setData] = useState<Workspace | null>(null)
  // The service cannot be reached right now; what was shown stays.
  const [down, setDown] = useState(false)
  const [unitId, setUnitId] = useState<string | null>(null)
  const [pick, setPick] = useState<Pick>(null)
  const [onlyChanged, setOnlyChanged] = useState(true)
  const [q, setQ] = useState('')
  // The rail is open unless the user folded it; remembered.
  const [railOpen, setRailOpen] = useState(() => recall(RAIL) !== 'closed')
  const toggleRail = useCallback(() => setRailOpen(o => { remember(RAIL, o ? 'closed' : 'open'); return !o }), [])
  // The side panel shows one section or is folded; which, is remembered.
  const [side, setSide] = useState<Section | null>(() => { const v = recall(SIDE); return v === 'inbox' || v === 'agents' || v === 'commits' ? v : null })
  const showSide = (next: Section | null) => { setSide(next); remember(SIDE, next ?? 'closed') }
  // Narrow screens: the same content as a sheet over the page; not remembered.
  const [sheet, setSheet] = useState<Section | null>(null)

  // What the user put in during this sitting; the data shown is the data with it on top.
  const [edits, setEdits] = useState<Edits>(NO_EDITS)
  const ws = useMemo(() => data && withEdits(data, edits), [data, edits])
  // What each live session is about, for the agent marks' tooltips; a name that repeats
  // across workspaces is left out rather than guessed.
  const topics = useMemo(() => {
    const live = (data?.sessions ?? []).filter(s => !s.ended)
    const once = (name: string) => live.filter(s => s.agent === name).length === 1
    return new Map(live.flatMap(s => (s.topic && once(s.agent) ? [[s.agent, s.topic] as const] : [])))
  }, [data])

  // The file picked in the code layer and lines picked in a diff; a question takes them as context.
  const [file, setFile] = useState<string | null>(null)
  const [lines, setLines] = useState<Lines | null>(null)
  // Lines belong to the view they were picked in: the single file, or the list.
  useEffect(() => setLines(l => (l && l.commit === undefined && l.path === file ? l : null)), [file])
  // Questions asked in this sitting, by unit and task, after the recorded ones.
  const [asked, setAsked] = useState<Record<string, Record<string, Turn[]>>>({})
  // A request for the ask box to take the cursor, with more context for one task.
  const [askReq, setAskReq] = useState<{ at: number; task: string | null; extra: AskContext[] }>({ at: 0, task: null, extra: [] })
  useEffect(() => setAskReq(r => (r.extra.length ? { ...r, extra: [] } : r)), [lines])

  const unit = unitsOf(ws).find(i => i.id === unitId) ?? null
  const sel = useMemo(() => (unit ? selectionOf(unit, pick) : null), [unit, pick])

  // Whatever opens a unit (the rail, the inbox, an agent) sets the pick the same way, so the
  // file tree opens on what is picked wherever the user came from.
  const pickUnit = useCallback((w: Workspace, id: string, next: Pick = null) => {
    if (!unitsOf(w).some(i => i.id === id)) return
    setUnitId(id)
    setPick(next)
  }, [])

  // The first unit opens once there is one; later updates keep what is open.
  useEffect(() => watchWorkspace(w => {
    setData(w)
    setDown(false)
    setUnitId(id => id ?? unitsOf(w)[0]?.id ?? null)
  }, () => setDown(true)), [])

  // Picking the same thing again clears it.
  const onPick = useCallback((next: Pick) => {
    setPick(now => (next && now && next.id === now.id && next.finding === now.finding ? null : next))
  }, [])

  // Ask why about picked lines: the task that last changed them is picked, with the others
  // that changed them as more context, and its box takes the cursor.
  const askAbout = (l: Lines) => {
    if (!unit || !sel) return
    const upTo = l.commit ?? shownDelta(unit, sel, l.path)?.touched.at(-1)
    if (upTo === undefined) return
    const owners = [...new Set(blame(unit, l.path, upTo).slice(l.from - 1, l.to).filter(k => k >= 0))].sort((a, b) => b - a)
    // None of the unit's work wrote these lines: ask the last task that touched the file.
    const order = owners.length ? owners : [upTo]
    const tasks = [...new Set(order.map(k => unit.tasks.find(n => n.output.commits?.includes(unit.commits[k].id))).filter((n): n is Task => !!n))]
    if (!tasks.length) return
    setPick({ kind: 'task', id: tasks[0].id })
    setAskReq({ at: Date.now(), task: tasks[0].id, extra: tasks.slice(1).map(n => ({ kind: 'task', label: n.taskId ? `#${n.taskId}` : n.title })) })
  }

  // Keys: Escape clears what is picked; `?` asks about picked lines, or brings the cursor to
  // the ask box; Cmd/Ctrl+B folds the rail.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'b' && !isField(e.target)) {
        e.preventDefault()
        toggleRail()
        return
      }
      if (isField(e.target)) return
      if (e.key === 'Escape') setPick(null)
      if (e.key === '?' && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault()
        if (lines) askAbout(lines)
        else setAskReq(r => ({ ...r, at: Date.now() }))
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  const here = unitId ? { unit: unitId, pick } : null
  // A row in the panel opens its unit and task; the panel stays, the sheet gets out of the way.
  const open = (id: string, next: Pick) => { if (ws) pickUnit(ws, id, next); setSheet(null) }
  // A commit opened from the panel: its unit and task, and the code block brings it into view.
  const [commit, setCommit] = useState<CommitFocus | null>(null)
  const onCommit = (u: string, task: string, id: string) => { open(u, { kind: 'task', id: task }); setCommit({ unit: u, id, at: Date.now() }) }
  const panel = { ws: ws!, here, onOpen: open, onCommit, commit }

  const onAsk = (node: Task, question: string, context: AskContext[]) => {
    if (!ws || !unit) return
    const at = unit.id, id = node.id
    const turn: Turn = { question, context, answer: null, step: 'starting', at: new Date().toISOString() }
    const put = (next: Turn) => setAsked(all => ({ ...all, [at]: { ...all[at], [id]: (all[at]?.[id] ?? []).map(x => (x === turn || x.at === turn.at ? next : x)) } }))
    setAsked(all => ({ ...all, [at]: { ...all[at], [id]: [...(all[at]?.[id] ?? []), turn] } }))
    // The agent that did the work is asked only while it is idle; a gone session cannot be.
    const who = askeeOf(unit, node)
    const session = who ? ws.sessions.find(s => s.agent === who) : undefined
    const busy = agents(ws).some(a => a.agent === who && a.state === 'working')
    const owner = who ? { agent: who, state: !session || session.ended ? 'ended' as const : busy ? 'busy' as const : 'idle' as const } : null
    let now = turn
    ask(question, context, owner, step => { now = { ...now, step }; put(now) })
      .then(r => put({ ...now, ...r, step: undefined }))
  }
  const talk: Talk | null = ws && unit
    ? {
        sessions: ws.sessions, file, lines, asked: asked[unit.id] ?? {}, onAsk,
        onFile: p => { setFile(p); document.querySelector('.code-grid')?.scrollIntoView({ block: 'start' }) },
        focusAt: askReq.at,
        extra: askReq.task === pick?.id ? askReq.extra : [],
      }
    : null

  // ---- what the user puts in, and where it is on its way ----
  const [delivery, setDelivery] = useState<Record<string, Delivery>>({})
  const [message, setMessage] = useState<Record<string, string>>({})
  const cancels = useRef<Record<string, () => void>>({})
  // Who waits for the answer: the agent of the work the gate or review follows.
  const ownerOf = (u: Unit, n: Task) => u.tasks.find(x => x.id === n.inputs.find(i => i.primary)?.from)?.agent ?? null
  const track = (key: string) => (d: Delivery) => setDelivery(all => ({ ...all, [key]: d }))
  const sent = (key: string, out: { message: string; cancel: () => void }) => {
    cancels.current[key] = out.cancel
    setMessage(all => ({ ...all, [key]: out.message }))
  }
  const drop = <T,>(all: Record<string, T>, key: string) => { const { [key]: _gone, ...rest } = all; return rest }
  const act: Act = {
    submitGate: (u, g, r) => {
      const key = gateKey(u, g)
      setEdits(e => ({ ...e, gates: { ...e.gates, [key]: r } }))
      sent(key, submitGate(u, g, r, ownerOf(u, g) ?? MANAGER, track(key)))
    },
    decide: (u, n, f, d, note) => {
      const key = findingKey(u, n, f)
      setEdits(e => ({ ...e, decisions: { ...e.decisions, [key]: { decision: d, note } } }))
      sent(key, decideFinding(u, n, f, d, note, track(key)))
    },
    undo: key => {
      cancels.current[key]?.()
      delete cancels.current[key]
      setEdits(e => {
        const g = e.gates[key]
        if (g) [...g.attachments, ...g.findings.flatMap(f => f.attachments)].forEach(detach)
        return { ...e, gates: drop(e.gates, key), decisions: drop(e.decisions, key) }
      })
      setDelivery(all => drop(all, key))
      setMessage(all => drop(all, key))
    },
    choreDone: id => { setEdits(e => ({ ...e, chores: [...e.chores, id] })); choreDone(id) },
    delivery,
    message,
  }

  return (
    <ActContext.Provider value={act}>
    <Topics.Provider value={topics}>
    <div className="grid min-h-screen grid-cols-[auto_minmax(0,1fr)_auto] max-[860px]:grid-cols-[minmax(0,1fr)]">
      <Rail ws={ws} unitId={unitId} onUnit={id => ws && pickUnit(ws, id)} q={q} onQ={setQ} open={railOpen} onToggle={toggleRail} />
      <main className="flex min-w-0 flex-col gap-5 px-6 pt-5 pb-10 max-[860px]:p-4">
        {down && <p role="status" className="text-sm text-ink-2">{t('service.unreachable')}</p>}
        {ws && !unitsOf(ws).length && <Start />}
        {ws && unit && sel && talk && (
          <>
            <header className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <h1 className="flex flex-wrap items-baseline gap-x-3 text-xl font-[650] tracking-[-0.012em] wrap-anywhere">
                  {unit.title}
                  {unit.branch && <span className="font-mono text-xs font-normal tracking-normal text-ink-2">{unit.branch}</span>}
                </h1>
                {unit.purpose && <p className="mt-0.5 max-w-[75ch] text-sm text-ink-2">{unit.purpose}</p>}
              </div>
              <div className="flex flex-none gap-1 min-[861px]:hidden"><SideButtons active={sheet} onPick={s => setSheet(now => (now === s ? null : s))} /></div>
            </header>
            <Activity unit={unit} pick={pick} onPick={onPick} talk={talk} fitKey={railOpen} />
            <CodeLayer
              root={repoOf(ws, unit).modules}
              unit={unit}
              sel={sel}
              onlyChanged={onlyChanged}
              onOnlyChanged={setOnlyChanged}
              onPick={onPick}
              file={file}
              onFile={setFile}
              lines={lines}
              onLines={setLines}
              onAsk={askAbout}
              commit={commit?.unit === unit.id ? commit : null}
            />
          </>
        )}
      </main>
      {ws && (
        <>
          <SidePanel {...panel} section={side} onSection={s => showSide(side === s ? null : s)} onClose={() => showSide(null)} />
          <SideSheet {...panel} section={sheet} onSection={s => setSheet(now => (now === s ? null : s))} onClose={() => setSheet(null)} />
        </>
      )}
      <Notes />
    </div>
    </Topics.Provider>
    </ActContext.Provider>
  )
}
