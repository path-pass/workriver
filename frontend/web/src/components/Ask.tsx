// Asking why about a piece of work. One box under the picked node's detail; what the
// question is about is the current selection, shown as chips above it. A temporary agent
// answers from the records and the code, and asks the agent that did the work only when it
// is idle, so an agent at work is never interrupted.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { askeeOf, baseName, finderLabel, findingsOf, ms, type Lines } from '@/data'
import { contentLang, useT } from '@/i18n'
import type { AskContext, Session, Thread, Turn, Task, Unit } from '@/types'
import { ChipName, Icon, Tip, Who } from './bits'

/** What a conclusion request says to the agent; content, not UI text. */
const CONCLUDE = 'Draft a conclusion of this discussion.'

export interface Talk {
  sessions: Session[]
  /** The file picked in the code layer, and lines picked in it. */
  file: string | null
  lines: Lines | null
  /** Questions asked in this sitting, by node id, after the recorded ones. */
  asked: Record<string, Turn[]>
  /** `thread`: the conversation it continues, or null for a new one. */
  onAsk: (node: Task, question: string, context: AskContext[], thread: string | null, conclude?: boolean) => void
  /** Ends a conversation (with the service). */
  onEnd: (thread: string) => void
  /** Questions go to the service and real agents (not the sample). */
  live: boolean
  onFile: (path: string) => void
  /** Changes when the box should take the cursor (`?`, or "Ask why" on picked lines). */
  focusAt: number
  /** More context to go with the next question: other tasks that changed the picked lines. */
  extra: AskContext[]
}

export const isField = (el: EventTarget | null) =>
  el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable)

/** File paths in an answer become links that pick the file. */
function Answer({ text, onFile }: { text: string; onFile: (path: string) => void }) {
  const parts: ReactNode[] = []
  let last = 0
  for (const m of text.matchAll(/[\w-]+(?:\/[\w.-]+)+\.\w+/g)) {
    parts.push(text.slice(last, m.index))
    parts.push(<button key={m.index} type="button" className="font-mono text-[12px] text-you-ink underline-offset-3 hover:underline" onClick={() => onFile(m[0])}>{m[0]}</button>)
    last = m.index + m[0].length
  }
  parts.push(text.slice(last))
  // A long path or word breaks where it must rather than run out of the answer.
  return <p className="whitespace-pre-wrap wrap-anywhere" lang={contentLang(text)}>{parts}</p>
}

function Chips({ chips, onDrop }: { chips: AskContext[]; onDrop?: (c: AskContext) => void }) {
  const { t } = useT()
  return (
    <span className="flex flex-wrap gap-1">
      {chips.map(c => (
        <span key={c.kind + c.label} className="ctx">
          <ChipName name={c.label} />
          {onDrop && c.kind !== 'task' && (
            <button type="button" aria-label={t('ask.remove', { label: c.label })} onClick={() => onDrop(c)}><Icon name="no" className="size-3" /></button>
          )}
        </span>
      ))}
    </span>
  )
}

/**
 * A task's conversations: from the service, or the sample's single one; questions asked in
 * this sitting that the data does not have yet go with the last.
 */
export function threadsOf(n: Task, talk: Talk): Thread[] {
  const list: Thread[] = n.threads ?? (n.thread?.length ? [{ id: '', ended: false, live: false, turns: n.thread }] : [])
  const extra = talk.asked[n.id] ?? []
  if (!extra.length) return list
  const last = list.at(-1)
  return last && !last.ended ? [...list.slice(0, -1), { ...last, turns: [...last.turns, ...extra] }] : [...list, { id: '', ended: false, live: false, turns: extra }]
}

export const turnsOf = (n: Task, talk: Talk) => threadsOf(n, talk).flatMap(t => t.turns)

function TurnView({ turn, agent, onFile }: { turn: Turn; agent: string | null; onFile: (path: string) => void }) {
  const { t } = useT()
  return (
    <li className="flex flex-col gap-1">
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        {/* Asked by another agent (through workriver): its mark, where the user's question has none. */}
        {turn.by && <span className="self-center"><Who name={turn.by} /></span>}
        <span className={turn.conclude ? 'text-ink-2' : 'font-semibold'} lang={turn.conclude ? undefined : contentLang(turn.question)}>{turn.conclude ? t('ask.concludeAsked') : turn.question}</span>
        <Chips chips={turn.context} />
      </div>
      {turn.failed ? (
        <Tip text={turn.failed.reason}>
          <p className="text-del">{t('ask.cannot', { reason: t(`deliver.reason.${turn.failed.code}`) })}</p>
        </Tip>
      ) : turn.answer === null ? (
        <p className="text-ink-2" aria-live="polite">
          <span className="pending" aria-hidden />{t(`ask.step.${turn.step ?? 'starting'}`, { agent: agent ?? '' })}
        </p>
      ) : turn.conclude ? null : (
        <>
          <Answer text={turn.answer} onFile={onFile} />
          {turn.consulted && (
            <p className="flex items-center gap-1.5 text-xs text-ink-2">
              {turn.consulted.outcome === 'asked' && <Who name={turn.consulted.agent} />}
              {t(`ask.${turn.consulted.outcome}`, { agent: turn.consulted.agent })}
            </p>
          )}
        </>
      )}
    </li>
  )
}

