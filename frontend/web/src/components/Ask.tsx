// The side panel's Ask section: two kinds of conversation, one box under each.
// Why: the picked task's conversations (with none picked, the unit's own); what a question is
// about is the current selection, shown as chips above the box. A temporary agent answers
// from the records and the code, and asks the agent that did the work only when it is idle,
// so an agent at work is never interrupted.
// Manager: the user's messages to a workspace's manager, typed into its terminal, and what
// it replied (`workriver reply`).
import { useEffect, useLayoutEffect, useRef, useState, type DragEvent, type ReactNode, type RefObject } from 'react'
import { Button } from '@/components/ui/button'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { askeeOf, baseName, finderLabel, findingsOf, ms, type Lines } from '@/data'
import { contentLang, useT } from '@/i18n'
import { attach, detach, focusAgent, MAX_ATTACHMENT } from '@/runtime'
import type { Attachment } from '@/attachments/types'
import type { Task } from '@/work/types'
import type { Intent, Unit } from '@/planning/types'
import type { Session } from '@/agents/types'
import type { AskContext, ManagerMessage, Thread, Turn } from '@/conversations/types'
import { Choice } from './Choice'
import { ChipName, Icon, Mark, Prose, Tip, Who } from './bits'
import { Thumbs } from './Inputs'

/** What a conclusion request says to the agent; content, not UI text. */
const CONCLUDE = 'Draft a conclusion of this discussion.'

export interface Talk {
  sessions: Session[]
  /** The file picked in the code layer, and lines picked in it. */
  file: string | null
  lines: Lines | null
  /** Questions asked in this sitting, by node id ('' for the unit itself), after the recorded ones. */
  asked: Record<string, Turn[]>
  /** `node`: the task asked about, or null for the unit; `thread`: the conversation it continues, or null for a new one;
   *  `files`: what the user attached in the box, still held by the page. */
  onAsk: (node: Task | null, question: string, context: AskContext[], thread: string | null, conclude?: boolean, files?: Attachment[]) => void
  /** The intent on screen: its page, or the open unit's. */
  intent?: string
  /** Set to now to show the conversation with the manager. */
  managerAt?: number
  /** With `managerAt`: the workspace whose conversation to show; none: the one on screen. */
  to?: string
  /** The user has seen a workspace's conversation, up to the manager's newest words shown. */
  onRead: (workspace: string, at: string) => void
  /** Ends a conversation (with the service). */
  onEnd: (thread: string) => void
  /** Questions go to the service and real agents (not the sample). */
  live: boolean
  onFile: (path: string) => void
  /** Changes when the box should take the cursor (`?`, or "Ask why" on picked lines). */
  focusAt: number
  /** Opens the side panel on Ask, the box taking the cursor (the node detail's "Ask why"). */
  onOpen: () => void
  /** More context to go with the next question: other tasks that changed the picked lines. */
  extra: AskContext[]
  /** For the manager conversation: the intents (whose workspace runs the picked unit), the messages so far. */
  intents: Intent[]
  messages: ManagerMessage[]
  onTell: (to: { intent?: string; workspace: string }, text: string, manager: string) => void
}

export const isField = (el: EventTarget | null) =>
  el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable)

/** An answer: short Markdown, its file paths links that pick the file. */
const Answer = ({ text, onFile }: { text: string; onFile: (path: string) => void }) => <Prose text={text} fold={false} onFile={onFile} />

/** Whether pasted html holds text of its own (tags, styles and scripts left out). Parsed
 *  as an inert document: nothing in it runs or loads. */
function wordsIn(html: string) {
  if (!html) return false
  const doc = new DOMParser().parseFromString(html, 'text/html')
  doc.querySelectorAll('style, script').forEach(el => el.remove())
  return !!doc.body.textContent?.trim()
}

/** A drag that carries files (not text dragged into the box). */
const hasFiles = (e: DragEvent) => e.dataTransfer.types.includes('Files')

