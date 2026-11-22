// The picked task in full: what went in, what came out, its checks and findings, and what
// can be done with it.
import { useState, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { untested, finderLabel, findingsOf } from './select'
import { ms } from '@/planning/select'
import { commitStats } from '@/code/select'
import type { Pick, Task, Trace } from './types'
import { dropTask, record } from '@/api/actions'
import { Input } from '@/ui/primitives/input'
import { contentLang, useT } from '@/i18n'
import { growth } from './growth'
import { linkTo, pathOf } from '@/app/link'
import { cn } from '@/lib/utils'
import type { Unit } from '@/planning/types'
import { threadsOf } from '@/conversations/select'
import { Questions } from '@/conversations/Turns'
import { Discussion } from '@/conversations/Discussion'
import type { Talk } from '@/conversations/types'
import { GateForm, GateSent } from './GateForm'
import { Chose } from '@/content/choice/Choice'
import { Thumbs } from '@/attachments/Thumbs'
import { gateKey } from './actions'
import { TerminalButton, Who } from '@/agents/Mark'
import { Prose } from '@/content/Prose'
import { Slot, SLOT } from '@/content/Chips'
import { CopyButton } from '@/ui/CopyButton'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { RunSteps } from '@/commands/context'
import { Choice } from '@/ui/Choice'
import { NodeName, glyph, grewText } from './Nodes'
import { Findings, FindingNotes } from './Findings'

/** The checks run on a piece of work, one line each; with none, done work says so quietly. */
function Checks({ n }: { n: Task }) {
  const { t } = useT()
  const list = n.output.evidence ?? []
  if (!list.length) return untested(n) ? <div className="mt-2 flex items-center gap-1.5 text-xs text-ink-2"><Icon name="untested" className="size-3.5" />{t('evidence.none')}</div> : null
  return (
    <ul className="mt-2 flex flex-col gap-1" aria-label={t('evidence.label')}>
      {list.map((e, k) => (
        <li key={k} className="check" data-result={e.result}>
          <Icon name={e.result === 'passed' ? 'check' : 'no'} className="size-3.5" />
          <span className="sr-only">{t(`evidence.${e.result}`)}</span>
          <code className="min-w-0 truncate">{e.command}</code>
          <CopyButton text={e.command} />
          <span className="flex-none text-ink-2" lang={contentLang(e.summary)}>{e.summary}</span>
          {e.attachment && <Thumbs list={[e.attachment]} />}
        </li>
      ))}
    </ul>
  )
}

/**
 * Who did the work, as its harness knows it: the harness, the model its agent said, the
 * session id and where the harness keeps the session (on another machine: which one).
 */
export function Traced({ trace }: { trace: Trace }) {
  const { t } = useT()
  return (
    <div className="flex flex-col gap-0.5 border-b border-line px-3 py-1.5 text-xs text-ink-2 wrap-anywhere" aria-label={t('task.trace')}>
      <span>{[trace.harness ?? t('task.noHarness'), trace.model ?? t('task.noModel')].join(' · ')} · <code>{trace.session}</code></span>
      {trace.file
        ? <span className="flex items-center gap-1"><code className="min-w-0">{trace.file}</code><CopyButton text={trace.file} />{trace.machine && <span className="flex-none">{t('task.onMachine', { machine: trace.machine })}</span>}</span>
        : <span>{t(`task.noFile.${trace.unknown ?? 'harness'}`)}</span>}
    </div>
  )
}

/**
 * What the manager or the user added to what a task asks after it was recorded: under what
 * it asked first, never merged into it; each with its author's mark and time, oldest first.
 */
export function Added({ n }: { n: Task }) {
  const { t, when } = useT()
  const list = n.input.notes ?? []
  if (!list.length) return null
  return (
    <ul className="mt-2 flex flex-col gap-2 border-l-2 border-line pl-2.5" aria-label={t('task.addedList')}>
      {list.map((x, k) => (
        <li key={k}>
          <div className="flex items-center gap-1.5 text-xs text-ink-2">
            <Who name={x.by} />
            {x.user ? t('task.addedByYou') : t('task.added', { by: x.by })} · {when(ms(x.at))}
          </div>
          <div className="mt-0.5"><Prose text={x.text} /></div>
          <Thumbs list={x.attachments ?? []} />
        </li>
      ))}
    </ul>
  )
}

export function Detail({ unit, n, finding, onPick, talk }: { unit: Unit; n: Task; finding?: string; onPick: (p: Pick) => void; talk: Talk }) {
  const { t, num, when } = useT()
  const byId = (id?: string) => unit.tasks.find(x => x.id === id)
  const commitsOf = (x?: Task) => unit.commits.filter(c => x?.output.commits?.includes(c.id))
  const quiet = (text: string) => <div className="text-xs text-ink-2">{text}</div>
  const paths = (list: string[]) => <div className="font-mono text-xs text-ink-2">{list.join(t('list.sep'))}</div>
  const threads = threadsOf(n)
  const found = findingsOf(n).length > 0 && <Findings unit={unit} n={n} finding={finding} onPick={onPick} />
  let head: ReactNode, input: ReactNode, output: ReactNode
  if (n.kind === 'review') {
    const of = byId(n.inputs.find(i => i.primary)?.from)
    const files = [...new Set(commitsOf(of).filter(c => !c.merge).flatMap(c => c.files.map(f => f.path)))]
    head = <><Icon name={glyph(n)} />{finderLabel(t, n)} · {t(n.state === 'dropped' ? 'state.dropped' : n.result === 'running' ? 'state.running' : 'state.done')}</>
    input = <>{of && <div><Slot text={t('io.changesOf', { label: SLOT })}><NodeName n={of} /></Slot></div>}{files.length ? paths(files) : quiet(t('io.noChanges'))}<Added n={n} /></>
    output = <>{found || quiet(t(n.result === 'running' ? 'io.noFindingsYet' : 'io.noFindings'))}<Checks n={n} /><Thumbs list={n.output.attachments ?? []} /></>
  } else if (n.kind === 'gate') {
    head = <><Icon name={glyph(n)} />{finderLabel(t, n)} · {t(`state.${n.state}`)}</>
    // Whoever did the work the gate follows wrote its steps (the sample's guess; the service knows).
    // The results this gate asks about that the user may still discuss with their agents.
    const discussed = n.state === 'waiting' ? unit.tasks.filter(x => x.output.gate === n.id && x.discuss === 'open') : []
    input = <>
      <Prose text={n.input.text || n.title} />
      <Added n={n} />
      <RunSteps steps={n.input.steps ?? []} ask={{ unit: unit.id, from: `${unit.id}#${n.id}`, by: byId(n.inputs.find(i => i.primary)?.from)?.agent ?? undefined }} />
      {discussed.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {discussed.map(x => (
            <Button key={x.id} variant="outline" size="xs" className="rounded-sm" onClick={() => onPick({ kind: 'task', id: x.id })}>
              <Icon name="talk" className="size-3" />{t('discuss.atGate', { task: `#${x.taskId ?? x.id}`, agent: x.agent ?? '' })}
            </Button>
          ))}
        </div>
      )}
    </>
    // What was attached to the gate (by its agent) is there to look at before answering it.
    output = n.state === 'waiting'
      ? <div className="flex flex-col gap-2"><Thumbs list={n.output.attachments ?? []} /><GateForm key={gateKey(unit, n)} unit={unit} gate={n} /></div>
      : <>
          {n.output.selected?.map(v => <Chose key={v} choice={n.input.choice} value={v} />)}
          {n.output.note && <Prose text={n.output.note} />}
          <Thumbs list={n.output.attachments ?? []} />
          <GateSent unit={unit} gate={n} />
          {found && <div className="mt-3">{found}</div>}
        </>
  } else {
    const commits = commitsOf(n)
    const grew = growth(unit).get(n.id)
    const causes = n.input.causes.flatMap(c => {
      const by = byId(c.review)
      return by ? findingsOf(by).filter(f => c.findings.includes(f.id)).map(f => ({ by, f })) : []
    })
    head = <>{n.agent ? <Who name={n.agent} /> : <Icon name={glyph(n)} />}{[t(n.design ? 'kind.design' : n.research ? 'kind.research' : 'kind.implement'), n.agent, t(`state.${n.state}`)].filter(Boolean).join(' · ')}</>
    input = <>
      <Prose text={n.input.text || n.title} />
      <Added n={n} />
      {n.state === 'unregistered' && quiet(t('io.unknownInput'))}
      {/* The gap is closed by saying which task the commit belongs to; nothing is rewritten. */}
      {n.state === 'unregistered' && n.output.commits?.[0] && (
        <label className="mt-2 flex items-center gap-2 text-xs">
          {t('io.linkTo')}
          <Choice
            label={t('io.linkTo')}
            value=""
            placeholder="…"
            onChange={v => { record('commit.linked', { unit: unit.id, hash: n.output.commits![0], task: Number(v) }).catch(err => console.warn(err)) }}
            options={unit.tasks.filter(x => x.taskId && x.kind !== 'gate').map(x => ({ value: x.id, label: `#${x.taskId} ${x.title || finderLabel(t, x)}`, lang: x.title ? contentLang(x.title) : undefined }))}
            className="h-7 w-auto max-w-[60%] text-xs"
          />
        </label>
      )}
      {causes.length > 0 && (
        <div className="mt-2">
          <h4 className="text-xs text-ink-2">{t('finding.causes')}</h4>
          {causes.map(({ by, f }) => (
            <button key={by.id + f.id} type="button" className="cause" onClick={() => onPick({ kind: 'task', id: by.id, finding: f.id })}>
              <Icon name={glyph(by)} />
              <span className="flex-none">{finderLabel(t, by)}</span>
              <span className="flex-none font-mono text-xs font-semibold">{f.id}</span>
              <span className="min-w-0 whitespace-pre-line" lang={contentLang(f.problem)}>{f.problem}</span>
            </button>
          ))}
          {/* What was added to those findings later is part of what this work must meet. */}
          {causes.map(({ by, f }) => <FindingNotes key={by.id + f.id} f={f} id />)}
          <Thumbs list={causes.flatMap(c => c.f.attachments ?? [])} />
        </div>
      )}
    </>
    // Delivered work shows its note and checks even without a commit of its own.
    output = <>
      {commits.length
        ? commits.map(c => {
            const s = commitStats(c)
            return (
              <div key={c.id}>
                {c.live ? t('io.uncommitted') : <span lang={contentLang(c.subject)}>{c.subject}</span>}
                <span className="text-xs text-ink-2"> · <span className="whitespace-nowrap">{t('stat.files', { count: s.files })}</span> +{num(s.add)} −{num(s.del)}{c.linked && ` · ${t('io.linked')}`}{c.rebasedFrom && ` · ${t('io.rebasedFrom', { hash: c.rebasedFrom })}`}</span>
              </div>
            )
          })
        : (n.lostCommits?.length ?? 0) > 0 ? quiet(t('io.lostCommits', { commits: n.lostCommits!.join(t('list.sep')) }))
        : n.state !== 'done' && <>{quiet(t('io.notStarted'))}{n.plans.length > 0 && <div className="font-mono text-xs text-ink-2">{t('io.willChange', { paths: n.plans.join(t('list.sep')) })}</div>}</>}
      {grew && <div className="mt-1 flex items-start gap-1.5 text-xs text-ink-2"><Icon name="grew" className="mt-px size-3.5 flex-none" />{grewText(t, grew)}</div>}
      {n.output.note && <div className="mt-1"><Prose text={n.output.note} /></div>}
      <Thumbs list={n.output.attachments ?? []} />
      <Checks n={n} />
    </>
  }
  return (
    <div className="mt-1 overflow-hidden rounded-md border border-you">
      <div className="flex items-center gap-1.5 bg-you-bg px-3 py-2 text-sm font-semibold text-you-ink [--who-ground:transparent]">
        {head}
        {/* Blind: its findings were made without seeing other work; weigh agreement accordingly. */}
        {n.blind && <Tip text={t('task.blindHint')}><span className="flex items-center gap-1 font-normal"><Icon name="blind" className="size-3.5" />{t('task.blind')}</span></Tip>}
        <span className="ml-auto flex items-center gap-1">
          {/* The agent on work that is running or still to do, while its session lives: to its terminal. */}
          {n.agent && (n.state === 'running' || n.state === 'todo') && talk.sessions.some(s => s.agent === n.agent && !s.ended) && (
            <TerminalButton agent={n.agent} session={n.agentSession} className="text-you-ink hover:bg-surface/60" />
          )}
          {/* Its own link: to send someone straight here. */}
          <CopyButton text={linkTo(pathOf(unit.id, { kind: 'task', id: n.id }))} aria={t('link.copy')} className="text-you-ink hover:bg-surface/60" />
        </span>
      </div>
      {n.trace && <Traced trace={n.trace} />}
      {/* No longer needed: by whom and when, then why in their words. */}
      {n.dropped && (
        <div className="flex flex-col gap-0.5 border-b border-line px-3 py-2 text-sm wrap-anywhere">
          <span className="flex items-center gap-1.5 text-xs text-ink-2">
            <Icon name="dropped" className="size-3.5" />
            {n.dropped.by === 'user' ? t('task.droppedByYou', { when: when(ms(n.dropped.at)) }) : t('task.droppedBy', { by: n.dropped.by, when: when(ms(n.dropped.at)) })}
          </span>
          <span className="whitespace-pre-line" lang={contentLang(n.dropped.reason)}>{n.dropped.reason}</span>
        </div>
      )}
      <div className={cn('grid text-sm wrap-anywhere', n.kind === 'review' ? 'min-[701px]:grid-cols-[1fr_2fr]' : 'grid-cols-[repeat(auto-fit,minmax(260px,1fr))]')}>
        <div className="min-w-0 p-3"><h3 className="mb-1.5 text-xs font-semibold text-ink-2">{t('io.input')}</h3>{input}</div>
        <div className="min-w-0 border-line p-3 max-[700px]:border-t min-[701px]:border-l">
          <h3 className="mb-1.5 flex items-baseline gap-2 text-xs font-semibold text-ink-2">
            {t('io.output')}
            {/* Corrected after delivery: what shows is the latest. */}
            {n.amended && <span className="font-normal">{t('io.amended', { when: when(ms(n.amended)) })}</span>}
          </h3>
          {output}
        </div>
      </div>
      {/* Past questions about it (the user's, and agents' to each other), read-only; and the
          discussion of a delivered design or research with its agent, with its box. */}
      {(threads.length > 0 || n.discuss) && (
        <div className="border-t border-line p-3">
          <Questions id={`questions-${unit.id}-${n.id}`} key={unit.id + n.id} threads={threads} agent={n.agent} onFile={talk.onFile} onRead={at => talk.onDiscussRead(unit.id, n.id, at)}>
            {n.discuss && <Discussion unit={unit} n={n} talk={talk} />}
          </Questions>
        </div>
      )}
      {/* Talking happens in the side panel; here only the way there, the task as its chip. */}
      <div className="flex items-center gap-2 border-t border-line px-3 py-2 text-xs text-ink-2">
        <Button variant="outline" size="xs" className="rounded-sm" onClick={talk.onOpen}><Icon name="talk" className="size-3" />{t('talk.ask')}</Button>
        {(n.state === 'todo' || n.state === 'running' || n.state === 'waiting') && <DropTask key={unit.id + n.id} unit={unit} n={n} />}
      </div>
    </div>
  )
}

