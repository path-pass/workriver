// The shell and the state of what is on screen: which unit, what is picked inside it, what
// the user has put in. Everything else is derived.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ActContext, findingKey, gateKey, type Act } from '@/act'
import { Activity } from '@/components/Flow'
import { SECTIONS, SideButtons, SidePanel, SideSheet, type CommitFocus, type Section } from '@/components/Side'
import { MachineChip, Notes, Progress, Topics } from '@/components/bits'
import { Start, StartIntent } from '@/components/Start'
import { Ask, isField, type Talk } from '@/components/Ask'
import { CodeLayer } from '@/components/CodeLayer'
import { Rail } from '@/components/Rail'
import { agents, askeeOf, blame, blockedAgents, inbox, loadCode, MOCK, NO_EDITS, repoOf, selectionOf, shownDelta, unitsOf, watchWorkspace, withCode, withEdits, type Edits, type Lines, type Pick, type UnitCode } from '@/data'
import { useT } from '@/i18n'
import { ask, askWhy, choreDone, notify, decideFinding, detach, endThread, MANAGER, mergeIntents, submitGate, type Delivery } from '@/runtime'
import type { AskContext, Intent, MachineAlert, Task, Turn, Unit, Workspace } from '@/types'

const SIDE = 'workriver.side'
const RAIL = 'workriver.rail'
const PROJECT = 'workriver.project'

/**
 * The unit with its files' texts, for the code layer: read when it is shown, and again only
 * when its `code` changes. Until the new texts are there, the unit as they were read for.
 * Null until the first arrive. The sample has them inline.
 */
function useCode(unit: Unit | null): Unit | null {
  const [got, setGot] = useState<{ unit: Unit; code: UnitCode } | null>(null)
  const want = unit && !MOCK && !unit.intentLevel ? `${unit.id}\0${unit.code ?? ''}` : null
  const latest = useRef(unit)
  latest.current = unit
  useEffect(() => {
    const u = latest.current
    if (!want || !u) return
    let gone = false
    loadCode(u).then(code => { if (!gone) setGot({ unit: u, code }) }, e => console.warn(e))
    return () => { gone = true }
  }, [want])
  return useMemo(() => {
    if (!unit || MOCK || unit.intentLevel) return unit
    if (got?.unit.id !== unit.id) return null
    return withCode(got.code.code === (unit.code ?? '') ? unit : got.unit, got.code)
  }, [unit, got])
}

/**
 * The same object as long as its content is the same: a new snapshot that did not change the
 * unit on screen does not make its graph and code be worked out and drawn again (about 0.4 s
 * for a unit with a few megabytes of code).
 */
