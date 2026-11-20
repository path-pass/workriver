// Running a command as the user: a gate's or chore's step, or one typed in the side panel.
// Nothing runs before the user has seen the exact text, where it runs and who wrote it, and
// confirmed; then the service types it into a plain shell of the user's in Herdr and the
// dialog follows what that shell shows until the command ends. Meanwhile a workriver agent
// reviews what the command will do; it only warns, Run never waits for it.
import { createContext, useContext, useEffect, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/ui/primitives/dialog'
import { Input } from '@/ui/primitives/input'
import { ms, noCode, unitsOf } from '@/planning/select'
import { contentLang, useT } from '@/i18n'
import { focusAgent, focusRun, prepareRun, readRun, runCommand } from '@/api/actions'
import type { RunAsk, RunReview, RunView } from '@/commands/types'
import type { Step } from '@/work/types'
import type { RecentRun, Verdict } from '@/commands/types'
import type { Workspace } from '@/workspace/types'
import { Choice } from '@/ui/Choice'
import { Steps } from './bits'
import { Prose } from '@/content/Prose'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'

/** Asks to run a command (opens the confirm dialog); App provides it. */
export const RunContext = createContext<(a: RunAsk) => void>(() => {})

/**
 * Commands that can destroy work for good, flagged in the dialog. A heuristic for the
 * obvious ones, not a guard: the user reads what they run.
 */
const DANGER = [
  /\brm\s+(?:-\S*\s+)*-\S*[rRf]/, // rm -rf, rm -r, rm -f
  /\bpush\b.*(?:--force\b|--force-with-lease|\s-f\b|\s\+\S)/, // push --force, -f, +ref
  /\breset\s+(?:.*\s)?--hard\b/,
  /\bclean\s+(?:.*\s)?-\S*f/, // clean -fd
  /\bbranch\s+(?:.*\s)?-D\b/,
  /\b(?:mkfs|dd\s+.*\bof=)/,
]
export const dangerous = (command: string) => DANGER.some(r => r.test(command))

/** A gate's or chore's steps, each command with "Run" next to its copy button. */
export function RunSteps({ steps, ask }: { steps: Step[]; ask: Omit<RunAsk, 'command'> }) {
  const run = useContext(RunContext)
  return <Steps steps={steps} onRun={(command, step) => run({ ...ask, command, step })} />
}

/** The confirm dialog, then the run's output, its exit status and its terminal. */
export function RunDialog({ ask, onClose }: { ask: RunAsk | null; onClose: () => void }) {
  const { t } = useT()
  const [view, setView] = useState<RunView | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  // Each new ask is prepared afresh: the service says where it runs and who wrote it. A run
  // already made (from the recent runs) is read as it is now.
  useEffect(() => {
    setView(null); setError(''); setBusy(false)
    if (!ask) return
    let gone = false
    ;(ask.id ? readRun(ask.id) : prepareRun(ask)).then(v => { if (!gone) setView(v) }, e => { if (!gone) setError(String(e)) })
    return () => { gone = true }
  }, [ask])
  // While it runs, what its shell shows, every second, until it ends or cannot be read; and
  // while its review is being written. A read sent before Run does not undo it.
  const id = view && ((view.started && view.exit === null && view.watching) || view.review?.state === 'pending' || view.review?.state === 'blocked') ? view.id : null
  useEffect(() => {
    if (!id) return
    const k = setInterval(() => readRun(id).then(v => setView(was => was?.started && !v.started ? { ...v, started: true, watching: true } : v), e => setError(String(e))), 1000)
    return () => clearInterval(k)
  }, [id])
  const go = () => {
    if (!view) return
    setBusy(true)
    // The exact text shown is what is sent: the service runs nothing else.
    runCommand(view.id, view.command).then(
      () => setView(v => v && { ...v, started: true, watching: true }),
      e => { setBusy(false); setError(t('run.failed', { error: String(e) })) },
    )
  }
  const danger = !!view && dangerous(view.command)
  const status = !view?.started ? null
    : view.exit === 0 ? <span className="text-water-ink">{t('run.ok')}</span>
    : view.exit !== null ? <span className="text-del">{t('run.exit', { code: view.exit })}</span>
    : view.watching ? <span className="text-amber-ink">{t('run.running')}</span>
    : view.kept === false ? null
    : <span className="text-ink-2">{t('run.lost')}</span>
  return (
    <Dialog open={!!ask} onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')} className="max-h-[85vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t('run.title')}</DialogTitle>
          <DialogDescription className="text-xs text-ink-2">{t('run.lead')}</DialogDescription>
        </DialogHeader>
        {view && (
          <div className="flex min-w-0 flex-col gap-3">
            <div className="flex flex-col gap-1">
              <span className="text-xs text-ink-2">{t('run.command')}</span>
              <pre className="max-h-40 overflow-auto rounded-md border bg-sunken px-2.5 py-1.5 font-mono text-[13px] whitespace-pre-wrap wrap-anywhere data-[danger]:border-del data-[danger]:bg-del-row" data-danger={danger || undefined}>{view.command}</pre>
              {danger && <p role="alert" className="flex items-start gap-1.5 text-xs text-del"><Icon name="dropped" className="mt-0.5 size-3.5 flex-none" />{t('run.danger')}</p>}
            </div>
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
              <dt className="text-ink-2">{t('run.dir')}</dt><dd className="font-mono wrap-anywhere">{view.dir}</dd>
              <dt className="text-ink-2">{t('run.machine')}</dt><dd>{view.machine ?? t('run.here')}</dd>
              <dt className="text-ink-2">{t('run.by')}</dt>
              <dd className={view.by === 'user' ? '' : 'text-amber-ink'}>{view.by === 'user' ? t('run.byYou') : t('run.byAgent', { agent: view.by })}</dd>
            </dl>
            {view.review && <Review review={view.review} />}
            {view.started && (
              <div className="flex flex-col gap-1.5">
                {view.kept === false
                  ? <p className="text-xs text-ink-2">{t('run.notKept')}</p>
                  : <pre aria-live="polite" className="max-h-80 min-h-12 overflow-auto rounded-md bg-sunken px-2.5 py-1.5 font-mono text-xs whitespace-pre-wrap wrap-anywhere">{view.output}</pre>}
                <div className="flex items-center gap-2 text-xs">
                  <span role="status" className="flex-1">{status}</span>
                  {view.focusable !== false && <Button variant="outline" size="xs" className="rounded-sm" onClick={() => focusRun(view.id)}><Icon name="terminal" className="size-3" />{t('run.open')}</Button>}
                </div>
              </div>
            )}
          </div>
        )}
        {error && <p role="alert" className="text-xs text-del wrap-anywhere">{error}</p>}
        <div className="flex justify-end gap-2">
          {view?.started ? (
            <DialogClose asChild><Button type="button" variant="ghost" size="sm">{t('run.close')}</Button></DialogClose>
          ) : (
            <>
              <DialogClose asChild><Button type="button" variant="ghost" size="sm">{t('run.cancel')}</Button></DialogClose>
              <Button type="button" size="sm" variant={danger ? 'destructive' : 'default'} disabled={!view || busy} onClick={go}><Icon name="play" className="size-3.5" />{t('run.go')}</Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

/** The verdict of a command's review, as a chip: ok in the done tone, caution amber, danger red. */
export function VerdictChip({ verdict }: { verdict: Verdict }) {
  const { t } = useT()
  const label = { ok: t('run.verdictOk'), caution: t('run.verdictCaution'), danger: t('run.verdictDanger') }[verdict]
  return (
    <span data-verdict={verdict} className="flex-none rounded-[9px] px-1.5 text-[11px] data-[verdict=caution]:bg-amber-bg data-[verdict=caution]:text-amber-ink data-[verdict=danger]:bg-del-row data-[verdict=danger]:text-del data-[verdict=ok]:bg-sunken data-[verdict=ok]:text-water-ink">
      {label}
    </span>
  )
}

/** What a workriver agent says the command will do: being written, its verdict and text, or why there is none. */
function Review({ review }: { review: RunReview }) {
  const { t } = useT()
  return (
    <section aria-labelledby="run-review" aria-live="polite" className="flex flex-col gap-1">
      <h3 id="run-review" className="flex items-center gap-2 text-xs text-ink-2">
        {t('run.review')}
        {review.state === 'done' && <VerdictChip verdict={review.verdict} />}
      </h3>
      {review.state === 'pending' && <p className="text-xs text-ink-2"><span className="pending" aria-hidden />{t('run.reviewing')}</p>}
      {review.state === 'blocked' && (
        <div className="flex flex-col items-start gap-1.5">
          <p className="text-xs text-you-ink"><span className="pending" aria-hidden />{t('run.reviewBlocked', { agent: review.agent })}</p>
          <Button variant="outline" size="xs" className="rounded-sm" onClick={() => focusAgent(review.session)}><Icon name="terminal" className="size-3.5" />{t('ask.toTerminal')}</Button>
        </div>
      )}
      {review.state === 'failed' && <p className="text-xs text-ink-2 wrap-anywhere">{t('run.reviewFailed', { reason: review.reason })}</p>}
      {review.state === 'done' && (
        <div className="rounded-md border px-2.5 py-1.5 text-[13px] wrap-anywhere data-[verdict=caution]:border-amber data-[verdict=danger]:border-del" data-verdict={review.verdict}>
          <Prose text={review.text} fold={false} />
        </div>
      )}
      {review.state === 'done' && <p className="text-[11px] text-ink-2">{t('run.reviewBy')}</p>}
    </section>
  )
}

/** The side panel's command box: where (a unit's work tree, or a project's folder) and what. */
export function RunBox({ ws, unit, project }: { ws: Workspace; unit: string | null; project: string | null }) {
  const { t } = useT()
  const run = useContext(RunContext)
  const units = ws.intents.filter(i => (i.state ?? 'active') === 'active' && (!project || i.project === project)).flatMap(i => i.units).filter(u => !noCode(u) && u.status !== 'merged' && u.progress !== 'done')
  const projects = (ws.projects ?? []).filter(p => !project || p.id === project)
  const options = [
    ...units.map(u => ({ value: `u:${u.id}`, label: t('run.unit', { unit: u.id, title: u.title }), lang: contentLang(u.title) })),
    ...projects.map(p => ({ value: `p:${p.id}`, label: t('run.project', { title: p.title }), lang: contentLang(p.title) })),
  ]
  const [where, setWhere] = useState('')
  const [command, setCommand] = useState('')
  // The unit on screen first, while the user has not picked another.
  const at = options.some(o => o.value === where) ? where : options.find(o => o.value === `u:${unit}`)?.value ?? options[0]?.value ?? ''
  if (!options.length) return <p className="px-2.5 text-sm text-ink-2">{t('run.nowhere')}</p>
  const submit = () => {
    if (!command.trim() || !at) return
    run(at.startsWith('u:') ? { command: command.trim(), unit: at.slice(2) } : { command: command.trim(), project: at.slice(2) })
  }
  return (
    <form className="flex flex-col gap-2 px-2.5" onSubmit={e => { e.preventDefault(); submit() }}>
      <label className="flex items-center gap-2 text-xs text-ink-2">
        <span className="flex-none">{t('run.where')}</span>
        <Choice label={t('run.where')} value={at} onChange={setWhere} options={options} className="h-7 flex-1" />
      </label>
      <div className="flex gap-1.5">
        <Input value={command} onChange={e => setCommand(e.target.value)} placeholder={t('run.placeholder')} aria-label={t('run.placeholder')} className="h-8 flex-1 font-mono text-[13px] md:text-[13px]" />
        <Button type="submit" size="sm" disabled={!command.trim()}><Icon name="play" className="size-3.5" />{t('run.go')}</Button>
      </div>
      <p className="text-xs text-ink-2">{t('run.hint')}</p>
    </form>
  )
}

/** The commands the user ran, newest first; a row opens its run (output, end, terminal). */
export function RecentRuns({ runs }: { runs: RecentRun[] }) {
  const { t, waited, when } = useT()
  const run = useContext(RunContext)
  if (!runs.length) return null
  return (
    <section aria-labelledby="side-run-recent" className="mt-4 px-2.5">
      <h3 id="side-run-recent" className="mb-1 text-xs text-ink-2">{t('run.recent')}</h3>
      <ul>
        {runs.map(r => {
          const at = ms(r.at)
          const from = r.from && (r.step ? t('run.fromStep', { from: r.from, step: r.step }) : r.from)
          return (
            <li key={r.id}>
              <button type="button" className="thread-row" onClick={() => run({ id: r.id, command: r.command })}>
                {/* When the row is short of room the origin gives way first; the command always keeps a few characters. */}
                <Tip text={r.command}><span className="min-w-[6ch] flex-1 truncate font-mono text-xs">{r.command}</span></Tip>
                {from && <span className="min-w-0 max-w-[40%] truncate font-mono text-xs text-ink-2">{from}</span>}
                <span className="flex-none rounded-[9px] bg-sunken px-1.5 text-[11px] data-[state=done]:text-water-ink data-[state=failed]:text-del data-[state=running]:bg-amber-bg data-[state=running]:text-amber-ink data-[state=unknown]:text-ink-2"
                  data-state={r.state === 'done' && r.exit !== 0 ? 'failed' : r.state}>
                  {r.state === 'running' && <span className="pending" aria-hidden />}
                  {r.state === 'running' ? t('run.stateRunning') : r.state === 'unknown' ? t('run.stateUnknown') : r.exit === 0 ? t('run.stateOk') : t('run.stateExit', { code: r.exit })}
                </span>
                {r.verdict && <VerdictChip verdict={r.verdict} />}
                {Number.isFinite(at) && <Tip text={when(at)}><span className="w-8 flex-none text-right text-xs text-ink-2">{waited(at)}</span></Tip>}
              </button>
            </li>
          )
        })}
      </ul>
    </section>
  )
}

/** For the sample, where a unit's or project's command would run (the service knows for real). */
export function mockWhere(ws: Workspace, a: RunAsk): Pick<RunAsk, 'dir' | 'machine'> {
  const u = unitsOf(ws).find(x => x.id === a.unit)
  return u ? { dir: u.repo, machine: u.machine } : { dir: ws.projects?.find(p => p.id === a.project)?.id ?? '~' }
}