/** What a question is about, as chips; `files` are those attached in the box, each with a × that takes it out. */
function Chips({ chips, onDrop, files = [], onRemove }: { chips: AskContext[]; onDrop?: (c: AskContext) => void; files?: Attachment[]; onRemove?: (a: Attachment) => void }) {
  const { t } = useT()
  return (
    <span className="flex flex-wrap gap-1">
      {chips.map(c => (
        <span key={c.kind + c.label} className="ctx">
          <ChipName name={c.label} />
          {onDrop && c.kind !== 'task' && c.kind !== 'unit' && (
            <button type="button" aria-label={t('ask.remove', { label: c.label })} onClick={() => onDrop(c)}><Icon name="no" className="size-3" /></button>
          )}
        </span>
      ))}
      {files.map(a => (
        <span key={a.id} className="ctx">
          <Icon name="attach" className="size-3" />
          <ChipName name={a.name} />
          {onRemove && <button type="button" aria-label={t('attach.remove', { name: a.name })} onClick={() => onRemove(a)}><Icon name="no" className="size-3" /></button>}
        </span>
      ))}
    </span>
  )
}

/** Recorded conversations, with questions asked in this sitting that the data does not have yet going with the last. */
function withAsked(list: Thread[], extra: Turn[]): Thread[] {
  if (!extra.length) return list
  const last = list.at(-1)
  return last && !last.ended ? [...list.slice(0, -1), { ...last, turns: [...last.turns, ...extra] }] : [...list, { id: '', ended: false, live: false, turns: extra }]
}

/** A task's conversations: from the service, or the sample's single one. */
export function threadsOf(n: Task, talk: Talk): Thread[] {
  return withAsked(n.threads ?? (n.thread?.length ? [{ id: '', ended: false, live: false, turns: n.thread }] : []), talk.asked[n.id] ?? [])
}

export const turnsOf = (n: Task, talk: Talk) => threadsOf(n, talk).flatMap(t => t.turns)

/** Who said it and when, above what was said. */
function Head({ who, at }: { who: ReactNode; at?: string }) {
  const { waited, when } = useT()
  const t0 = at ? ms(at) : NaN
  return (
    <p className="msg-head">
      {who}
      {Number.isFinite(t0) && <Tip text={when(t0)}><time dateTime={at}>{waited(t0)}</time></Tip>}
    </p>
  )
}

/** The user's (or an asking agent's) side: right, on its own ground. */
function Mine({ who, at, children }: { who: ReactNode; at?: string; children: ReactNode }) {
  return (
    <li className="msg" data-side="mine">
      <Head who={who} at={at} />
      <div className="bubble">{children}</div>
    </li>
  )
}

/** The other side: left, an agent's words. */
function Theirs({ who, at, children }: { who?: ReactNode; at?: string; children: ReactNode }) {
  return (
    <li className="msg" data-side="theirs">
      {who && <Head who={who} at={at} />}
      <div className="bubble">{children}</div>
    </li>
  )
}

/** A name with its agent mark. */
const Named = ({ name, manager }: { name: string; manager?: boolean }) => {
  const { t } = useT()
  return <span className="flex min-w-0 items-center gap-1"><Who name={name} /><span className="truncate">{name}</span>{manager && <Mark name="manager" label={t('manager.mark')} />}</span>
}

