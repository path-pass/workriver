// What to do first, shown in the main area while nothing is recorded yet; and starting one
// planned intent in an open workspace of its project (a workspace whose manager does not run
// shows the manager select, to seat one that does or start one). workriver starts a working
// agent (one that takes a seat) only on the user's own act, never on an agent's, and
// schedules none: which worker gets which task stays the manager's. The temporary agents
// that answer a question or review a command are as before.
import { useEffect, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from '@/ui/primitives/dialog'
import { useT } from '@/i18n'
import { setSpace, startIntent, type StartDelivery } from '@/api/actions'
import type { Intent } from './types'
import type { Space } from '@/agents/types'
import type { Workspace } from '@/workspace/types'
import { Choice } from '@/ui/Choice'
import { Steps } from '@/commands/context'
import { Content, Slot, SLOT } from '@/content/Chips'
import { Input } from '@/ui/primitives/input'
import { runs } from './spaces'
import { SeatPicker } from './Seat'
import { Icon } from '@/ui/Icon'
import { AddProjectButton } from './AddProject'

/** `project`: the title of the project shown, when only it has nothing yet. */
export function Start({ project }: { project?: string }) {
  const { t } = useT()
  return (
    <section aria-labelledby="start" className="flex max-w-[70ch] flex-col gap-3 pt-2">
      <h1 id="start" className="text-xl font-[650] tracking-[-0.012em]">{t('start.title')}</h1>
      <p className="text-sm text-ink-2">{project ? <Slot text={t('start.leadProject', { project: SLOT })}><Content text={project} /></Slot> : t('start.lead')}</p>
      <Steps
        steps={[
          { text: t('start.project'), action: <AddProjectButton />, command: 'workriver project add .' },
          { text: t('start.agent') },
          { text: t('start.seat'), command: 'workriver workspace manager ws1 <agent>' },
          { text: t('start.begin'), command: 'workriver backlog list' },
          { text: t('start.work') },
        ]}
      />
      <p className="text-sm text-ink-2">{t('start.trailer')} <code className="font-mono text-xs">Task: u1#3</code></p>
      <Steps steps={[{ text: t('start.import'), command: 'workriver guide import' }]} />
    </section>
  )
}

/** The workspace an intent of each project was last started in, kept in this browser. */
const LAST = 'workriver.start.'
const lastIn = (project: string) => { try { return localStorage.getItem(LAST + project) } catch { return null } }
const keepLast = (project: string, space: string) => { try { localStorage.setItem(LAST + project, space) } catch { /* not remembered */ } }

/** The open workspaces an intent can start in: those of its project (one of no project: any;
 *  it takes that workspace's project), in the order they were made. */
export function placesOf(ws: Workspace, project: string): Space[] {
  return (ws.workspaces ?? []).filter(w => !w.closed && (!project || w.project === project))
}

/** The one preselected: the only one, else the one last started in, else the first whose manager runs. */
export function firstPlace(places: Space[], last: string | null): Space | undefined {
  return places.find(p => p.id === last) ?? places.find(p => runs(p.manager)) ?? places[0]
}

/**
 * Starting a planned intent: pick an open workspace of its project (each with its manager and
 * state); workriver records the start as the user's and tells that manager to split it. A
 * workspace whose manager does not run shows the manager select; a project with no open
 * workspace offers only a new one.
 */
export function StartIntent({ ws, intent, onClose, onStarted }: { ws: Workspace | null; intent: Intent | null; onClose: () => void; onStarted: (intent: string, workspace: string) => void }) {
  const { t } = useT()
  const project = intent?.project ?? ''
  const places = ws ? placesOf(ws, project) : []
  const [picked, setPicked] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [name, setName] = useState('')
  const [done, setDone] = useState<{ label: string; delivery: StartDelivery } | null>(null)
  useEffect(() => { setPicked(''); setBusy(false); setError(''); setName(''); setDone(null) }, [intent])
  const at = places.find(p => p.id === picked) ?? firstPlace(places, lastIn(project))
  const titleOf = (folder: string) => ws?.projects?.find(p => p.id === folder)?.title ?? ''
  // Of no project, any workspace takes it: each then says its project.
  const option = (p: Space) => {
    const label = project ? `${p.name} ${p.id}` : t('plan.optionProject', { label: `${p.name} ${p.id}`, project: titleOf(p.project) })
    return { value: p.id, label: p.manager ? t('plan.option', { label, manager: p.manager.name, state: t(`manager.state.${p.manager.state}`) }) : t('plan.optionNone', { label }) }
  }
  const fail = (e: unknown) => { setBusy(false); setError(t('plan.failed', { error: e instanceof Error ? e.message : String(e) })) }
  const start = () => {
    if (!intent || !at) return
    setBusy(true)
    const manager = runs(at.manager) ? at.manager.name : undefined
    startIntent(intent.id, at.id, manager).then(
      delivery => { keepLast(project, at.id); setDone({ label: `${at.name} ${at.id}`, delivery }); onStarted(intent.id, at.id) },
      fail,
    )
  }
  const add = () => { if (project && name.trim()) setSpace({ action: 'add', project, name: name.trim() }).then(() => setName(''), fail) }
  const said = (d: StartDelivery) => d.step === 'not-delivered'
    ? t('deliver.notDelivered', { reason: t(`deliver.reason.${d.reasonCode ?? 'unknown'}`) })
    : d.step === 'held' ? t('deliver.held', { agent: d.to ?? '' })
    : t('deliver.delivered', { agent: d.to ?? '' })
  return (
    <Dialog open={!!intent} onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')}>
        {intent && (
          <>
            <DialogTitle><Slot text={t('plan.title', { title: SLOT })}><Content text={intent.title} /></Slot></DialogTitle>
            {done ? (
              <>
                <DialogDescription>{t('plan.started', { label: done.label })}</DialogDescription>
                <p role="status" className={done.delivery.step === 'not-delivered' ? 'text-sm text-del' : 'text-sm text-ink-2'} title={done.delivery.reason}>{said(done.delivery)}</p>
                <div className="flex justify-end"><DialogClose asChild><Button size="sm" variant="ghost">{t('plan.close')}</Button></DialogClose></div>
              </>
            ) : !places.length ? (
              <>
                <DialogDescription>{project ? t('plan.noWorkspace') : t('plan.noWorkspaceAny')}</DialogDescription>
                {!project && <AddProjectButton className="self-start" />}
                {project && (
                  <form className="flex gap-2" onSubmit={e => { e.preventDefault(); add() }}>
                    <Input value={name} onChange={e => setName(e.target.value)} placeholder={t('rail.spaceName')} aria-label={t('rail.spaceName')} className="h-8 flex-1 text-sm md:text-sm" />
                    <Button type="submit" size="sm" disabled={!name.trim()}>{t('rail.spaceAdd')}</Button>
                  </form>
                )}
                {error && <p role="alert" className="text-xs wrap-anywhere text-del">{error}</p>}
              </>
            ) : (
              <>
                <DialogDescription className="sr-only">{t('plan.pickLabel')}</DialogDescription>
                <label className="flex items-center gap-2 text-xs text-ink-2">
                  <span className="flex-none">{t('plan.pickLabel')}</span>
                  <Choice label={t('plan.pickLabel')} value={at?.id ?? ''} onChange={setPicked} options={places.map(option)} placeholder={t('plan.pickOther')} className="h-8 flex-1" />
                </label>
                {at && runs(at.manager) && <p className="text-sm text-ink-2">{t('plan.goHint', { manager: at.manager.name })}</p>}
                {/* Nobody would split the work: the manager select, right here. */}
                {ws && at && !runs(at.manager) && (
                  <>
                    <p className="text-sm text-ink-2">{t('plan.none', { label: `${at.name} ${at.id}` })}</p>
                    <SeatPicker key={at.id} ws={ws} space={at} />
                  </>
                )}
                {error && <p role="alert" className="text-xs wrap-anywhere text-del">{error}</p>}
                <div className="flex justify-end gap-2">
                  <DialogClose asChild><Button type="button" variant="ghost" size="sm">{t('rail.addCancel')}</Button></DialogClose>
                  {at && <Button type="button" size="sm" variant={runs(at.manager) ? 'default' : 'outline'} disabled={busy} onClick={start}><Icon name="play" className="size-3.5" />{runs(at.manager) ? t('plan.go') : t('plan.goAnyway')}</Button>}
                </div>
              </>
            )}
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
