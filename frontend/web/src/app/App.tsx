// The shell and the state of what is on screen: which unit, what is picked inside it, what
// the user has put in. Everything else is derived.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ActContext, findingKey, gateKey, type Act } from '@/work/actions'
import { Activity } from '@/work/Activity'
import { SECTIONS, SideButtons, SidePanel, SideSheet, type Section } from './Side'
import type { CommitFocus, Lines } from '@/code/types'
import { Progress, Squashed, Rail } from '@/planning/Rail'
import { Topics } from '@/agents/Mark'
import { MachineChip, alertLimit, alertValue, topLine } from '@/machines/format'
import { CopyButton } from '@/ui/CopyButton'
import { Start, StartIntent } from '@/planning/Start'
import { mockWhere, RunDialog } from '@/commands/Run'
import { RunContext } from '@/commands/context'
import type { RunAsk } from '@/commands/types'
import { Manager } from '@/conversations/Manager'
import type { Talk, AskContext, ManagerMessage } from '@/conversations/types'
import { isField } from '@/lib/isField'
import { CodeLayer } from '@/code/CodeLayer'
import { ViewerContext } from '@/attachments/Viewer'
import { IntentPage } from '@/planning/IntentPage'
import { MOCK, watchWorkspace } from '@/workspace/source'
import { blockedAgents, inbox } from '@/attention/select'
import { selectionOf } from '@/work/select'
import { firstUnit, intentOf, noCode, unitsOf } from '@/planning/select'
import { repoOf } from '@/code/select'
import { blame, shownDelta } from '@/code/files'
import { NO_EDITS, withEdits, type Edits } from '@/workspace/edits'
import type { Pick, Task } from '@/work/types'
import { contentLang, useT } from '@/i18n'
import { linkTo, pathOf, placeOf, resolve } from './link'
import { choreDone, notify, decideFinding, detach, MANAGER, mergeIntents, mockReply, readMessages, submitGate, tellManager, type Delivery, NOTE, type Remote } from '@/api/actions'
import type { Attachment } from '@/attachments/types'
import type { Intent, Unit } from '@/planning/types'
import type { MachineAlert } from '@/machines/types'
import type { Workspace } from '@/workspace/types'
import { useCode } from '@/code/useCode'