/** One exchange: the question, then the answer (or how far it has got). */
function TurnView({ turn, agent, onFile }: { turn: Turn; agent: string | null; onFile: (path: string) => void }) {
  const { t } = useT()
  const answered = turn.answer !== null && !turn.conclude
  const by = turn.answerBy === 'user' ? t('ask.you') : turn.answerBy ?? t('ask.answerer')
  // Stored files it points at open from the turn; what is left of the context stays chips.
  const stored = turn.attachments ?? []
  const chips = turn.context.filter(c => !(c.kind === 'attachment' && stored.some(a => a.id === c.id)))
  return (
    <>
      {/* Asked by another agent (through workriver): its mark and name, where the user is "you". */}
      <Mine who={turn.by ? <Named name={turn.by} /> : t('ask.you')} at={turn.at}>
        <p className={turn.conclude ? 'text-ink-2' : 'whitespace-pre-line wrap-anywhere'} lang={turn.conclude ? undefined : contentLang(turn.question)}>{turn.conclude ? t('ask.concludeAsked') : turn.question}</p>
        {chips.length > 0 && <div className="mt-1.5"><Chips chips={chips} /></div>}
        <Thumbs list={stored} />
      </Mine>
      {turn.failed ? (
        <Theirs>
          <Tip text={turn.failed.reason}>
            <p className="text-del">{t('ask.cannot', { reason: t(`deliver.reason.${turn.failed.code}`) })}</p>
          </Tip>
        </Theirs>
      ) : turn.answer === null && turn.step === 'blocked' && turn.blocked ? (
        // Its agent waits for a person in its terminal (a first-run or permission prompt): the
        // question goes in by itself once that is answered there.
        <Theirs>
          <div className="flex flex-col items-start gap-1.5">
            <p className="text-you-ink"><span className="pending" aria-hidden />{t('ask.step.blocked', { agent: turn.blocked.agent })}</p>
            <Button variant="outline" size="xs" className="rounded-sm" onClick={() => focusAgent(turn.blocked!.session)}><Icon name="terminal" className="size-3.5" />{t('ask.toTerminal')}</Button>
          </div>
        </Theirs>
      ) : turn.answer === null ? (
        <Theirs>
          <p className="text-ink-2"><span className="pending" aria-hidden />{t(`ask.step.${turn.step ?? 'starting'}`, { agent: agent ?? '' })}</p>
        </Theirs>
      ) : turn.conclude ? (
        // The conclusion stands where it was asked for: questions after it come below it.
        <li className="rounded-md bg-sunken p-2">
          <h4 className="text-xs font-semibold text-ink-2">{t('ask.conclusion')}</h4>
          <Answer text={turn.answer!} onFile={onFile} />
        </li>
      ) : answered ? (
        <Theirs who={by} at={turn.answeredAt}>
          <Answer text={turn.answer!} onFile={onFile} />
          {turn.consulted && (
            <p className="mt-1 flex items-center gap-1.5 text-xs text-ink-2">
              {turn.consulted.outcome === 'asked' && <Who name={turn.consulted.agent} />}
              {t(`ask.${turn.consulted.outcome}`, { agent: turn.consulted.agent })}
            </p>
          )}
        </Theirs>
      ) : null}
    </>
  )
}

/** How far a conversation has got, from its last exchange. */
function stateOf(th: Thread): 'ended' | 'failed' | 'waiting' | 'answered' {
  const last = th.turns.at(-1)
  return th.ended ? 'ended' : last?.failed ? 'failed' : last && last.answer === null ? 'waiting' : 'answered'
}

/** A conversation folded to one line: its first question, who asked, how many exchanges,
 *  how far it has got, when last. */
function ThreadRow({ th, open, body, onToggle }: { th: Thread; open: boolean; body: string; onToggle: () => void }) {
  const { t, num, waited, when } = useT()
  const first = th.turns[0]
  const last = th.turns.at(-1)
  const state = stateOf(th)
  const at = last ? ms(last.at) : NaN
  return (
    <button type="button" className="thread-row" aria-expanded={open} aria-controls={open ? body : undefined} onClick={onToggle}>
      <Icon name="chev" className="size-3.5 flex-none transition-transform" />
      {first?.by ? <Who name={first.by} /> : null}
      <span className="line-clamp-1 min-w-0 flex-1 wrap-anywhere" lang={contentLang(first?.question)}>{first?.question}</span>
      <span className="flex-none text-ink-2">{t('ask.exchanges', { count: th.turns.length, n: num(th.turns.length) })}</span>
      <span className="thread-state flex-none" data-state={state}>{t(`ask.state.${state}`)}</span>
      {Number.isFinite(at) && <Tip text={when(at)}><span className="thread-age flex-none text-right text-ink-2">{waited(at)}</span></Tip>}
    </button>
  )
}

/**
 * A chat's scroll area: it takes the room it is given and starts at the latest message; it
 * follows new ones only while the reader is at the bottom. A log, so new messages are read out.
 */
function Log({ id, size, label, children }: { id?: string; size: string; label: string; children: ReactNode }) {
  const box = useRef<HTMLDivElement>(null)
  const atBottom = useRef(true)
  useLayoutEffect(() => {
    const el = box.current
    if (el && atBottom.current) el.scrollTop = el.scrollHeight
  }, [size])
  return (
    <div
      ref={box} id={id} role="log" aria-label={label} tabIndex={0} className="chat-scroll"
      onScroll={e => { const el = e.currentTarget; atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24 }}
    >
      {children}
    </div>
  )
}

