// A conversation as a chat: each exchange, a conversation folded to one line or open, and
// the user's messages to a manager with its replies.
import { useLayoutEffect, useRef, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { ms } from '@/planning/select'
import { contentLang, useT } from '@/i18n'
import { focusAgent } from '@/api/actions'
import type { Attachment } from '@/attachments/types'
import type { AskContext, ManagerMessage, Thread, Turn } from './types'
import { Who } from '@/agents/Mark'
import { Prose } from '@/content/Prose'
import { ChipName } from '@/content/Chips'
import { Icon, Mark } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Thumbs } from '@/attachments/Thumbs'
import type { Talk } from './types'
import { stateOf } from './select'

/** An answer: short Markdown, its file paths links that pick the file. */
const Answer = ({ text, onFile }: { text: string; onFile: (path: string) => void }) => <Prose text={text} fold={false} onFile={onFile} />

/** What a question is about, as chips; `files` are those attached in the box, each with a × that takes it out. */
export function Chips({ chips, onDrop, files = [], onRemove }: { chips: AskContext[]; onDrop?: (c: AskContext) => void; files?: Attachment[]; onRemove?: (a: Attachment) => void }) {
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

/** An open conversation: the exchanges as a chat, then what can be done with it. */
export function ThreadBody({ id, th, agent, talk, onConclude }: { id: string; th: Thread; agent: string | null; talk: Talk; onConclude: () => void }) {
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

export function MessageView({ m }: { m: ManagerMessage }) {
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