const SIDE = 'workriver.side'
const RAIL = 'workriver.rail'
const PROJECT = 'workriver.project'

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
  const { t, lang } = useT()
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
  const [sideWidth, setSideWidth] = useState(0)
  const toggleRail = useCallback(() => setRailOpen(o => { remember(RAIL, o ? 'closed' : 'open'); return !o }), [])
  // The side panel shows one section or is folded; which, is remembered.
  // (A section remembered as 'ask', from when the page had its own Ask, opens as 'talk'.)
  const [side, setSide] = useState<Section | null>(() => { const r = recall(SIDE); const v = (r === 'ask' ? 'talk' : r) as Section; return SECTIONS.includes(v) ? v : null })
  // The project the rail and the backlog show, or all; remembered.
  const [picked, setProject] = useState<string | null>(() => recall(PROJECT) || null)
  // The main area follows the switch: the unit on screen stays if it is the project's, else the
  // project's first unit opens, or its empty state when it has none.
  const pickProject = (p: string | null) => {
    setProject(p)
    remember(PROJECT, p ?? '')
    if (!ws || (unitId && (!p || intentOf(ws, unitId)?.project === p))) return
    const first = firstUnit(ws, p)
    if (first) return pickUnit(ws, first.id)
    setUnitId(null)
    setPick(null)
    if (location.pathname !== '/') history.pushState(null, '', '/')
  }
  // A planned intent whose start steps are shown.
  const [starting, setStarting] = useState<Intent | null>(null)
  const showSide = (next: Section | null) => { setSide(next); remember(SIDE, next ?? 'closed') }
  // Narrow screens: the same content as a sheet over the page; not remembered.
  const [sheet, setSheet] = useState<Section | null>(null)
  // How wide the side panel is now, open or folded to its strip: the attachment viewer stops there.
  useEffect(() => document.documentElement.style.setProperty('--side-now', side ? 'var(--side-w, 340px)' : '2.75rem'), [side])
  // A file dragged over the page is never opened by the browser in the page's place, wherever
  // it is let go; only what takes files (`data-takes-files`: the Manager panel, a gate's form)
  // offers to, and takes them in its own handler.
  useEffect(() => {
    const keep = (e: DragEvent) => {
      if (!e.dataTransfer?.types.includes('Files')) return
      e.preventDefault()
      if (e.type === 'dragover' && !(e.target instanceof Element && e.target.closest('[data-takes-files]'))) e.dataTransfer.dropEffect = 'none'
    }
    window.addEventListener('dragover', keep)
    window.addEventListener('drop', keep)
    return () => { window.removeEventListener('dragover', keep); window.removeEventListener('drop', keep) }
  }, [])

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
  // A machine alert: a notification when it rises, and again only when it becomes critical;
  // never again while it stays (nor when it steps back to a warning).
  const alerted = useRef<Set<string> | null>(null)
  useEffect(() => {
    if (!data) return
    const key = (a: MachineAlert) => `${a.machine}:${a.metric}:${a.since}`
    const keys = (a: MachineAlert) => (a.level === 'crit' ? [key(a), `${key(a)}:crit`] : [key(a)])
    const now = data.alerts ?? []
    const before = alerted.current
    alerted.current = new Set(now.flatMap(keys))
    if (before)
      for (const a of now.filter(a => keys(a).some(k => !before.has(k)))) {
        const body = [`${t(`metrics.level.${a.level}`)}: ${alertLimit(t, a)}`, topLine(t, lang, a)].filter(Boolean).join('\n')
        notify(t('metrics.notice', { machine: a.here ? t('metrics.here') : a.machine, metric: t(`metrics.${a.metric}`), value: alertValue(t, lang, a) }), body, `alert:${key(a)}:${a.level}`, false)
      }
  }, [data, t, lang])
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

  // The file picked in the code layer and lines picked in a diff; a message takes them as chips.
  const [file, setFile] = useState<string | null>(null)
  const [lines, setLines] = useState<Lines | null>(null)
  // Lines belong to the view they were picked in: the single file, or the list.
  useEffect(() => setLines(l => (l && l.commit === undefined && l.path === file ? l : null)), [file])
  // A request for the Manager box to take the cursor, with more chips for one task.
  const [askReq, setAskReq] = useState<{ at: number; task: string | null; extra: AskContext[] }>({ at: 0, task: null, extra: [] })
  // A request for the Manager section to show a workspace's conversation (Needs you, an intent's page).
  const [managerAt, setManagerAt] = useState(0)
  // The workspace whose conversation it shows (from Needs you); none: the one on screen.
  const [talkTo, setTalkTo] = useState<string | undefined>()
  const showTalk = (workspace?: string) => { openAsk(false); setTalkTo(workspace); setManagerAt(Date.now()) }
  useEffect(() => setAskReq(r => (r.extra.length ? { ...r, extra: [] } : r)), [lines])
  // The stored attachment the viewer shows, if any: a message can point at it.
  const [viewing, setViewing] = useState<Attachment | null>(null)

  const unit = useSame(unitsOf(ws).find(i => i.id === unitId) ?? null)
  // An intent's page is open (`unitId` is its id); `unit` is then its own earlier work, if any.
  const page = ws?.intents.find(i => i.id === unitId) ?? null
  const sel = useMemo(() => (unit ? selectionOf(unit, pick) : null), [unit, pick])
  // The same unit with its files' texts, for the code and the lines picked in it.
  const coded = useCode(unit)
  const codeSel = useMemo(() => (coded ? selectionOf(coded, pick) : null), [coded, pick])
  const tree = useSame(ws && unit && !noCode(unit) ? repoOf(ws, unit).modules : null)

  // What the address named and is not there; whether it has been followed; the latest data.
  const [missing, setMissing] = useState<string | null>(null)
  const followed = useRef(false)
  const latest = useRef<Workspace | null>(null)
  // Whatever opens a unit (the rail, the inbox, an agent) sets the pick the same way, so the
  // file tree opens on what is picked wherever the user came from.
  const pickUnit = useCallback((w: Workspace, id: string, next: Pick = null) => {
    if (!unitsOf(w).some(i => i.id === id) && !w.intents.some(i => i.id === id)) return
    followed.current = true
    setMissing(null)
    setUnitId(id)
    setPick(next)
  }, [])

  // The address names what is open (`/u1/3`): it is followed on load and on back/forward,
  // and set when something else is opened or picked. The root opens the first unit.
  const follow = useCallback((w: Workspace) => {
    const p = placeOf(location.pathname, location.search)
    // The root opens the first unit of the project remembered (or of all).
    const kept = recall(PROJECT)
    const first = firstUnit(w, w.projects?.some(x => x.id === kept) ? kept : null)
    const r = p ? resolve(w, p) : first ? { unit: first.id, pick: null } : null
    setMissing(p && !r ? `${p.id}${p.task ? `#${p.task}` : ''}` : null)
    if (!r) { setUnitId(null); return false }
    history.replaceState(null, '', pathOf(r.unit, r.pick))
    // A linked unit outside the project the rail shows: the rail shows all, for now.
    const project = intentOf(w, r.unit)?.project
    setProject(p => (p && project !== p ? null : p))
    setUnitId(r.unit)
    setPick(r.pick)
    return true
  }, [])
  useEffect(() => {
    const back = () => { if (latest.current) followed.current = follow(latest.current) }
    window.addEventListener('popstate', back)
    return () => window.removeEventListener('popstate', back)
  }, [follow])
  useEffect(() => {
    if (!unitId) return
    const path = pathOf(unitId, pick)
    if (path !== location.pathname + location.search) history.pushState(null, '', path)
  }, [unitId, pick])

  // Until the address is followed (it may name what is not there yet), each update tries
  // again; then later updates keep what is open.
  useEffect(() => watchWorkspace(w => {
    setData(w)
    setDown(false)
    latest.current = w
    if (!followed.current) followed.current = follow(w)
  }, () => setDown(true)), [follow])

  // Picking the same thing again clears it.
  const onPick = useCallback((next: Pick) => {
    setPick(now => (next && now && next.id === now.id && next.finding === now.finding ? null : next))
  }, [])

  // The side panel on Manager (the sheet on a narrow screen); asked about what is on screen,
  // its box takes the cursor, on the unit's own workspace.
  const openAsk = (focus = true) => {
    if (window.matchMedia('(width < 860px)').matches) setSheet('talk')
    else showSide('talk')
    if (focus) { setTalkTo(undefined); setAskReq(r => ({ ...r, at: Date.now() })) }
  }

  // Ask the manager about picked lines: the task that last changed them is picked, with the
  // others that changed them as more chips (the manager gets who wrote them), and the box
  // takes the cursor.
  const askAbout = (l: Lines) => {
    const unit = coded, sel = codeSel
    if (!unit || !sel) return
    const upTo = l.commit ?? shownDelta(unit, sel, l.path)?.touched.at(-1)
    const owners = upTo === undefined ? [] : [...new Set(blame(unit, l.path, upTo).slice(l.from - 1, l.to).filter(k => k >= 0))].sort((a, b) => b - a)
    const tasks = [...new Set(owners.map(k => unit.tasks.find(n => n.output.commits?.includes(unit.commits[k].id))).filter((n): n is Task => !!n))]
    // No task wrote these lines: the unit's chip, with the lines.
    setTalkTo(undefined)
    if (!tasks.length) {
      setPick(null)
      setAskReq({ at: Date.now(), task: null, extra: [] })
      openAsk(false)
      return
    }
    setPick({ kind: 'task', id: tasks[0].id })
    setAskReq({ at: Date.now(), task: tasks[0].id, extra: tasks.slice(1).map(n => ({ kind: 'task', label: n.taskId ? `#${n.taskId}` : n.title, task: Number(n.id) })) })
    openAsk(false)
  }

  // Keys: Escape clears what is picked; `?` asks the manager about picked lines, or brings
  // the cursor to the Manager box; Cmd/Ctrl+B folds the rail.
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
        else openAsk()
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

  // Messages to managers: the service's, then any it refused (or, with the sample, all of them).
  const [told, setTold] = useState<ManagerMessage[]>([])
  const onTell: Talk['onTell'] = (to, text, manager, about, files) => {
    // The files go with it as attachments; the sample shows them from the page.
    const context = [...about.context, ...files.map(a => ({ kind: 'attachment' as const, label: a.name, id: a.id }))]
    const m: ManagerMessage = { workspace: to.workspace, intent: to.intent, manager, text, ...about, context, attachments: files, at: new Date().toISOString(), step: MOCK ? 'delivered' : 'sending', replies: [] }
    if (MOCK) {
      setTold(all => [...all, m])
      setTimeout(() => setTold(all => all.map(x => (x === m ? { ...m, replies: [{ by: manager, text: mockReply(text), at: new Date().toISOString() }] } : x))), 1200)
      return
    }
    // Stored (or refused), the page lets its copies go: the service's message shows the stored ones.
    tellManager(to, text, about, files)
      .catch(e => setTold(all => [...all, { ...m, attachments: undefined, step: 'not-delivered', reasonCode: 'runtime-error', reason: String(e) }]))
      .finally(() => files.forEach(detach))
  }
  // Read: the manager's words up to `at` are no longer new, here at once (the sample keeps it
  // only in memory), and in every browser of the user's once the service has it.
  const onRead = (workspace: string, at: string) => {
    setEdits(e => ({ ...e, read: { ...e.read, [workspace]: at } }))
    readMessages(workspace, at)
  }
  // The manager conversation needs no unit on screen (Needs you opens it from anywhere).
  const talk: Talk | null = ws
    ? {
        sessions: ws.sessions, file, lines,
        intent: page?.id ?? (unit ? intentOf(ws, unit.id)?.id : undefined), managerAt, to: talkTo, onRead,
        intents: ws.intents, messages: [...(ws.messages ?? []), ...told], onTell,
        // The sheet gets out of the way of the file, as for a row of the panel.
        onFile: p => { setFile(p); setSheet(null); document.querySelector('.code-grid')?.scrollIntoView({ block: 'start' }) },
        focusAt: askReq.at,
        onOpen: () => openAsk(),
        extra: [
          ...(askReq.task && askReq.task === pick?.id ? askReq.extra : []),
          ...(viewing ? [{ kind: 'attachment' as const, label: viewing.name, id: viewing.id }] : []),
        ],
      }
    : null
  // The viewer hears this once: the latest openAsk is called through a ref.
  const askNow = useRef(openAsk)
  askNow.current = openAsk
  const viewer = useMemo(() => ({ onView: setViewing, onAsk: () => askNow.current() }), [])

  // The Manager section's chips: the picked task, in the unit on screen.
  const asking = unit?.tasks.find(n => n.id === pick?.id) ?? null
  const panel = { ws: ws!, here, onOpen: open, onCommit, commit, project, onTalk: showTalk, talk: <Manager unit={unit} n={asking} finding={pick?.finding} talk={talk} /> }

  // A command the user asked to run: the confirm dialog shows it. A gate or chore of the
  // intent's own work has no work tree: it runs in the intent's project folder.
  const [running, setRunning] = useState<RunAsk | null>(null)
  const run = useCallback((a: RunAsk) => {
    if (!ws) return
    const at0 = a.unit && unitsOf(ws).find(u => u.id === a.unit)
    const own = at0 && noCode(at0)
    const at = own ? { ...a, unit: undefined, project: a.project ?? intentOf(ws, a.unit!)?.project } : a
    setRunning(MOCK ? { ...at, ...mockWhere(ws, at) } : at)
  }, [ws])

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
    <RunContext.Provider value={run}>
    <ViewerContext.Provider value={viewer}>
    <Topics.Provider value={topics}>
    <div className="grid min-h-screen grid-cols-[auto_minmax(0,1fr)_auto] max-[860px]:grid-cols-[minmax(0,1fr)]">
      <Rail ws={ws} unitId={unitId} onUnit={id => ws && pickUnit(ws, id)} onIntent={id => ws && pickUnit(ws, id)} q={q} onQ={setQ} open={railOpen} onToggle={toggleRail} project={project} onProject={pickProject} onStart={setStarting} onWidth={setRailWidth} />
      <main className="flex min-w-0 flex-col gap-5 px-6 pt-5 pb-10 max-[860px]:p-4">
        {down && <p role="status" className="text-sm text-ink-2">{t('service.unreachable')}</p>}
        {missing && <p role="status" className="text-sm text-ink-2">{t('link.notFound', { ref: missing })}</p>}
        {ws && !unitsOf(ws).length && <Start />}
        {ws && !unit && !missing && unitsOf(ws).length > 0 && !firstUnit(ws, project) && <Start project={ws.projects?.find(p => p.id === project)?.title} />}
        {ws && page && (
          <IntentPage
            ws={ws}
            intent={page}
            earlier={unit}
            pick={pick}
            onPick={onPick}
            talk={talk}
            fitKey={`${railOpen}:${railWidth}:${side ? sideWidth : 0}`}
            onUnit={id => pickUnit(ws, id)}
            onStart={setStarting}
            onTalk={() => showTalk()}
            aside={<SideButtons active={sheet} onPick={s => setSheet(now => (now === s ? null : s))} />}
          />
        )}
        {ws && !page && unit && sel && talk && (
          <>
            <header className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <h1 className="flex flex-wrap items-baseline gap-x-3 text-xl font-[650] tracking-[-0.012em] wrap-anywhere">
                  <span><span className="mr-2 font-mono text-base font-normal tracking-normal text-ink-2">{unit.id}</span><span lang={contentLang(unit.title)}>{unit.title}</span></span>
                  <CopyButton text={linkTo(pathOf(unit.id, null))} aria={t('link.copy')} className="self-center text-ink-2" />
                  {unit.branch && <span className="max-w-full min-w-0 truncate font-mono text-xs font-normal tracking-normal text-ink-2" title={unit.branch}>{unit.branch}</span>}
                  {unit.kind === 'spike' && <span className="tag font-normal tracking-normal" title={t('unit.spikeWhy')}>{t('unit.spike')}</span>}
                  <MachineChip machine={unit.machine} away={ws.machines?.some(m => m.label === unit.machine && !m.reachable)} since={ws.machines?.find(m => m.label === unit.machine && !m.tunnel)?.downSince} />
                  <Progress unit={unit} />
                  {unit.squash && unit.progress === 'ready' && <Squashed unit={unit} />}
                </h1>
                {unit.purpose && <p className="mt-0.5 max-w-[75ch] text-sm text-ink-2" lang={contentLang(unit.purpose)}>{unit.purpose}</p>}
                {/* Rebased onto a new branch, or renamed: the same unit, said quietly. */}
                {(unit.previously?.length ?? 0) > 0 && <p className="mt-0.5 font-mono text-xs text-ink-2">{t('unit.previously', { branches: unit.previously!.join(t('list.sep')) })}</p>}
              </div>
              <div className="flex flex-none gap-1 min-[861px]:hidden"><SideButtons active={sheet} onPick={s => setSheet(now => (now === s ? null : s))} /></div>
            </header>
            <Activity unit={unit} pick={pick} onPick={onPick} talk={talk} fitKey={`${railOpen}:${railWidth}:${side ? sideWidth : 0}`} />
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
          <SidePanel {...panel} section={side} onSection={s => showSide(side === s ? null : s)} onClose={() => showSide(null)} onWidth={setSideWidth} />
          <SideSheet {...panel} section={sheet} onSection={s => setSheet(now => (now === s ? null : s))} onClose={() => setSheet(null)} />
        </>
      )}
      <StartIntent ws={ws} intent={starting} onClose={() => setStarting(null)} onStarted={(i, w) => setEdits(e => ({ ...e, starts: { ...e.starts, [i]: w } }))} />
      <RunDialog ask={running} onClose={() => setRunning(null)} />
      <Notes />
    </div>
    </Topics.Provider>
    </ViewerContext.Provider>
    </RunContext.Provider>
    </ActContext.Provider>
  )
}

/** A short note at the bottom of the screen: what the UI cannot do itself yet, or where a
 *  terminal brought to the front on another machine is (read longer). */
export function Notes() {
  const { t } = useT()
  const [text, setText] = useState<string | null>(null)
  useEffect(() => {
    let timer = 0
    const on = (e: Event) => {
      const d = (e as CustomEvent<{ agent?: string; remote?: Remote }>).detail
      const r = d.remote
      setText(r ? t('terminal.remote', { what: r.agent ?? t('terminal.command'), machine: r.machine, workspace: r.workspace ?? '?' }) : t('terminal.note', { agent: d.agent }))
      clearTimeout(timer)
      timer = window.setTimeout(() => setText(null), r ? 8000 : 2500)
    }
    window.addEventListener(NOTE, on)
    return () => { window.removeEventListener(NOTE, on); clearTimeout(timer) }
  }, [t])
  return (
    <div role="status" className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex justify-center px-4">
      {text && <div className="max-w-xl rounded-md bg-ink px-3 py-2 text-xs text-canvas shadow-pick">{text}</div>}
    </div>
  )
}