/** An open conversation: the exchanges as a chat, then what can be done with it. */
function ThreadBody({ id, th, agent, talk, onConclude }: { id: string; th: Thread; agent: string | null; talk: Talk; onConclude: () => void }) {
  const { t } = useT()
  const size = th.turns.length + ':' + th.turns.map(x => `${x.answer?.length ?? 0}${x.step ?? ''}${x.failed ? 'f' : ''}`).join(',') + ':' + (th.conclusion?.length ?? 0)
  return (
    <>
      <Log id={id} size={size} label={t('ask.log')}>
        <ol className="chat">
          {th.turns.map((turn, j) => <TurnView key={turn.id ?? j} turn={turn} agent={agent} onFile={talk.onFile} />)}
        </ol>
        {/* A conclusion with no request in the thread (the sample's): after it. */}
        {th.conclusion && !th.turns.some(x => x.conclude && x.answer !== null) && (
          <div className="mt-3 rounded-md bg-sunken p-2">
            <h4 className="text-xs font-semibold text-ink-2">{t('ask.conclusion')}</h4>
            <Answer text={th.conclusion} onFile={talk.onFile} />
          </div>
        )}
      </Log>
      {/* With the service: ask for a conclusion, or end the conversation (its agent closes). */}
      {talk.live && th.id && (
        th.ended
          ? <p className="mt-2 flex-none text-xs text-ink-2">{t('ask.threadEnded')}</p>
          : (
            <div className="mt-2 flex flex-none flex-wrap gap-1.5">
              <Button variant="outline" size="xs" className="rounded-sm" onClick={onConclude}>{t('ask.conclude')}</Button>
              <Tip text={t('ask.endHint')}><Button variant="ghost" size="xs" className="rounded-sm" onClick={() => talk.onEnd(th.id)}>{t('ask.end')}</Button></Tip>
            </div>
          )
      )}
    </>
  )
}

/** Conversations shown before "earlier" is opened. */
const LATEST = 3

/** The last request for the cursor that a box took, so a box shown later does not take it again. */
let tookFocus = 0

/** The box fixed at the bottom: chips and actions above it, a line under it. Enter sends, Shift+Enter breaks the line.
 *  With `onFiles` it takes files too: pasted into it (a screenshot), or picked with its button. */
function Box({ label, placeholder, focusAt, above, below, disabled, onSend, onFiles }: {
  label: string; placeholder: string; focusAt: number; above?: ReactNode; below: ReactNode; disabled?: boolean; onSend: (text: string) => void; onFiles?: (files: File[]) => void
}) {
  const { t } = useT()
  const [text, setText] = useState('')
  const box = useRef<HTMLTextAreaElement>(null)
  const picker = useRef<HTMLInputElement>(null)
  // Asked for from outside ("Ask why", `?`): the cursor comes here, once per request, and only
  // into a box on screen (the wide panel stays mounted, hidden, on a narrow screen). A frame
  // later, so a sheet opening around it first notes where focus was, and gives it back on close.
  useEffect(() => {
    if (!focusAt || focusAt === tookFocus) return
    const k = requestAnimationFrame(() => {
      const el = box.current
      if (!el || !el.getClientRects().length) return
      tookFocus = focusAt
      el.focus()
    })
    return () => cancelAnimationFrame(k)
  }, [focusAt])
  // The box grows with what is typed.
  useEffect(() => {
    const el = box.current
    if (el) { el.style.height = 'auto'; el.style.height = `${el.scrollHeight}px` }
  }, [text])
  const send = () => {
    const q = text.trim()
    if (!q || disabled) return
    onSend(q)
    setText('')
  }
  return (
    <div className="flex-none border-t border-line px-4 pt-3 pb-4 text-sm">
      {above}
      <div className="askbox">
        <textarea
          ref={box}
          rows={1}
          value={text}
          aria-label={label}
          placeholder={placeholder}
          onChange={e => setText(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); send() }
          }}
          onPaste={e => {
            // A text box first. Rich text (a spreadsheet's cells, a document's paragraph) comes
            // with a picture of itself: the text goes in, the picture is not taken. Html with
            // no text of its own (a browser's "Copy image": only an <img>) is not that.
            // Otherwise the files are taken, and text of its own beside them goes in too (not
            // when it only names them, as a file manager's copy does, or is the picture's
            // address in that <img>).
            const got = [...e.clipboardData.files]
            if (!onFiles || !got.length) return
            const html = e.clipboardData.getData('text/html')
            if (wordsIn(html)) return
            const text = e.clipboardData.getData('text/plain').trim()
            if (!text || got.some(f => f.name && text.includes(f.name)) || html.includes(text)) e.preventDefault()
            onFiles(got)
          }}
        />
        {onFiles && (
          <>
            <Tip text={t('ask.attach')}>
              <Button type="button" variant="ghost" size="icon-xs" aria-label={t('ask.attach')} onClick={() => picker.current?.click()}><Icon name="attach" className="size-3.5" /></Button>
            </Tip>
            <input ref={picker} type="file" multiple hidden onChange={e => { onFiles([...(e.target.files ?? [])]); e.target.value = '' }} />
          </>
        )}
        <Button size="icon-xs" aria-label={t('ask.send')} disabled={!text.trim() || disabled} onClick={send}><Icon name="send" className="size-3.5" /></Button>
      </div>
      <p className="mt-1.5 text-xs text-ink-2">{below}</p>
    </div>
  )
}