/** How far a conversation has got, from its last exchange. */
function stateOf(th: Thread): 'ended' | 'failed' | 'waiting' | 'answered' {
  const last = th.turns.at(-1)
  return th.ended ? 'ended' : last?.failed ? 'failed' : last && last.answer === null ? 'waiting' : 'answered'
}

/** A conversation folded to one line: its first question, who asked, how many exchanges,
 *  how far it has got, when last. */
function ThreadRow({ th, open, onToggle }: { th: Thread; open: boolean; onToggle: () => void }) {
  const { t, num, waited, when } = useT()
  const first = th.turns[0]
  const last = th.turns.at(-1)
  const state = stateOf(th)
  const at = last ? ms(last.at) : NaN
  return (
    <button type="button" className="thread-row" aria-expanded={open} onClick={onToggle}>
      <Icon name="chev" className="size-3.5 flex-none transition-transform" />
      {first?.by ? <Who name={first.by} /> : null}
      <span className="min-w-0 flex-1 truncate" lang={contentLang(first?.question)}>{first?.question}</span>
      <span className="flex-none text-ink-2">{t('ask.exchanges', { count: th.turns.length, n: num(th.turns.length) })}</span>
      <span className="thread-state flex-none" data-state={state}>{t(`ask.state.${state}`)}</span>
      {Number.isFinite(at) && <Tip text={when(at)}><span className="w-8 flex-none text-right text-ink-2">{waited(at)}</span></Tip>}
    </button>
  )
}

/** An open conversation: its own scroll area, at the latest exchange; it follows new answers
 *  only while the reader is at the bottom. */
function ThreadBody({ th, n, talk, chips }: { th: Thread; n: Task; talk: Talk; chips: AskContext[] }) {
  const { t } = useT()
  const box = useRef<HTMLDivElement>(null)
  const atBottom = useRef(true)
  const size = th.turns.length + ':' + th.turns.map(x => x.answer?.length ?? 0).join(',') + ':' + (th.conclusion?.length ?? 0)
  useLayoutEffect(() => {
    const el = box.current
    if (el && atBottom.current) el.scrollTop = el.scrollHeight
  }, [size])
  return (
    <>
      <div ref={box} className="thread-scroll" onScroll={e => { const el = e.currentTarget; atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24 }}>
        <ol className="flex flex-col gap-3">
          {th.turns.map((turn, j) => <TurnView key={turn.id ?? j} turn={turn} agent={n.agent} onFile={talk.onFile} />)}
        </ol>
        {th.conclusion && (
          <div className="mt-2 rounded-md bg-sunken p-2">
            <h4 className="text-xs font-semibold text-ink-2">{t('ask.conclusion')}</h4>
            <Answer text={th.conclusion} onFile={talk.onFile} />
          </div>
        )}
      </div>
      {/* With the service: ask for a conclusion, or end the conversation (its agent closes). */}
      {talk.live && th.id && (
        th.ended
          ? <p className="mt-2 text-xs text-ink-2">{t('ask.threadEnded')}</p>
          : (
            <div className="mt-2 flex flex-wrap gap-1.5">
              <Button variant="outline" size="xs" className="rounded-sm" onClick={() => talk.onAsk(n, CONCLUDE, [chips[0]], th.id, true)}>{t('ask.conclude')}</Button>
              <Tip text={t('ask.endHint')}><Button variant="ghost" size="xs" className="rounded-sm" onClick={() => talk.onEnd(th.id)}>{t('ask.end')}</Button></Tip>
            </div>
          )
      )}
    </>
  )
}

/** Conversations shown before "earlier" is opened. */
const LATEST = 3