function useSame<T>(value: T): T {
  // Uncommitted work is dated "now" in every snapshot; that alone is no change.
  const json = JSON.stringify(value, function (this: { live?: boolean }, k, v) { return k === 'date' && this.live ? undefined : v })
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => value, [json])
}

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
  // The rail's width once a drag has ended: the graph refits then, not while dragging.
  const [railWidth, setRailWidth] = useState(0)
  const toggleRail = useCallback(() => setRailOpen(o => { remember(RAIL, o ? 'closed' : 'open'); return !o }), [])
  // The side panel shows one section or is folded; which, is remembered.
  const [side, setSide] = useState<Section | null>(() => { const v = recall(SIDE) as Section; return SECTIONS.includes(v) ? v : null })
  // The project the rail and the backlog show, or all; remembered.
  const [picked, setProject] = useState<string | null>(() => recall(PROJECT) || null)
  const pickProject = (p: string | null) => { setProject(p); remember(PROJECT, p ?? '') }
  // A planned intent whose start steps are shown.
  const [starting, setStarting] = useState<Intent | null>(null)
  const showSide = (next: Section | null) => { setSide(next); remember(SIDE, next ?? 'closed') }
  // Narrow screens: the same content as a sheet over the page; not remembered.
  const [sheet, setSheet] = useState<Section | null>(null)

  // What the user put in during this sitting; the data shown is the data with it on top.
  const [edits, setEdits] = useState<Edits>(NO_EDITS)
  const ws = useMemo(() => data && withEdits(data, edits), [data, edits])
  // A remembered project that is no longer there (another data folder) shows all.
  const project = data?.projects?.some(p => p.id === picked) ? picked : null
  // Agents newly waiting in their terminal: a notification (if wanted), and the count in the
  // tab's title with the gates waiting.
  const seen = useRef<Set<string> | null>(null)
  useEffect(() => {
    if (!data) return
    const now = blockedAgents(data)
    const before = seen.current
    seen.current = new Set(now.map(s => s.session ?? s.agent))
    // The first data only sets what is known; it is not news.
    if (before) for (const s of now) if (!before.has(s.session ?? s.agent)) notify(t('agents.notice', { agent: s.agent }), s.topic, s.session ?? s.agent)
  }, [data, t])
  // A machine newly past an alert threshold: a notification too, once per alert.
  const alerted = useRef<Set<string> | null>(null)
  useEffect(() => {
    if (!data) return
    const key = (a: MachineAlert) => `${a.machine}:${a.metric}:${a.since}`
    const now = data.alerts ?? []
    const before = alerted.current
    alerted.current = new Set(now.map(key))
    if (before)
      for (const a of now.filter(a => !before.has(key(a)))) {
        const value = t('metrics.percent', { value: Math.round(a.value) })
        notify(t('metrics.notice', { machine: a.here ? t('metrics.here') : a.machine, metric: t(`metrics.${a.metric}`), value }), t('metrics.noticeBody', { percent: a.percent, minutes: a.minutes }), `alert:${key(a)}`, false)
      }
  }, [data, t])
  const blocking = useMemo(() => (ws ? inbox(ws).blocking.length : 0), [ws])
  useEffect(() => { document.title = blocking ? `(${blocking}) workriver` : 'workriver' }, [blocking])
  // What each live session is about, for the agent marks' tooltips; a name that repeats
  // across workspaces is left out rather than guessed.
  const topics = useMemo(() => {
    const live = (data?.sessions ?? []).filter(s => !s.ended)
    const once = (name: string) => live.filter(s => s.agent === name).length === 1
    // An agent on another machine says so there too.
    const about = (s: (typeof live)[number]) => [s.topic, s.machine && `@${s.machine}`].filter(Boolean).join(' · ')
    return new Map(live.flatMap(s => (about(s) && once(s.agent) ? [[s.agent, about(s)] as const] : [])))
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

  const unit = useSame(unitsOf(ws).find(i => i.id === unitId) ?? null)
  const sel = useMemo(() => (unit ? selectionOf(unit, pick) : null), [unit, pick])
  // The same unit with its files' texts, for the code and the lines picked in it.
  const coded = useCode(unit)
  const codeSel = useMemo(() => (coded ? selectionOf(coded, pick) : null), [coded, pick])
  const tree = useSame(ws && unit && !unit.intentLevel ? repoOf(ws, unit).modules : null)

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

  // The side panel on Ask (the sheet on a narrow screen), its box taking the cursor when a
  // task is picked; with none, the panel says what to pick and nothing waits to take it later.
  const openAsk = (focus = true) => {
    if (window.matchMedia('(width < 860px)').matches) setSheet('ask')
    else showSide('ask')
    if (focus) setAskReq(r => ({ ...r, at: Date.now() }))
  }

  // Ask why about picked lines: the task that last changed them is picked, with the others
  // that changed them as more context, and its box takes the cursor.
  const askAbout = (l: Lines) => {
    const unit = coded, sel = codeSel
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
    openAsk(false)
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
      // An Escape that closed the sheet (Radix takes it first) leaves the pick alone.
      if (e.key === 'Escape' && !e.defaultPrevented) setPick(null)
      if (e.key === '?' && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault()
        if (lines) askAbout(lines)
        else openAsk(!!pick)
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

  const onAsk = (node: Task, question: string, context: AskContext[], thread: string | null, conclude = false) => {
    if (!ws || !unit) return
    const at = unit.id, id = node.id
    const turn: Turn = { question, context, answer: null, step: 'starting', at: new Date().toISOString(), conclude }
    const put = (next: Turn) => setAsked(all => ({ ...all, [at]: { ...all[at], [id]: (all[at]?.[id] ?? []).map(x => (x === turn || x.at === turn.at ? next : x)) } }))
    // With the service, the question and its answer come back with the data; only a refusal
    // to take it stays here.
    if (!MOCK) {
      askWhy(unit, node, question, context, thread, conclude).catch(e => {
        setAsked(all => ({ ...all, [at]: { ...all[at], [id]: [...(all[at]?.[id] ?? []), { ...turn, step: undefined, failed: { code: 'runtime-error', reason: String(e) } }] } }))
      })
      return
    }
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
        onEnd: thread => { endThread(thread).catch(e => console.warn(e)) },
        live: !MOCK,
        // The sheet gets out of the way of the file, as for a row of the panel.
        onFile: p => { setFile(p); setSheet(null); document.querySelector('.code-grid')?.scrollIntoView({ block: 'start' }) },
        focusAt: askReq.at,
        onOpen: () => openAsk(),
        extra: askReq.task === pick?.id ? askReq.extra : [],
      }
    : null

  // The Ask section asks about the picked task, in the unit on screen.
  const asking = unit?.tasks.find(n => n.id === pick?.id) ?? null
  const panel = { ws: ws!, here, onOpen: open, onCommit, commit, project, ask: <Ask unit={unit} n={asking} finding={pick?.finding} talk={talk} /> }

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
    merge: (into, from, title) => mergeIntents(into, from, title).then(() => setEdits(e => ({ ...e, merges: [...e.merges, { into, from, title }] }))),
    delivery,
    message,
  }

  return (
    <ActContext.Provider value={act}>
    <Topics.Provider value={topics}>
    <div className="grid min-h-screen grid-cols-[auto_minmax(0,1fr)_auto] max-[860px]:grid-cols-[minmax(0,1fr)]">
      <Rail ws={ws} unitId={unitId} onUnit={id => ws && pickUnit(ws, id)} q={q} onQ={setQ} open={railOpen} onToggle={toggleRail} project={project} onProject={pickProject} onStart={setStarting} onWidth={setRailWidth} />
      <main className="flex min-w-0 flex-col gap-5 px-6 pt-5 pb-10 max-[860px]:p-4">
        {down && <p role="status" className="text-sm text-ink-2">{t('service.unreachable')}</p>}
        {ws && !unitsOf(ws).length && <Start />}
        {ws && unit && sel && talk && (
          <>
            <header className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <h1 className="flex flex-wrap items-baseline gap-x-3 text-xl font-[650] tracking-[-0.012em] wrap-anywhere">
                  {unit.intentLevel ? `${unit.title} · ${t('rail.intentWork')}` : unit.title}
                  {unit.branch && <span className="max-w-full min-w-0 truncate font-mono text-xs font-normal tracking-normal text-ink-2" title={unit.branch}>{unit.branch}</span>}
                  <MachineChip machine={unit.machine} away={ws.machines?.some(m => m.label === unit.machine && !m.reachable)} since={ws.machines?.find(m => m.label === unit.machine && !m.tunnel)?.downSince} />
                  {!unit.intentLevel && <Progress unit={unit} />}
                </h1>
                {unit.purpose && <p className="mt-0.5 max-w-[75ch] text-sm text-ink-2">{unit.purpose}</p>}
                {/* Rebased onto a new branch, or renamed: the same unit, said quietly. */}
                {(unit.previously?.length ?? 0) > 0 && <p className="mt-0.5 font-mono text-xs text-ink-2">{t('unit.previously', { branches: unit.previously!.join(t('list.sep')) })}</p>}
              </div>
              <div className="flex flex-none gap-1 min-[861px]:hidden"><SideButtons active={sheet} onPick={s => setSheet(now => (now === s ? null : s))} /></div>
            </header>
            <Activity unit={unit} pick={pick} onPick={onPick} talk={talk} fitKey={`${railOpen}:${railWidth}`} />
            {tree && coded && codeSel && <CodeLayer
              root={tree}
              unit={coded}
              sel={codeSel}
              onlyChanged={onlyChanged}
              onOnlyChanged={setOnlyChanged}
              onPick={onPick}
              file={file}
              onFile={setFile}
              lines={lines}
              onLines={setLines}
              onAsk={askAbout}
              commit={commit?.unit === unit.id ? commit : null}
            />}
          </>
        )}
      </main>
      {ws && (
        <>
          <SidePanel {...panel} section={side} onSection={s => showSide(side === s ? null : s)} onClose={() => showSide(null)} />
          <SideSheet {...panel} section={sheet} onSection={s => setSheet(now => (now === s ? null : s))} onClose={() => setSheet(null)} />
        </>
      )}
      <StartIntent intent={starting} onClose={() => setStarting(null)} />
      <Notes />
    </div>
    </Topics.Provider>
    </ActContext.Provider>
  )
}