type Mode = 'why' | 'manager'
/** The last request to show the manager that was followed. */
let managerSeen = 0
const MODE = 'workriver.ask.mode'

/** The side panel's Ask section: why (about the picked task, or the unit) or the manager. */
export function Ask({ unit, n, finding, talk }: { unit: Unit | null; n: Task | null; finding?: string; talk: Talk | null }) {
  const { t } = useT()
  const [mode, setMode] = useState<Mode>(() => { try { return localStorage.getItem(MODE) === 'manager' ? 'manager' : 'why' } catch { return 'why' } })
  const pickMode = (m: Mode) => { setMode(m); try { localStorage.setItem(MODE, m) } catch { /* not remembered */ } }
  // Asked for from outside ("Ask why", `?`, picked lines): that is a question.
  const focusAt = talk?.focusAt ?? 0
  const seen = useRef(focusAt)
  useEffect(() => { if (focusAt !== seen.current) { seen.current = focusAt; setMode('why') } }, [focusAt])
  // Asked for from an intent's page: its manager.
  // (Kept outside the component: the section may mount with the request already made.)
  const managerAt = talk?.managerAt ?? 0
  useEffect(() => { if (managerAt > managerSeen) { managerSeen = managerAt; pickMode('manager') } }, [managerAt])
  // Why about a task, or with none picked about the unit itself (not the intent's own work).
  const about = unit && talk && (n || !unit.intentLevel)
  // Where a question can be asked, its conversations take the files dropped on the panel.
  const takeFiles = useRef<((files: File[]) => void) | null>(null)
  return (
    // A file dropped anywhere on the panel is attached to the question being written, if one
    // can be. (The browser never opens a dropped file, here or elsewhere: App's window listener.)
    <section
      aria-labelledby="side-ask" className="flex min-h-0 flex-1 flex-col" data-takes-files={about && !(mode === 'manager' && talk) ? '' : undefined}
      onDrop={e => { if (hasFiles(e)) { e.preventDefault(); takeFiles.current?.([...e.dataTransfer.files]) } }}
    >
      <div className="mb-2 flex flex-none items-center gap-2 px-4.5">
        <h2 id="side-ask" className="text-sm font-[650]">{t('ask.why')}</h2>
        <ToggleGroup
          type="single" variant="outline" size="sm" value={mode} aria-label={t('ask.mode.label')} className="ml-auto"
          onValueChange={v => { if (v) pickMode(v as Mode) }}
        >
          <ToggleGroupItem value="why" className="h-6 px-2.5 text-xs data-[state=on]:bg-sunken data-[state=on]:font-semibold">{t('ask.mode.why')}</ToggleGroupItem>
          <ToggleGroupItem value="manager" className="h-6 px-2.5 text-xs data-[state=on]:bg-sunken data-[state=on]:font-semibold"><Icon name="manager" className="size-3" />{t('ask.mode.manager')}</ToggleGroupItem>
        </ToggleGroup>
      </div>
      {mode === 'manager' && talk
        ? <ManagerTalk unit={unit} talk={talk} />
        : about
          ? <Conversations key={unit.id} unit={unit} n={n} finding={finding} talk={talk} takeFiles={takeFiles} />
          : <p className="px-4.5 text-sm text-ink-2">{t('ask.empty')}</p>}
    </section>
  )
}

