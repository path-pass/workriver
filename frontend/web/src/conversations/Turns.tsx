// A conversation as a chat: each exchange, a conversation folded to one line or open (past
// questions, read-only), and the user's messages to a manager with its replies.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { ms } from '@/planning/select'
import { contentLang, useT } from '@/i18n'
import { focusAgent } from '@/api/actions'
import type { Attachment } from '@/attachments/types'
import type { Answer, AskContext, Asked, Copy, ManagerMessage, Thread, Turn } from './types'
import { ChoiceSurface, Chose } from '@/content/choice/Choice'
import { needsNote, noteWords, showsNothing, type Choice } from '@/content/choice/surface'
import { Input } from '@/ui/primitives/input'
import { Who } from '@/agents/Mark'
import { Prose } from '@/content/Prose'
import { ChipName, Content, Slot, SLOT } from '@/content/Chips'
import { useSpace } from '@/planning/spaces'
import { Icon, Mark } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Thumbs } from '@/attachments/Thumbs'
import { stateOf } from './select'

/** An answer: short Markdown, its file paths links that pick the file. */
const Answer = ({ text, onFile }: { text: string; onFile: (path: string) => void }) => <Prose text={text} fold={false} onFile={onFile} />

/** What a question or message is about, as chips (each with a × that leaves it out, given
 *  `onDrop`); `files` are those attached in the box, each with a × that takes it out. */
