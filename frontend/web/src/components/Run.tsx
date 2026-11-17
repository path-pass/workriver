// Running a command as the user: a gate's or chore's step, or one typed in the side panel.
// Nothing runs before the user has seen the exact text, where it runs and who wrote it, and
// confirmed; then the service types it into a plain shell of the user's in Herdr and the
// dialog follows what that shell shows until the command ends.
import { createContext, useContext, useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { noCode, unitsOf } from '@/data'
import { useT } from '@/i18n'
import { focusRun, prepareRun, readRun, runCommand, type RunAsk, type RunView } from '@/runtime'
import type { Step, Workspace } from '@/types'
import { Choice } from './Choice'
import { Icon, Steps } from './bits'

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
  return <Steps steps={steps} onRun={command => run({ ...ask, command })} />
}

/** The confirm dialog, then the run's output, its exit status and its terminal. */
export function RunDialog({ ask, onClose }: { ask: RunAsk | null; onClose: () => void }) {
  const { t } = useT()
  const [view, setView] = useState<RunView | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  // Each new ask is prepared afresh: the service says where it runs and who wrote it.
  useEffect(() => {
    setView(null); setError(''); setBusy(false)
    if (!ask) return
    let gone = false
    prepareRun(ask).then(v => { if (!gone) setView(v) }, e => { if (!gone) setError(String(e)) })
    return () => { gone = true }
  }, [ask])
  // While it runs, what its shell shows, every second, until it ends or cannot be read.
  const id = view?.started && view.exit === null && view.watching ? view.id : null
  useEffect(() => {
    if (!id) return
    const k = setInterval(() => readRun(id).then(setView, e => setError(String(e))), 1000)
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
    : <span className="text-ink-2">{t('run.lost')}</span>
  return (
    <Dialog open={!!ask} onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')} className="max-h-[85vh] overflow-y-auto sm:max-w-xl">
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
            {view.started && (
              <div className="flex flex-col gap-1.5">
                <pre aria-live="polite" className="max-h-80 min-h-12 overflow-auto rounded-md bg-sunken px-2.5 py-1.5 font-mono text-xs whitespace-pre-wrap wrap-anywhere">{view.output}</pre>
                <div className="flex items-center gap-2 text-xs">
                  <span role="status" className="flex-1">{status}</span>
                  <Button variant="outline" size="xs" className="rounded-sm" onClick={() => focusRun(view.id)}><Icon name="terminal" className="size-3" />{t('run.open')}</Button>
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

/** The side panel's command box: where (a unit's work tree, or a project's folder) and what. */
export function RunBox({ ws, unit, project }: { ws: Workspace; unit: string | null; project: string | null }) {
  const { t } = useT()
  const run = useContext(RunContext)
  const units = ws.intents.filter(i => (i.state ?? 'active') === 'active' && (!project || i.project === project)).flatMap(i => i.units).filter(u => !noCode(u) && u.status !== 'merged' && u.progress !== 'done')
  const projects = (ws.projects ?? []).filter(p => !project || p.id === project)
  const options = [
    ...units.map(u => ({ value: `u:${u.id}`, label: t('run.unit', { unit: u.id, title: u.title }) })),
    ...projects.map(p => ({ value: `p:${p.id}`, label: t('run.project', { title: p.title }) })),
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

/** For the sample, where a unit's or project's command would run (the service knows for real). */
export function mockWhere(ws: Workspace, a: RunAsk): Pick<RunAsk, 'dir' | 'machine'> {
  const u = unitsOf(ws).find(x => x.id === a.unit)
  return u ? { dir: u.repo, machine: u.machine } : { dir: ws.projects?.find(p => p.id === a.project)?.id ?? '~' }
}