function Conversations({ unit, n, finding, talk, takeFiles }: { unit: Unit; n: Task | null; finding?: string; talk: Talk; takeFiles: RefObject<((files: File[]) => void) | null> }) {
  const { t } = useT()
  const threads = n ? threadsOf(n, talk) : withAsked(unit.threads ?? [], talk.asked[''] ?? [])
  // The box continues the last open conversation, unless the user starts a new one.
  const [fresh, setFresh] = useState(false)
  const current = fresh ? undefined : threads.filter(x => !x.ended).at(-1)
  // Whom the question goes to: the task's agent; on a gate (the user's own task), the agent
  // that asked for it, or else the manager bot. It is asked only while its session is there.
  // About the unit: nobody but the temporary agent.
  const askee = n && askeeOf(unit, n)
  const live = !!askee && talk.sessions.some(x => x.agent === askee && !x.ended)
  const who = !n ? unit.title : n.kind === 'gate' ? n.title : n.taskId ? `#${n.taskId}` : finderLabel(t, n)
  const placeholder = !n ? t('ask.placeholderUnit', { unit: unit.title })
    : n.kind === 'gate' ? (askee ? t('ask.placeholderGate', { agent: askee }) : t('ask.placeholderManager'))
    : t('ask.placeholder', { who })

  // The selection, as what the question is about. A chip left out stays out until it changes.
  const f = n ? findingsOf(n).find(x => x.id === finding) : undefined
  // Picked lines say more than their file, so they take the file's place.
  const lines = talk.lines
  const chips: AskContext[] = [
    n ? { kind: 'task', label: who } : { kind: 'unit', label: who },
    ...talk.extra,
    ...(f ? [{ kind: 'finding' as const, label: f.id }] : []),
    ...(lines
      ? [{ kind: 'lines' as const, label: `${baseName(lines.path)} L${lines.from}${lines.to > lines.from ? `–${lines.to}` : ''}`, path: lines.path, from: lines.from, to: lines.to }]
      : talk.file ? [{ kind: 'file' as const, label: baseName(talk.file), path: talk.file }] : []),
  ]
  const chipKey = chips.map(c => c.label).join('|')
  const [dropped, setDropped] = useState<string[]>([])
  useEffect(() => setDropped([]), [chipKey])
  const context = chips.filter(c => !dropped.includes(c.label))

  // Files dropped on the panel, pasted into the box or picked: the page holds them, shown as
  // chips, until the question is sent (then they are stored); one taken out, or left when
  // the panel goes away, is let go and never stored. Too large for the store: refused at once.
  const [files, setFiles] = useState<Attachment[]>([])
  const [refused, setRefused] = useState<string[]>([])
  const held = useRef(files)
  held.current = files
  useEffect(() => () => held.current.forEach(detach), [])
  const addFiles = (list: File[]) => {
    setRefused(list.filter(f => f.size > MAX_ATTACHMENT).map(f => f.name))
    setFiles(fs => [...fs, ...list.filter(f => f.size <= MAX_ATTACHMENT).map(attach)])
  }
  const remove = (a: Attachment) => { detach(a); setFiles(fs => fs.filter(x => x !== a)); setRefused([]) }
  // A refusal is about the question it was made on: gone with another task picked.
  useEffect(() => setRefused([]), [n?.id])
  useEffect(() => { takeFiles.current = addFiles; return () => { takeFiles.current = null } })

  // One conversation open at a time; the one just asked in stays open. At first, one that
  // still waits for its answer, else the latest.
  const keyOf = (th: Thread, k: number) => th.id || `#${k}`
  const waiting = threads.findLastIndex(th => stateOf(th) === 'waiting')
  const first = waiting >= 0 ? waiting : threads.length - 1
  const firstKey = first >= 0 ? keyOf(threads[first], first) : null
  const [open, setOpen] = useState<string | null>(firstKey)
  const [openLast, setOpenLast] = useState(false)
  const [all, setAll] = useState(false)
  // Another piece of work: its waiting (or latest) conversation open, the rest folded.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { setOpen(firstKey); setOpenLast(false); setAll(false); setFresh(false) }, [n?.id])
  const openKey = openLast && threads.length ? keyOf(threads[threads.length - 1], threads.length - 1) : open
  const send = (q: string) => {
    talk.onAsk(n, q, context, current?.id || null, false, files)
    setFiles([])
    setRefused([])
    setFresh(false)
    // The conversation asked in: the current one, or the new one it starts (the last).
    if (current) { setOpen(keyOf(current, threads.indexOf(current))); setOpenLast(false) } else setOpenLast(true)
  }
  const earlier = all ? 0 : Math.max(0, threads.length - LATEST)

  return (
    <>
      {/* The thread list stays compact; the open conversation takes the rest of the height.
          Relative, as the chat is: see `.chat-scroll`. */}
      <div className="relative flex min-h-0 flex-1 flex-col overflow-y-auto px-4 pb-3 text-sm">
        {earlier > 0 && (
          <button type="button" className="mb-1 flex-none self-start text-xs text-ink-2 hover:text-ink" onClick={() => setAll(true)}>{t('ask.earlier', { count: earlier })}</button>
        )}
        {!threads.length && !n && <p className="text-ink-2">{t('ask.unitHint')}</p>}
        {threads.map((th, k) => {
          if (k < earlier) return null
          const key = keyOf(th, k)
          const isOpen = openKey === key
          const body = `thread-${unit.id}-${n?.id ?? ''}-${key}`
          return (
            <section key={key} className="thread" data-open={isOpen || undefined}>
              <ThreadRow th={th} open={isOpen} body={body} onToggle={() => { setOpenLast(false); setOpen(isOpen ? null : key) }} />
              {isOpen && <ThreadBody id={body} th={th} agent={n?.agent ?? null} talk={talk} onConclude={() => talk.onAsk(n, CONCLUDE, [chips[0]], th.id, true)} />}
            </section>
          )
        })}
      </div>
      {/* Every node, and every unit, has records to answer from, so the box is never disabled. */}
      <Box
        label={t('ask.label')}
        placeholder={placeholder}
        focusAt={talk.focusAt}
        onSend={send}
        onFiles={addFiles}
        above={
          <>
            <div className="mb-1.5 flex items-start gap-2">
              <span className="min-w-0 flex-1"><Chips chips={context} onDrop={c => { setDropped(d => [...d, c.label]); setRefused([]) }} files={files} onRemove={remove} /></span>
              {current && <Button variant="ghost" size="xs" className="-my-0.5 flex-none rounded-sm" onClick={() => setFresh(true)}><Icon name="plus" className="size-3" />{t('ask.fresh')}</Button>}
            </div>
            {refused.length > 0 && (
              <p className="mb-1.5 flex items-start gap-1 text-xs text-del" role="alert">
                <span className="min-w-0 flex-1 wrap-anywhere">{t('ask.tooLarge', { names: refused.join(', '), max: MAX_ATTACHMENT / 1024 / 1024 })}</span>
                <button type="button" className="flex-none rounded-sm hover:text-ink" aria-label={t('ask.dismiss')} onClick={() => setRefused([])}><Icon name="no" className="size-3.5" /></button>
              </p>
            )}
          </>
        }
        below={!n ? t('ask.howUnit') : live ? t('ask.how', { agent: askee }) : t('ask.howAlone')}
      />
    </>
  )
}