export function Chips({ chips, onDrop, files = [], onRemove }: { chips: AskContext[]; onDrop?: (c: AskContext) => void; files?: Attachment[]; onRemove?: (a: Attachment) => void }) {
  const { t } = useT()
  return (
    <span className="flex flex-wrap gap-1">
      {chips.map(c => (
        <span key={c.kind + c.label} className="ctx">
          <ChipName name={c.label} />
          {onDrop && (
            <button type="button" aria-label={t('talk.remove', { label: c.label })} onClick={() => onDrop(c)}><Icon name="no" className="size-3" /></button>
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

/** One exchange: the question, then the answer (or how far it has got). In a discussion
 *  (`discuss`) the question goes to the task's own agent, so how far it got reads so. */
function TurnView({ turn, agent, discuss, onFile }: { turn: Turn; agent: string | null; discuss?: boolean; onFile: (path: string) => void }) {
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
            <p className="text-del">{t(discuss ? 'discuss.cannot' : 'ask.cannot', { reason: t(`deliver.reason.${turn.failed.code}`) })}</p>
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
      ) : turn.answer === null && turn.step === 'held' ? (
        // Its agent is at work or on a blind task: it goes in once that agent is free (why on hover).
        <Theirs>
          <Tip text={turn.reason ?? ''}><p className="text-ink-2"><span className="pending" aria-hidden />{t('discuss.held', { agent: agent ?? '' })}</p></Tip>
        </Theirs>
      ) : turn.answer === null ? (
        <Theirs>
          <p className="text-ink-2"><span className="pending" aria-hidden />{discuss ? t(turn.step === 'reading' ? 'discuss.reading' : 'discuss.sending', { agent: agent ?? '' }) : t(`ask.step.${turn.step ?? 'starting'}`, { agent: agent ?? '' })}</p>
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
              <Who name={turn.consulted.agent} />
              {t('ask.asked', { agent: turn.consulted.agent })}
            </p>
          )}
        </Theirs>
      ) : null}
    </>
  )
}

/** A conversation folded to one line: its first question, who asked, how many exchanges,
 *  how far it has got, when last. */
export function ThreadRow({ th, open, body, onToggle }: { th: Thread; open: boolean; body: string; onToggle: () => void }) {
  const { t, num, waited, when } = useT()
  const first = th.turns[0]
  const last = th.turns.at(-1)
  const state = stateOf(th)
  const at = last ? ms(last.at) : NaN
  return (
    <button type="button" className="thread-row" aria-expanded={open} aria-controls={open ? body : undefined} onClick={onToggle}>
      <Icon name="chev" className="size-3.5 flex-none transition-transform" />
      {first?.by ? <Who name={first.by} /> : null}
      {th.discuss && <span className="ctx flex-none">{t('discuss.tag')}</span>}
      <span className="line-clamp-1 min-w-0 flex-1 wrap-anywhere" lang={contentLang(first?.question)}>{first?.conclude ? t('ask.concludeAsked') : first?.question}</span>
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
export function Log({ id, size, label, children }: { id?: string; size: string; label: string; children: ReactNode }) {
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

/** An open conversation: the exchanges as a chat. Read-only: new questions go to the manager. */
export function ThreadBody({ id, th, agent, onFile }: { id: string; th: Thread; agent: string | null; onFile: (path: string) => void }) {
  const { t } = useT()
  const size = th.turns.length + ':' + th.turns.map(x => `${x.answer?.length ?? 0}${x.step ?? ''}${x.failed ? 'f' : ''}`).join(',') + ':' + (th.conclusion?.length ?? 0)
  return (
    <>
      <Log id={id} size={size} label={t('ask.log')}>
        <ol className="chat">
          {th.turns.map((turn, j) => <TurnView key={turn.id ?? j} turn={turn} agent={agent} discuss={th.discuss} onFile={onFile} />)}
        </ol>
        {/* A conclusion with no request in the thread (the sample's): after it. */}
        {th.conclusion && !th.turns.some(x => x.conclude && x.answer !== null) && (
          <div className="mt-3 rounded-md bg-sunken p-2">
            <h4 className="text-xs font-semibold text-ink-2">{t('ask.conclusion')}</h4>
            <Answer text={th.conclusion} onFile={onFile} />
          </div>
        )}
      </Log>
      {th.ended && <p className="mt-2 flex-none text-xs text-ink-2">{t('ask.threadEnded')}</p>}
    </>
  )
}

/** Conversations shown before "earlier" is opened. */
const LATEST = 3

/** How long a discussion is open on screen before its new answers count as read, in ms. */
const READ_AFTER = 1500

/**
 * Past questions about a task (or a unit), the user's and agents': each conversation folded to
 * one row, one opened at a time as a chat. The task's discussion (the user's with the agent of
 * a delivered design or research) opens by itself, and its new answers are read once it has
 * been on screen a moment (`onRead`); its box is below (`Discussion.tsx`). Other questions go
 * to the manager; agents keep asking each other through workriver.
 */
export function Questions({ id, threads, agent, onFile, onRead, children }: { id: string; threads: Thread[]; agent: string | null; onFile: (path: string) => void; onRead?: (at: string) => void; children?: ReactNode }) {
  const { t } = useT()
  const keyOf = (th: Thread, k: number) => th.id || `#${k}`
  const talk = threads.findIndex(th => th.discuss)
  const [open, setOpen] = useState<string | null>(talk >= 0 ? keyOf(threads[talk], talk) : null)
  const [all, setAll] = useState(false)
  // A discussion that starts while the block is on screen opens too.
  const talkKey = talk >= 0 ? keyOf(threads[talk], talk) : null
  const seenTalk = useRef(talkKey)
  useEffect(() => { if (talkKey && talkKey !== seenTalk.current) { seenTalk.current = talkKey; setOpen(talkKey) } }, [talkKey])
  // The newest answer not read, while the discussion is open on screen (not in a hidden tab):
  // measured on the discussion itself, not on the block around it.
  const shown = useRef<HTMLElement>(null)
  const newest = open !== null && open === talkKey ? threads[talk].turns.filter(x => x.unread && x.answeredAt).map(x => x.answeredAt!).sort((a, b) => ms(a) - ms(b)).at(-1) : undefined
  useEffect(() => {
    if (!newest || !onRead) return
    let k = 0
    const look = () => {
      clearTimeout(k)
      k = window.setTimeout(() => { if (!document.hidden && shown.current?.getClientRects().length) onRead(newest) }, READ_AFTER)
    }
    look()
    document.addEventListener('visibilitychange', look)
    return () => { clearTimeout(k); document.removeEventListener('visibilitychange', look) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [newest])
  if (!threads.length && !children) return null
  // Only past conversations fold behind "earlier": the discussion always shows.
  const others = threads.filter(th => !th.discuss)
  const earlier = all ? 0 : Math.max(0, others.length - LATEST)
  const hidden = (th: Thread) => !th.discuss && others.indexOf(th) < earlier
  return (
    <section aria-labelledby={`${id}-h`} className="flex flex-col text-sm">
      <h3 id={`${id}-h`} className="mb-1.5 flex items-baseline gap-2 text-xs font-semibold text-ink-2">
        {t('ask.questions')}{threads.length > 0 && <span className="font-normal">{t('ask.conversations', { count: threads.length })}</span>}
      </h3>
      {earlier > 0 && (
        <button type="button" className="mb-1 self-start text-xs text-ink-2 hover:text-ink" onClick={() => setAll(true)}>{t('ask.earlier', { count: earlier })}</button>
      )}
      {threads.map((th, k) => {
        if (hidden(th)) return null
        const key = keyOf(th, k)
        const isOpen = open === key
        const body = `${id}-${key}`
        return (
          <section key={key} ref={th.discuss ? shown : undefined} className="thread" data-open={isOpen || undefined}>
            <ThreadRow th={th} open={isOpen} body={body} onToggle={() => setOpen(isOpen ? null : key)} />
            {isOpen && <ThreadBody id={body} th={th} agent={agent} onFile={onFile} />}
          </section>
        )
      })}
      {children}
    </section>
  )
}

/** Where the user's message to a manager (or one copy of it, after its `place`) is, under it. */
function Step({ m, place }: { m: Pick<ManagerMessage, 'manager' | 'step' | 'reasonCode' | 'reason' | 'blocked'>; place?: ReactNode }) {
  const { t } = useT()
  const at = place && <span className="mr-1.5 font-semibold text-ink">{place}</span>
  if (m.step === 'blocked' && m.blocked)
    return (
      <div className="mt-1.5 flex flex-col items-start gap-1.5">
        <p className="text-xs text-you-ink">{at}{t('talk.step.blocked', { agent: m.manager })}</p>
        <Button variant="outline" size="xs" className="rounded-sm" onClick={() => focusAgent(m.blocked!.session)}><Icon name="terminal" className="size-3.5" />{t('ask.toTerminal')}</Button>
      </div>
    )
  // Its manager is on a blind task: it goes once that task is delivered (the service's words on hover).
  if (m.step === 'held')
    return <Tip text={m.reason ?? ''}><p className="mt-1 text-xs text-ink-2">{at}{t('talk.step.held', { agent: m.manager })}</p></Tip>
  if (m.step === 'not-delivered')
    return <Tip text={m.reason ?? ''}><p className="mt-1 text-xs text-del">{at}{t('talk.step.not-delivered', { reason: t(`deliver.reason.${m.reasonCode ?? 'unknown'}`) })}</p></Tip>
  return <p className="mt-1 text-xs text-ink-2">{at}{m.step === 'sending' && <span className="pending" aria-hidden />}{t(`talk.step.${m.step}`, { agent: m.manager })}</p>
}

/** What the user picked with a message: its unit or task and the rest as chips, then the
 *  stored files it points at, as attachments show everywhere (pictures as thumbnails). */
function Picked({ m }: { m: ManagerMessage }) {
  const stored = m.attachments ?? []
  const own: AskContext[] = m.unit ? [{ kind: m.task ? 'task' : 'unit', label: m.task ? `${m.unit}#${m.task}` : m.unit }] : []
  const chips = [...own, ...(m.context ?? []).filter(c => !(c.kind === 'attachment' && stored.some(a => a.id === c.id)))]
  if (!chips.length && !stored.length) return null
  return (
    <>
      {chips.length > 0 && <div className="mt-1.5"><Chips chips={chips} /></div>}
      <Thumbs list={stored} />
    </>
  )
}

/**
 * A manager's question with options, under its words: answered, the option picked; open, the
 * options to pick from (a note where the option asks one); else shown, not to be answered.
 */
function Question({ q, onAnswer }: { q: Asked; onAnswer: (selected: string, note: string) => void }) {
  const { t } = useT()
  const [picked, setPicked] = useState<string>()
  const [note, setNote] = useState('')
  const [missing, setMissing] = useState(false)
  if (!q.choice) return null
  const choice = q.choice
  if (q.answered) return <div className="mt-2">{q.answered.selected.map(v => <Chose key={v} choice={choice} value={v} />)}</div>
  if (!q.open)
    return (
      <div className="mt-2">
        <ChoiceSurface choice={choice} disabled />
        <p className="mt-1.5 text-xs text-ink-2">{t('choice.closed')}</p>
      </div>
    )
  const needed = !!picked && needsNote(choice, picked)
  const words = noteWords(choice, picked)
  const answer = () => {
    if (!picked || (needed && showsNothing(note))) return setMissing(true)
    onAnswer(picked, note.trim())
  }
  return (
    <div className="mt-2">
      <ChoiceSurface choice={choice} picked={picked} onPick={v => { setPicked(v); setMissing(false) }} onAnswer={answer}>
        <Input
          value={note} onChange={e => { setNote(e.target.value); setMissing(false) }}
          placeholder={t(words?.prompt ?? 'choice.note')} aria-label={t(words?.prompt ?? 'choice.note')}
          aria-required={needed || undefined} aria-invalid={(missing && needed) || undefined} className="h-8 text-sm md:text-sm"
        />
        {missing && <p className="text-xs text-del" role="alert">{t(words?.missing ?? 'choice.needNote')}</p>}
      </ChoiceSurface>
    </div>
  )
}

/** `onAnswer`: the user answers a manager's question; `asked`: the options a message of the user's answered. */
export function MessageView({ m, onAnswer, asked }: { m: ManagerMessage; onAnswer: (a: Answer, note: string) => void; asked: (a: Answer) => Choice | undefined }) {
  const { t } = useT()
  const answer = (reply: number) => (selected: string, note: string) => m.id && onAnswer({ message: m.id, ...(reply && { reply }), selected: [selected] }, note)
  // To every manager: each copy's replies, in the order they came, labelled by its workspace.
  const copies = m.to ?? []
  const replies = copies.length
    ? copies.flatMap(c => c.replies.map(r => ({ ...r, place: <Place c={c} /> }))).sort((a, b) => ms(a.at) - ms(b.at))
    : m.replies.map((r, k) => ({ ...r, place: null, k: k + 1 }))
  // A line of workriver's own: nobody's bubble, never unread.
  if (m.notice) return <Notice m={m} />
  return (
    <>
      {/* One the manager started: its words alone, nothing of the user's above them. */}
      {m.byManager
        ? <Theirs who={<Named name={m.manager} manager />} at={m.at}><Prose text={m.text} fold={false} /><Question q={m} onAnswer={answer(0)} /></Theirs>
        : (
          <Mine who={t('ask.you')} at={m.at}>
            {m.answer?.selected.map(v => <Chose key={v} choice={asked(m.answer!)} value={v} />)}
            {m.text && <p className="whitespace-pre-line wrap-anywhere" lang={contentLang(m.text)}>{m.text}</p>}
            <Picked m={m} />
            {copies.length
              ? <ul className="mt-1 flex flex-col" aria-label={t('talk.copies')}>{copies.map((c, k) => <li key={k}><Step m={c} place={<Place c={c} />} /></li>)}</ul>
              : <Step m={m} />}
          </Mine>
        )}
      {replies.map((r, j) => (
        <Theirs key={j} who={<>{<Named name={r.by} manager />}{r.place && <span className="min-w-0 truncate">{r.place}</span>}</>} at={r.at}>
          <Prose text={r.text} fold={false} />
          {/* A reply to every manager asks nothing to choose (the service refuses one). */}
          {'k' in r && <Question q={r} onAnswer={answer(r.k)} />}
        </Theirs>
      ))}
    </>
  )
}

/** A system line in a conversation: the seated manager's terminal was gone and it shows
 *  again where it was, so it is the manager again by itself. */
function Notice({ m }: { m: ManagerMessage }) {
  const { t, when, waited } = useT()
  return (
    <li className="chat-note" data-notice={m.notice}>
      <Icon name="manager" className="size-3 flex-none" />
      <span className="min-w-0"><Slot text={t(`talk.notice.${m.notice!}`, { manager: SLOT })}><Content text={m.manager} /></Slot></span>
      <Tip text={when(ms(m.at))}><time className="flex-none" dateTime={m.at}>{waited(ms(m.at))}</time></Tip>
    </li>
  )
}

/** The workspace a copy of a message to every manager went to, by its name and id (and its
 *  manager's machine, when another). */
function Place({ c }: { c: Copy }) {
  const { t } = useT()
  const space = useSpace(c.workspace)
  const name = space?.name ?? ''
  const text = c.machine ? t('talk.place', { workspace: name, machine: c.machine }) : name
  return <span className="inline-flex min-w-0 items-baseline gap-1"><span className="min-w-0 truncate" lang={contentLang(name)}>{text}</span><span className="flex-none font-mono text-[11px] opacity-80">{c.workspace}</span></span>
}