/** The user marks work not done yet as no longer needed, saying why; nothing is deleted. */
function DropTask({ unit, n }: { unit: Unit; n: Task }) {
  const { t } = useT()
  const [open, setOpen] = useState(false)
  const [reason, setReason] = useState('')
  const [error, setError] = useState('')
  if (!open) return <Button variant="ghost" size="xs" className="ml-auto rounded-sm" onClick={() => setOpen(true)}><Icon name="dropped" className="size-3" />{t('task.drop')}</Button>
  const submit = () => { dropTask(unit, n, reason.trim()).then(() => setOpen(false), (e: Error) => setError(e.message)) }
  return (
    <form className="ml-auto flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2" onSubmit={e => { e.preventDefault(); if (reason.trim()) submit() }}>
      <Input autoFocus aria-label={t('task.dropReason')} placeholder={t('task.dropReasonHint')} value={reason} onChange={e => setReason(e.target.value)} className="h-7 min-w-48 flex-1 text-xs" />
      <Button type="submit" size="xs" className="rounded-sm" disabled={!reason.trim()}>{t('task.dropConfirm')}</Button>
      <Button type="button" variant="ghost" size="xs" className="rounded-sm" onClick={() => { setOpen(false); setError('') }}>{t('task.dropCancel')}</Button>
      {error && <p role="alert" className="w-full text-xs wrap-anywhere text-del">{t('task.dropFailed', { error })}</p>}
    </form>
  )
}