/** Where the user's message to a manager is, under it. */
function Step({ m }: { m: ManagerMessage }) {
  const { t } = useT()
  if (m.step === 'blocked' && m.blocked)
    return (
      <div className="mt-1.5 flex flex-col items-start gap-1.5">
        <p className="text-xs text-you-ink">{t('talk.step.blocked', { agent: m.manager })}</p>
        <Button variant="outline" size="xs" className="rounded-sm" onClick={() => focusAgent(m.blocked!.session)}><Icon name="terminal" className="size-3.5" />{t('ask.toTerminal')}</Button>
      </div>
    )
  if (m.step === 'not-delivered')
    return <Tip text={m.reason ?? ''}><p className="mt-1 text-xs text-del">{t('talk.step.not-delivered', { reason: t(`deliver.reason.${m.reasonCode ?? 'unknown'}`) })}</p></Tip>
  return <p className="mt-1 text-xs text-ink-2">{m.step === 'sending' && <span className="pending" aria-hidden />}{t(`talk.step.${m.step}`, { agent: m.manager })}</p>
}

/** How long a conversation is on screen before its new messages count as read, in ms. */
const READ_AFTER = 1500

/**
 * A conversation with a workspace's manager: the picked unit's intent's workspace, or one
 * picked from those with a manager. What the user writes is typed into the manager's
 * terminal (held while it waits for a person there); it replies with `workriver reply`.
 */
