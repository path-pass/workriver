// Asking why about a piece of work. One box under the picked node's detail; what the
// question is about is the current selection, shown as chips above it. A temporary agent
// answers from the records and the code, and asks the agent that did the work only when it
// is idle, so an agent at work is never interrupted.
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { askeeOf, baseName, finderLabel, findingsOf, type Lines } from '@/data'
import { useT } from '@/i18n'
import type { AskContext, Session, Turn, Task, Unit } from '@/types'
import { Icon, Who } from './bits'

export interface Talk {
  sessions: Session[]
  /** The file picked in the code layer, and lines picked in it. */
  file: string | null
  lines: Lines | null
  /** Questions asked in this sitting, by node id, after the recorded ones. */
  asked: Record<string, Turn[]>
  onAsk: (node: Task, question: string, context: AskContext[]) => void
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
  return <p className="whitespace-pre-wrap">{parts}</p>
}

function Chips({ chips, onDrop }: { chips: AskContext[]; onDrop?: (c: AskContext) => void }) {
  const { t } = useT()
  return (
    <span className="flex flex-wrap gap-1">
      {chips.map(c => (
        <span key={c.kind + c.label} className="ctx">
          {c.label}
          {onDrop && c.kind !== 'task' && (
            <button type="button" aria-label={t('ask.remove', { label: c.label })} onClick={() => onDrop(c)}><Icon name="no" className="size-3" /></button>
          )}
        </span>
      ))}
    </span>
  )
}

export const turnsOf = (n: Task, talk: Talk) => [...(n.thread ?? []), ...(talk.asked[n.id] ?? [])]

export function Ask({ unit, n, finding, talk }: { unit: Unit; n: Task; finding?: string; talk: Talk }) {
  const { t } = useT()
  const turns = turnsOf(n, talk)
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
      ? [{ kind: 'lines' as const, label: `${baseName(lines.path)} L${lines.from}${lines.to > lines.from ? `–${lines.to}` : ''}`, path: lines.path }]
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
  const send = () => {
    const q = text.trim()
    if (!q) return
    talk.onAsk(n, q, context)
    setText('')
  }

  return (
    <div className="border-t border-line p-3 text-sm">
      {turns.length > 0 && (
        <ol className="mb-3 flex flex-col gap-3">
          {turns.map((turn, k) => (
            <li key={k} className="flex flex-col gap-1">
              <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1"><span className="font-semibold">{turn.question}</span><Chips chips={turn.context} /></div>
              {turn.answer === null ? (
                <p className="text-ink-2" aria-live="polite">
                  <span className="pending" aria-hidden />{t(`ask.step.${turn.step ?? 'starting'}`, { agent: n.agent ?? '' })}
                </p>
              ) : (
                <>
                  <Answer text={turn.answer} onFile={talk.onFile} />
                  {turn.consulted && (
                    <p className="flex items-center gap-1.5 text-xs text-ink-2">
                      {turn.consulted.outcome === 'asked' && <Who name={turn.consulted.agent} />}
                      {t(`ask.${turn.consulted.outcome}`, { agent: turn.consulted.agent })}
                    </p>
                  )}
                </>
              )}
            </li>
          ))}
        </ol>
      )}
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