export function Ask({ unit, n, finding, talk }: { unit: Unit; n: Task; finding?: string; talk: Talk }) {
  const { t } = useT()
  const threads = threadsOf(n, talk)
  // The box continues the last open conversation, unless the user starts a new one.
  const [fresh, setFresh] = useState(false)
  const current = fresh ? undefined : threads.filter(x => !x.ended).at(-1)
  // Whom the question goes to: the task's agent; on a gate (the user's own task), the agent
  // that asked for it, or else the manager bot. It is asked only while its session is there.
  const askee = askeeOf(unit, n)
  const live = !!askee && talk.sessions.some(x => x.agent === askee && !x.ended)
  const who = n.kind === 'gate' ? n.title : n.taskId ? `#${n.taskId}` : finderLabel(t, n)
  const placeholder = n.kind === 'gate'
    ? (askee ? t('ask.placeholderGate', { agent: askee }) : t('ask.placeholderManager'))
    : t('ask.placeholder', { who })

  // The selection, as what the question is about. A chip left out stays out until it changes.
  const f = findingsOf(n).find(x => x.id === finding)
  // Picked lines say more than their file, so they take the file's place.
  const lines = talk.lines
  const chips: AskContext[] = [
    { kind: 'task', label: who },
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

  const [text, setText] = useState('')
  const box = useRef<HTMLTextAreaElement>(null)
  // Asked for from outside (`?`, "Ask why"): the cursor comes here.
  useEffect(() => {
    if (!talk.focusAt) return
    box.current?.focus()
    box.current?.scrollIntoView({ block: 'nearest' })
  }, [talk.focusAt])
  // The box grows with what is typed.
  useEffect(() => {
    const el = box.current
    if (el) { el.style.height = 'auto'; el.style.height = `${el.scrollHeight}px` }
  }, [text])
  // One conversation open at a time; the one just asked in stays open. At first, one that
  // still waits for its answer.
  const keyOf = (th: Thread, k: number) => th.id || `#${k}`
  const waiting = threads.findLastIndex(th => stateOf(th) === 'waiting')
  const waitingKey = waiting >= 0 ? keyOf(threads[waiting], waiting) : null
  const [open, setOpen] = useState<string | null>(waitingKey)
  const [openLast, setOpenLast] = useState(false)
  const [all, setAll] = useState(false)
  // Another piece of work: its waiting conversation open, if any, the rest folded.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { setOpen(waitingKey); setOpenLast(false); setAll(false) }, [n.id])
  const openKey = openLast && threads.length ? keyOf(threads[threads.length - 1], threads.length - 1) : open
  const send = () => {
    const q = text.trim()
    if (!q) return
    talk.onAsk(n, q, context, current?.id || null)
    setText('')
    setFresh(false)
    // The conversation asked in: the current one, or the new one it starts (the last).
    if (current) { setOpen(keyOf(current, threads.indexOf(current))); setOpenLast(false) } else setOpenLast(true)
  }
  const earlier = all ? 0 : Math.max(0, threads.length - LATEST)

  return (
    <div className="border-t border-line p-3 text-sm">
      {earlier > 0 && (
        <button type="button" className="mb-1 text-xs text-ink-2 hover:text-ink" onClick={() => setAll(true)}>{t('ask.earlier', { count: earlier })}</button>
      )}
      {threads.map((th, k) => {
        if (k < earlier) return null
        const key = keyOf(th, k)
        const isOpen = openKey === key
        return (
          <section key={key} className="thread" data-open={isOpen || undefined}>
            <ThreadRow th={th} open={isOpen} onToggle={() => { setOpenLast(false); setOpen(isOpen ? null : key) }} />
            {isOpen && <ThreadBody th={th} n={n} talk={talk} chips={chips} />}
          </section>
        )
      })}
      {current && (
        <div className="mt-3"><Button variant="ghost" size="xs" className="rounded-sm" onClick={() => setFresh(true)}><Icon name="plus" className="size-3" />{t('ask.fresh')}</Button></div>
      )}
      {threads.length > 0 && <div className="mb-3" />}
      {/* Every node has records to answer from, so the box is never disabled. */}
      <>
          <div className="mb-1.5"><Chips chips={context} onDrop={c => setDropped(d => [...d, c.label])} /></div>
          <div className="askbox">
            <textarea
              ref={box}
              rows={1}
              value={text}
              aria-label={t('ask.label')}
              placeholder={placeholder}
              onChange={e => setText(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); send() }
              }}
            />
            <Button size="icon-xs" aria-label={t('ask.send')} disabled={!text.trim()} onClick={send}><Icon name="send" className="size-3.5" /></Button>
          </div>
          <p className="mt-1.5 text-xs text-ink-2">{live ? t('ask.how', { agent: askee }) : t('ask.howAlone')}</p>
      </>
    </div>
  )
}