function ManagerTalk({ unit, talk }: { unit: Unit | null; talk: Talk }) {
  const { t } = useT()
  const intent = talk.intents.find(i => i.id === talk.intent) ?? (unit ? talk.intents.find(i => i.units.some(u => u.id === unit.id)) : undefined)
  const own = intent?.state === 'active' ? intent.workspace?.label : undefined
  const running = talk.sessions.filter(s => s.manager && !s.ended && s.group)
  const labels = [...new Set([own, ...running.map(s => s.group!.label), ...talk.messages.map(m => m.workspace)].filter((l): l is string => !!l))]
  const [chosen, setChosen] = useState<string | null>(talk.to ?? null)
  // Asked for again (Needs you, an intent's page): the workspace named, or the one on screen.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => setChosen(talk.to ?? null), [talk.managerAt])
  const label = (chosen && labels.includes(chosen) ? chosen : null) ?? own ?? labels[0] ?? null
  const manager = running.find(s => s.group!.label === label)
  const list = talk.messages.filter(m => m.workspace === label)
  // The manager's newest words not read yet: read once the conversation has been on screen a
  // moment (not in a hidden tab, nor in the panel hidden on a narrow screen).
  const newest = list.flatMap(m => [...(m.byManager && m.unread ? [m.at] : []), ...m.replies.filter(r => r.unread).map(r => r.at)]).sort((a, b) => ms(a) - ms(b)).at(-1)
  const shown = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!label || !newest) return
    let k = 0
    const look = () => {
      clearTimeout(k)
      k = window.setTimeout(() => { if (!document.hidden && shown.current?.getClientRects().length) talk.onRead(label, newest) }, READ_AFTER)
    }
    look()
    document.addEventListener('visibilitychange', look)
    return () => { clearTimeout(k); document.removeEventListener('visibilitychange', look) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [label, newest])
  const size = list.length + ':' + list.map(m => m.step + m.replies.length).join(',')
  const options = labels.map(l => {
    const m = running.find(s => s.group!.label === l)
    return { value: l, label: m ? `${l} · ${m.agent}` : l }
  })
  return (
    <>
      <div ref={shown} className="flex min-h-0 flex-1 flex-col px-4 pb-3 text-sm">
        {labels.length > 1 && <Choice value={label ?? ''} onChange={setChosen} options={options} label={t('talk.workspace')} className="mb-2 flex-none" />}
        {label === null
          ? <p className="text-ink-2">{t('talk.noWorkspace')}</p>
          : !list.length
            ? <p className="text-ink-2">{t('talk.empty', { workspace: label })}</p>
            : (
              <Log size={size} label={t('talk.label')}>
                <ol className="chat">
                  {list.map((m, k) => (
                    <MessageView key={m.id ?? `local-${k}`} m={m} />
                  ))}
                </ol>
              </Log>
            )}
      </div>
      <Box
        label={t('talk.label')}
        placeholder={t('talk.placeholder', { agent: manager?.agent ?? t('manager.state.none') })}
        focusAt={0}
        disabled={!manager || !label}
        onSend={text => label && manager && talk.onTell(label === own && intent ? { intent: intent.id, workspace: label } : { workspace: label }, text, manager.agent)}
        below={manager ? t('talk.how', { agent: manager.agent }) : t('talk.none')}
      />
    </>
  )
}

function MessageView({ m }: { m: ManagerMessage }) {
  const { t } = useT()
  return (
    <>
      {/* One the manager started: its words alone, nothing of the user's above them. */}
      {m.byManager
        ? <Theirs who={<Named name={m.manager} manager />} at={m.at}><Prose text={m.text} fold={false} /></Theirs>
        : (
          <Mine who={t('ask.you')} at={m.at}>
            <p className="whitespace-pre-line wrap-anywhere" lang={contentLang(m.text)}>{m.text}</p>
            <Step m={m} />
          </Mine>
        )}
      {m.replies.map((r, j) => (
        <Theirs key={j} who={<Named name={r.by} manager />} at={r.at}>
          <Prose text={r.text} fold={false} />
        </Theirs>
      ))}
    </>
  )
}
